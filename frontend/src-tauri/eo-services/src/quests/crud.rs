//! Quest CRUD: the enriched quest reads, create / update / soft-delete,
//! the quest-mob rows, and the mob-name autocomplete.

use serde_json::{json, Map, Value};

use crate::db::DbError;
use crate::time::to_iso_utc;

use super::payload::{json_truthy, value_to_sql};
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
        let bases = self
            .db
            .with_reader(move |conn| {
                let mut stmt = conn.prepare(&sql)?;
                let mut rows = stmt.query([])?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    out.push(row_to_quest(row));
                }
                Ok(out)
            })
            .await?;
        let mut quests = Vec::with_capacity(bases.len());
        for mut quest in bases {
            self.enrich_quest(&mut quest).await?;
            quests.push(Value::Object(quest));
        }
        Ok(quests)
    }

    /// A single quest by ID, enriched; `None` when absent.
    pub async fn get_quest(&self, quest_id: i64) -> Result<Option<Value>, QuestError> {
        let sql = format!("{QUEST_SELECT} WHERE q.id = ?");
        let Some(mut quest) = self
            .db
            .with_reader(move |conn| {
                use rusqlite::OptionalExtension as _;
                Ok(conn
                    .query_row(&sql, rusqlite::params![quest_id], |row| {
                        Ok(row_to_quest(row))
                    })
                    .optional()?)
            })
            .await?
        else {
            return Ok(None);
        };
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
        let planet = match data.get("planet") {
            None => json!("Calypso"),
            Some(value) => value.clone(),
        };
        // The bind order matches the INSERT's column list; the owned
        // parameters move into the writer closure.
        let params: Vec<rusqlite::types::Value> = vec![
            value_to_sql(data.get("name").expect("quest payload carries name")),
            value_to_sql(&planet),
            value_to_sql(data.get("waypoint").unwrap_or(&Value::Null)),
            value_to_sql(data.get("cooldown_hours").unwrap_or(&Value::Null)),
            value_to_sql(data.get("reward_ped").unwrap_or(&Value::Null)),
            rusqlite::types::Value::Integer(i64::from(json_truthy(data.get("reward_is_skill")))),
            value_to_sql(&json!(markup)),
            value_to_sql(data.get("notes").unwrap_or(&Value::Null)),
            value_to_sql(data.get("chain_name").unwrap_or(&Value::Null)),
            value_to_sql(data.get("chain_position").unwrap_or(&Value::Null)),
            value_to_sql(data.get("chain_total").unwrap_or(&Value::Null)),
            value_to_sql(data.get("category").unwrap_or(&Value::Null)),
            value_to_sql(data.get("reward_description").unwrap_or(&Value::Null)),
        ];
        // The original's truthiness gate: an empty (or null) mobs payload
        // writes nothing, and the `expect` fires only for a truthy
        // non-list, exactly as the original's did.
        let mobs: Option<Vec<Value>> = match data.get("mobs") {
            Some(mobs) if json_truthy(Some(mobs)) => {
                Some(mobs.as_array().expect("mobs is a list").to_vec())
            }
            _ => None,
        };

        let quest_id = self
            .db
            .with_writer(move |conn| {
                let tx = conn.transaction()?;
                tx.execute(
                    "INSERT INTO quests (name, planet, waypoint, cooldown_hours, \
                     reward_ped, reward_is_skill, expected_reward_markup_percent, \
                     notes, chain_name, chain_position, chain_total, \
                     category, reward_description) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    rusqlite::params_from_iter(params),
                )?;
                let quest_id = tx.last_insert_rowid();
                if let Some(mobs) = &mobs {
                    set_quest_mobs(&tx, quest_id, mobs)?;
                }
                tx.commit()?;
                Ok(quest_id)
            })
            .await?;

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

        // A present-but-null mobs payload refuses: the original crashes
        // after its mob delete, and the next commit on the shared
        // connection silently ratifies the wipe; the typed refusal plus
        // rollback is the sanctioned repair shape. Validated before the
        // writer closure (the refusal is a `QuestError::Invalid`, not a
        // driver error), so nothing commits either way: an early return
        // and an atomic rollback leave the same nothing-written state.
        let mobs: Option<Vec<Value>> = match data.get("mobs") {
            Some(mobs) => Some(
                mobs.as_array()
                    .ok_or_else(|| {
                        QuestError::Invalid("'mobs' must be a list of mob names".to_string())
                    })?
                    .clone(),
            ),
            None => None,
        };

        let update = (!updates.is_empty()).then(|| {
            let set_clause = updates
                .iter()
                .map(|(key, _)| format!("{key} = ?"))
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!("UPDATE quests SET {set_clause} WHERE id = ?");
            let mut params: Vec<rusqlite::types::Value> = updates
                .iter()
                .map(|(_, value)| value_to_sql(value))
                .collect();
            params.push(rusqlite::types::Value::Integer(quest_id));
            (sql, params)
        });

        self.db
            .with_writer(move |conn| {
                let tx = conn.transaction()?;
                if let Some((sql, params)) = update {
                    tx.execute(&sql, rusqlite::params_from_iter(params))?;
                }
                if let Some(mobs) = &mobs {
                    set_quest_mobs(&tx, quest_id, mobs)?;
                }
                tx.commit()?;
                Ok(())
            })
            .await?;

        self.get_quest(quest_id).await
    }

    /// Soft-delete a quest, detaching it from every playlist.
    pub async fn delete_quest(&self, quest_id: i64) -> Result<bool, QuestError> {
        let affected = self
            .db
            .with_writer(move |conn| {
                Ok(conn.execute(
                    "UPDATE quests SET is_active = 0 WHERE id = ? AND is_active = 1",
                    rusqlite::params![quest_id],
                )?)
            })
            .await?;
        if affected > 0 {
            self.db
                .with_writer(move |conn| {
                    conn.execute(
                        "DELETE FROM quest_playlist_items WHERE quest_id = ?",
                        rusqlite::params![quest_id],
                    )?;
                    Ok(())
                })
                .await?;
            return Ok(true);
        }
        Ok(false)
    }

    // ── Mob autocomplete ────────────────────────────────────────────

    /// All distinct mob names across active quests, for autocomplete.
    pub async fn get_all_mob_names(&self) -> Result<Vec<String>, QuestError> {
        Ok(self
            .db
            .with_reader(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT DISTINCT qm.mob_name FROM quest_mobs qm \
                     JOIN quests q ON q.id = qm.quest_id \
                     WHERE q.is_active = 1 \
                     ORDER BY qm.mob_name",
                )?;
                let mut rows = stmt.query([])?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    out.push(row.get::<_, String>(0)?);
                }
                Ok(out)
            })
            .await?)
    }

    // ── Shared helpers ──────────────────────────────────────────────

    async fn quest_mobs(&self, quest_id: i64) -> Result<Vec<String>, QuestError> {
        Ok(self
            .db
            .with_reader(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT mob_name FROM quest_mobs WHERE quest_id = ? ORDER BY mob_name",
                )?;
                let mut rows = stmt.query(rusqlite::params![quest_id])?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    let name = row.get::<_, String>(0)?;
                    if !name.is_empty() {
                        out.push(name);
                    }
                }
                Ok(out)
            })
            .await?)
    }
}

