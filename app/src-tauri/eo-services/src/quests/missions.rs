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

use super::lifecycle::{NotableEventKind, RewardCapture, RewardItemEvidence};
use super::payload::json_truthy;
use super::{QuestError, QuestService};

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
    /// oldest-started completes first, deterministically. The marker may
    /// independently be a named reward. In that case the
    /// pre-publish filter suppresses only a whole line whose units can all be
    /// assigned safely; this post-publish probe mirrors that assignment.
    pub async fn signal_reward_filter(
        &self,
        loot_items: &[Value],
    ) -> Result<Option<Value>, QuestError> {
        let mut candidates: Vec<Value> = self
            .get_quests(true)
            .await?
            .into_iter()
            .filter(|quest| {
                json_truthy(quest.get("started_at"))
                    && quest.get("completion_trigger").and_then(Value::as_str)
                        == Some("signal_item")
            })
            .collect();
        candidates.sort_by(signal_candidate_order);
        let mut indices = Vec::new();
        let mut used: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for (index, item) in loot_items.iter().enumerate() {
            let name = item
                .get("item_name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let quantity = item
                .get("quantity")
                .and_then(Value::as_i64)
                .unwrap_or(1)
                .max(1);
            let matching: Vec<&Value> = candidates
                .iter()
                .filter(|quest| {
                    quest
                        .get("signal_loot_item")
                        .and_then(Value::as_str)
                        .is_some_and(|signal| signal.trim().eq_ignore_ascii_case(name))
                })
                .collect();
            let offset = used.entry(name.to_ascii_lowercase()).or_default();
            let assigned: Vec<&Value> = matching
                .iter()
                .skip(*offset)
                .take(quantity as usize)
                .copied()
                .collect();
            *offset += assigned.len();
            let safely_owned = assigned.len() == quantity as usize
                && assigned
                    .iter()
                    .all(|quest| signal_is_named_reward(quest, name));
            if safely_owned {
                indices.push(index);
            }
        }
        Ok((!indices.is_empty()).then(|| json!({ "suppress_loot_indices": indices })))
    }

    pub async fn signal_loot_check(&self, loot: &[SignalLoot]) -> Result<(), QuestError> {
        if loot.is_empty() {
            return Ok(());
        }
        let mut candidates: Vec<Value> = self
            .get_quests(true)
            .await?
            .into_iter()
            .filter(|quest| {
                json_truthy(quest.get("started_at"))
                    && quest.get("completion_trigger").and_then(Value::as_str)
                        == Some("signal_item")
            })
            .collect();
        candidates.sort_by(signal_candidate_order);
        if candidates.is_empty() {
            return Ok(());
        }

        // One marker completes one run: each signal item's unit count
        // (line occurrences times stacked quantity) is a budget the
        // candidate walk draws down, so a single marker never completes
        // two quests sharing it, and two markers in one tick (two runs
        // paid at once) complete two, stacked or not.
        let mut assignments: Vec<(Value, RewardItemEvidence)> = Vec::new();
        let mut used: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for line in loot {
            let quantity = line.quantity.max(1);
            let key = line.item_name.trim().to_ascii_lowercase();
            let matching: Vec<&Value> = candidates
                .iter()
                .filter(|quest| {
                    quest
                        .get("signal_loot_item")
                        .and_then(Value::as_str)
                        .is_some_and(|signal| {
                            signal.trim().eq_ignore_ascii_case(line.item_name.trim())
                        })
                })
                .collect();
            let offset = used.entry(key).or_default();
            let assigned: Vec<&Value> = matching
                .iter()
                .skip(*offset)
                .take(quantity as usize)
                .copied()
                .collect();
            *offset += assigned.len();
            let named_count = assigned
                .iter()
                .filter(|quest| signal_is_named_reward(quest, &line.item_name))
                .count();
            let whole_line_safely_named = assigned.len() == quantity as usize
                && !assigned.is_empty()
                && named_count == assigned.len();
            let unit_value = line.value_ped * (1.0 / quantity as f64);
            for quest in assigned {
                let is_named = signal_is_named_reward(quest, &line.item_name);
                if is_named && !whole_line_safely_named {
                    continue;
                }
                assignments.push((
                    quest.clone(),
                    RewardItemEvidence {
                        item_name: line.item_name.trim().to_string(),
                        quantity: 1,
                        value_ped: unit_value,
                    },
                ));
            }
        }
        for (quest, reward_item) in assignments {
            let quest_id = quest["id"].as_i64().expect("quest id");
            let quest_name = quest["name"].as_str().expect("quest name");
            let tick_item = json!({
                "item_name": reward_item.item_name,
                "quantity": reward_item.quantity,
                "value": reward_item.value_ped.value(),
            });
            let decision = reward_decision(&quest, &[tick_item], &[], false);
            self.complete_quest_with_reward_capture(quest_id, decision.capture())
                .await?;
            self.record_notable_event(NotableEventKind::Completed, quest_name, Ped::ZERO)
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
        self.quest_reward_filter_with_context(mission_name, loot_items, skill_gains, true)
            .await
    }

    pub async fn quest_reward_filter_with_context(
        &self,
        mission_name: &str,
        loot_items: &[Value],
        skill_gains: &[Value],
        isolated_completion_tick: bool,
    ) -> Result<Option<Value>, QuestError> {
        let Some(quest) = self.match_quest_by_mission_name(mission_name, true).await? else {
            return Ok(None);
        };
        let decision = reward_decision(&quest, loot_items, skill_gains, isolated_completion_tick);
        Ok(decision.suppression_json())
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
            let decision = reward_decision(
                &quest,
                &completion.loot_items,
                &completion.skill_gains,
                completion.isolated,
            );
            self.complete_quest_with_reward_capture(quest_id, decision.capture())
                .await?;

            let reward_ped = quest.get("reward_ped").and_then(Value::as_f64).map(Ped);
            let is_skill = json_truthy(quest.get("reward_is_skill"));
            let mut description = quest["name"].as_str().expect("quest name").to_string();
            if let Some(suppressed) = decision.description {
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

fn signal_candidate_order(a: &Value, b: &Value) -> std::cmp::Ordering {
    a.get("started_at")
        .and_then(Value::as_f64)
        .partial_cmp(&b.get("started_at").and_then(Value::as_f64))
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| a["id"].as_i64().cmp(&b["id"].as_i64()))
}

fn signal_is_named_reward(quest: &Value, item_name: &str) -> bool {
    quest.get("reward_policy").and_then(Value::as_str) == Some("named_items")
        && quest
            .get("reward_item_names")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .any(|reward| reward.trim().eq_ignore_ascii_case(item_name.trim()))
}

fn reward_item_evidence(item: &Value) -> Option<RewardItemEvidence> {
    let item_name = item.get("item_name")?.as_str()?.trim();
    if item_name.is_empty() {
        return None;
    }
    Some(RewardItemEvidence {
        item_name: item_name.to_string(),
        quantity: item
            .get("quantity")
            .and_then(Value::as_i64)
            .unwrap_or(1)
            .max(1),
        value_ped: Ped(item
            .get("value")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            .max(0.0)),
    })
}

#[derive(Debug, Clone)]
struct RewardDecision {
    outcome: &'static str,
    policy: String,
    loot_indices: Vec<usize>,
    skill_indices: Vec<usize>,
    items: Vec<RewardItemEvidence>,
    reason: Option<String>,
    description: Option<String>,
    evidence_json: String,
}

impl RewardDecision {
    fn suppression_json(&self) -> Option<Value> {
        (!self.loot_indices.is_empty() || !self.skill_indices.is_empty()).then(|| {
            json!({
                "suppress_loot_indices": self.loot_indices,
                "suppress_skill_indices": self.skill_indices,
            })
        })
    }

    fn capture(&self) -> RewardCapture {
        RewardCapture {
            outcome: self.outcome,
            policy_snapshot: self.policy.clone(),
            items: self.items.clone(),
            unresolved_reason: self.reason.clone(),
            evidence_json: Some(self.evidence_json.clone()),
            had_tracked_loot: !self.loot_indices.is_empty(),
        }
    }
}

/// One deterministic decision shared by pre-publish suppression and
/// post-publish immutable completion persistence.
fn reward_decision(
    quest: &Value,
    loot_items: &[Value],
    skill_gains: &[Value],
    isolated_completion_tick: bool,
) -> RewardDecision {
    let policy = quest
        .get("reward_policy")
        .and_then(Value::as_str)
        .unwrap_or("none")
        .to_string();
    let evidence_json = json!({
        "loot": loot_items,
        "skills": skill_gains,
        "isolated": isolated_completion_tick,
    })
    .to_string();
    let mut decision = RewardDecision {
        outcome: if policy == "none" {
            "none"
        } else {
            "confirmed"
        },
        policy: policy.clone(),
        loot_indices: Vec::new(),
        skill_indices: Vec::new(),
        items: Vec::new(),
        reason: None,
        description: None,
        evidence_json,
    };
    match policy.as_str() {
        "none" | "fixed_ped" => {}
        "fixed_pes" => {
            if !skill_gains.is_empty() {
                decision.skill_indices.push(0);
                decision.description = Some("skill reward separated".to_string());
            }
        }
        "named_items" => {
            let expected: Vec<String> = quest
                .get("reward_item_names")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|name| name.trim().to_ascii_lowercase())
                .filter(|name| !name.is_empty())
                .collect();
            for (index, item) in loot_items.iter().enumerate() {
                let name = item
                    .get("item_name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase();
                if expected.iter().any(|expected| expected == &name) {
                    decision.loot_indices.push(index);
                    if let Some(item) = reward_item_evidence(item) {
                        decision.items.push(item);
                    }
                }
            }
            let found_all = expected.iter().all(|expected| {
                decision
                    .items
                    .iter()
                    .any(|item| item.item_name.trim().eq_ignore_ascii_case(expected))
            });
            if expected.is_empty() || !found_all {
                decision.outcome = "unresolved";
                decision.loot_indices.clear();
                decision.items.clear();
                decision.reason =
                    Some("One or more expected reward items were missing".to_string());
            } else {
                decision.description = Some(format!(
                    "{} reward item line(s) separated",
                    decision.items.len()
                ));
            }
        }
        "completion_clump" => {
            if isolated_completion_tick && !loot_items.is_empty() {
                decision.loot_indices = (0..loot_items.len()).collect();
                decision.items = loot_items.iter().filter_map(reward_item_evidence).collect();
                decision.description = Some(format!(
                    "{} completion reward line(s) separated",
                    decision.items.len()
                ));
            } else {
                decision.outcome = "unresolved";
                decision.reason = Some(if loot_items.is_empty() {
                    "The completion carried no reward loot".to_string()
                } else {
                    "The completion tick also carried activity evidence".to_string()
                });
            }
        }
        _ => {
            decision.outcome = "unresolved";
            decision.reason = Some("The stored reward policy is unsupported".to_string());
        }
    }
    decision
}
