//! The quest lifecycle: start / complete / cancel, the completion and
//! overlay-event records, the cooldown predicate, and the cancel flow's
//! reward-undo helpers.

use serde_json::Value;

use crate::db::DbError;
use crate::ped::Ped;
use crate::time::to_iso_utc;

use super::payload::json_truthy;
use super::{QuestError, QuestService};

/// The overlay-event vocabulary the quest flows record: a started
/// quest, a completed liquid reward, a completed skill (PES) reward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NotableEventKind {
    Started,
    Completed,
    CompletedPes,
}

impl NotableEventKind {
    fn as_str(self) -> &'static str {
        match self {
            NotableEventKind::Started => "quest_started",
            NotableEventKind::Completed => "quest_completed",
            NotableEventKind::CompletedPes => "quest_completed_pes",
        }
    }
}

impl QuestService {
    // ── Quest actions ───────────────────────────────────────────────

    /// Mark a quest as in-progress; `None` when absent or inactive.
    pub async fn start_quest(&self, quest_id: i64) -> Result<Option<Value>, QuestError> {
        let now = self.now_epoch();
        let affected = self
            .db
            .with_writer(move |conn| {
                Ok(conn.execute(
                    "UPDATE quests SET started_at = ? WHERE id = ? AND is_active = 1",
                    rusqlite::params![now, quest_id],
                )?)
            })
            .await?;
        if affected > 0 {
            // Deliberately no interval-layer report: starting a quest is
            // an administrative fact (it is in the mission log), not a
            // declaration that the gameplay from here advances it. Bulk
            // pickup makes the two diverge; which stretches of play are
            // toward a quest is the user's focus declaration.
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
        let now = self.now_epoch();
        self.db
            .with_writer(move |conn| {
                conn.execute(
                    "UPDATE quests SET started_at = NULL WHERE id = ?",
                    rusqlite::params![quest_id],
                )?;
                Ok(())
            })
            .await?;
        // A focused stretch of this quest (when the user declared one)
        // closes at the completion moment, before the reward is
        // recorded, so it bounds the quest's own play and not the
        // bookkeeping that follows it. Only this quest's stretch
        // closes; a sibling daily's focus keeps running. With no focus
        // declared this is a no-op.
        self.report_focus_closed(quest_id).await;

        let reward_ped = quest.get("reward_ped").and_then(Value::as_f64).map(Ped);
        if let Some(reward) = reward_ped.filter(|reward| reward.is_positive()) {
            let name = quest["name"].as_str().expect("quest name").to_string();
            if json_truthy(quest.get("reward_is_skill")) {
                // Skill rewards are PES, not PED: a claim row, not a
                // ledger entry.
                self.db
                    .with_writer(move |conn| {
                        conn.execute(
                            "INSERT INTO quest_claims (quest_id, quest_name, ped_value, claimed_at) \
                             VALUES (?, ?, ?, ?)",
                            rusqlite::params![quest_id, name, reward.value(), now],
                        )?;
                        Ok(())
                    })
                    .await?;
                let day = crate::daily_rollup::epoch_day(now);
                self.db
                    .with_writer(move |conn| crate::daily_rollup::refresh_days(conn, [day]))
                    .await?;
            } else {
                let ledger_id = self.next_id();
                let date = to_iso_utc(now);
                let refresh_date = date.clone();
                self.db
                    .with_writer(move |conn| {
                        conn.execute(
                            "INSERT INTO ledger_entries (id, date, type, description, amount, tag) \
                             VALUES (?, ?, ?, ?, ?, ?)",
                            rusqlite::params![
                                ledger_id,
                                date,
                                "markup",
                                format!("Quest: {name}"),
                                reward.value(),
                                "quest_reward"
                            ],
                        )?;
                        Ok(())
                    })
                    .await?;
                self.db
                    .with_writer(move |conn| {
                        crate::daily_rollup::refresh_days(conn, [refresh_date])
                    })
                    .await?;
            }
        }

        let session_id = self.current_session();
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
            self.db
                .with_writer(move |conn| {
                    conn.execute(
                        "UPDATE quests SET started_at = NULL WHERE id = ? AND is_active = 1",
                        rusqlite::params![quest_id],
                    )?;
                    Ok(())
                })
                .await?;
            return self.get_quest(quest_id).await;
        }

        if !self.is_quest_cooling(&quest) {
            return Ok(Some(quest));
        }

        // The original groups the completion delete and the optional
        // reward undo under one commit.
        let reward_ped = quest.get("reward_ped").and_then(Value::as_f64).map(Ped);
        let reward_is_skill = json_truthy(quest.get("reward_is_skill"));
        // Extracted before the closure so the panic on a missing name fires
        // in exactly the branch the original's `.expect` did (undo requested,
        // a positive liquid reward).
        let undo_name = if undo_reward {
            quest["name"].as_str().map(str::to_string)
        } else {
            None
        };
        self.db
            .with_writer(move |conn| {
                let tx = conn.transaction()?;
                tx.execute(
                    "DELETE FROM session_quest_completions \
                     WHERE id = ( \
                         SELECT id FROM session_quest_completions \
                         WHERE quest_id = ? \
                         ORDER BY completed_at DESC, id DESC \
                         LIMIT 1 \
                     )",
                    rusqlite::params![quest_id],
                )?;

                if undo_reward {
                    if let Some(reward) = reward_ped.filter(|reward| reward.is_positive()) {
                        if reward_is_skill {
                            delete_latest_quest_claim(&tx, quest_id)?;
                        } else {
                            delete_latest_quest_reward_entry(
                                &tx,
                                undo_name.as_deref().expect("quest name"),
                                reward,
                            )?;
                        }
                    }
                }
                tx.commit()?;
                Ok(())
            })
            .await?;
        self.get_quest(quest_id).await
    }

