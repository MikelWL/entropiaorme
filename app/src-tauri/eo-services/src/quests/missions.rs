//! Chat.log mission detection: name normalisation and matching
//! (exact, containment, fuzzy), mission auto-start, and the
//! reward-suppression filter for MISSION_COMPLETE ticks.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::{json, Value};
use unicode_normalization::UnicodeNormalization;

use crate::difflib::sequence_ratio;
use crate::ped::Ped;

use super::lifecycle::NotableEventKind;
use super::payload::json_truthy;
use super::{QuestError, QuestService};

/// Loot values this close to the reward (in PED) count as its echo.
const REWARD_MATCH_TOLERANCE: Ped = Ped(0.02);

/// Stripped from chat.log mission names before matching.
static REPEATABLE_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s*\(repeatable\)\s*$").expect("the suffix pattern compiles")
});

/// The fuzzy-match floor for mission-name matching.
pub const FUZZY_THRESHOLD: f64 = 0.8;

/// Normalise a quest name for comparison: NFKD decomposition, ASCII
/// only, trimmed, lowercased.
pub fn normalize_quest_name(name: &str) -> String {
    name.nfkd()
        .filter(char::is_ascii)
        .collect::<String>()
        .trim()
        .to_lowercase()
}

impl QuestService {
    // ── Chat.log mission detection ──────────────────────────────────

    /// Find a quest whose name matches a chat.log mission name: the
    /// "(repeatable)" suffix strips, then a normalised exact match, a
    /// normalised containment (five characters minimum), and finally
    /// the highest fuzzy score at or above the threshold.
    pub async fn match_quest_by_mission_name(
        &self,
        mission_name: &str,
    ) -> Result<Option<Value>, QuestError> {
        let stripped = REPEATABLE_SUFFIX.replace(mission_name, "");
        let mission_norm = normalize_quest_name(stripped.trim());
        let quests = self.get_quests(true).await?;

        for quest in &quests {
            if normalize_quest_name(quest["name"].as_str().expect("quest name")) == mission_norm {
                return Ok(Some(quest.clone()));
            }
        }

        for quest in &quests {
            let quest_norm = normalize_quest_name(quest["name"].as_str().expect("quest name"));
            if quest_norm.len() >= 5 && mission_norm.contains(&quest_norm) {
                return Ok(Some(quest.clone()));
            }
        }

        let mission_chars: Vec<char> = mission_norm.chars().collect();
        let mut best_score = 0.0f64;
        let mut best_quest: Option<&Value> = None;
        for quest in &quests {
            let quest_norm = normalize_quest_name(quest["name"].as_str().expect("quest name"));
            let quest_chars: Vec<char> = quest_norm.chars().collect();
            let score = sequence_ratio(&quest_chars, &mission_chars);
            if score > best_score {
                best_score = score;
                best_quest = Some(quest);
            }
        }
        Ok(if best_score >= FUZZY_THRESHOLD {
            best_quest.cloned()
        } else {
            None
        })
    }

    /// A "New Mission received" chat.log event: match the mission to a
    /// known quest and start tracking it as if the user clicked Start.
    pub async fn start_quest_from_mission(&self, mission_name: &str) -> Result<(), QuestError> {
        let Some(quest) = self.match_quest_by_mission_name(mission_name).await? else {
            return Ok(());
        };
        if json_truthy(quest.get("started_at")) {
            return Ok(());
        }
        self.start_quest(quest["id"].as_i64().expect("quest id"))
            .await?;
        self.record_notable_event(
            NotableEventKind::Started,
            quest["name"].as_str().expect("quest name"),
            Ped::ZERO,
        )
        .await;
        Ok(())
    }

