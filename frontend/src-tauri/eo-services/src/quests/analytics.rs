//! The analytics readers: per-quest and per-playlist sustainability
//! metrics over curated session links, with the engine's own numeric
//! types preserved on the wire.

use serde_json::{json, Map, Value};
use sqlx::Row;

use super::payload::json_truthy;
use super::{QuestError, QuestService, PLAYLIST_GROUP_IMMEDIATE, PLAYLIST_GROUP_LONG_HORIZON};

impl QuestService {
    // ── Analytics ───────────────────────────────────────────────────

    /// Per-quest sustainability metrics across all linked sessions:
    /// raw totals (the frontend derives averages), only for quests
    /// with at least one curated linked session.
    pub async fn get_quest_analytics(&self) -> Result<Vec<Value>, QuestError> {
        let quest_rows = sqlx::query(
            "SELECT q.id, q.name, q.planet, q.category, q.reward_ped, \
                    q.reward_is_skill, q.expected_reward_markup_percent \
             FROM quests q \
             WHERE q.is_active = 1 \
             ORDER BY q.name",
        )
        .fetch_all(self.db.read())
        .await?;

        let mut results = Vec::new();
        for row in quest_rows {
            let quest_id = row.get::<i64, _>(0);
            let stats = self.compute_quest_session_stats(quest_id).await?;
            if stats["linked_sessions"] == json!(0) {
                continue;
            }
            let reward_ped = row.get::<Option<f64>, _>(4);
            let reward_is_skill = row.get::<i64, _>(5) != 0;
            let markup = row.get::<Option<f64>, _>(6);
            // The original's `or 0` collapses an absent or zero reward
            // to the integer zero.
            let reward_value = match reward_ped {
                Some(reward) if reward != 0.0 => json!(reward),
                _ => json!(0),
            };
            let linked_sessions = stats["linked_sessions"].as_i64().expect("session count");
            let mut entry = Map::new();
            entry.insert("quest_id".into(), json!(quest_id));
            entry.insert("quest_name".into(), json!(row.get::<String, _>(1)));
            entry.insert("planet".into(), json!(row.get::<String, _>(2)));
            entry.insert("category".into(), json!(row.get::<Option<String>, _>(3)));
            entry.insert("reward_ped".into(), reward_value.clone());
            entry.insert("reward_is_skill".into(), json!(reward_is_skill));
            entry.insert("expected_reward_markup_percent".into(), json!(markup));
            entry.insert(
                "total_expected_reward_ped".into(),
                expected_reward_total(&reward_value, reward_is_skill, markup, linked_sessions),
            );
            for (key, value) in stats.as_object().expect("stats object") {
                entry.insert(key.clone(), value.clone());
            }
            results.push(Value::Object(entry));
        }
        Ok(results)
    }

    /// Aggregate economics for all sessions where this quest was
    /// completed, via the curated analytics link table.
    async fn compute_quest_session_stats(&self, quest_id: i64) -> Result<Value, QuestError> {
        let rows = sqlx::query(
            "SELECT session_id FROM session_quest_analytics_links \
             WHERE quest_id = ? AND link_type = 'quest'",
        )
        .bind(quest_id)
        .fetch_all(self.db.read())
        .await?;
        let session_ids: Vec<String> = rows.into_iter().map(|row| row.get(0)).collect();
        self.compute_session_set_stats(&session_ids).await
    }

    /// Per-playlist sustainability metrics from curated linked
    /// sessions, for every active playlist.
    pub async fn get_all_playlist_analytics(&self) -> Result<Vec<Value>, QuestError> {
        let playlists = self.get_playlists(true).await?;
        let mut results = Vec::new();
        for playlist in playlists {
            let playlist_id = playlist["id"].as_i64().expect("playlist id");
            if let Some(stats) = self.get_playlist_analytics(playlist_id).await? {
                results.push(stats);
            }
        }
        Ok(results)
    }

