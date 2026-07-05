//! Mob/tag selection: the manual-mob and free-text-tag commands and
//! the current/confirmed mob state transitions they share with the
//! session lifecycle.

use crate::mob_lookup_service::python_whitespace;

use super::{HuntTracker, TrackerCommandError, TrackerState};

impl HuntTracker {
    /// Immediately set the active free-text tag for tag-mode kill
    /// stamping.
    pub fn set_manual_tag(&self, tag: &str) -> Result<(), TrackerCommandError> {
        let mut state = self.lock_state();
        if state.session.is_none() {
            return Err(TrackerCommandError::NoActiveSession);
        }
        if state.session_mob_tracking_mode != "tag" {
            return Err(TrackerCommandError::NotTagMode);
        }
        let cleaned = tag.trim_matches(python_whitespace);
        if cleaned.is_empty() {
            return Err(TrackerCommandError::EmptyTag);
        }
        state.session_mob_tracking_tag = cleaned.to_string();
        Self::set_session_tag(&mut state, cleaned);
        Ok(())
    }

    /// Immediately set the active mob for manual kill stamping.
    pub fn set_manual_mob(
        &self,
        mob_name: &str,
        species: &str,
        maturity: &str,
    ) -> Result<(), TrackerCommandError> {
        let mut state = self.lock_state();
        if state.session.is_none() {
            return Err(TrackerCommandError::NoActiveSession);
        }
        if state.session_mob_tracking_mode == "tag" {
            return Err(TrackerCommandError::TagModeLocksMob);
        }
        if !(self.providers.manual_mob_entry_enabled)() {
            return Err(TrackerCommandError::ManualEntryDisabled);
        }
        Self::set_manual_mob_state(&mut state, mob_name, species, maturity);
        Ok(())
    }

    /// Clear the current/confirmed mob state, returning the released
    /// name.
    pub fn release_current_mob(&self) -> Option<String> {
        let mut state = self.lock_state();
        let released = if !state.confirmed_mob_name.is_empty() {
            Some(state.confirmed_mob_name.clone())
        } else if !state.current_mob_name.is_empty() {
            Some(state.current_mob_name.clone())
        } else {
            None
        };
        Self::clear_mob_state(&mut state);
        released
    }
    pub(super) fn clear_mob_state(state: &mut TrackerState) {
        state.current_mob_name.clear();
        state.current_mob_species.clear();
        state.current_mob_maturity.clear();
        state.confirmed_mob_name.clear();
        state.confirmed_mob_species.clear();
        state.confirmed_mob_maturity.clear();
        state.mob_source = None;
    }

    pub(super) fn set_session_tag(state: &mut TrackerState, tag: &str) {
        state.current_mob_name = tag.to_string();
        state.current_mob_species.clear();
        state.current_mob_maturity.clear();
        state.confirmed_mob_name = tag.to_string();
        state.confirmed_mob_species.clear();
        state.confirmed_mob_maturity.clear();
        state.mob_source = Some("tag");
    }

    pub(super) fn set_manual_mob_state(
        state: &mut TrackerState,
        name: &str,
        species: &str,
        maturity: &str,
    ) {
        state.current_mob_name = name.to_string();
        state.current_mob_species = species.to_string();
        state.current_mob_maturity = maturity.to_string();
        state.confirmed_mob_name = name.to_string();
        state.confirmed_mob_species = species.to_string();
        state.confirmed_mob_maturity = maturity.to_string();
        state.mob_source = Some("manual");
    }
}
