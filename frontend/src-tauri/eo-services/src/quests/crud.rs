//! Quest CRUD: the enriched quest reads, create / update / soft-delete,
//! the quest-mob rows, and the mob-name autocomplete.

use serde_json::{json, Map, Value};
use sqlx::sqlite::SqliteConnection;
use sqlx::Row;

use crate::tracker::to_iso_utc;

use super::payload::{bind_json, json_truthy};
use super::{QuestError, QuestService};

/// The enriched quest SELECT: every quest column plus the latest
/// completion instant (cooldown and completion counts derive at read
/// time; no counter column exists).
const QUEST_SELECT: &str = "\
    SELECT q.id, q.name, q.planet, q.waypoint, q.cooldown_hours, \
           q.reward_ped, q.reward_is_skill, q.expected_reward_markup_percent, \
           q.notes, q.chain_name, q.chain_position, q.chain_total, \
           q.started_at, q.is_active, q.created_at, q.category, \
           q.reward_description, q.updated_at, \
           (SELECT MAX(completed_at) \
            FROM session_quest_completions \
            WHERE quest_id = q.id) AS last_completed_at \
    FROM quests q";

impl QuestService {
    // ── Quest CRUD ──────────────────────────────────────────────────

    /// List all quests, enriched with mobs and playlist membership.
    pub async fn get_quests(&self, active_only: bool) -> Result<Vec<Value>, QuestError> {
        let where_clause = if active_only {
            "WHERE q.is_active = 1"
        } else {
            ""
        };
        let sql = format!("{QUEST_SELECT} {where_clause} ORDER BY q.created_at ASC");
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .fetch_all(self.db.read())
            .await?;
        let mut quests = Vec::with_capacity(rows.len());
        for row in rows {
            let mut quest = row_to_quest(&row);
            self.enrich_quest(&mut quest).await?;
            quests.push(Value::Object(quest));
        }
        Ok(quests)
    }

    /// A single quest by ID, enriched; `None` when absent.
    pub async fn get_quest(&self, quest_id: i64) -> Result<Option<Value>, QuestError> {
        let sql = format!("{QUEST_SELECT} WHERE q.id = ?");
        let Some(row) = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(quest_id)
            .fetch_optional(self.db.read())
            .await?
        else {
            return Ok(None);
        };
        let mut quest = row_to_quest(&row);
        self.enrich_quest(&mut quest).await?;
        Ok(Some(Value::Object(quest)))
    }

    async fn enrich_quest(&self, quest: &mut Map<String, Value>) -> Result<(), QuestError> {
        let quest_id = quest["id"].as_i64().expect("integer quest id");
        quest.insert("mobs".into(), json!(self.quest_mobs(quest_id).await?));
        quest.insert(
            "playlist_ids".into(),
            json!(self.quest_playlist_ids(quest_id).await?),
        );
        Ok(())
    }

