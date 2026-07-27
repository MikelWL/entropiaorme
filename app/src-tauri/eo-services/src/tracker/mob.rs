//! The declared-mob layer: the per-kill mob stamp's vocabulary and the
//! commands that drive the declaration. The declared mob is one of the
//! session's independent attributions (beside the session-name and
//! skill-boost facets, `session::SessionFacets`), changeable
//! mid-session, and it feeds the per-kill stamp until dynamic mob
//! detection can feed the same fields from evidence.

use super::actor::TrackerActor;
use super::TrackerCommandError;

/// Where a kill's mob stamp came from. `Declared` is the player's
/// declaration feeding the stamp; `Detected` is reserved for automatic
/// detection driving the same fields. A kill recorded with no
/// declaration in force carries no source (its stamp is "Unknown").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MobStampSource {
    Declared,
    Detected,
}

impl MobStampSource {
    /// The wire/database string.
    pub fn as_str(self) -> &'static str {
        match self {
            MobStampSource::Declared => "declared",
            MobStampSource::Detected => "detected",
        }
    }
}

/// The mob the session currently declares it is hunting: the stamp
/// source for kills recorded while it is in force. Display name and
/// catalogue identity always travel together, so a stamped name
/// without its species/maturity pair is unrepresentable.
#[derive(Debug, Clone, PartialEq)]
pub struct DeclaredMob {
    /// The display name ("<maturity> <species>", or the bare species
    /// when no maturity is set).
    pub name: String,
    pub species: String,
    pub maturity: String,
}

impl DeclaredMob {
    /// Build the declaration from a species/maturity pair, deriving
    /// the display name the way the session-start and reload paths
    /// always have.
    pub(super) fn from_parts(species: String, maturity: String) -> Self {
        let name = if maturity.is_empty() {
            species.clone()
        } else {
            format!("{maturity} {species}")
        };
        DeclaredMob {
            name,
            species,
            maturity,
        }
    }
}

impl TrackerActor {
    /// Immediately set the declared mob for kill stamping. Mid-session
    /// changes are allowed by design: the declaration is the current
    /// stamp source, not a session-frozen mode.
    pub(super) fn set_declared_mob(
        &mut self,
        mob_name: &str,
        species: &str,
        maturity: &str,
    ) -> Result<(), TrackerCommandError> {
        let Some(active) = self.session.active_mut() else {
            return Err(TrackerCommandError::NoActiveSession);
        };
        active.declared_mob = Some(DeclaredMob {
            name: mob_name.to_string(),
            species: species.to_string(),
            maturity: maturity.to_string(),
        });
        Ok(())
    }

    /// Clear the declared mob, returning the released name. Idle is a
    /// no-op (idle carries no declaration to release).
    pub(super) fn release_declared_mob(&mut self) -> Option<String> {
        let active = self.session.active_mut()?;
        let released = active
            .declared_mob
            .take()
            .map(|declared| declared.name)
            .filter(|name| !name.is_empty());
        released
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mob_stamp_source_wire_strings() {
        assert_eq!(MobStampSource::Declared.as_str(), "declared");
        assert_eq!(MobStampSource::Detected.as_str(), "detected");
    }

    #[test]
    fn declared_mob_display_name_derivation() {
        let bare = DeclaredMob::from_parts("Atrox".into(), String::new());
        assert_eq!(bare.name, "Atrox");
        let mature = DeclaredMob::from_parts("Atrox".into(), "Old".into());
        assert_eq!(mature.name, "Old Atrox");
    }
}
