//! The unified Activities control: what the overlay offers a player
//! mid-session, and the two verbs that move it.
//!
//! One control replaced the separate quest-focus picker and segment
//! field, so this module owns the whole of it: the roster-fed offerings
//! read, the exclusive-switch and co-activate verbs, and the promotion
//! of a segment named during play back into the session's roster.
//!
//! Three rules from the model shape every read here.
//!
//! **The roster filters offerings, never facts.** An authored roster
//! decides what is worth offering; it never decides what is true. A
//! mission the log carries surfaces its chip whether or not anyone
//! rostered it, because an in-progress mission is an administrative
//! fact, and so does a quest whose stretch is already standing.
//!
//! **The control appears only when it has something to offer**, and is
//! absent otherwise: not disabled, not an empty state. Configuration
//! alone produces no chrome (an empty roster with self-named segments
//! off is a deliberately simple session, and gets no activity surface);
//! facts always do. It is likewise absent while idle, because every
//! declaration here is "from now on" and there is no now to declare
//! into until a session runs.
//!
//! **A family stands for whichever variant the day serves.** A family
//! roster entry resolves to its in-progress member and acts on that
//! quest; with none in progress it says so, with the family's own
//! cooldown as the reason when one is running. This is why the ready
//! cue counts rows rather than quests: four rotating variants of one
//! daily are one thing you can go and do.

use eo_services::db::Db;
use eo_services::quests::{read_quest_offers, QuestOffer};
use eo_services::session_definitions::{RosterEntryKind, SessionDefinitionService};
use eo_services::tracker::{ActiveActivity, ActivityKey, ActivityRef, IntervalKind};
use eo_services::tracking_models::ActiveSessionView;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::session_definitions::{definition_error, SessionRosterEntryKind};
use crate::tracking::tracker_conflict;
use crate::{Api, ApiError, Nullable};

// ── Request arguments ───────────────────────────────────────────────

/// What an Activities verb acts on. Narrower than a roster entry's kind
/// on purpose: a family is a way of OFFERING a quest, so the control
/// resolves it to the variant in play and the verb only ever declares
/// the two things a session interval can record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActivityTargetKind {
    Quest,
    Segment,
}

// ── Response models ─────────────────────────────────────────────────

/// One standing activity, as the control renders its chip.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActiveActivityView {
    /// Row identity, shared with the offering it came from.
    pub key: String,
    pub kind: ActivityTargetKind,
    pub name: String,
    /// The quest whose stretch is standing; null for a segment, whose
    /// name is its only identity.
    pub quest_id: Nullable<i64>,
}

/// The acknowledgement both verbs echo: the standing set now in force,
/// in declaration order.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActivityStateResult {
    pub active: Vec<ActiveActivityView>,
}

/// The Activities control's strip-level readout, carried on every
/// tracking frame so the chip renders without a round trip of its own:
/// whether the control appears, the ready cue, and the standing set.
/// The menu's rows come from the fuller read below, computed the same
/// way, so the two cannot disagree.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySummary {
    pub visible: bool,
    pub ad_hoc_segments: bool,
    pub ready_count: i64,
    pub active: Vec<ActiveActivityView>,
}

impl From<&ActivityOptionsResult> for ActivitySummary {
    fn from(picture: &ActivityOptionsResult) -> Self {
        Self {
            visible: picture.visible,
            ad_hoc_segments: picture.ad_hoc_segments,
            ready_count: picture.ready_count,
            active: picture.active.clone(),
        }
    }
}

/// One row the control offers.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActivityOption {
    /// Stable identity across refreshes: the kind and what it points at.
    pub key: String,
    /// How the row was reached, in the roster's own vocabulary; a
    /// `quest_family` row acts on the variant it resolved to.
    pub kind: SessionRosterEntryKind,
    pub name: String,
    /// The quest a declaration acts on: the family's serving variant,
    /// the quest itself, or null for a segment row and for a family
    /// with nothing to serve.
    pub quest_id: Nullable<i64>,
    /// Whether a stretch of this row is standing on the running session.
    pub active: bool,
    /// Whether declaring it would do anything right now.
    pub available: bool,
    /// Why it would not, in the user's words; null when it would.
    pub unavailable_reason: Nullable<String>,
    /// When the gate lifts (fractional epoch seconds), so the control
    /// can count down; null when nothing gates the row.
    pub available_from: Nullable<f64>,
    /// A repeatable run rather than a mission-log quest: declaring it
    /// starts it, and its signal loot ends it.
    pub signal_quest: bool,
    /// Surfaced as a fact rather than offered by the roster.
    pub off_roster: bool,
}

