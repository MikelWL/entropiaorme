//! The wire contracts and the frozen-evidence emitters for the
//! EntropiaOrme backend.
//!
//! Two groups live here. The **live contracts**: [`domain_events`] (the
//! typed frontend-facing event union, gated against the committed
//! event-schema snapshot), [`bus`] (the monomorphic domain-event channel
//! with its drop-behind delivery shaping), and [`metrics`] (the
//! in-process metrics snapshot shapes).
//!
//! The **frozen-evidence emitters**: [`normalizer`] (the shared
//! canonicaliser), [`fingerprint`] (the event-stream JSONL),
//! [`db_snapshot`] (the DB-state snapshot), and [`http_fingerprint`] (the
//! response goldens). Hermetic tests assert them against the committed
//! goldens on every run; the goldens are the banked equivalence evidence
//! and pin the codebase's own ratified contract (ADR-0017).
//!
//! One live production policy also lives in [`normalizer`]: the
//! [`normalizer::round_half_even`] rounding and the Python-format JSON
//! writers are consumed by services at runtime (cost figures, timestamp
//! strings, settings and session-summary persistence), not only by the
//! emitters.

pub mod bus;
pub mod db_snapshot;
pub mod domain_events;
pub mod fingerprint;
pub mod http_fingerprint;
pub mod metrics;
pub mod normalizer;

/// Identifies this crate in diagnostics and smoke checks.
pub fn crate_name() -> &'static str {
    "eo-wire"
}

#[cfg(test)]
mod tests {
    #[test]
    fn crate_name_is_stable() {
        assert_eq!(super::crate_name(), "eo-wire");
    }
}
