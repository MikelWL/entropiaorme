//! The session-definitions family: CRUD over the deliberate activity
//! families tracked sessions are instances of.
//!
//! A definition is authored data (name, opt-in ad-hoc segment flag,
//! ordered activity roster); tracked sessions reference one through a
//! nullable id stamped at session start, while the session's own name
//! facet keeps being stamped with the definition's name at selection
//! time. Selection itself is a tracking-family verb
//! (`tracking_definition_select`): this module is the management
//! surface. Like quest families, definitions publish no domain event;
//! the frontend refetches after its own mutations.

use eo_services::session_definitions::{
    RosterEntryInput, RosterEntryKind, SessionDefinitionError,
    SessionDefinitionInput as ServiceDefinitionInput,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Nullable;
use crate::{Api, ApiError};

// ── Request arguments ───────────────────────────────────────────────

/// A definition create or update payload, in the frontend's snake_case
/// field casing. One DTO serves both operations; the roster always
/// binds in full and replaces the stored roster wholesale on update
/// (the playlist-items precedent).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SessionDefinitionInput {
    pub name: String,
    #[serde(default)]
    pub ad_hoc_segments: bool,
    #[serde(default)]
    pub roster: Vec<SessionRosterEntryInput>,
}

/// One authored roster entry: what kind of activity it references and
/// the kind-appropriate payload (`ref_id` for the referencing kinds,
/// `label` for a plain segment).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SessionRosterEntryInput {
    pub kind: SessionRosterEntryKind,
    #[serde(default)]
    pub ref_id: Option<i64>,
    #[serde(default)]
    pub label: Option<String>,
}

/// What a roster entry references, in the stored snake_case
/// vocabulary: `quest_family` stands for whichever variant the family
/// serves today, `quest` is a single quest (a signal-completed boss or
/// a standalone mission-log quest outside any family), and `segment`
/// is a plain authored label with no reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionRosterEntryKind {
    QuestFamily,
    Quest,
    Segment,
}

impl SessionRosterEntryKind {
    fn to_service(self) -> RosterEntryKind {
        match self {
            SessionRosterEntryKind::QuestFamily => RosterEntryKind::QuestFamily,
            SessionRosterEntryKind::Quest => RosterEntryKind::Quest,
            SessionRosterEntryKind::Segment => RosterEntryKind::Segment,
        }
    }

    fn from_service(kind: RosterEntryKind) -> Self {
        match kind {
            RosterEntryKind::QuestFamily => SessionRosterEntryKind::QuestFamily,
            RosterEntryKind::Quest => SessionRosterEntryKind::Quest,
            RosterEntryKind::Segment => SessionRosterEntryKind::Segment,
        }
    }
}

impl SessionDefinitionInput {
    fn to_service(&self) -> ServiceDefinitionInput {
        ServiceDefinitionInput {
            name: self.name.clone(),
            ad_hoc_segments: self.ad_hoc_segments,
            roster: self
                .roster
                .iter()
                .map(|entry| RosterEntryInput {
                    kind: entry.kind.to_service(),
                    ref_id: entry.ref_id,
                    label: entry.label.clone(),
                })
                .collect(),
        }
    }
}

// ── Response models ─────────────────────────────────────────────────

/// A roster entry in the wire shape: the stored fact plus the resolved
/// display name of its target. Entry order is the roster order.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionRosterEntry {
    pub id: String,
    pub kind: SessionRosterEntryKind,
    /// The referenced target's id (stringified); null for a segment.
    pub ref_id: Nullable<String>,
    /// The authored segment label; null for the referencing kinds.
    pub label: Nullable<String>,
    /// The referenced target's current name (or the segment label);
    /// null when the target has since been deleted, so the authoring
    /// surface can show and repair the hole instead of hiding it.
    pub display_name: Nullable<String>,
}

/// A session definition in the wire shape: the authored fields plus
/// the derived instance count (how many tracked sessions reference it).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionDefinition {
    pub id: String,
    pub name: String,
    pub ad_hoc_segments: bool,
    pub instance_count: i64,
    /// Authored instant (fractional epoch seconds).
    pub created_at: f64,
    pub updated_at: Nullable<f64>,
    pub roster: Vec<SessionRosterEntry>,
}

impl SessionDefinition {
    pub(crate) fn from_service(
        definition: &eo_services::session_definitions::SessionDefinition,
    ) -> Self {
        Self {
            id: definition.id.to_string(),
            name: definition.name.clone(),
            ad_hoc_segments: definition.ad_hoc_segments,
            instance_count: definition.instance_count,
            created_at: definition.created_at,
            updated_at: definition.updated_at.into(),
            roster: definition
                .roster
                .iter()
                .map(|entry| SessionRosterEntry {
                    id: entry.id.to_string(),
                    kind: SessionRosterEntryKind::from_service(entry.kind),
                    ref_id: entry.ref_id.map(|id| id.to_string()).into(),
                    label: entry.label.clone().into(),
                    display_name: entry.display_name.clone().into(),
                })
                .collect(),
        }
    }
}

/// Map a service error to the reply: a validation rejection is the
/// caller's 400 with the message verbatim; a database failure is the
/// internal reply.
pub(crate) fn definition_error(
    context: &'static str,
) -> impl FnOnce(SessionDefinitionError) -> ApiError {
    move |error| match error {
        SessionDefinitionError::Invalid(message) => ApiError::bad_request(message),
        SessionDefinitionError::Db(_) => ApiError::internal(context)(error),
    }
}

// ── Facade methods ──────────────────────────────────────────────────

impl Api {
    /// List the active session definitions, oldest-authored first.
    pub async fn session_definitions_list(&self) -> Result<Vec<SessionDefinition>, ApiError> {
        let definitions = self
            .session_definitions
            .list(true)
            .await
            .map_err(definition_error("session definitions list"))?;
        Ok(definitions
            .iter()
            .map(SessionDefinition::from_service)
            .collect())
    }

    /// Create a session definition with its roster.
    pub async fn session_definition_create(
        &self,
        input: SessionDefinitionInput,
    ) -> Result<SessionDefinition, ApiError> {
        let created = self
            .session_definitions
            .create(input.to_service())
            .await
            .map_err(definition_error("session definition create"))?;
        Ok(SessionDefinition::from_service(&created))
    }

    /// Update a session definition; the roster is replaced wholesale.
    /// A missing (or soft-deleted) definition is a 404.
    pub async fn session_definition_update(
        &self,
        definition_id: i64,
        input: SessionDefinitionInput,
    ) -> Result<SessionDefinition, ApiError> {
        match self
            .session_definitions
            .update(definition_id, input.to_service())
            .await
            .map_err(definition_error("session definition update"))?
        {
            Some(updated) => Ok(SessionDefinition::from_service(&updated)),
            None => Err(ApiError::not_found("Session definition not found")),
        }
    }

    /// Soft-delete a session definition. Instances keep their stamped
    /// reference (the stamp is recorded history); the definition just
    /// stops being offered. A missing definition is a 404.
    pub async fn session_definition_delete(&self, definition_id: i64) -> Result<(), ApiError> {
        let deleted = self
            .session_definitions
            .delete(definition_id)
            .await
            .map_err(definition_error("session definition delete"))?;
        if !deleted {
            return Err(ApiError::not_found("Session definition not found"));
        }
        Ok(())
    }
}