/// What the Activities control shows.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActivityOptionsResult {
    /// Whether the control appears at all.
    pub visible: bool,
    /// Whether this session's definition opts into naming segments in
    /// play: the free-text row, and promotion into the roster.
    pub ad_hoc_segments: bool,
    /// How many rows could be declared right now, counting a family
    /// once and never counting a cooling or unreceived one: the ready
    /// cue, stated so it cannot promise more than the roster can do.
    pub ready_count: i64,
    pub options: Vec<ActivityOption>,
    pub active: Vec<ActiveActivityView>,
}

// ── The offerings computation ───────────────────────────────────────

/// The reasons a row cannot be declared, worded for the player. The
/// remaining time is deliberately not baked in: `available_from` rides
/// with the row so the control can count it down live.
const REASON_COOLDOWN: &str = "On cooldown";
const REASON_NOT_RECEIVED: &str = "Not in your mission log yet";
const REASON_NO_VARIANT: &str = "No variant received yet";

fn quest_key(quest_id: i64) -> String {
    format!("quest:{quest_id}")
}

fn segment_key(label: &str) -> String {
    format!("segment:{label}")
}

/// Whether a gate is still running at `now`.
fn cooling(available_from: Option<f64>, now: f64) -> bool {
    available_from.is_some_and(|lift| lift > now)
}

/// Whether a quest can be declared, and why not when it cannot.
///
/// An in-progress quest is always declarable: the log carries it (or a
/// signal run is open on it), and a cooldown gates STARTING the next
/// one, never playing toward the one in hand.
fn quest_availability(offer: &QuestOffer, now: f64) -> (bool, Option<&'static str>) {
    if offer.in_progress {
        return (true, None);
    }
    if cooling(offer.available_from, now) {
        return (false, Some(REASON_COOLDOWN));
    }
    // A signal quest's in-progress state IS the declaration, so an
    // uncooled one is always startable; a mission-log quest has to be
    // in the log first, and nothing here can put it there.
    if offer.signal_quest {
        (true, None)
    } else {
        (false, Some(REASON_NOT_RECEIVED))
    }
}

/// The variant a family serves right now: its in-progress member, the
/// earliest-authored one when several are (a family is one slot in
/// game, so more than one in progress is already unusual).
fn serving_member(offers: &[QuestOffer], family_id: i64) -> Option<&QuestOffer> {
    offers
        .iter()
        .find(|offer| offer.family_id == Some(family_id) && offer.in_progress)
}

/// When a family's gate lifts: the latest of its members' gates, since
/// completing any member cools the whole slot.
fn family_available_from(offers: &[QuestOffer], family_id: i64) -> Option<f64> {
    offers
        .iter()
        .filter(|offer| offer.family_id == Some(family_id))
        .filter_map(|offer| offer.available_from)
        .fold(None, |latest: Option<f64>, lift| {
            Some(latest.map_or(lift, |current| current.max(lift)))
        })
}

/// The standing set in wire shape.
fn active_views(standing: &[ActiveActivity]) -> Vec<ActiveActivityView> {
    standing
        .iter()
        .map(|activity| match activity.kind {
            IntervalKind::Quest => ActiveActivityView {
                key: quest_key(activity.quest_id.unwrap_or_default()),
                kind: ActivityTargetKind::Quest,
                name: activity.name.clone(),
                quest_id: activity.quest_id.into(),
            },
            _ => ActiveActivityView {
                key: segment_key(&activity.name),
                kind: ActivityTargetKind::Segment,
                name: activity.name.clone(),
                quest_id: None.into(),
            },
        })
        .collect()
}

