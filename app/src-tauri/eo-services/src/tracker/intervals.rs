//! The session-segment engine: intervals and the contexts that stamp
//! events with them.
//!
//! A session is not uniform. A pill holds for part of it, a quest
//! spans a stretch, and a player-drawn lap will slice one run. All
//! three are the same primitive, so they share this engine and differ
//! only by [`IntervalKind`].
//!
//! Two responsibilities, deliberately split (see `0019_session_intervals`):
//! an interval is authoritative for duration and cost, and a context is
//! authoritative for attribution. Opening or closing any interval mints
//! a fresh context; every event written afterwards stamps that context.
//! Attribution therefore never compares a wall-clock declaration against
//! a chat-log event timestamp, which is the one thing that would be
//! silently wrong here (the two clocks are an hour apart).

use crate::db::{Db, DbError};

/// What an interval records. An open vocabulary rather than a closed
/// enum at the schema boundary: adding a kind is a product decision, and
/// must not need a migration or a schema change on the event tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntervalKind {
    /// A skill-affecting modifier in force (a pill, a ring). Carries a
    /// magnitude, where zero means "declared, nothing in force".
    Modifier,
    /// Reserved for player-drawn slices of the session (one run, one
    /// lap); nothing writes it yet, and the engine needs no change
    /// when something does.
    Lap,
    /// The stretch a declared quest or playlist spans.
    Quest,
    /// Reserved for a later consumable-timer kind; nothing writes it
    /// yet, and the engine needs no change when something does.
    Consumable,
}

impl IntervalKind {
    pub fn as_str(self) -> &'static str {
        match self {
            IntervalKind::Modifier => "modifier",
            IntervalKind::Lap => "lap",
            IntervalKind::Quest => "quest",
            IntervalKind::Consumable => "consumable",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "modifier" => IntervalKind::Modifier,
            "lap" => IntervalKind::Lap,
            "quest" => IntervalKind::Quest,
            "consumable" => IntervalKind::Consumable,
            _ => return None,
        })
    }
}

/// What to open, as one value rather than a run of positional
/// arguments whose order would be easy to transpose at a call site.
#[derive(Debug, Clone, PartialEq)]
pub struct IntervalSpec {
    pub kind: IntervalKind,
    pub label: Option<String>,
    pub ref_id: Option<i64>,
    pub magnitude: Option<f64>,
    /// Close any already-open interval of the same kind first. True for
    /// the kinds that admit one at a time (a modifier declaration
    /// replaces the standing one rather than silently stacking).
    pub exclusive: bool,
}

impl IntervalSpec {
    pub fn new(kind: IntervalKind) -> Self {
        Self {
            kind,
            label: None,
            ref_id: None,
            magnitude: None,
            exclusive: true,
        }
    }

    pub fn label(mut self, label: Option<String>) -> Self {
        self.label = label;
        self
    }

    pub fn ref_id(mut self, ref_id: Option<i64>) -> Self {
        self.ref_id = ref_id;
        self
    }

    pub fn magnitude(mut self, magnitude: Option<f64>) -> Self {
        self.magnitude = magnitude;
        self
    }

    pub fn stacking(mut self) -> Self {
        self.exclusive = false;
        self
    }
}

/// One open interval, as the live session holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenInterval {
    pub id: i64,
    pub kind: IntervalKind,
    pub label: Option<String>,
    pub ref_id: Option<i64>,
    pub magnitude: Option<f64>,
}

/// The session's live segment state: which intervals are open, and the
/// context that identifies that set for stamping.
///
/// Held inside `ActiveSession`, so it is dropped wholesale when the
/// session stops and no segment state can leak into the next one.
#[derive(Debug, Default)]
pub struct SegmentState {
    open: Vec<OpenInterval>,
    context_id: Option<i64>,
}

impl SegmentState {
    /// The context every event written right now stamps. None only
    /// before the session's opening context has been minted.
    pub fn context_id(&self) -> Option<i64> {
        self.context_id
    }

    /// The open interval of a kind that admits only one at a time.
    pub fn open_of_kind(&self, kind: IntervalKind) -> Option<&OpenInterval> {
        self.open.iter().find(|interval| interval.kind == kind)
    }

    /// The modifier magnitude in force, as the denormalised per-row
    /// mirror wants it. None when no modifier is declared at all, which
    /// is a different fact from a declared zero.
    pub fn modifier_magnitude(&self) -> Option<f64> {
        self.open_of_kind(IntervalKind::Modifier)
            .and_then(|interval| interval.magnitude)
    }
}

/// Mint a context for the given open set and make it current. Every
/// mutation routes through here, so a context and the set it names are
/// written together or not at all.
async fn mint_context(
    db: &Db,
    session_id: &str,
    now: f64,
    open: &[OpenInterval],
) -> Result<i64, DbError> {
    let session = session_id.to_string();
    let members: Vec<i64> = open.iter().map(|interval| interval.id).collect();
    db.with_writer(move |conn| {
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO session_contexts (session_id, created_at) VALUES (?, ?)",
            rusqlite::params![session, now],
        )?;
        let context_id = tx.last_insert_rowid();
        for interval_id in &members {
            tx.execute(
                "INSERT INTO session_context_intervals (context_id, interval_id) VALUES (?, ?)",
                rusqlite::params![context_id, interval_id],
            )?;
        }
        tx.commit()?;
        Ok(context_id)
    })
    .await
}

