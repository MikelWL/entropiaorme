//! Tree Cutting's durable source-activity vocabulary.
//!
//! A yield tier describes the board class a recorded swing made
//! available. It is deliberately not a claim that the app observed the
//! physical tree: the tool caps what can be extracted, so the yielded
//! board is the economically distinguishable result.

use serde::Serialize;

/// The effective yield tier evidenced by a harvesting swing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HarvestYieldTier {
    Short,
    Long,
    Huge,
    #[default]
    Unknown,
}

impl HarvestYieldTier {
    /// The stable database and wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Short => "short",
            Self::Long => "long",
            Self::Huge => "huge",
            Self::Unknown => "unknown",
        }
    }

    /// The user-facing label, for text the player reads.
    ///
    /// Deliberately not `as_str`: the stored spelling predates the board
    /// framing and does not read across one-for-one (see the mapping table on
    /// `yield_tier_for_board`). The frontend keeps its own copy of this
    /// mapping for rendering; the two must agree, and this side is pinned by
    /// `labels_match_the_board_vocabulary`.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Short => "Short Boards",
            Self::Long => "Boards",
            Self::Huge => "Long Boards",
            Self::Unknown => "Unclassified",
        }
    }

    /// Parse a database value. Schema constraints keep production rows
    /// inside this vocabulary; unknown input degrades conservatively.
    pub fn from_db(value: &str) -> Self {
        match value {
            "short" => Self::Short,
            "long" => Self::Long,
            "huge" => Self::Huge,
            _ => Self::Unknown,
        }
    }

    /// Stable presentation order for tier-first analytics.
    pub const fn sort_rank(self) -> u8 {
        match self {
            Self::Short => 0,
            Self::Long => 1,
            Self::Huge => 2,
            Self::Unknown => 3,
        }
    }
}

/// How a durable yield tier was established.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarvestYieldSource {
    Board,
    Inferred,
}

impl HarvestYieldSource {
    /// The stable database spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Board => "board",
            Self::Inferred => "inferred",
        }
    }
}

/// Classify the board name carried by a harvesting loot group.
///
/// Wood Shavings and non-board items provide no tier evidence.
///
/// The stored tier vocabulary predates the board framing and does not read
/// across to the labels one-for-one, so the mapping is spelled out here and
/// nowhere else has to infer it:
///
/// | board name           | stored tier | label       |
/// |----------------------|-------------|-------------|
/// | `Short ... Board`    | `short`     | Short Boards|
/// | `... Board`          | `long`      | Boards      |
/// | `Long ... Board`     | `huge`      | Long Boards |
///
/// The stored names are retained because migration 0013 has been applied to
/// live databases; renaming them is a later forward migration, not an edit.
pub fn yield_tier_for_board(name: &str) -> Option<HarvestYieldTier> {
    if !name.ends_with(" Board") {
        return None;
    }
    if name.starts_with("Short ") {
        Some(HarvestYieldTier::Short)
    } else if name.starts_with("Long ") {
        Some(HarvestYieldTier::Huge)
    } else {
        Some(HarvestYieldTier::Long)
    }
}

/// Classify a loot group from its board evidence.
///
/// A normal harvest group carries at most one board class. Conflicting
/// board classes are treated as unsupported evidence rather than
/// choosing one arbitrarily.
pub fn yield_tier_for_names<'a>(
    names: impl IntoIterator<Item = &'a str>,
) -> Option<HarvestYieldTier> {
    let mut tier = None;
    for name in names {
        let Some(candidate) = yield_tier_for_board(name) else {
            continue;
        };
        match tier {
            None => tier = Some(candidate),
            Some(existing) if existing == candidate => {}
            Some(_) => return None,
        }
    }
    tier
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_names_map_to_effective_yield_tiers() {
        assert_eq!(
            yield_tier_for_board("Short Moonleaf Board"),
            Some(HarvestYieldTier::Short)
        );
        assert_eq!(
            yield_tier_for_board("Moonleaf Board"),
            Some(HarvestYieldTier::Long)
        );
        assert_eq!(
            yield_tier_for_board("Long Moonleaf Board"),
            Some(HarvestYieldTier::Huge)
        );
        assert_eq!(
            yield_tier_for_board("Longleaf Board"),
            Some(HarvestYieldTier::Long)
        );
        assert_eq!(yield_tier_for_board("Wood Shavings"), None);
    }

    #[test]
    fn group_classification_refuses_conflicting_board_evidence() {
        assert_eq!(
            yield_tier_for_names(["Wood Shavings", "Long Moonleaf Board"]),
            Some(HarvestYieldTier::Huge)
        );
        assert_eq!(
            yield_tier_for_names(["Moonleaf Board", "Long Moonleaf Board"]),
            None
        );
    }

    #[test]
    fn labels_match_the_board_vocabulary() {
        // The stored spelling crosses over the labels, so this pins the
        // mapping rather than leaving it to be re-derived at each call site.
        assert_eq!(HarvestYieldTier::Short.label(), "Short Boards");
        assert_eq!(HarvestYieldTier::Long.label(), "Boards");
        assert_eq!(HarvestYieldTier::Huge.label(), "Long Boards");
        assert_eq!(HarvestYieldTier::Unknown.label(), "Unclassified");
        // And the board name that produces each tier round-trips to its label.
        assert_eq!(
            yield_tier_for_board("Short Moonleaf Board")
                .unwrap()
                .label(),
            "Short Boards"
        );
        assert_eq!(
            yield_tier_for_board("Moonleaf Board").unwrap().label(),
            "Boards"
        );
        assert_eq!(
            yield_tier_for_board("Long Moonleaf Board").unwrap().label(),
            "Long Boards"
        );
    }
}