/// One quest row to its dict shape, with the derived cooldown expiry
/// (UTC ISO instant) computed from the latest completion.
fn row_to_quest(row: &rusqlite::Row) -> Map<String, Value> {
    let mut quest = Map::new();
    quest.insert("id".into(), json!(row.get_unwrap::<_, i64>("id")));
    quest.insert("name".into(), json!(row.get_unwrap::<_, String>("name")));
    quest.insert(
        "planet".into(),
        json!(row.get_unwrap::<_, String>("planet")),
    );
    quest.insert(
        "waypoint".into(),
        json!(row.get_unwrap::<_, Option<String>>("waypoint")),
    );
    quest.insert(
        "cooldown_hours".into(),
        json!(row.get_unwrap::<_, Option<f64>>("cooldown_hours")),
    );
    quest.insert(
        "reward_ped".into(),
        json!(row.get_unwrap::<_, Option<f64>>("reward_ped")),
    );
    quest.insert(
        "reward_is_skill".into(),
        json!(row.get_unwrap::<_, i64>("reward_is_skill")),
    );
    quest.insert(
        "expected_reward_markup_percent".into(),
        json!(row.get_unwrap::<_, Option<f64>>("expected_reward_markup_percent")),
    );
    quest.insert(
        "notes".into(),
        json!(row.get_unwrap::<_, Option<String>>("notes")),
    );
    quest.insert(
        "chain_name".into(),
        json!(row.get_unwrap::<_, Option<String>>("chain_name")),
    );
    quest.insert(
        "chain_position".into(),
        json!(row.get_unwrap::<_, Option<i64>>("chain_position")),
    );
    quest.insert(
        "chain_total".into(),
        json!(row.get_unwrap::<_, Option<i64>>("chain_total")),
    );
    quest.insert(
        "started_at".into(),
        json!(row.get_unwrap::<_, Option<f64>>("started_at")),
    );
    quest.insert(
        "is_active".into(),
        json!(row.get_unwrap::<_, i64>("is_active")),
    );
    quest.insert(
        "created_at".into(),
        json!(row.get_unwrap::<_, f64>("created_at")),
    );
    quest.insert(
        "category".into(),
        json!(row.get_unwrap::<_, Option<String>>("category")),
    );
    quest.insert(
        "reward_description".into(),
        json!(row.get_unwrap::<_, Option<String>>("reward_description")),
    );
    quest.insert(
        "updated_at".into(),
        json!(row.get_unwrap::<_, Option<f64>>("updated_at")),
    );
    let last_completed = row.get_unwrap::<_, Option<f64>>("last_completed_at");
    quest.insert("last_completed_at".into(), json!(last_completed));

    let cooldown_hours = row.get_unwrap::<_, Option<f64>>("cooldown_hours");
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

fn set_quest_mobs(
    conn: &rusqlite::Connection,
    quest_id: i64,
    mobs: &[Value],
) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM quest_mobs WHERE quest_id = ?",
        rusqlite::params![quest_id],
    )?;
    for mob in mobs {
        let mob = mob.as_str().expect("mob names are strings").trim();
        if !mob.is_empty() {
            conn.execute(
                "INSERT OR IGNORE INTO quest_mobs (quest_id, mob_name) VALUES (?, ?)",
                rusqlite::params![quest_id, mob],
            )?;
        }
    }
    Ok(())
}
