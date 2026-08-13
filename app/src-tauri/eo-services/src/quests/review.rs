//! Append-only review of ambiguous reward captures.

use rusqlite::OptionalExtension as _;
use serde_json::{json, Value};

use super::{QuestError, QuestService};

impl QuestService {
    pub async fn unresolved_reward_reviews(&self) -> Result<Vec<Value>, QuestError> {
        Ok(self
            .db
            .with_reader(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT c.id, c.quest_id, q.name, c.completed_at, \
                            c.reward_policy_snapshot, c.reward_unresolved_reason, \
                            c.reward_evidence_json \
                     FROM session_quest_completions c \
                     JOIN quests q ON q.id = c.quest_id \
                     WHERE c.reward_outcome = 'unresolved' \
                       AND NOT EXISTS (SELECT 1 FROM quest_reward_reviews r \
                                       WHERE r.completion_id = c.id) \
                     ORDER BY c.completed_at DESC, c.id DESC",
                )?;
                let mut rows = stmt.query([])?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    let evidence: Option<String> = row.get(6)?;
                    let parsed = evidence
                        .as_deref()
                        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                        .unwrap_or_else(|| json!({}));
                    out.push(json!({
                        "completion_id": row.get::<_, i64>(0)?,
                        "quest_id": row.get::<_, i64>(1)?,
                        "quest_name": row.get::<_, String>(2)?,
                        "completed_at": row.get::<_, f64>(3)?,
                        "policy": row.get::<_, Option<String>>(4)?,
                        "reason": row.get::<_, Option<String>>(5)?,
                        "loot": parsed.get("loot").cloned().unwrap_or_else(|| json!([])),
                        "isolated": parsed.get("isolated").and_then(Value::as_bool).unwrap_or(false),
                    }));
                }
                Ok(out)
            })
            .await?)
    }

    pub async fn resolve_reward_review(
        &self,
        completion_id: i64,
        selected_indices: &[i64],
        declare_none: bool,
    ) -> Result<(), QuestError> {
        let now = self.now_epoch();
        let selected_indices = selected_indices.to_vec();
        self.db
            .with_writer(move |conn| {
                let tx = conn.transaction()?;
                let (session_id, context_id, completed_at, policy, evidence): (
                    String,
                    Option<i64>,
                    f64,
                    Option<String>,
                    Option<String>,
                ) = tx
                    .query_row(
                        "SELECT session_id, activity_context_id, completed_at, \
                                reward_policy_snapshot, reward_evidence_json \
                         FROM session_quest_completions WHERE id = ? \
                           AND reward_outcome = 'unresolved'",
                        rusqlite::params![completion_id],
                        |row| {
                            Ok((
                                row.get(0)?,
                                row.get(1)?,
                                row.get(2)?,
                                row.get(3)?,
                                row.get(4)?,
                            ))
                        },
                    )
                    .optional()?
                    .ok_or_else(|| {
                        rusqlite::Error::InvalidParameterName("unresolved completion not found".to_string())
                    })?;
                let already_reviewed: i64 = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM quest_reward_reviews WHERE completion_id = ?)",
                    rusqlite::params![completion_id],
                    |row| row.get(0),
                )?;
                if already_reviewed != 0 {
                    return Err(crate::db::DbError::Sqlite(
                        rusqlite::Error::InvalidParameterName(
                            "completion has already been reviewed".to_string(),
                        ),
                    ));
                }

                if declare_none {
                    tx.execute(
                        "INSERT INTO quest_reward_reviews \
                         (completion_id, outcome, policy, note, reviewed_at) \
                         VALUES (?, 'none', 'none', 'Declared no separate reward', ?)",
                        rusqlite::params![completion_id, now],
                    )?;
                    tx.commit()?;
                    return Ok(());
                }

                if selected_indices.is_empty() {
                    return Err(crate::db::DbError::Sqlite(
                        rusqlite::Error::InvalidParameterName(
                            "select at least one reward item".to_string(),
                        ),
                    ));
                }
                let context_id = context_id.ok_or_else(|| {
                    rusqlite::Error::InvalidParameterName(
                        "the completion has no activity context for safe reclassification"
                            .to_string(),
                    )
                })?;
                let evidence = evidence
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                    .ok_or_else(|| {
                        rusqlite::Error::InvalidParameterName("reward evidence is unreadable".to_string())
                    })?;
                let loot = evidence
                    .get("loot")
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        rusqlite::Error::InvalidParameterName("reward evidence has no loot lines".to_string())
                    })?;
                let mut sources = Vec::new();
                for index in selected_indices {
                    let candidate = loot.get(index as usize).ok_or_else(|| {
                        rusqlite::Error::InvalidParameterName("reward selection is out of range".to_string())
                    })?;
                    let item_name = candidate
                        .get("item_name")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let quantity = candidate
                        .get("quantity")
                        .and_then(Value::as_i64)
                        .unwrap_or(1)
                        .max(1);
                    let value_ped = candidate
                        .get("value")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0)
                        .max(0.0);
                    let matches = {
                        let mut stmt = tx.prepare(
                            "SELECT li.id FROM kill_loot_items li \
                             JOIN kills k ON k.id = li.kill_id \
                             WHERE k.session_id = ? AND k.context_id = ? \
                               AND li.deactivated_at IS NULL \
                               AND li.item_name = ? AND li.quantity = ? \
                               AND abs(li.value_ped - ?) < 0.000001 \
                               AND abs(k.timestamp - ?) <= 30",
                        )?;
                        let matches = stmt
                            .query_map(
                            rusqlite::params![
                                session_id,
                                context_id,
                                item_name,
                                quantity,
                                value_ped,
                                completed_at
                            ],
                            |row| row.get::<_, i64>(0),
                        )?
                            .collect::<rusqlite::Result<Vec<_>>>()?;
                        matches
                    };
                    if matches.len() != 1 {
                        return Err(crate::db::DbError::Sqlite(
                            rusqlite::Error::InvalidParameterName(format!(
                                "{item_name} cannot be reclassified safely: expected one exact acquisition, found {}",
                                matches.len()
                            )),
                        ));
                    }
                    sources.push((matches[0], item_name.to_string(), quantity, value_ped));
                }

                tx.execute(
                    "INSERT INTO quest_reward_reviews \
                     (completion_id, outcome, policy, note, reviewed_at) \
                     VALUES (?, 'confirmed', ?, 'Confirmed from observed completion evidence', ?)",
                    rusqlite::params![
                        completion_id,
                        policy.as_deref().unwrap_or("named_items"),
                        now
                    ],
                )?;
                let review_id = tx.last_insert_rowid();
                for (source_id, item_name, quantity, value_ped) in sources {
                    tx.execute(
                        "INSERT INTO quest_reward_review_items \
                         (review_id, source_loot_item_id, item_name, quantity, value_ped) \
                         VALUES (?, ?, ?, ?, ?)",
                        rusqlite::params![review_id, source_id, item_name, quantity, value_ped],
                    )?;
                    tx.execute(
                        "INSERT INTO session_quest_completion_reward_items \
                         (completion_id, item_name, quantity, value_ped) VALUES (?, ?, ?, ?)",
                        rusqlite::params![completion_id, item_name, quantity, value_ped],
                    )?;
                    tx.execute(
                        "UPDATE kill_loot_items SET deactivated_at = ? WHERE id = ?",
                        rusqlite::params![now, source_id],
                    )?;
                }
                crate::session_rollup::recompute_session(&tx, &session_id)?;
                crate::daily_rollup::refresh_session_days(&tx, &session_id)?;
                tx.commit()?;
                Ok(())
            })
            .await?;
        Ok(())
    }
}
