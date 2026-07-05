//! The quest lifecycle: start / complete / cancel, the completion and
//! overlay-event records, the cooldown predicate, and the cancel flow's
//! reward-undo helpers.

use serde_json::Value;
use sqlx::sqlite::SqliteConnection;
use sqlx::Row;

use crate::tracker::{naive_to_epoch, to_iso_utc};

use super::payload::json_truthy;
use super::{QuestError, QuestService};

impl QuestService {
    // ── Quest actions ───────────────────────────────────────────────

    /// Mark a quest as in-progress; `None` when absent or inactive.
    pub async fn start_quest(&self, quest_id: i64) -> Result<Option<Value>, QuestError> {
        let now = naive_to_epoch(self.clock.now());
        let affected =
            sqlx::query("UPDATE quests SET started_at = ? WHERE id = ? AND is_active = 1")
                .bind(now)
                .bind(quest_id)
                .execute(self.db.write())
                .await?
                .rows_affected();
        if affected > 0 {
            self.get_quest(quest_id).await
        } else {
            Ok(None)
        }
    }

    /// Complete a quest: clear the in-progress state, record the
    /// reward (liquid rewards into the ledger, skill rewards into
    /// quest claims), and link the completion to the active session
    /// (or a synthetic key when none is active). Each step commits
    /// separately, exactly as the original's commit points fall.
    pub async fn complete_quest(&self, quest_id: i64) -> Result<Option<Value>, QuestError> {
        let Some(quest) = self.get_quest(quest_id).await? else {
            return Ok(None);
        };
        let now = naive_to_epoch(self.clock.now());
        sqlx::query("UPDATE quests SET started_at = NULL WHERE id = ?")
            .bind(quest_id)
            .execute(self.db.write())
            .await?;

        let reward_ped = quest.get("reward_ped").and_then(Value::as_f64);
        if let Some(reward) = reward_ped.filter(|&reward| reward > 0.0) {
            let name = quest["name"].as_str().expect("quest name");
            if json_truthy(quest.get("reward_is_skill")) {
                // Skill rewards are PES, not PED: a claim row, not a
                // ledger entry.
                sqlx::query(
                    "INSERT INTO quest_claims (quest_id, quest_name, ped_value, claimed_at) \
                     VALUES (?, ?, ?, ?)",
                )
                .bind(quest_id)
                .bind(name)
                .bind(reward)
                .bind(now)
                .execute(self.db.write())
                .await?;
                let mut conn = self.db.write().acquire().await?;
                crate::daily_rollup::refresh_days(&mut conn, [crate::daily_rollup::epoch_day(now)])
                    .await?;
            } else {
                let ledger_id = self.next_id();
                let date = to_iso_utc(now);
                sqlx::query(
                    "INSERT INTO ledger_entries (id, date, type, description, amount, tag) \
                     VALUES (?, ?, ?, ?, ?, ?)",
                )
                .bind(&ledger_id)
                .bind(&date)
                .bind("markup")
                .bind(format!("Quest: {name}"))
                .bind(reward)
                .bind("quest_reward")
                .execute(self.db.write())
                .await?;
                let mut conn = self.db.write().acquire().await?;
                crate::daily_rollup::refresh_days(&mut conn, [date.as_str()]).await?;
            }
        }

        let session_id = self.lock_session().clone();
        self.record_session_completion(session_id.as_deref(), quest_id, Some(now))
            .await?;
        self.get_quest(quest_id).await
    }

    /// Undo an in-progress quest, or reset an active cooldown back to
    /// ready by deleting the most recent completion (optionally
    /// undoing the recorded reward). A quest that is neither started
    /// nor cooling returns as-is.
    pub async fn cancel_quest(
        &self,
        quest_id: i64,
        undo_reward: bool,
    ) -> Result<Option<Value>, QuestError> {
        let Some(quest) = self.get_quest(quest_id).await? else {
            return Ok(None);
        };

        if !quest["started_at"].is_null() {
            sqlx::query("UPDATE quests SET started_at = NULL WHERE id = ? AND is_active = 1")
                .bind(quest_id)
                .execute(self.db.write())
                .await?;
            return self.get_quest(quest_id).await;
        }

        if !self.is_quest_cooling(&quest) {
            return Ok(Some(quest));
        }

        // The original groups the completion delete and the optional
        // reward undo under one commit.
        let mut tx = self.db.write().begin().await?;
        sqlx::query(
            "DELETE FROM session_quest_completions \
             WHERE id = ( \
                 SELECT id FROM session_quest_completions \
                 WHERE quest_id = ? \
                 ORDER BY completed_at DESC, id DESC \
                 LIMIT 1 \
             )",
        )
        .bind(quest_id)
        .execute(&mut *tx)
        .await?;

        if undo_reward {
            let reward_ped = quest.get("reward_ped").and_then(Value::as_f64);
            if let Some(reward) = reward_ped.filter(|&reward| reward > 0.0) {
                if json_truthy(quest.get("reward_is_skill")) {
                    delete_latest_quest_claim(&mut tx, quest_id).await?;
                } else {
                    delete_latest_quest_reward_entry(
                        &mut tx,
                        quest["name"].as_str().expect("quest name"),
                        reward,
                    )
                    .await?;
                }
            }
        }
        tx.commit().await?;
        self.get_quest(quest_id).await
    }