/// The whole picture the Activities control renders from: whether it
/// appears, what it offers, and what is standing.
///
/// Shared with the tracking snapshot, which publishes the strip-level
/// subset of it on every frame: one computation, so the chip's cue and
/// the menu's rows can never disagree about what is available. A free
/// function rather than a facade method because the guide-mode demo
/// assembles the same snapshot over its own database and tracker, with
/// no definition service of its own (`definitions: None`, which reads
/// as a session outside any definition).
pub(crate) async fn activity_picture(
    db: &Db,
    definitions: Option<&SessionDefinitionService>,
    active: Option<&ActiveSessionView>,
    now: f64,
) -> Result<ActivityOptionsResult, ApiError> {
    let empty = ActivityOptionsResult {
        visible: false,
        ad_hoc_segments: false,
        ready_count: 0,
        options: Vec::new(),
        active: Vec::new(),
    };
    // Absent while idle: every declaration here is "from now on", and
    // there is no now to declare into without a session.
    let Some(active) = active else {
        return Ok(empty);
    };

    // The session's stamped definition, not the current selection: a
    // running session is an instance of what it started as, and the
    // roster it offers must be that one. A definition deleted
    // mid-session resolves to none, leaving only the facts.
    let definition = match (definitions, active.definition_id) {
        (Some(service), Some(id)) => service
            .get_active(id)
            .await
            .map_err(definition_error("activity options definition"))?,
        _ => None,
    };
    let ad_hoc_segments = definition
        .as_ref()
        .is_some_and(|definition| definition.ad_hoc_segments);

    let offers = read_quest_offers(db)
        .await
        .map_err(ApiError::internal("activity options offers"))?;
    let standing = active_views(&active.active_activities);
    let standing_quests: Vec<i64> = active
        .active_activities
        .iter()
        .filter_map(|activity| activity.quest_id)
        .collect();
    let is_standing_quest = |quest_id: i64| standing_quests.contains(&quest_id);
    let is_standing_segment = |label: &str| {
        active.active_activities.iter().any(|activity| {
            activity.kind == IntervalKind::Segment && activity.name.eq_ignore_ascii_case(label)
        })
    };

    // ── The roster's offerings, in the order they were authored ──
    let mut options: Vec<ActivityOption> = Vec::new();
    // Which quests the roster already speaks for, so a fact is not
    // repeated below as an off-roster row.
    let mut represented: Vec<i64> = Vec::new();

    for entry in definition
        .as_ref()
        .map(|definition| definition.roster.as_slice())
        .unwrap_or_default()
    {
        match entry.kind {
            RosterEntryKind::Segment => {
                let Some(label) = entry.label.as_deref() else {
                    continue;
                };
                options.push(ActivityOption {
                    key: segment_key(label),
                    kind: SessionRosterEntryKind::Segment,
                    name: label.to_string(),
                    quest_id: None.into(),
                    active: is_standing_segment(label),
                    // A segment needs only a running session; the
                    // player's own naming has nothing to wait for.
                    available: true,
                    unavailable_reason: None.into(),
                    available_from: None.into(),
                    signal_quest: false,
                    off_roster: false,
                });
            }
            RosterEntryKind::Quest => {
                // A reference whose target was deleted is a hole to
                // repair in the authoring surface, not a chip: it
                // reads as a missing display name there, and offers
                // nothing here.
                let Some(offer) = entry
                    .ref_id
                    .and_then(|id| offers.iter().find(|offer| offer.id == id))
                else {
                    continue;
                };
                let (available, reason) = quest_availability(offer, now);
                represented.push(offer.id);
                options.push(ActivityOption {
                    key: quest_key(offer.id),
                    kind: SessionRosterEntryKind::Quest,
                    name: offer.name.clone(),
                    quest_id: Some(offer.id).into(),
                    active: is_standing_quest(offer.id),
                    available,
                    unavailable_reason: reason.map(str::to_string).into(),
                    available_from: offer.available_from.into(),
                    signal_quest: offer.signal_quest,
                    off_roster: false,
                });
            }
            RosterEntryKind::QuestFamily => {
                let Some(family_id) = entry.ref_id else {
                    continue;
                };
                // The family's own name, as the roster read resolved
                // it; a deleted family leaves the hole visible in
                // the editor and offers nothing here.
                let Some(name) = entry.display_name.clone() else {
                    continue;
                };
                let serving = serving_member(&offers, family_id);
                if let Some(offer) = serving {
                    represented.push(offer.id);
                }
                let available_from = family_available_from(&offers, family_id);
                let (available, reason) = match serving {
                    Some(_) => (true, None),
                    None if cooling(available_from, now) => (false, Some(REASON_COOLDOWN)),
                    None => (false, Some(REASON_NO_VARIANT)),
                };
                options.push(ActivityOption {
                    key: format!("quest_family:{family_id}"),
                    kind: SessionRosterEntryKind::QuestFamily,
                    // The variant in play is what a tap records, so
                    // the row says which one it is; the family alone
                    // when there is nothing to serve.
                    name: match serving {
                        Some(offer) => offer.name.clone(),
                        None => name,
                    },
                    quest_id: serving.map(|offer| offer.id).into(),
                    active: serving.is_some_and(|offer| is_standing_quest(offer.id)),
                    available,
                    unavailable_reason: reason.map(str::to_string).into(),
                    available_from: available_from.into(),
                    signal_quest: serving.is_some_and(|offer| offer.signal_quest),
                    off_roster: false,
                });
            }
        }
    }
    let rostered = options.len();

    // ── The facts, whatever the roster says ──
    // An in-progress mission or a standing stretch is true of this
    // session; hiding it because nobody rostered it would make the
    // control lie about what is being recorded.
    for offer in &offers {
        if represented.contains(&offer.id) {
            continue;
        }
        if !offer.in_progress && !is_standing_quest(offer.id) {
            continue;
        }
        let (available, reason) = quest_availability(offer, now);
        options.push(ActivityOption {
            key: quest_key(offer.id),
            kind: SessionRosterEntryKind::Quest,
            name: offer.name.clone(),
            quest_id: Some(offer.id).into(),
            active: is_standing_quest(offer.id),
            available,
            unavailable_reason: reason.map(str::to_string).into(),
            available_from: offer.available_from.into(),
            signal_quest: offer.signal_quest,
            off_roster: true,
        });
    }
    // A segment named in play before its promotion landed (or on a
    // definition that never opted in) still shows while it runs.
    for activity in &active.active_activities {
        if activity.kind != IntervalKind::Segment {
            continue;
        }
        if options
            .iter()
            .any(|option| option.name.eq_ignore_ascii_case(&activity.name))
        {
            continue;
        }
        options.push(ActivityOption {
            key: segment_key(&activity.name),
            kind: SessionRosterEntryKind::Segment,
            name: activity.name.clone(),
            quest_id: None.into(),
            active: true,
            available: true,
            unavailable_reason: None.into(),
            available_from: None.into(),
            signal_quest: false,
            off_roster: true,
        });
    }

    let ready_count = options
        .iter()
        .filter(|option| option.available && !option.active)
        .count() as i64;
    Ok(ActivityOptionsResult {
        // Configuration produces no chrome; facts always do.
        visible: rostered > 0 || ad_hoc_segments || options.len() > rostered,
        ad_hoc_segments,
        ready_count,
        options,
        active: standing,
    })
}