    /// A loot tick that carried no mission completion: complete the
    /// in-progress signal quests its items pay for (the instance-boss
    /// pattern: the marker item arrives inside the boss's loot clump,
    /// with no mission-log line to route by).
    ///
    /// Matching is trimmed and case-insensitive on the item name. Each
    /// occurrence of a signal item completes at most ONE quest (one
    /// marker, one run); when several in-progress quests share a signal
    /// item, the oldest-started completes first, deterministically. No
    /// reward bookkeeping and no suppression happen here: a signal
    /// quest's reward IS the tracked loot, so completion only records
    /// the lifecycle fact, the overlay event, and the focus close.
    pub async fn signal_loot_check(&self, item_names: &[String]) -> Result<(), QuestError> {
        if item_names.is_empty() {
            return Ok(());
        }
        let candidates: Vec<(i64, String, String)> = self
            .db
            .with_reader(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, name, signal_loot_item FROM quests \
                     WHERE is_active = 1 AND started_at IS NOT NULL \
                       AND signal_loot_item IS NOT NULL \
                     ORDER BY started_at ASC, id ASC",
                )?;
                let mut rows = stmt.query([])?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    out.push((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ));
                }
                Ok(out)
            })
            .await?;
        if candidates.is_empty() {
            return Ok(());
        }

        // One marker completes one run: each signal item's occurrence
        // count is a budget the candidate walk draws down, so a single
        // marker never completes two quests sharing it, and two markers
        // in one tick (two runs paid at once) complete two.
        let mut budget: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for name in item_names {
            *budget.entry(name.trim().to_ascii_lowercase()).or_insert(0) += 1;
        }
        for (quest_id, quest_name, signal) in candidates {
            let key = signal.trim().to_ascii_lowercase();
            let Some(remaining) = budget.get_mut(&key) else {
                continue;
            };
            if *remaining == 0 {
                continue;
            }
            *remaining -= 1;
            self.complete_quest(quest_id).await?;
            self.record_notable_event(NotableEventKind::Completed, &quest_name, Ped::ZERO)
                .await;
        }
        Ok(())
    }

    /// A MISSION_COMPLETE tick: match the mission, auto-complete the
    /// quest, and name which loot item or skill gain to suppress so
    /// the reward is not double-counted by tracking.
    pub async fn quest_reward_filter(
        &self,
        mission_name: &str,
        loot_items: &[Value],
        skill_gains: &[Value],
    ) -> Result<Option<Value>, QuestError> {
        let Some(quest) = self.match_quest_by_mission_name(mission_name).await? else {
            return Ok(None);
        };

        self.complete_quest(quest["id"].as_i64().expect("quest id"))
            .await?;

        let reward_ped = quest.get("reward_ped").and_then(Value::as_f64).map(Ped);
        let is_skill = json_truthy(quest.get("reward_is_skill"));
        let mut result = None;
        let mut suppressed_desc: Option<String> = None;

        if is_skill {
            // The in-game skill pop-up is the same PES reward just
            // recorded as a claim; suppress it from tracking.
            if !skill_gains.is_empty() {
                result = Some(json!({
                    "suppress_loot_index": null,
                    "suppress_skill_index": 0,
                }));
                suppressed_desc = Some("skill reward suppressed".to_string());
            }
        } else if let Some(reward) = reward_ped {
            if !loot_items.is_empty() {
                if reward.is_positive() {
                    let mut best_idx: Option<usize> = None;
                    let mut best_diff = Ped(f64::INFINITY);
                    for (index, item) in loot_items.iter().enumerate() {
                        let value = Ped(item.get("value").and_then(Value::as_f64).unwrap_or(0.0));
                        let diff = (value - reward).abs();
                        if diff < best_diff && diff <= REWARD_MATCH_TOLERANCE {
                            best_diff = diff;
                            best_idx = Some(index);
                        }
                    }
                    if let Some(best_idx) = best_idx {
                        result = Some(json!({
                            "suppress_loot_index": best_idx,
                            "suppress_skill_index": null,
                        }));
                        let item_name = loot_items[best_idx]
                            .get("item_name")
                            .and_then(Value::as_str)
                            .unwrap_or("?");
                        suppressed_desc = Some(format!(
                            "{item_name} ({:.2} PED) suppressed",
                            reward.value()
                        ));
                    }
                } else {
                    // A non-positive reward still suppresses the
                    // cheapest item of the tick.
                    let mut min_idx = 0usize;
                    let mut min_value = Ped(f64::INFINITY);
                    for (index, item) in loot_items.iter().enumerate() {
                        let value = Ped(item.get("value").and_then(Value::as_f64).unwrap_or(0.0));
                        if value < min_value {
                            min_value = value;
                            min_idx = index;
                        }
                    }
                    result = Some(json!({
                        "suppress_loot_index": min_idx,
                        "suppress_skill_index": null,
                    }));
                    let item_name = loot_items[min_idx]
                        .get("item_name")
                        .and_then(Value::as_str)
                        .unwrap_or("?");
                    suppressed_desc = Some(format!("{item_name} suppressed"));
                }
            }
        }

        let mut description = quest["name"].as_str().expect("quest name").to_string();
        if let Some(suppressed) = suppressed_desc {
            description.push_str(": ");
            description.push_str(&suppressed);
        }
        let kind = if is_skill {
            NotableEventKind::CompletedPes
        } else {
            NotableEventKind::Completed
        };
        self.record_notable_event(kind, &description, reward_ped.unwrap_or(Ped::ZERO))
            .await;

        Ok(result)
    }
}