    /// Insert an overlay event when a tracking session is active; any
    /// failure is swallowed, exactly as the original's bare except.
    pub(super) async fn record_notable_event(
        &self,
        event_type: &str,
        description: &str,
        value_ped: f64,
    ) {
        // The original gates on truthiness, so an empty session id
        // skips the write exactly like an absent one.
        let Some(session_id) = self.lock_session().clone().filter(|id| !id.is_empty()) else {
            return;
        };
        let now = naive_to_epoch(self.clock.now());
        let _ = sqlx::query(
            "INSERT INTO notable_events \
             (session_id, kill_id, event_type, mob_or_item, value_ped, timestamp) \
             VALUES (?, NULL, ?, ?, ?, ?)",
        )
        .bind(&session_id)
        .bind(event_type)
        .bind(description)
        .bind(value_ped)
        .bind(now)
        .execute(self.db.write())
        .await;
    }

    /// Record a completion for cooldown and analytics: keyed by the
    /// active session, or a synthetic `manual-` key so a session-less
    /// completion still feeds the derived cooldown.
    async fn record_session_completion(
        &self,
        session_id: Option<&str>,
        quest_id: i64,
        completed_at: Option<f64>,
    ) -> Result<(), QuestError> {
        let key = match session_id {
            Some(session_id) => session_id.to_string(),
            None => format!("manual-{}", self.next_id()),
        };
        let ts = match completed_at {
            Some(ts) => ts,
            None => naive_to_epoch(self.clock.now()),
        };
        sqlx::query(
            "INSERT OR IGNORE INTO session_quest_completions \
             (session_id, quest_id, completed_at) VALUES (?, ?, ?)",
        )
        .bind(&key)
        .bind(quest_id)
        .bind(ts)
        .execute(self.db.write())
        .await?;
        Ok(())
    }

    /// Whether the quest's cooldown window is still open against the
    /// injected clock.
    fn is_quest_cooling(&self, quest: &Value) -> bool {
        let last = quest.get("last_completed_at").and_then(Value::as_f64);
        let cooldown_hours = quest.get("cooldown_hours").and_then(Value::as_f64);
        let (Some(last), Some(cooldown_hours)) = (last, cooldown_hours) else {
            return false;
        };
        if cooldown_hours <= 0.0 {
            return false;
        }
        (last + cooldown_hours * 3600.0) > naive_to_epoch(self.clock.now())
    }
}

/// Delete the newest claim for a quest (the cancel flow's undo).
pub(super) async fn delete_latest_quest_claim(
    conn: &mut SqliteConnection,
    quest_id: i64,
) -> Result<bool, QuestError> {
    let Some(row) = sqlx::query(
        "SELECT id, claimed_at FROM quest_claims \
         WHERE quest_id = ? \
         ORDER BY claimed_at DESC, id DESC \
         LIMIT 1",
    )
    .bind(quest_id)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(false);
    };
    sqlx::query("DELETE FROM quest_claims WHERE id = ?")
        .bind(row.get::<i64, _>(0))
        .execute(&mut *conn)
        .await?;
    // The undone claim may sit days back; reland its day's rollup.
    crate::daily_rollup::refresh_days(
        &mut *conn,
        [crate::daily_rollup::epoch_day(row.get::<f64, _>(1))],
    )
    .await?;
    Ok(true)
}

/// Delete the newest matching quest-reward ledger entry (the cancel
/// flow's undo for liquid rewards).
pub(super) async fn delete_latest_quest_reward_entry(
    conn: &mut SqliteConnection,
    quest_name: &str,
    reward_ped: f64,
) -> Result<bool, QuestError> {
    let Some(row) = sqlx::query(
        "SELECT id, date FROM ledger_entries \
         WHERE type = 'markup' \
           AND tag = 'quest_reward' \
           AND description = ? \
           AND amount = ? \
         ORDER BY date DESC, id DESC \
         LIMIT 1",
    )
    .bind(format!("Quest: {quest_name}"))
    .bind(reward_ped)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(false);
    };
    sqlx::query("DELETE FROM ledger_entries WHERE id = ?")
        .bind(row.get::<String, _>(0))
        .execute(&mut *conn)
        .await?;
    // The undone reward may sit days back; reland its day's rollup.
    crate::daily_rollup::refresh_days(&mut *conn, [row.get::<String, _>(1)]).await?;
    Ok(true)
}