    /// Analytics for a single playlist from curated linked sessions;
    /// `None` when the playlist is absent.
    pub async fn get_playlist_analytics(
        &self,
        playlist_id: i64,
    ) -> Result<Option<Value>, QuestError> {
        let Some(playlist) = self.get_playlist(playlist_id).await? else {
            return Ok(None);
        };

        let immediate_ids = self
            .playlist_quest_ids(playlist_id, Some(PLAYLIST_GROUP_IMMEDIATE))
            .await?;
        let long_horizon_ids = self
            .playlist_quest_ids(playlist_id, Some(PLAYLIST_GROUP_LONG_HORIZON))
            .await?;
        if immediate_ids.is_empty() {
            return Ok(Some(json!({
                "playlist_id": playlist_id,
                "playlist_name": playlist["name"],
                "quest_count": 0,
                "long_horizon_quest_count": long_horizon_ids.len(),
                "matched_sessions": 0,
                "total_reward_ped": 0,
                "total_immediate_reward_ped": 0,
                "total_bonus_reward_ped": 0,
                "total_skill_reward_ped": 0,
                "total_immediate_skill_reward_ped": 0,
                "total_bonus_skill_reward_ped": 0,
                "total_expected_reward_ped": 0,
                "total_expected_immediate_reward_ped": 0,
                "total_expected_bonus_reward_ped": 0,
                "total_duration": 0,
                "weapon_cost": 0,
                "heal_cost": 0,
                "enhancer_cost": 0,
                "armour_cost": 0,
                "loot_tt": 0,
                "skill_tt": 0,
            })));
        }

        let session_ids = self.curated_playlist_session_ids(playlist_id).await?;
        let stats = if session_ids.is_empty() {
            json!({
                "linked_sessions": 0,
                "total_duration": 0,
                "weapon_cost": 0,
                "heal_cost": 0,
                "enhancer_cost": 0,
                "armour_cost": 0,
                "loot_tt": 0,
                "skill_tt": 0,
            })
        } else {
            self.compute_session_set_stats(&session_ids).await?
        };
        let reward_stats = self
            .compute_playlist_reward_stats(&session_ids, &immediate_ids, &long_horizon_ids)
            .await?;

        let mut entry = Map::new();
        entry.insert("playlist_id".into(), json!(playlist_id));
        entry.insert("playlist_name".into(), playlist["name"].clone());
        entry.insert("quest_count".into(), json!(immediate_ids.len()));
        entry.insert(
            "long_horizon_quest_count".into(),
            json!(long_horizon_ids.len()),
        );
        for (key, value) in reward_stats.as_object().expect("reward stats") {
            entry.insert(key.clone(), value.clone());
        }
        entry.insert("matched_sessions".into(), stats["linked_sessions"].clone());
        for (key, value) in stats.as_object().expect("stats object") {
            entry.insert(key.clone(), value.clone());
        }
        Ok(Some(Value::Object(entry)))
    }

    async fn curated_playlist_session_ids(
        &self,
        playlist_id: i64,
    ) -> Result<Vec<String>, QuestError> {
        let rows = sqlx::query(
            "SELECT session_id FROM session_quest_analytics_links \
             WHERE playlist_id = ? AND link_type = 'playlist'",
        )
        .bind(playlist_id)
        .fetch_all(self.db.read())
        .await?;
        Ok(rows.into_iter().map(|row| row.get(0)).collect())
    }