    /// Insert an overlay event when a tracking session is active; any
    /// failure is swallowed, exactly as the original's bare except.
    pub(super) async fn record_notable_event(
        &self,
        kind: NotableEventKind,
        description: &str,
        value: Ped,
    ) {
        // The original gates on truthiness, so an empty session id
        // skips the write exactly like an absent one.
        let Some(session_id) = self.current_session().filter(|id| !id.is_empty()) else {
            return;
        };
        let now = self.now_epoch();
        let event_type = kind.as_str();
        let description = description.to_string();
        let value = value.value();
        let _ = self
            .db
            .with_writer(move |conn| {
                conn.execute(
                    "INSERT INTO notable_events \
                     (session_id, kill_id, event_type, mob_or_item, value_ped, timestamp) \
                     VALUES (?, NULL, ?, ?, ?, ?)",
                    rusqlite::params![session_id, event_type, description, value, now],
                )?;
                Ok(())
            })
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
            None => self.now_epoch(),
        };
        self.db
            .with_writer(move |conn| {
                conn.execute(
                    "INSERT OR IGNORE INTO session_quest_completions \
                     (session_id, quest_id, completed_at) VALUES (?, ?, ?)",
                    rusqlite::params![key, quest_id, ts],
                )?;
                Ok(())
            })
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
        (last + cooldown_hours * 3600.0) > self.now_epoch()
    }
}

/// Delete the newest claim for a quest (the cancel flow's undo).
pub(super) fn delete_latest_quest_claim(
    conn: &rusqlite::Connection,
    quest_id: i64,
) -> Result<bool, DbError> {
    use rusqlite::OptionalExtension as _;
    let Some((id, claimed_at)) = conn
        .query_row(
            "SELECT id, claimed_at FROM quest_claims \
             WHERE quest_id = ? \
             ORDER BY claimed_at DESC, id DESC \
             LIMIT 1",
            rusqlite::params![quest_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)?)),
        )
        .optional()?
    else {
        return Ok(false);
    };
    conn.execute(
        "DELETE FROM quest_claims WHERE id = ?",
        rusqlite::params![id],
    )?;
    // The undone claim may sit days back; reland its day's rollup.
    crate::daily_rollup::refresh_days(conn, [crate::daily_rollup::epoch_day(claimed_at)])?;
    Ok(true)
}

/// Delete the newest matching quest-reward ledger entry (the cancel
/// flow's undo for liquid rewards).
pub(super) fn delete_latest_quest_reward_entry(
    conn: &rusqlite::Connection,
    quest_name: &str,
    reward_ped: Ped,
) -> Result<bool, DbError> {
    use rusqlite::OptionalExtension as _;
    let Some((id, date)) = conn
        .query_row(
            "SELECT id, date FROM ledger_entries \
             WHERE type = 'markup' \
               AND tag = 'quest_reward' \
               AND description = ? \
               AND amount = ? \
             ORDER BY date DESC, id DESC \
             LIMIT 1",
            rusqlite::params![format!("Quest: {quest_name}"), reward_ped.value()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
    else {
        return Ok(false);
    };
    conn.execute(
        "DELETE FROM ledger_entries WHERE id = ?",
        rusqlite::params![id],
    )?;
    // The undone reward may sit days back; reland its day's rollup.
    crate::daily_rollup::refresh_days(conn, [date])?;
    Ok(true)
}