// ── Facade methods ──────────────────────────────────────────────────

impl Api {
    /// What the Activities control offers right now.
    pub async fn tracking_activity_options(&self) -> Result<ActivityOptionsResult, ApiError> {
        let readout = self
            .tracker
            .snapshot()
            .await
            .map_err(ApiError::internal("activity options readout"))?;
        let now = eo_services::time::naive_to_epoch(self.clock.now());
        activity_picture(
            &self.db,
            Some(&self.session_definitions),
            readout.active.as_ref(),
            now,
        )
        .await
    }

    /// Declare an activity: the play from now on advances this quest, or
    /// belongs to this named slice of the session.
    ///
    /// The default is the one-tap switch, exclusive across both kinds:
    /// one control offers them, so a tap means "this is what I am doing
    /// now" and seals whatever was standing. `additive` co-activates
    /// instead, for the hunt that genuinely advances two quests at once.
    ///
    /// A signal quest starts in the same motion (its in-progress state
    /// IS the declaration); a mission-log quest the log does not carry
    /// is a 400, because play cannot be toward a mission you have not
    /// been given. A segment with a blank name is auto-numbered, and one
    /// named in play is promoted into the session's roster when the
    /// definition opts into self-named segments. 409 when no session is
    /// active.
    pub async fn tracking_activity_activate(
        &self,
        kind: ActivityTargetKind,
        quest_id: Option<i64>,
        label: Option<String>,
        additive: Option<bool>,
    ) -> Result<ActivityStateResult, ApiError> {
        let activity = match kind {
            ActivityTargetKind::Quest => {
                let Some(quest_id) = quest_id else {
                    return Err(ApiError::bad_request("A quest activity needs a quest_id"));
                };
                ActivityRef::Quest {
                    quest_id,
                    name: self.prepare_quest_activity(quest_id).await?,
                }
            }
            ActivityTargetKind::Segment => ActivityRef::Segment {
                name: label.as_deref().unwrap_or_default().trim().to_string(),
            },
        };
        let promoting = match &activity {
            ActivityRef::Segment { name } if !name.is_empty() => Some(name.clone()),
            _ => None,
        };

        let standing = self
            .tracker
            .activate_activity(activity, additive.unwrap_or(false))
            .await
            .map_err(tracker_conflict)?;

        // Promotion follows the declaration, never precedes it: a
        // roster entry for a segment that failed to open would offer a
        // chip for something that never happened.
        if let Some(name) = promoting {
            self.promote_named_segment(&name).await?;
        }
        Ok(ActivityStateResult {
            active: active_views(&standing),
        })
    }

