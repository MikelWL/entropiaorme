//! The quest lifecycle: start / complete / cancel, the completion and
//! overlay-event records, the cooldown predicate, and the cancel flow's
//! reward-undo helpers.

use serde_json::Value;

use crate::db::DbError;
use crate::ped::Ped;
use crate::time::to_iso_utc;

use super::payload::json_truthy;
use super::{QuestError, QuestService};

/// One actual item observed as a quest reward. This is completion evidence,
/// not a market valuation: analytics resolves its current markup separately.
#[derive(Debug, Clone)]
pub(super) struct RewardItemEvidence {
    pub item_name: String,
    pub quantity: i64,
    pub value_ped: Ped,
}

#[derive(Debug, Clone)]
pub(super) struct RewardCapture {
    pub outcome: &'static str,
    pub policy_snapshot: String,
    pub items: Vec<RewardItemEvidence>,
    pub unresolved_reason: Option<String>,
    pub evidence_json: Option<String>,
    pub had_tracked_loot: bool,
    pub manual_clump: Option<ManualClumpClaim>,
}

/// Exact source ownership carried from the hand-in candidate read into the
/// completion transaction. Every field is revalidated under the writer lock.
#[derive(Debug, Clone)]
pub(super) struct ManualClumpClaim {
    pub clump_id: i64,
    pub source_id: String,
    pub source_kind: String,
    pub source_record_id: String,
    pub session_id: String,
}