    /// Create a quest and return it.
    pub async fn create_quest(&self, data: &Value) -> Result<Value, QuestError> {
        let markup = normalize_expected_reward_markup(
            data.get("reward_ped"),
            data.get("reward_is_skill"),
            data.get("expected_reward_markup_percent"),
        );
        let mut tx = self.db.write().begin().await?;
        let query = sqlx::query(
            "INSERT INTO quests (name, planet, waypoint, cooldown_hours, \
             reward_ped, reward_is_skill, expected_reward_markup_percent, \
             notes, chain_name, chain_position, chain_total, \
             category, reward_description) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        );
        let planet = match data.get("planet") {
            None => json!("Calypso"),
            Some(value) => value.clone(),
        };
        let result = bind_json(query, data.get("name").expect("quest payload carries name"));
        let result = bind_json(result, &planet);
        let result = bind_json(result, data.get("waypoint").unwrap_or(&Value::Null));
        let result = bind_json(result, data.get("cooldown_hours").unwrap_or(&Value::Null));
        let result = bind_json(result, data.get("reward_ped").unwrap_or(&Value::Null));
        let result = result.bind(i64::from(json_truthy(data.get("reward_is_skill"))));
        let markup_value = json!(markup);
        let result = bind_json(result, &markup_value);
        let result = bind_json(result, data.get("notes").unwrap_or(&Value::Null));
        let result = bind_json(result, data.get("chain_name").unwrap_or(&Value::Null));
        let result = bind_json(result, data.get("chain_position").unwrap_or(&Value::Null));
        let result = bind_json(result, data.get("chain_total").unwrap_or(&Value::Null));
        let result = bind_json(result, data.get("category").unwrap_or(&Value::Null));
        let result = bind_json(
            result,
            data.get("reward_description").unwrap_or(&Value::Null),
        );
        let quest_id = result.execute(&mut *tx).await?.last_insert_rowid();

        if let Some(mobs) = data.get("mobs") {
            // The original's truthiness gate: an empty (or null) mobs
            // payload writes nothing.
            if json_truthy(Some(mobs)) {
                set_quest_mobs(&mut tx, quest_id, mobs.as_array().expect("mobs is a list")).await?;
            }
        }
        tx.commit().await?;

        Ok(self
            .get_quest(quest_id)
            .await?
            .expect("the quest was just inserted"))
    }

    /// Update a quest's fields; `None` when the quest is absent.
    pub async fn update_quest(
        &self,
        quest_id: i64,
        data: &Value,
    ) -> Result<Option<Value>, QuestError> {
        let Some(existing) = self.get_quest(quest_id).await? else {
            return Ok(None);
        };

        const ALLOWED: [&str; 13] = [
            "name",
            "planet",
            "waypoint",
            "cooldown_hours",
            "reward_ped",
            "reward_is_skill",
            "notes",
            "chain_name",
            "chain_position",
            "chain_total",
            "category",
            "reward_description",
            "expected_reward_markup_percent",
        ];
        let mut updates: Vec<(&str, Value)> = Vec::new();
        for key in ALLOWED {
            if let Some(value) = data.get(key) {
                let value = if key == "reward_is_skill" {
                    json!(i64::from(json_truthy(Some(value))))
                } else {
                    value.clone()
                };
                updates.push((key, value));
            }
        }

        // A change to any reward field re-normalises the stored markup
        // from the merged (incoming-over-existing) reward picture.
        let reward_keys = [
            "reward_ped",
            "reward_is_skill",
            "expected_reward_markup_percent",
        ];
        if reward_keys.iter().any(|key| data.get(key).is_some()) {
            let merged = |key: &str| {
                data.get(key)
                    .cloned()
                    .unwrap_or_else(|| existing.get(key).cloned().unwrap_or(Value::Null))
            };
            let markup = normalize_expected_reward_markup(
                Some(&merged("reward_ped")),
                Some(&merged("reward_is_skill")),
                Some(&merged("expected_reward_markup_percent")),
            );
            let entry = ("expected_reward_markup_percent", json!(markup));
            match updates
                .iter_mut()
                .find(|(key, _)| *key == "expected_reward_markup_percent")
            {
                Some(existing_entry) => *existing_entry = entry,
                None => updates.push(entry),
            }
        }

        let mut tx = self.db.write().begin().await?;
        if !updates.is_empty() {
            let set_clause = updates
                .iter()
                .map(|(key, _)| format!("{key} = ?"))
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!("UPDATE quests SET {set_clause} WHERE id = ?");
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
            for (_, value) in &updates {
                query = bind_json(query, value);
            }
            query.bind(quest_id).execute(&mut *tx).await?;
        }

        if let Some(mobs) = data.get("mobs") {
            // A present-but-null mobs payload refuses: the original
            // crashes after its mob delete, and the next commit on the
            // shared connection silently ratifies the wipe; the typed
            // refusal plus rollback is the sanctioned repair shape.
            let mobs = mobs.as_array().ok_or_else(|| {
                QuestError::Invalid("'mobs' must be a list of mob names".to_string())
            })?;
            set_quest_mobs(&mut tx, quest_id, mobs).await?;
        }
        tx.commit().await?;

        self.get_quest(quest_id).await
    }

    /// Soft-delete a quest, detaching it from every playlist.
    pub async fn delete_quest(&self, quest_id: i64) -> Result<bool, QuestError> {
        let affected =
            sqlx::query("UPDATE quests SET is_active = 0 WHERE id = ? AND is_active = 1")
                .bind(quest_id)
                .execute(self.db.write())
                .await?
                .rows_affected();
        if affected > 0 {
            sqlx::query("DELETE FROM quest_playlist_items WHERE quest_id = ?")
                .bind(quest_id)
                .execute(self.db.write())
                .await?;
            return Ok(true);
        }
        Ok(false)
    }

    // ── Mob autocomplete ────────────────────────────────────────────

    /// All distinct mob names across active quests, for autocomplete.
    pub async fn get_all_mob_names(&self) -> Result<Vec<String>, QuestError> {
        let rows = sqlx::query(
            "SELECT DISTINCT qm.mob_name FROM quest_mobs qm \
             JOIN quests q ON q.id = qm.quest_id \
             WHERE q.is_active = 1 \
             ORDER BY qm.mob_name",
        )
        .fetch_all(self.db.read())
        .await?;
        Ok(rows.into_iter().map(|row| row.get(0)).collect())
    }

    // ── Shared helpers ──────────────────────────────────────────────

    async fn quest_mobs(&self, quest_id: i64) -> Result<Vec<String>, QuestError> {
        let rows =
            sqlx::query("SELECT mob_name FROM quest_mobs WHERE quest_id = ? ORDER BY mob_name")
                .bind(quest_id)
                .fetch_all(self.db.read())
                .await?;
        Ok(rows
            .into_iter()
            .map(|row| row.get::<String, _>(0))
            .filter(|name| !name.is_empty())
            .collect())
    }
}

/// One quest row to its dict shape, with the derived cooldown expiry
/// (UTC ISO instant) computed from the latest completion.
fn row_to_quest(row: &sqlx::sqlite::SqliteRow) -> Map<String, Value> {
    let mut quest = Map::new();
    quest.insert("id".into(), json!(row.get::<i64, _>("id")));
    quest.insert("name".into(), json!(row.get::<String, _>("name")));
    quest.insert("planet".into(), json!(row.get::<String, _>("planet")));
    quest.insert(
        "waypoint".into(),
        json!(row.get::<Option<String>, _>("waypoint")),
    );
    quest.insert(
        "cooldown_hours".into(),
        json!(row.get::<Option<f64>, _>("cooldown_hours")),
    );
    quest.insert(
        "reward_ped".into(),
        json!(row.get::<Option<f64>, _>("reward_ped")),
    );
    quest.insert(
        "reward_is_skill".into(),
        json!(row.get::<i64, _>("reward_is_skill")),
    );
    quest.insert(
        "expected_reward_markup_percent".into(),
        json!(row.get::<Option<f64>, _>("expected_reward_markup_percent")),
    );
    quest.insert("notes".into(), json!(row.get::<Option<String>, _>("notes")));
    quest.insert(
        "chain_name".into(),
        json!(row.get::<Option<String>, _>("chain_name")),
    );
    quest.insert(
        "chain_position".into(),
        json!(row.get::<Option<i64>, _>("chain_position")),
    );
    quest.insert(
        "chain_total".into(),
        json!(row.get::<Option<i64>, _>("chain_total")),
    );
    quest.insert(
        "started_at".into(),
        json!(row.get::<Option<f64>, _>("started_at")),
    );
    quest.insert("is_active".into(), json!(row.get::<i64, _>("is_active")));
    quest.insert("created_at".into(), json!(row.get::<f64, _>("created_at")));
    quest.insert(
        "category".into(),
        json!(row.get::<Option<String>, _>("category")),
    );
    quest.insert(
        "reward_description".into(),
        json!(row.get::<Option<String>, _>("reward_description")),
    );
    quest.insert(
        "updated_at".into(),
        json!(row.get::<Option<f64>, _>("updated_at")),
    );
    let last_completed = row.get::<Option<f64>, _>("last_completed_at");
    quest.insert("last_completed_at".into(), json!(last_completed));

    let cooldown_hours = row.get::<Option<f64>, _>("cooldown_hours");
    let expires = match (last_completed, cooldown_hours) {
        (Some(last), Some(hours)) if hours > 0.0 => Some(to_iso_utc(last + hours * 3600.0)),
        _ => None,
    };
    quest.insert("cooldown_expires_at".into(), json!(expires));
    quest
}

/// The stored markup only exists for liquid (non-skill) rewards with a
/// positive PED value; anything else normalises to null.
fn normalize_expected_reward_markup(
    reward_ped: Option<&Value>,
    reward_is_skill: Option<&Value>,
    expected_markup: Option<&Value>,
) -> Option<f64> {
    if json_truthy(reward_is_skill) {
        return None;
    }
    let reward_ped = reward_ped.filter(|value| !value.is_null())?;
    let reward_ped = reward_ped.as_f64().expect("numeric reward_ped");
    if reward_ped <= 0.0 {
        return None;
    }
    let expected_markup = expected_markup.filter(|value| !value.is_null())?;
    Some(expected_markup.as_f64().expect("numeric expected markup"))
}

async fn set_quest_mobs(
    conn: &mut SqliteConnection,
    quest_id: i64,
    mobs: &[Value],
) -> Result<(), QuestError> {
    sqlx::query("DELETE FROM quest_mobs WHERE quest_id = ?")
        .bind(quest_id)
        .execute(&mut *conn)
        .await?;
    for mob in mobs {
        let mob = mob.as_str().expect("mob names are strings").trim();
        if !mob.is_empty() {
            sqlx::query("INSERT OR IGNORE INTO quest_mobs (quest_id, mob_name) VALUES (?, ?)")
                .bind(quest_id)
                .bind(mob)
                .execute(&mut *conn)
                .await?;
        }
    }
    Ok(())
}