impl SegmentState {
    /// Mint the session's opening context: the empty set. An event
    /// stamped with it was recorded under the segment model with nothing
    /// declared, which the record must be able to tell apart from an
    /// event that predates the model (those carry no context at all).
    pub async fn open_session(
        &mut self,
        db: &Db,
        session_id: &str,
        now: f64,
    ) -> Result<(), DbError> {
        self.open.clear();
        self.context_id = Some(mint_context(db, session_id, now, &[]).await?);
        Ok(())
    }

    /// Open an interval and adopt the context that includes it.
    ///
    /// `exclusive` kinds close any already-open interval of the same
    /// kind first, in the same motion: declaring a new modifier replaces
    /// the standing one rather than stacking a second silently.
    pub async fn open_interval(
        &mut self,
        db: &Db,
        session_id: &str,
        now: f64,
        spec: IntervalSpec,
    ) -> Result<i64, DbError> {
        let IntervalSpec {
            kind,
            label,
            ref_id,
            magnitude,
            exclusive,
        } = spec;
        if exclusive {
            self.close_kind_rows(db, now, kind).await?;
        }
        let session = session_id.to_string();
        let insert_label = label.clone();
        let kind_str = kind.as_str();
        let interval_id = db
            .with_writer(move |conn| {
                conn.execute(
                    "INSERT INTO session_intervals \
                     (session_id, kind, label, ref_id, magnitude, started_at) \
                     VALUES (?, ?, ?, ?, ?, ?)",
                    rusqlite::params![session, kind_str, insert_label, ref_id, magnitude, now],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await?;
        self.open.push(OpenInterval {
            id: interval_id,
            kind,
            label,
            ref_id,
            magnitude,
        });
        self.context_id = Some(mint_context(db, session_id, now, &self.open).await?);
        Ok(interval_id)
    }

    /// Close every open interval of a kind and adopt the narrower
    /// context. Returns what was closed.
    pub async fn close_kind(
        &mut self,
        db: &Db,
        session_id: &str,
        now: f64,
        kind: IntervalKind,
    ) -> Result<Vec<OpenInterval>, DbError> {
        let closed = self.close_kind_rows(db, now, kind).await?;
        if !closed.is_empty() {
            self.context_id = Some(mint_context(db, session_id, now, &self.open).await?);
        }
        Ok(closed)
    }

    /// Stamp the end time on a kind's open rows and drop them from the
    /// open set, WITHOUT minting a context. Private because a caller
    /// that stops here would leave events stamping a context naming an
    /// interval that has already ended.
    async fn close_kind_rows(
        &mut self,
        db: &Db,
        now: f64,
        kind: IntervalKind,
    ) -> Result<Vec<OpenInterval>, DbError> {
        let (closing, keeping): (Vec<_>, Vec<_>) = std::mem::take(&mut self.open)
            .into_iter()
            .partition(|interval| interval.kind == kind);
        self.open = keeping;
        if closing.is_empty() {
            return Ok(closing);
        }
        let ids: Vec<i64> = closing.iter().map(|interval| interval.id).collect();
        db.with_writer(move |conn| {
            let tx = conn.transaction()?;
            for id in &ids {
                tx.execute(
                    "UPDATE session_intervals SET ended_at = ? WHERE id = ? AND ended_at IS NULL",
                    rusqlite::params![now, id],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await?;
        Ok(closing)
    }

    /// Close everything still open at the session's end, so no interval
    /// outlives the session that owns it.
    pub async fn close_session(&mut self, db: &Db, now: f64) -> Result<(), DbError> {
        let ids: Vec<i64> = self.open.iter().map(|interval| interval.id).collect();
        self.open.clear();
        self.context_id = None;
        if ids.is_empty() {
            return Ok(());
        }
        db.with_writer(move |conn| {
            let tx = conn.transaction()?;
            for id in &ids {
                tx.execute(
                    "UPDATE session_intervals SET ended_at = ? WHERE id = ? AND ended_at IS NULL",
                    rusqlite::params![now, id],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_kind_round_trips_its_wire_string() {
        for kind in [
            IntervalKind::Modifier,
            IntervalKind::Lap,
            IntervalKind::Quest,
            IntervalKind::Consumable,
        ] {
            assert_eq!(IntervalKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(IntervalKind::parse("nonsense"), None);
    }

    /// A declared zero is a real modifier magnitude: it records "I am
    /// deliberately unboosted", which is the baseline an effect can be
    /// measured against. Only an absent interval means "not declared".
    #[test]
    fn a_declared_zero_magnitude_is_not_an_absent_modifier() {
        let mut state = SegmentState::default();
        assert_eq!(state.modifier_magnitude(), None);

        state.open.push(OpenInterval {
            id: 1,
            kind: IntervalKind::Modifier,
            label: None,
            ref_id: None,
            magnitude: Some(0.0),
        });
        assert_eq!(state.modifier_magnitude(), Some(0.0));
    }
}