    /// Aggregate economics for a set of sessions: completed-session
    /// durations and costs, weapon costs through the per-tool stats,
    /// and loot and skill totals.
    async fn compute_session_set_stats(&self, session_ids: &[String]) -> Result<Value, QuestError> {
        if session_ids.is_empty() {
            return Ok(json!({
                "linked_sessions": 0,
                "total_duration": 0,
                "weapon_cost": 0,
                "heal_cost": 0,
                "enhancer_cost": 0,
                "armour_cost": 0,
                "loot_tt": 0,
                "skill_tt": 0,
            }));
        }

        let placeholders = vec!["?"; session_ids.len()].join(",");
        let bind_all = |sql: String| {
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
            for session_id in session_ids {
                query = query.bind(session_id.as_str());
            }
            query
        };

        let sess_row = bind_all(format!(
            "SELECT COUNT(*), \
                    COALESCE(SUM(s.ended_at - s.started_at), 0), \
                    COALESCE(SUM(s.heal_cost), 0), \
                    COALESCE(SUM(s.armour_cost), 0) \
             FROM tracking_sessions s \
             WHERE s.id IN ({placeholders}) AND s.is_active = 0"
        ))
        .fetch_one(self.db.read())
        .await?;

        let weapon_cost = bind_all(format!(
            "SELECT COALESCE(SUM(ts.cost_per_shot * ts.shots_fired), 0) \
             FROM kill_tool_stats ts \
             JOIN kills k ON k.id = ts.kill_id \
             WHERE k.session_id IN ({placeholders})"
        ))
        .fetch_one(self.db.read())
        .await?;

        let enhancer_cost = bind_all(format!(
            "SELECT COALESCE(SUM(k.enhancer_cost), 0) \
             FROM kills k \
             WHERE k.session_id IN ({placeholders})"
        ))
        .fetch_one(self.db.read())
        .await?;

        let loot_tt = bind_all(format!(
            "SELECT COALESCE(SUM(k.loot_total_ped), 0) \
             FROM kills k \
             WHERE k.session_id IN ({placeholders})"
        ))
        .fetch_one(self.db.read())
        .await?;

        let skill_tt = bind_all(format!(
            "SELECT COALESCE(SUM(sg.ped_value), 0) \
             FROM skill_gains sg \
             WHERE sg.session_id IN ({placeholders})"
        ))
        .fetch_one(self.db.read())
        .await?;

        Ok(json!({
            "linked_sessions": row_i64(&sess_row, 0),
            "total_duration": sql_number(&sess_row, 1),
            "weapon_cost": sql_number(&weapon_cost, 0),
            "heal_cost": sql_number(&sess_row, 2),
            "enhancer_cost": sql_number(&enhancer_cost, 0),
            "armour_cost": sql_number(&sess_row, 3),
            "loot_tt": sql_number(&loot_tt, 0),
            "skill_tt": sql_number(&skill_tt, 0),
        }))
    }

    async fn compute_playlist_reward_stats(
        &self,
        session_ids: &[String],
        immediate_ids: &[i64],
        long_horizon_ids: &[i64],
    ) -> Result<Value, QuestError> {
        if session_ids.is_empty() {
            return Ok(json!({
                "total_reward_ped": 0,
                "total_immediate_reward_ped": 0,
                "total_bonus_reward_ped": 0,
                "total_skill_reward_ped": 0,
                "total_immediate_skill_reward_ped": 0,
                "total_bonus_skill_reward_ped": 0,
                "total_expected_reward_ped": 0,
                "total_expected_immediate_reward_ped": 0,
                "total_expected_bonus_reward_ped": 0,
            }));
        }

        let immediate = self
            .sum_session_quest_rewards(session_ids, immediate_ids, false, None)
            .await?;
        let bonus = self
            .sum_session_quest_rewards(session_ids, long_horizon_ids, false, None)
            .await?;
        let immediate_skill = self
            .sum_session_quest_rewards(session_ids, immediate_ids, false, Some(true))
            .await?;
        let bonus_skill = self
            .sum_session_quest_rewards(session_ids, long_horizon_ids, false, Some(true))
            .await?;
        let expected_immediate = self
            .sum_session_quest_rewards(session_ids, immediate_ids, true, None)
            .await?;
        let expected_bonus = self
            .sum_session_quest_rewards(session_ids, long_horizon_ids, true, None)
            .await?;
        Ok(json!({
            "total_reward_ped": number_sum(&immediate, &bonus),
            "total_immediate_reward_ped": immediate,
            "total_bonus_reward_ped": bonus,
            "total_skill_reward_ped": number_sum(&immediate_skill, &bonus_skill),
            "total_immediate_skill_reward_ped": immediate_skill,
            "total_bonus_skill_reward_ped": bonus_skill,
            "total_expected_reward_ped": number_sum(&expected_immediate, &expected_bonus),
            "total_expected_immediate_reward_ped": expected_immediate,
            "total_expected_bonus_reward_ped": expected_bonus,
        }))
    }