enum CompletionWrite {
    Applied,
    Skipped,
    Refused(String),
}

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
    /// The start also stamps `last_started_at`, the DURABLE start fact:
    /// `started_at` is lifecycle state (completion and cancel clear
    /// it), while a pickup-anchored cooldown needs the instant the
    /// mission was collected to survive both.
    pub async fn start_quest(&self, quest_id: i64) -> Result<Option<Value>, QuestError> {
        let now = self.now_epoch();
        let affected = self
            .db
            .with_writer(move |conn| {
                let tx = conn.transaction()?;
                let affected = tx.execute(
                    "UPDATE quests SET started_at = ?, last_started_at = ? \
                     WHERE id = ? AND is_active = 1 AND started_at IS NULL",
                    rusqlite::params![now, now, quest_id],
                )?;
                if affected > 0 {
                    tx.execute(
                        "INSERT INTO quest_runs(quest_id, status, started_at) \
                         VALUES (?, 'in_progress', ?)",
                        rusqlite::params![quest_id, now],
                    )?;
                }
                tx.commit()?;
                Ok(affected)
            })
            .await?;
        if affected > 0 {
            // Deliberately no interval-layer report: starting a quest is
            // an administrative fact (it is in the mission log), not a
            // declaration that the gameplay from here advances it. Bulk
            // pickup makes the two diverge; which stretches of play are
            // toward a quest is the user's own declaration.
            self.get_quest(quest_id).await
        } else {
            Ok(None)
        }
    }

    /// Complete a quest from an administrative/manual action. Tick-driven
    /// completion supplies a typed reward capture instead.
    pub async fn complete_quest(&self, quest_id: i64) -> Result<Option<Value>, QuestError> {
        self.complete_quest_with_evidence(quest_id, None).await
    }

    pub(super) async fn complete_quest_with_reward_capture(
        &self,
        quest_id: i64,
        capture: RewardCapture,
    ) -> Result<Option<Value>, QuestError> {
        self.complete_quest_with_evidence(quest_id, Some(capture))
            .await
    }

    /// Complete a quest and preserve the immutable economic evidence in one
    /// database transaction. The activity context is snapshotted before the
    /// declared stretch closes, matching the context already stamped on the
    /// completion tick's costs and loot.
    async fn complete_quest_with_evidence(
        &self,
        quest_id: i64,
        capture: Option<RewardCapture>,
    ) -> Result<Option<Value>, QuestError> {
        let Some(quest) = self.get_quest(quest_id).await? else {
            return Ok(None);
        };
        let now = self.now_epoch();
        let session_id = self.current_session();
        if let Some(existing_session_id) = session_id.as_deref().filter(|id| !id.is_empty()) {
            let existing_session_id = existing_session_id.to_string();
            let already_completed = self
                .db
                .with_reader(move |conn| {
                    Ok(conn.query_row(
                        "SELECT EXISTS(SELECT 1 FROM session_quest_completions \
                         WHERE session_id = ? AND quest_id = ?)",
                        rusqlite::params![existing_session_id, quest_id],
                        |row| row.get::<_, i64>(0),
                    )? != 0)
                })
                .await?;
            if already_completed {
                return Ok(Some(quest));
            }
        }
        let attribution = self
            .completion_attribution(session_id.as_deref(), quest_id)
            .await?;
        let manual_hand_in = capture
            .as_ref()
            .is_some_and(|capture| capture.manual_clump.is_some());
        // A declared stretch of this quest (when the user declared one)
        // closes at the completion moment, before the reward is
        // recorded, so it bounds the quest's own play and not the
        // bookkeeping that follows it. Only this quest's stretch
        // closes; a sibling daily's stretch keeps running. With no
        // stretch declared this is a no-op.
        if !manual_hand_in {
            self.report_stretch_closed(quest_id).await;
        }

        let policy = quest
            .get("reward_policy")
            .and_then(Value::as_str)
            .unwrap_or("none");
        let capture = capture.unwrap_or_else(|| match policy {
            "none" => RewardCapture {
                outcome: "none",
                policy_snapshot: policy.to_string(),
                items: Vec::new(),
                unresolved_reason: None,
                evidence_json: None,
                had_tracked_loot: false,
                manual_clump: None,
            },
            "fixed_ped" | "fixed_pes" => RewardCapture {
                outcome: "confirmed",
                policy_snapshot: policy.to_string(),
                items: Vec::new(),
                unresolved_reason: None,
                evidence_json: None,
                had_tracked_loot: false,
                manual_clump: None,
            },
            _ => RewardCapture {
                outcome: "unresolved",
                policy_snapshot: policy.to_string(),
                items: Vec::new(),
                unresolved_reason: Some(
                    "Completion was recorded without a reward-bearing chat tick".to_string(),
                ),
                evidence_json: None,
                had_tracked_loot: false,
                manual_clump: None,
            },
        });
        let reward_ped = quest.get("reward_ped").and_then(Value::as_f64).map(Ped);
        let reward_is_skill = policy == "fixed_pes";
        let reward_source = match reward_ped.filter(|reward| reward.is_positive()) {
            Some(_) if reward_is_skill => "skill",
            Some(_) => "ledger",
            None if capture.had_tracked_loot => "tracked_loot",
            None => "none",
        };
        let reward_kind = match reward_source {
            "ledger" => "fixed_liquid",
            "skill" => "skill",
            _ if !capture.items.is_empty() => "item",
            "tracked_loot" => "included_in_loot",
            _ => "none",
        };
        let expected_markup = quest
            .get("expected_reward_markup_percent")
            .and_then(Value::as_f64);
        let name = quest["name"].as_str().expect("quest name").to_string();
        // Preserve the established deterministic identifier order: a liquid
        // reward receives its ledger id before a session-less completion
        // receives its synthetic key.
        let ledger_id = (reward_source == "ledger").then(|| self.next_id());
        let completion_key = session_id
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| format!("manual-{}", self.next_id()));
        let (activity_context_id, activity_interval_id) = attribution.unzip();
        let reward_value = reward_ped.map(Ped::value);
        let reward_outcome = capture.outcome;
        let policy_snapshot = capture.policy_snapshot;
        let unresolved_reason = capture.unresolved_reason;
        let evidence_json = capture.evidence_json;
        let reward_items = capture.items;
        let manual_clump = capture.manual_clump;
        let reclassified_source_id = manual_clump.as_ref().map(|claim| claim.source_id.clone());
        let ledger_id_for_write = ledger_id.clone();
        let resolution = self
            .db
            .with_writer(move |conn| {
                let tx = conn.transaction()?;
                let run: Option<(i64, f64)> = {
                    use rusqlite::OptionalExtension as _;
                    tx.query_row(
                        "SELECT id, started_at FROM quest_runs \
                         WHERE quest_id = ? AND status = 'in_progress'",
                        rusqlite::params![quest_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()?
                };
                let (run_id, run_started_at) = match run {
                    Some(run) => run,
                    None => {
                        tx.execute(
                            "INSERT INTO quest_runs(quest_id, status, started_at) \
                             VALUES (?, 'in_progress', ?)",
                            rusqlite::params![quest_id, now],
                        )?;
                        (tx.last_insert_rowid(), now)
                    }
                };
                if let Some(claim) = &manual_clump {
                    let valid = tx.query_row(
                        "SELECT EXISTS( \
                         SELECT 1 FROM quest_reward_clumps c \
                         JOIN session_context_intervals sci ON sci.context_id = c.context_id \
                         JOIN session_intervals i ON i.id = sci.interval_id \
                         WHERE c.id = ? AND c.source_id = ? AND c.source_kind = ? \
                           AND c.source_record_id = ? AND c.session_id = ? \
                           AND c.claimed_completion_id IS NULL \
                           AND i.kind = 'quest' AND i.ref_id = ? \
                           AND c.observed_at >= ? \
                           AND ( \
                             (c.source_kind = 'kill' AND EXISTS( \
                               SELECT 1 FROM kills k \
                               WHERE k.id = c.source_record_id \
                                 AND k.loot_source_id = c.source_id \
                                 AND k.session_id = c.session_id \
                                 AND k.context_id = c.context_id)) \
                             OR \
                             (c.source_kind = 'harvest' AND EXISTS( \
                               SELECT 1 FROM harvest_events h \
                               WHERE h.id = c.source_record_id \
                                 AND h.loot_source_id = c.source_id \
                                 AND h.session_id = c.session_id \
                                 AND h.context_id = c.context_id)) \
                           ))",
                        rusqlite::params![
                            claim.clump_id,
                            claim.source_id,
                            claim.source_kind,
                            claim.source_record_id,
                            claim.session_id,
                            quest_id,
                            run_started_at,
                        ],
                        |row| row.get::<_, i64>(0),
                    )? != 0;
                    if !valid {
                        return Ok(CompletionWrite::Refused(
                            "That loot clump is no longer available for this quest".to_string(),
                        ));
                    }
                }
                tx.execute(
                    "UPDATE quests SET started_at = NULL WHERE id = ?",
                    rusqlite::params![quest_id],
                )?;
                let inserted = tx.execute(
                    "INSERT OR IGNORE INTO session_quest_completions \
                     (session_id, quest_id, completed_at, activity_context_id, \
                      activity_interval_id, reward_source, reward_kind, reward_ped, \
                      expected_reward_markup_percent, reward_outcome, \
                      reward_policy_snapshot, reward_unresolved_reason, reward_evidence_json, \
                      quest_run_id) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    rusqlite::params![
                        completion_key,
                        quest_id,
                        now,
                        activity_context_id,
                        activity_interval_id,
                        reward_source,
                        reward_kind,
                        reward_value,
                        expected_markup,
                        reward_outcome,
                        policy_snapshot,
                        unresolved_reason,
                        evidence_json,
                        run_id,
                    ],
                )?;
                if inserted == 0 {
                    tx.commit()?;
                    return Ok(CompletionWrite::Skipped);
                }
                let completion_id = tx.last_insert_rowid();
                tx.execute(
                    "UPDATE quest_runs SET status = 'completed', completed_at = ?, \
                     completion_id = ? WHERE id = ?",
                    rusqlite::params![now, completion_id, run_id],
                )?;
                tx.execute(
                    "INSERT OR IGNORE INTO quest_run_intervals(run_id, interval_id) \
                     SELECT ?, id FROM session_intervals \
                     WHERE kind = 'quest' AND ref_id = ? \
                       AND started_at >= ? AND started_at <= ?",
                    rusqlite::params![run_id, quest_id, run_started_at, now],
                )?;
                for item in reward_items {
                    tx.execute(
                        "INSERT INTO session_quest_completion_reward_items \
                         (completion_id, item_name, quantity, value_ped) \
                         VALUES (?, ?, ?, ?)",
                        rusqlite::params![
                            completion_id,
                            item.item_name,
                            item.quantity.max(1),
                            item.value_ped.value().max(0.0),
                        ],
                    )?;
                }
                if let Some(claim) = &manual_clump {
                    match claim.source_kind.as_str() {
                        "kill" => {
                            tx.execute(
                                "UPDATE kill_loot_items SET deactivated_at = ? \
                                 WHERE kill_id = ? AND deactivated_at IS NULL",
                                rusqlite::params![now, claim.source_record_id],
                            )?;
                            tx.execute(
                                "UPDATE kills SET loot_total_ped = 0 \
                                 WHERE id = ? AND loot_source_id = ?",
                                rusqlite::params![claim.source_record_id, claim.source_id],
                            )?;
                        }
                        "harvest" => {
                            tx.execute(
                                "UPDATE harvest_loot_items SET deactivated_at = ? \
                                 WHERE harvest_id = ? AND deactivated_at IS NULL",
                                rusqlite::params![now, claim.source_record_id],
                            )?;
                            tx.execute(
                                "UPDATE harvest_events SET loot_total_ped = 0 \
                                 WHERE id = ? AND loot_source_id = ?",
                                rusqlite::params![claim.source_record_id, claim.source_id],
                            )?;
                        }
                        _ => {
                            return Ok(CompletionWrite::Refused(
                                "The loot clump has an unsupported source".to_string(),
                            ));
                        }
                    }
                    let claimed = tx.execute(
                        "UPDATE quest_reward_clumps SET claimed_completion_id = ? \
                         WHERE id = ? AND claimed_completion_id IS NULL",
                        rusqlite::params![completion_id, claim.clump_id],
                    )?;
                    if claimed != 1 {
                        return Ok(CompletionWrite::Refused(
                            "That loot clump has already been used".to_string(),
                        ));
                    }
                    crate::session_rollup::recompute_session(&tx, &claim.session_id)?;
                    crate::daily_rollup::refresh_session_days(&tx, &claim.session_id)?;
                }
                if let Some(reward) = reward_ped.filter(|reward| reward.is_positive()) {
                    if reward_is_skill {
                        tx.execute(
                            "INSERT INTO quest_claims (quest_id, quest_name, ped_value, claimed_at) \
                             VALUES (?, ?, ?, ?)",
                            rusqlite::params![quest_id, name, reward.value(), now],
                        )?;
                        let claim_id = tx.last_insert_rowid();
                        tx.execute(
                            "UPDATE session_quest_completions SET quest_claim_id = ? \
                             WHERE id = ?",
                            rusqlite::params![claim_id, completion_id],
                        )?;
                        crate::daily_rollup::refresh_days(
                            &tx,
                            [crate::daily_rollup::epoch_day(now)],
                        )?;
                    } else if let Some(ledger_id) = ledger_id_for_write {
                        let date = to_iso_utc(now);
                        tx.execute(
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
                        tx.execute(
                            "UPDATE session_quest_completions SET ledger_entry_id = ? \
                             WHERE id = ?",
                            rusqlite::params![ledger_id, completion_id],
                        )?;
                        crate::daily_rollup::refresh_days(&tx, [date])?;
                    }
                }
                tx.commit()?;
                Ok(CompletionWrite::Applied)
            })
            .await?;
        match resolution {
            CompletionWrite::Refused(message) => return Err(QuestError::Invalid(message)),
            CompletionWrite::Applied => {
                if let Some(source_id) = reclassified_source_id {
                    self.report_loot_reclassified(source_id).await;
                }
                if manual_hand_in {
                    self.report_stretch_closed(quest_id).await;
                }
            }
            CompletionWrite::Skipped => {}
        }
        self.get_quest(quest_id).await
    }

    /// The exact declared quest stretch and activity signature currently in
    /// force. Absence means the completion is administrative evidence only;
    /// it must never claim the whole session's economics after the fact.
    async fn completion_attribution(
        &self,
        session_id: Option<&str>,
        quest_id: i64,
    ) -> Result<Option<(i64, i64)>, QuestError> {
        use rusqlite::OptionalExtension as _;
        let Some(session_id) = session_id.filter(|id| !id.is_empty()) else {
            return Ok(None);
        };
        let session_id = session_id.to_string();
        Ok(self
            .db
            .with_reader(move |conn| {
                Ok(conn
                    .query_row(
                        "SELECT c.id, i.id \
                         FROM session_contexts c \
                         JOIN session_context_intervals sci ON sci.context_id = c.id \
                         JOIN session_intervals i ON i.id = sci.interval_id \
                         WHERE c.session_id = ? AND i.kind = 'quest' \
                           AND i.ref_id = ? AND i.ended_at IS NULL \
                         ORDER BY c.id DESC LIMIT 1",
                        rusqlite::params![session_id, quest_id],
                        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                    )
                    .optional()?)
            })
            .await?)
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
                    let tx = conn.transaction()?;
                    tx.execute(
                        "UPDATE quests SET started_at = NULL WHERE id = ? AND is_active = 1",
                        rusqlite::params![quest_id],
                    )?;
                    tx.execute(
                        "UPDATE quest_runs SET status = 'cancelled' \
                         WHERE quest_id = ? AND status = 'in_progress'",
                        rusqlite::params![quest_id],
                    )?;
                    tx.commit()?;
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
        //
        // A pickup-anchored quest additionally clears its durable start
        // stamp: for that anchor, "reset the cooldown" IS forgetting
        // the last collection. The first cancel of a started quest
        // deliberately keeps the stamp (an abandoned mission's timer
        // keeps running in game); cancelling AGAIN while cooling is the
        // explicit "that start should not gate me" correction, exactly
        // parallel to the completion-anchored double-cancel.
        let clear_pickup_stamp = quest
            .get("cooldown_anchor")
            .and_then(Value::as_str)
            .is_some_and(|anchor| anchor == "pickup");
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
                use rusqlite::OptionalExtension as _;
                let tx = conn.transaction()?;
                let completion = tx
                    .query_row(
                        "SELECT id, ledger_entry_id, quest_claim_id, quest_run_id \
                         FROM session_quest_completions \
                         WHERE quest_id = ? \
                         ORDER BY completed_at DESC, id DESC LIMIT 1",
                        rusqlite::params![quest_id],
                        |row| {
                            Ok((
                                row.get::<_, i64>(0)?,
                                row.get::<_, Option<String>>(1)?,
                                row.get::<_, Option<i64>>(2)?,
                                row.get::<_, Option<i64>>(3)?,
                            ))
                        },
                    )
                    .optional()?;
                if let Some((completion_id, _, _, run_id)) = &completion {
                    tx.execute(
                        "DELETE FROM session_quest_completion_reward_items \
                         WHERE completion_id = ?",
                        rusqlite::params![completion_id],
                    )?;
                    tx.execute(
                        "UPDATE session_quest_completions SET quest_run_id = NULL \
                         WHERE id = ?",
                        rusqlite::params![completion_id],
                    )?;
                    if let Some(run_id) = run_id {
                        tx.execute(
                            "DELETE FROM quest_run_intervals WHERE run_id = ?",
                            rusqlite::params![run_id],
                        )?;
                        tx.execute(
                            "DELETE FROM quest_runs WHERE id = ?",
                            rusqlite::params![run_id],
                        )?;
                    }
                    tx.execute(
                        "DELETE FROM session_quest_completions WHERE id = ?",
                        rusqlite::params![completion_id],
                    )?;
                }
                if clear_pickup_stamp {
                    tx.execute(
                        "UPDATE quests SET last_started_at = NULL WHERE id = ?",
                        rusqlite::params![quest_id],
                    )?;
                }

                if undo_reward {
                    let linked = completion
                        .as_ref()
                        .is_some_and(|(_, ledger, claim, _)| ledger.is_some() || claim.is_some());
                    if let Some((_, Some(ledger_id), _, _)) = &completion {
                        delete_quest_reward_entry_by_id(&tx, ledger_id)?;
                    }
                    if let Some((_, _, Some(claim_id), _)) = &completion {
                        delete_quest_claim_by_id(&tx, *claim_id)?;
                    }
                    // Completions predating immutable links retain the exact
                    // legacy undo behaviour. New completions always delete by
                    // identity, so similarly named/rewarded quests cannot
                    // reverse one another's rows.
                    if !linked {
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

    /// Whether the quest's OWN cooldown window is still open against
    /// the injected clock, anchored per its own anchor. Deliberately
    /// blind to the family window: this predicate gates the cancel
    /// flow's reset branch, so FAMILY cooling alone never makes a
    /// member cancellable. (The reset itself disavows the quest's own
    /// start fact; a family window derived from that same fact moves
    /// with it, which is the point of the correction.)
    fn is_quest_cooling(&self, quest: &Value) -> bool {
        let anchor = quest.get("cooldown_anchor").and_then(Value::as_str);
        let last = match anchor {
            Some("pickup") => quest.get("last_started_at").and_then(Value::as_f64),
            _ => quest.get("last_completed_at").and_then(Value::as_f64),
        };
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

fn delete_quest_claim_by_id(conn: &rusqlite::Connection, claim_id: i64) -> Result<bool, DbError> {
    use rusqlite::OptionalExtension as _;
    let claimed_at = conn
        .query_row(
            "SELECT claimed_at FROM quest_claims WHERE id = ?",
            rusqlite::params![claim_id],
            |row| row.get::<_, f64>(0),
        )
        .optional()?;
    let Some(claimed_at) = claimed_at else {
        return Ok(false);
    };
    conn.execute(
        "DELETE FROM quest_claims WHERE id = ?",
        rusqlite::params![claim_id],
    )?;
    crate::daily_rollup::refresh_days(conn, [crate::daily_rollup::epoch_day(claimed_at)])?;
    Ok(true)
}

fn delete_quest_reward_entry_by_id(
    conn: &rusqlite::Connection,
    ledger_id: &str,
) -> Result<bool, DbError> {
    use rusqlite::OptionalExtension as _;
    let date = conn
        .query_row(
            "SELECT date FROM ledger_entries WHERE id = ?",
            rusqlite::params![ledger_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(date) = date else {
        return Ok(false);
    };
    conn.execute(
        "DELETE FROM ledger_entries WHERE id = ?",
        rusqlite::params![ledger_id],
    )?;
    crate::daily_rollup::refresh_days(conn, [date])?;
    Ok(true)
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
