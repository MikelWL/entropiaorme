//! Mob/tag selection vocabulary and commands: the session-capture
//! input mode, the source a kill stamp came from, and the selection
//! state machine the manual-mob and free-text-tag commands drive.

use crate::mob_lookup_service::python_whitespace;

use super::actor::TrackerActor;
use super::TrackerCommandError;

/// The input mode a session is captured under, snapshotted at session
/// start from the live config. The configured value is free text at
/// rest; anything other than `tag` behaves as mob mode everywhere, so
/// parsing normalises to the two real modes at the capture boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackingMode {
    Mob,
    Tag,
}

impl TrackingMode {
    /// Parse the configured mode string (the config field is free
    /// text; only `"tag"` selects tag mode, as every behaviour branch
    /// has always keyed).
    pub fn from_config(raw: &str) -> Self {
        if raw == "tag" {
            TrackingMode::Tag
        } else {
            TrackingMode::Mob
        }
    }

    /// The wire/database string.
    pub fn as_str(self) -> &'static str {
        match self {
            TrackingMode::Mob => "mob",
            TrackingMode::Tag => "tag",
        }
    }
}

/// Where the current mob stamp came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MobSource {
    Tag,
    Manual,
}

impl MobSource {
    /// The wire string the readout carries.
    pub fn as_str(self) -> &'static str {
        match self {
            MobSource::Tag => "tag",
            MobSource::Manual => "manual",
        }
    }
}

/// The mob/tag selection stamped onto kills: unset, a free-text tag
/// (tag-mode sessions), or a manually configured mob. The variant IS
/// the source, so a stamped name without a source (or vice versa) is
/// unrepresentable.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum MobSelection {
    #[default]
    Unset,
    Tag(String),
    Manual {
        /// The display name ("<maturity> <species>", or the bare
        /// species when no maturity is set).
        name: String,
        species: String,
        maturity: String,
    },
}

impl MobSelection {
    /// The display name a kill stamps, when set.
    pub fn name(&self) -> Option<&str> {
        match self {
            MobSelection::Unset => None,
            MobSelection::Tag(tag) => Some(tag),
            MobSelection::Manual { name, .. } => Some(name),
        }
    }

    /// The species/maturity pair a kill stamps (empty outside manual
    /// mode, exactly as the tag and unset stamps behaved).
    pub(super) fn species_maturity(&self) -> (&str, &str) {
        match self {
            MobSelection::Manual {
                species, maturity, ..
            } => (species, maturity),
            _ => ("", ""),
        }
    }

    pub(super) fn source(&self) -> Option<MobSource> {
        match self {
            MobSelection::Unset => None,
            MobSelection::Tag(_) => Some(MobSource::Tag),
            MobSelection::Manual { .. } => Some(MobSource::Manual),
        }
    }

    /// Build the manual selection from a species/maturity pair,
    /// deriving the display name the way the session-start and
    /// reload paths always have.
    pub(super) fn manual_from_parts(species: String, maturity: String) -> Self {
        let name = if maturity.is_empty() {
            species.clone()
        } else {
            format!("{maturity} {species}")
        };
        MobSelection::Manual {
            name,
            species,
            maturity,
        }
    }
}

impl TrackerActor {
    /// Immediately set the active free-text tag for tag-mode kill
    /// stamping.
    pub(super) fn set_manual_tag(&mut self, tag: &str) -> Result<(), TrackerCommandError> {
        let Some(active) = self.session.active_mut() else {
            return Err(TrackerCommandError::NoActiveSession);
        };
        if active.mode != TrackingMode::Tag {
            return Err(TrackerCommandError::NotTagMode);
        }
        let cleaned = tag.trim_matches(python_whitespace);
        if cleaned.is_empty() {
            return Err(TrackerCommandError::EmptyTag);
        }
        active.tag = cleaned.to_string();
        active.mob = MobSelection::Tag(cleaned.to_string());
        Ok(())
    }

    /// Immediately set the active mob for manual kill stamping.
    pub(super) fn set_manual_mob(
        &mut self,
        mob_name: &str,
        species: &str,
        maturity: &str,
    ) -> Result<(), TrackerCommandError> {
        let Self {
            session, providers, ..
        } = self;
        let Some(active) = session.active_mut() else {
            return Err(TrackerCommandError::NoActiveSession);
        };
        if active.mode == TrackingMode::Tag {
            return Err(TrackerCommandError::TagModeLocksMob);
        }
        // The provider may read the database or config; the actor
        // simply runs it inline.
        if !providers.config.manual_mob_entry_enabled() {
            return Err(TrackerCommandError::ManualEntryDisabled);
        }
        active.mob = MobSelection::Manual {
            name: mob_name.to_string(),
            species: species.to_string(),
            maturity: maturity.to_string(),
        };
        Ok(())
    }

    /// Clear the current mob selection, returning the released name.
    /// Idle is a no-op (idle carries no selection to release).
    pub(super) fn release_current_mob(&mut self) -> Option<String> {
        let active = self.session.active_mut()?;
        let released = active.mob.name().map(str::to_string);
        active.mob = MobSelection::Unset;
        released
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mob_source_wire_strings() {
        assert_eq!(MobSource::Tag.as_str(), "tag");
        assert_eq!(MobSource::Manual.as_str(), "manual");
    }
}