    /// The summed rewards of a session set's completions over a quest
    /// set, optionally as the markup-expected value or filtered to
    /// skill rewards. NULL rewards contribute nothing to the sum; an
    /// empty id set short-circuits to the integer zero, and a falsy
    /// sum collapses to it, both as the original returns.
    async fn sum_session_quest_rewards(
        &self,
        session_ids: &[String],
        quest_ids: &[i64],
        expected: bool,
        skill_only: Option<bool>,
    ) -> Result<Value, QuestError> {
        if session_ids.is_empty() || quest_ids.is_empty() {
            return Ok(json!(0));
        }
        let session_placeholders = vec!["?"; session_ids.len()].join(",");
        let quest_placeholders = vec!["?"; quest_ids.len()].join(",");
        let reward_expr = if expected {
            "CASE \
                WHEN q.reward_is_skill = 1 OR q.reward_ped IS NULL THEN q.reward_ped \
                WHEN q.expected_reward_markup_percent IS NULL THEN q.reward_ped \
                ELSE q.reward_ped * q.expected_reward_markup_percent / 100.0 \
            END"
        } else {
            "q.reward_ped"
        };
        let skill_filter = match skill_only {
            Some(true) => " AND q.reward_is_skill = 1",
            Some(false) => " AND q.reward_is_skill = 0",
            None => "",
        };
        let sql = format!(
            "SELECT COALESCE(SUM({reward_expr}), 0) \
             FROM session_quest_completions sqc \
             JOIN quests q ON q.id = sqc.quest_id \
             WHERE sqc.session_id IN ({session_placeholders}) \
               AND sqc.quest_id IN ({quest_placeholders}) \
               {skill_filter}"
        );
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
        for session_id in session_ids {
            query = query.bind(session_id.as_str());
        }
        for quest_id in quest_ids {
            query = query.bind(quest_id);
        }
        let row = query.fetch_one(self.db.read()).await?;
        let value = sql_number(&row, 0);
        Ok(if json_truthy(Some(&value)) {
            value
        } else {
            json!(0)
        })
    }

    /// Playlist quest ids in item order, optionally filtered to one
    /// group.
    async fn playlist_quest_ids(
        &self,
        playlist_id: i64,
        group_type: Option<&str>,
    ) -> Result<Vec<i64>, QuestError> {
        let mut sql =
            String::from("SELECT quest_id FROM quest_playlist_items WHERE playlist_id = ?");
        if group_type.is_some() {
            sql.push_str(" AND group_type = ?");
        }
        sql.push_str(" ORDER BY sort_order");
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(playlist_id);
        if let Some(group_type) = group_type {
            query = query.bind(group_type);
        }
        let rows = query.fetch_all(self.db.read()).await?;
        Ok(rows.into_iter().map(|row| row.get(0)).collect())
    }
}

/// The expected total reward over a completion count: skill rewards
/// and unmarked rewards multiply plainly; marked positive liquid
/// rewards apply the markup percentage. A non-positive count is the
/// integer zero, and the collapsed integer-zero reward multiplies in
/// integers, both exactly as the original returns them.
fn expected_reward_total(
    reward: &Value,
    reward_is_skill: bool,
    expected_markup: Option<f64>,
    completions: i64,
) -> Value {
    if completions <= 0 {
        return json!(0);
    }
    let Some(reward_ped) = reward.as_f64().filter(|_| reward.is_f64()) else {
        return json!(reward.as_i64().unwrap_or(0) * completions);
    };
    match expected_markup {
        Some(markup) if !reward_is_skill && reward_ped > 0.0 => {
            json!(reward_ped * (markup / 100.0) * completions as f64)
        }
        _ => json!(reward_ped * completions as f64),
    }
}

/// A COUNT column: always an integer.
fn row_i64(row: &sqlx::sqlite::SqliteRow, index: usize) -> i64 {
    row.get::<i64, _>(index)
}

/// An aggregate column with the engine's own numeric type: SQLite
/// returns INTEGER for empty-set COALESCE fallbacks and integer sums,
/// REAL otherwise, and the original emits whichever arrives.
fn sql_number(row: &sqlx::sqlite::SqliteRow, index: usize) -> Value {
    match row.try_get::<f64, _>(index) {
        Ok(value) => json!(value),
        Err(_) => json!(row.get::<i64, _>(index)),
    }
}

/// The sum of two engine-typed numbers, integer when both are (the
/// original's Python addition).
fn number_sum(a: &Value, b: &Value) -> Value {
    match (a.as_i64(), b.as_i64()) {
        (Some(left), Some(right)) => json!(left + right),
        _ => json!(a.as_f64().unwrap_or(0.0) + b.as_f64().unwrap_or(0.0)),
    }
}
