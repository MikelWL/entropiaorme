//! Chat.log mission detection: name normalisation and matching
//! (exact, containment, fuzzy), mission auto-start, and the
//! reward-suppression filter for MISSION_COMPLETE ticks.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::{json, Value};
use unicode_normalization::UnicodeNormalization;

use crate::chatlog_watcher::{MissionCompletion, SignalLoot};
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

/// Split a colon-variant name into its normalised family and variant
/// parts (last colon wins): "ARIS - Daily Hunting 1: Weak Mortirex"
/// becomes ("aris - daily hunting 1", "weak mortirex"). `None` for a
/// name with no colon.
fn variant_split(name: &str) -> Option<(String, String)> {
    name.rsplit_once(':')
        .map(|(family, variant)| (normalize_quest_name(family), normalize_quest_name(variant)))
}

/// The normalised family part of a colon-variant name, for family
/// membership matching; `None` for a name with no colon.
pub(super) fn variant_family_part(name: &str) -> Option<String> {
    variant_split(name).map(|(family, _)| family)
}

impl QuestService {
    // ── Chat.log mission detection ──────────────────────────────────

    /// Find a quest whose name matches a chat.log mission name: the
    /// "(repeatable)" suffix strips, then a normalised exact match, a
    /// normalised containment (five characters minimum), and finally
    /// the highest fuzzy score at or above the threshold.
    ///
    /// A quest named as a colon-variant family ("Family: Variant") is
    /// held to a structural bar in the non-exact tiers: the mission
    /// line must itself carry the family (equal, or containing it) AND
    /// a variant that clears the fuzzy threshold on its own. The bare
    /// family line is a DIFFERENT mission (the umbrella chooser a
    /// pickup emits), and sibling variants share a long prefix that
    /// whole-string scoring would cross-match.
    ///
    /// `in_progress_only` restricts the candidates to started quests:
    /// the completion path's gate, so a mission line can never complete
    /// a quest the log does not carry as in progress.
    pub async fn match_quest_by_mission_name(
        &self,
        mission_name: &str,
        in_progress_only: bool,
    ) -> Result<Option<Value>, QuestError> {
        let stripped = REPEATABLE_SUFFIX.replace(mission_name, "");
        let mission_raw = stripped.trim();
        let mission_norm = normalize_quest_name(mission_raw);
        let quests: Vec<Value> = self
            .get_quests(true)
            .await?
            .into_iter()
            .filter(|quest| !in_progress_only || json_truthy(quest.get("started_at")))
            .collect();

        for quest in &quests {
            if normalize_quest_name(quest["name"].as_str().expect("quest name")) == mission_norm {
                return Ok(Some(quest.clone()));
            }
        }

        let mission_variant = variant_split(mission_raw);

        for quest in &quests {
            let name = quest["name"].as_str().expect("quest name");
            if name.contains(':') {
                continue; // Variant families match structurally below.
            }
            let quest_norm = normalize_quest_name(name);
            if quest_norm.len() >= 5 && mission_norm.contains(&quest_norm) {
                return Ok(Some(quest.clone()));
            }
        }

        let mission_chars: Vec<char> = mission_norm.chars().collect();
        let mut best_score = 0.0f64;
        let mut best_quest: Option<&Value> = None;
        for quest in &quests {
            let name = quest["name"].as_str().expect("quest name");
            let score = match variant_split(name) {
                Some((quest_family, quest_variant)) => match &mission_variant {
                    Some((mission_family, mission_variant))
                        if *mission_family == quest_family
                            || (quest_family.len() >= 5
                                && mission_family.contains(&quest_family)) =>
                    {
                        let quest_chars: Vec<char> = quest_variant.chars().collect();
                        let variant_chars: Vec<char> = mission_variant.chars().collect();
                        sequence_ratio(&quest_chars, &variant_chars)
                    }
                    _ => 0.0,
                },
                None => {
                    let quest_chars: Vec<char> = normalize_quest_name(name).chars().collect();
                    sequence_ratio(&quest_chars, &mission_chars)
                }
            };
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
    ///
    /// An UNKNOWN mission whose colon-split family part names an active
    /// quest family is a fresh variant of that family (the catalogue
    /// grows by observation: the giver rotates variants, and they are
    /// not enumerable up front). It auto-creates as a family member,
    /// named exactly as the line reads, and starts in the same motion,
    /// so the second encounter onward is an exact match with zero
    /// clicks. A line matching no quest AND no family stays ignored.
    pub async fn start_quest_from_mission(&self, mission_name: &str) -> Result<(), QuestError> {
        let quest = match self
            .match_quest_by_mission_name(mission_name, false)
            .await?
        {
            Some(quest) => quest,
            None => {
                let stripped = REPEATABLE_SUFFIX.replace(mission_name, "");
                let mission_raw = stripped.trim();
                let Some((family_part, variant_part)) = variant_split(mission_raw) else {
                    return Ok(());
                };
                if variant_part.is_empty() {
                    return Ok(());
                }
                let Some((family_id, planet)) = self.find_family_by_norm(&family_part).await?
                else {
                    return Ok(());
                };
                self.create_quest(&json!({
                    "name": mission_raw,
                    "planet": planet,
                    "family_id": family_id,
                }))
                .await?
            }
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
    /// UNIT of a signal item completes at most ONE quest (one marker,
    /// one run; a stacked line's quantity is that many markers); when
    /// several in-progress quests share a signal item, the
    /// oldest-started completes first, deterministically. No reward
    /// bookkeeping and no suppression happen here: a signal quest's
    /// reward IS the tracked loot, so completion only records the
    /// lifecycle fact, the overlay event, and the stretch close.
    pub async fn signal_loot_check(&self, loot: &[SignalLoot]) -> Result<(), QuestError> {
        if loot.is_empty() {
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

        // One marker completes one run: each signal item's unit count
        // (line occurrences times stacked quantity) is a budget the
        // candidate walk draws down, so a single marker never completes
        // two quests sharing it, and two markers in one tick (two runs
        // paid at once) complete two, stacked or not.
        let mut budget: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        for line in loot {
            *budget
                .entry(line.item_name.trim().to_ascii_lowercase())
                .or_insert(0) += line.quantity.max(1);
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
            self.complete_quest_with_loot_evidence(quest_id).await?;
            self.record_notable_event(NotableEventKind::Completed, &quest_name, Ped::ZERO)
                .await;
        }
        Ok(())
    }

    /// A MISSION_COMPLETE tick, before its publishes: match the mission
    /// to an in-progress quest and name which loot item or skill gain
    /// to suppress so the reward is not double-counted by tracking.
    /// Suppression MUST answer before the tick's loot publishes (the
    /// suppressed echo never reaches the consumers); the completion
    /// itself is deliberately NOT here; it lands post-publish through
    /// [`mission_complete_check`](Self::mission_complete_check), so the
    /// tick's own loot stamps into the declared stretch first.
    pub async fn quest_reward_filter(
        &self,
        mission_name: &str,
        loot_items: &[Value],
        skill_gains: &[Value],
    ) -> Result<Option<Value>, QuestError> {
        let Some(quest) = self.match_quest_by_mission_name(mission_name, true).await? else {
            return Ok(None);
        };
        let (result, _) = reward_suppression(&quest, loot_items, skill_gains);
        Ok(result)
    }

    /// A flushed tick's mission completions, strictly AFTER the tick's
    /// publishes (the watcher's post-publish probe): match each to an
    /// in-progress quest, complete it (closing any declared stretch),
    /// and record the overlay event. Ordered after the publishes so the
    /// tick's own loot (the final objective kill and the payout)
    /// stamps into the declared stretch before the completion closes it.
    pub async fn mission_complete_check(
        &self,
        completions: &[MissionCompletion],
    ) -> Result<(), QuestError> {
        for completion in completions {
            let Some(quest) = self
                .match_quest_by_mission_name(&completion.mission_name, true)
                .await?
            else {
                continue;
            };
            let quest_id = quest["id"].as_i64().expect("quest id");
            if completion.loot_items.is_empty() {
                self.complete_quest(quest_id).await?;
            } else {
                self.complete_quest_with_loot_evidence(quest_id).await?;
            }

            let reward_ped = quest.get("reward_ped").and_then(Value::as_f64).map(Ped);
            let is_skill = json_truthy(quest.get("reward_is_skill"));
            let (_, suppressed_desc) =
                reward_suppression(&quest, &completion.loot_items, &completion.skill_gains);
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
        }
        Ok(())
    }
}

/// The reward-echo decision for one completion: which loot item or
/// skill gain (by index) duplicates the quest's configured reward, plus
/// the overlay description of what got suppressed. Pure over the
/// quest row and the tick's data, so the pre-publish filter and the
/// post-publish completion check derive the SAME picture from it.
fn reward_suppression(
    quest: &Value,
    loot_items: &[Value],
    skill_gains: &[Value],
) -> (Option<Value>, Option<String>) {
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

    (result, suppressed_desc)
}