    /// End one standing activity, leaving the others running.
    /// Idempotent over the standing set, so a stale control cannot fail
    /// the user. 409 when no session is active.
    pub async fn tracking_activity_deactivate(
        &self,
        kind: ActivityTargetKind,
        quest_id: Option<i64>,
        label: Option<String>,
    ) -> Result<ActivityStateResult, ApiError> {
        let target = match kind {
            ActivityTargetKind::Quest => {
                let Some(quest_id) = quest_id else {
                    return Err(ApiError::bad_request("A quest activity needs a quest_id"));
                };
                ActivityKey::Quest(quest_id)
            }
            ActivityTargetKind::Segment => {
                let Some(label) = label
                    .map(|label| label.trim().to_string())
                    .filter(|label| !label.is_empty())
                else {
                    return Err(ApiError::bad_request("A segment activity needs a label"));
                };
                ActivityKey::Segment(label)
            }
        };
        let standing = self
            .tracker
            .deactivate_activity(target)
            .await
            .map_err(tracker_conflict)?;
        Ok(ActivityStateResult {
            active: active_views(&standing),
        })
    }

    /// Resolve the quest a declaration acts on, starting a signal quest
    /// in the same motion, and answer with the name its stretch records
    /// under.
    async fn prepare_quest_activity(&self, quest_id: i64) -> Result<String, ApiError> {
        let Some(quest) = self
            .quests
            .get_quest(quest_id)
            .await
            .map_err(ApiError::internal("activity quest lookup"))?
        else {
            return Err(ApiError::not_found("Quest not found"));
        };
        if quest["started_at"].is_null() {
            // A signal quest has no mission-log entry to mirror, so its
            // in-progress state IS the user's declaration: declaring it
            // starts it (and its signal loot will complete it). A
            // mission-log quest still refuses, because play cannot be
            // toward a quest the log does not carry.
            if quest["signal_loot_item"].is_null() {
                return Err(ApiError::bad_request(
                    "Quest is not in progress; start it before playing toward it",
                ));
            }
            self.quests
                .start_quest(quest_id)
                .await
                .map_err(ApiError::internal("signal quest start"))?;
        }
        Ok(quest["name"].as_str().unwrap_or_default().to_string())
    }

    /// Append a segment named in play to the running session's roster,
    /// when that definition opts into self-named segments. Auto-numbered
    /// shapes are skipped: "Segment 3" names nothing worth offering
    /// again. Silent when the definition does not opt in, or when the
    /// name is already a chip.
    async fn promote_named_segment(&self, name: &str) -> Result<(), ApiError> {
        if crate::tracking::is_auto_numbered_segment(name) {
            return Ok(());
        }
        let readout = self
            .tracker
            .snapshot()
            .await
            .map_err(ApiError::internal("segment promotion readout"))?;
        let Some(definition_id) = readout
            .active
            .as_ref()
            .and_then(|active| active.definition_id)
        else {
            return Ok(());
        };
        let opted_in = self
            .session_definitions
            .get_active(definition_id)
            .await
            .map_err(definition_error("segment promotion definition"))?
            .is_some_and(|definition| definition.ad_hoc_segments);
        if !opted_in {
            return Ok(());
        }
        self.session_definitions
            .promote_segment(definition_id, name)
            .await
            .map_err(definition_error("segment promotion"))?;
        Ok(())
    }
}
