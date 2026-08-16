//! Protection setup and measurement accounting.
//!
//! Armour and plates are independent economic layers. Named loadouts
//! compose them for live selection, while limited layers reconcile two
//! confirmed Trade Terminal observations. The current allocation policy
//! books only an unambiguous one-session window; every broader case is
//! retained as pending instead of guessed.

use std::sync::Arc;

use rusqlite::OptionalExtension;

use crate::clock::Clock;
use crate::db::{Db, DbError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectionSetKind {
    Armour,
    Plates,
}

impl ProtectionSetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Armour => "armour",
            Self::Plates => "plates",
        }
    }

    fn parse(value: &str) -> Result<Self, ProtectionError> {
        match value {
            "armour" => Ok(Self::Armour),
            "plates" => Ok(Self::Plates),
            _ => Err(ProtectionError::Stored("unknown protection set kind")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectionEconomyKind {
    Limited,
    Unlimited,
}

impl ProtectionEconomyKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Limited => "limited",
            Self::Unlimited => "unlimited",
        }
    }

    fn parse(value: &str) -> Result<Self, ProtectionError> {
        match value {
            "limited" => Ok(Self::Limited),
            "unlimited" => Ok(Self::Unlimited),
            _ => Err(ProtectionError::Stored("unknown protection economy kind")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationSource {
    Ocr,
    Manual,
}

impl ObservationSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ocr => "ocr",
            Self::Manual => "manual",
        }
    }

    fn parse(value: &str) -> Result<Self, ProtectionError> {
        match value {
            "ocr" => Ok(Self::Ocr),
            "manual" => Ok(Self::Manual),
            _ => Err(ProtectionError::Stored("unknown observation source")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconciliationStatus {
    Booked,
    Pending,
}

impl ReconciliationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Booked => "booked",
            Self::Pending => "pending",
        }
    }

    fn parse(value: &str) -> Result<Self, ProtectionError> {
        match value {
            "booked" => Ok(Self::Booked),
            "pending" => Ok(Self::Pending),
            _ => Err(ProtectionError::Stored("unknown reconciliation status")),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProtectionSet {
    pub id: i64,
    pub kind: ProtectionSetKind,
    pub name: String,
    pub economy_kind: ProtectionEconomyKind,
    pub markup_percent: Option<f64>,
    pub created_at: f64,
    pub archived_at: Option<f64>,
    pub latest_observation: Option<ProtectionObservation>,
    pub pending_reconciliations: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProtectionLoadout {
    pub id: i64,
    pub name: String,
    pub armour: Option<ProtectionSetRef>,
    pub plates: Option<ProtectionSetRef>,
    pub archived_at: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProtectionSetRef {
    pub id: i64,
    pub name: String,
    pub economy_kind: ProtectionEconomyKind,
    pub markup_percent: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProtectionObservation {
    pub id: i64,
    pub set_id: i64,
    pub tt_value_ped: f64,
    pub source: ObservationSource,
    pub raw_text: Option<String>,
    pub observed_at: f64,
    pub reset_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProtectionReconciliation {
    pub id: i64,
    pub set_id: i64,
    pub opening_observation_id: i64,
    pub closing_observation_id: i64,
    pub consumed_tt_ped: f64,
    pub markup_percent: f64,
    pub cost_ped: f64,
    pub status: ReconciliationStatus,
    pub session_id: Option<String>,
    pub reason: Option<String>,
    pub created_at: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObservationOutcome {
    pub observation: ProtectionObservation,
    pub reconciliation: Option<ProtectionReconciliation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProtectionOverview {
    pub sets: Vec<ProtectionSet>,
    pub loadouts: Vec<ProtectionLoadout>,
    pub active_loadout_id: Option<i64>,
    pub recent_reconciliations: Vec<ProtectionReconciliation>,
}

/// An immutable snapshot placed on a live protection interval.
#[derive(Debug, Clone, PartialEq)]
pub struct ProtectionSelection {
    pub loadout_id: i64,
    pub loadout_name: String,
    pub armour: Option<ProtectionSetRef>,
    pub plates: Option<ProtectionSetRef>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProtectionError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("{0}")]
    Invalid(&'static str),
    #[error("{0}")]
    NotFound(&'static str),
    #[error("{0}")]
    Conflict(&'static str),
    #[error("stored protection data is invalid: {0}")]
    Stored(&'static str),
}

impl From<rusqlite::Error> for ProtectionError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Db(DbError::Sqlite(error))
    }
}

pub struct ProtectionService {
    db: Db,
    clock: Arc<dyn Clock>,
}

/// Resolve the persisted default for session-start stamping. The tracker
/// uses this read directly so configuration and the opening interval share
/// the same database truth without a second service owner.
pub async fn active_selection(db: &Db) -> Result<Option<ProtectionSelection>, DbError> {
    db.with_reader(|conn| {
        let id: Option<i64> = conn
            .query_row(
                "SELECT active_loadout_id FROM protection_state WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        id.map(|id| read_selection(conn, id).map_err(protection_decode))
            .transpose()
    })
    .await
}

impl ProtectionService {
    pub fn new(db: Db, clock: Arc<dyn Clock>) -> Self {
        Self { db, clock }
    }

    fn now(&self) -> f64 {
        self.clock.now().and_utc().timestamp_micros() as f64 / 1_000_000.0
    }

    pub async fn overview(&self) -> Result<ProtectionOverview, ProtectionError> {
        self.db
            .with_reader(|conn| read_overview(conn))
            .await
            .map_err(ProtectionError::from)
    }

    pub async fn create_set(
        &self,
        kind: ProtectionSetKind,
        name: &str,
        economy_kind: ProtectionEconomyKind,
        markup_percent: Option<f64>,
    ) -> Result<ProtectionSet, ProtectionError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ProtectionError::Invalid("Set name is required"));
        }
        let markup = match economy_kind {
            ProtectionEconomyKind::Limited => match markup_percent {
                Some(value) if value.is_finite() && value >= 100.0 => Some(value),
                _ => {
                    return Err(ProtectionError::Invalid(
                        "Limited sets require an average markup of at least 100%",
                    ))
                }
            },
            ProtectionEconomyKind::Unlimited => None,
        };
        let name = name.to_string();
        let kind_text = kind.as_str();
        let economy_text = economy_kind.as_str();
        let now = self.now();
        let id = self
            .db
            .with_writer(move |conn| {
                conn.execute(
                    "INSERT INTO protection_sets \
                     (kind, name, economy_kind, markup_percent, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![kind_text, name, economy_text, markup, now],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await
            .map_err(map_constraint("An active set already uses that name"))?;
        self.set_by_id(id).await
    }

    pub async fn create_loadout(
        &self,
        name: &str,
        armour_set_id: Option<i64>,
        plate_set_id: Option<i64>,
    ) -> Result<ProtectionLoadout, ProtectionError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ProtectionError::Invalid("Loadout name is required"));
        }
        if armour_set_id.is_none()
            && plate_set_id.is_none()
            && !name.eq_ignore_ascii_case("No protection")
        {
            return Err(ProtectionError::Invalid(
                "An empty loadout must be named No protection",
            ));
        }
        self.validate_component(armour_set_id, ProtectionSetKind::Armour)
            .await?;
        self.validate_component(plate_set_id, ProtectionSetKind::Plates)
            .await?;
        let name = name.to_string();
        let now = self.now();
        let id = self
            .db
            .with_writer(move |conn| {
                conn.execute(
                    "INSERT INTO protection_loadouts \
                     (name, armour_set_id, plate_set_id, created_at) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![name, armour_set_id, plate_set_id, now],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await
            .map_err(map_constraint("An active loadout already uses that name"))?;
        self.loadout_by_id(id).await
    }

    pub async fn archive_set(&self, set_id: i64) -> Result<(), ProtectionError> {
        let now = self.now();
        let outcome = self
            .db
            .with_writer(move |conn| {
                let referenced: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM protection_loadouts \
                     WHERE archived_at IS NULL AND (armour_set_id = ?1 OR plate_set_id = ?1)",
                    [set_id],
                    |row| row.get(0),
                )?;
                if referenced > 0 {
                    return Ok(ArchiveOutcome::Referenced);
                }
                let changed = conn.execute(
                    "UPDATE protection_sets SET archived_at = ?1 \
                     WHERE id = ?2 AND archived_at IS NULL",
                    rusqlite::params![now, set_id],
                )?;
                Ok(if changed == 0 {
                    ArchiveOutcome::Missing
                } else {
                    ArchiveOutcome::Archived
                })
            })
            .await?;
        match outcome {
            ArchiveOutcome::Archived => Ok(()),
            ArchiveOutcome::Referenced => Err(ProtectionError::Conflict(
                "Archive loadouts using this set first",
            )),
            ArchiveOutcome::Missing => Err(ProtectionError::NotFound("Protection set not found")),
        }
    }

    pub async fn archive_loadout(&self, loadout_id: i64) -> Result<(), ProtectionError> {
        let now = self.now();
        let changed = self
            .db
            .with_writer(move |conn| {
                let tx = conn.transaction()?;
                let changed = tx.execute(
                    "UPDATE protection_loadouts SET archived_at = ?1 \
                     WHERE id = ?2 AND archived_at IS NULL",
                    rusqlite::params![now, loadout_id],
                )?;
                tx.execute(
                    "UPDATE protection_state SET active_loadout_id = NULL, updated_at = ?1 \
                     WHERE singleton = 1 AND active_loadout_id = ?2",
                    rusqlite::params![now, loadout_id],
                )?;
                tx.commit()?;
                Ok(changed)
            })
            .await?;
        if changed == 0 {
            return Err(ProtectionError::NotFound("Protection loadout not found"));
        }
        Ok(())
    }

    pub async fn persist_active_loadout(&self, loadout_id: i64) -> Result<(), ProtectionError> {
        self.loadout_by_id(loadout_id).await?;
        let now = self.now();
        self.db
            .with_writer(move |conn| {
                conn.execute(
                    "INSERT INTO protection_state(singleton, active_loadout_id, updated_at) \
                     VALUES (1, ?1, ?2) \
                     ON CONFLICT(singleton) DO UPDATE SET \
                     active_loadout_id = excluded.active_loadout_id, updated_at = excluded.updated_at",
                    rusqlite::params![loadout_id, now],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn active_selection(&self) -> Result<Option<ProtectionSelection>, ProtectionError> {
        self.db
            .with_reader(|conn| {
                let id: Option<i64> = conn
                    .query_row(
                        "SELECT active_loadout_id FROM protection_state WHERE singleton = 1",
                        [],
                        |row| row.get(0),
                    )
                    .optional()?
                    .flatten();
                id.map(|id| read_selection(conn, id).map_err(protection_decode))
                    .transpose()
            })
            .await
            .map_err(ProtectionError::from)
    }

    pub async fn selection(&self, loadout_id: i64) -> Result<ProtectionSelection, ProtectionError> {
        self.db
            .with_reader(move |conn| read_selection(conn, loadout_id).map_err(protection_decode))
            .await
            .map_err(ProtectionError::from)
    }

    pub async fn confirm_observation(
        &self,
        set_id: i64,
        client_token: &str,
        tt_value_ped: f64,
        source: ObservationSource,
        raw_text: Option<&str>,
        reset_reason: Option<&str>,
    ) -> Result<ObservationOutcome, ProtectionError> {
        if client_token.trim().is_empty() {
            return Err(ProtectionError::Invalid("Observation token is required"));
        }
        if !tt_value_ped.is_finite() || tt_value_ped < 0.0 {
            return Err(ProtectionError::Invalid("TT value must be zero or greater"));
        }
        let set = self.set_by_id(set_id).await?;
        if set.economy_kind != ProtectionEconomyKind::Limited {
            return Err(ProtectionError::Invalid(
                "TT observations are only used for limited protection",
            ));
        }
        let now = self.now();
        let token = client_token.trim().to_string();
        let source_text = source.as_str();
        let raw_text = raw_text
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string);
        let reset_reason = reset_reason
            .map(str::trim)
            .filter(|reason| !reason.is_empty())
            .map(str::to_string);
        let markup = set.markup_percent.expect("limited set carries markup");

        let outcome = self
            .db
            .with_writer(move |conn| {
                if let Some(existing_id) = conn
                    .query_row(
                        "SELECT id FROM protection_observations WHERE client_token = ?1",
                        [&token],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()?
                {
                    return read_observation_outcome(conn, existing_id)
                        .map(ObservationWriteOutcome::Saved);
                }

                let previous = conn
                    .query_row(
                        "SELECT id, tt_value_ped, observed_at FROM protection_observations \
                         WHERE set_id = ?1 ORDER BY observed_at DESC, id DESC LIMIT 1",
                        [set_id],
                        |row| {
                            Ok((
                                row.get::<_, i64>(0)?,
                                row.get::<_, f64>(1)?,
                                row.get::<_, f64>(2)?,
                            ))
                        },
                    )
                    .optional()?;

                if reset_reason.is_none()
                    && previous.is_some_and(|(_, value, _)| tt_value_ped > value + 0.000_000_1)
                {
                    return Ok(ObservationWriteOutcome::Increased);
                }

                let tx = conn.transaction()?;
                tx.execute(
                    "INSERT INTO protection_observations \
                     (set_id, client_token, tt_value_ped, source, raw_text, observed_at, reset_reason) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![
                        set_id,
                        token,
                        tt_value_ped,
                        source_text,
                        raw_text,
                        now,
                        reset_reason
                    ],
                )?;
                let observation_id = tx.last_insert_rowid();

                if reset_reason.is_none() {
                    if let Some((opening_id, opening_value, opening_at)) = previous {
                        let consumed_tt = (opening_value - tt_value_ped).max(0.0);
                        let cost = consumed_tt * markup / 100.0;
                        let (status, session_id, reason) =
                            resolve_single_session(&tx, set_id, opening_at, now)?;
                        tx.execute(
                            "INSERT INTO protection_reconciliations \
                             (set_id, opening_observation_id, closing_observation_id, \
                              consumed_tt_ped, markup_percent, cost_ped, status, session_id, \
                              reason, created_at) \
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                            rusqlite::params![
                                set_id,
                                opening_id,
                                observation_id,
                                consumed_tt,
                                markup,
                                cost,
                                status.as_str(),
                                session_id,
                                reason,
                                now
                            ],
                        )?;
                        if let Some(session_id) = &session_id {
                            let started_at: f64 = tx.query_row(
                                "SELECT started_at FROM tracking_sessions WHERE id = ?1",
                                [session_id],
                                |row| row.get(0),
                            )?;
                            tx.execute(
                                "UPDATE tracking_sessions \
                                 SET armour_cost = COALESCE(armour_cost, 0) + ?1 WHERE id = ?2",
                                rusqlite::params![cost, session_id],
                            )?;
                            crate::daily_rollup::refresh_days(
                                &tx,
                                [crate::daily_rollup::epoch_day(started_at)],
                            )?;
                            crate::session_summary::write_session_summary(&tx, session_id)?;
                        }
                    }
                }
                tx.commit()?;
                read_observation_outcome(conn, observation_id).map(ObservationWriteOutcome::Saved)
            })
            .await?;
        match outcome {
            ObservationWriteOutcome::Saved(outcome) => Ok(outcome),
            ObservationWriteOutcome::Increased => Err(ProtectionError::Conflict(
                "TT value increased; reset the baseline instead",
            )),
        }
    }

    async fn validate_component(
        &self,
        set_id: Option<i64>,
        expected: ProtectionSetKind,
    ) -> Result<(), ProtectionError> {
        let Some(set_id) = set_id else {
            return Ok(());
        };
        let set = self.set_by_id(set_id).await?;
        if set.archived_at.is_some() || set.kind != expected {
            return Err(ProtectionError::Invalid(
                "Loadout component has the wrong kind or is archived",
            ));
        }
        Ok(())
    }

    async fn set_by_id(&self, id: i64) -> Result<ProtectionSet, ProtectionError> {
        let row = self
            .db
            .with_reader(move |conn| read_set(conn, id).map_err(protection_decode))
            .await?;
        row.ok_or(ProtectionError::NotFound("Protection set not found"))
    }

    async fn loadout_by_id(&self, id: i64) -> Result<ProtectionLoadout, ProtectionError> {
        let row = self
            .db
            .with_reader(move |conn| read_loadout(conn, id).map_err(protection_decode))
            .await?;
        row.filter(|loadout| loadout.archived_at.is_none())
            .ok_or(ProtectionError::NotFound("Protection loadout not found"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveOutcome {
    Archived,
    Referenced,
    Missing,
}

enum ObservationWriteOutcome {
    Saved(ObservationOutcome),
    Increased,
}

fn map_constraint(message: &'static str) -> impl FnOnce(DbError) -> ProtectionError {
    move |error| match error {
        DbError::Sqlite(rusqlite::Error::SqliteFailure(_, _)) => ProtectionError::Conflict(message),
        other => ProtectionError::Db(other),
    }
}

fn read_set(
    conn: &rusqlite::Connection,
    id: i64,
) -> Result<Option<ProtectionSet>, ProtectionError> {
    let row = conn
        .query_row(
            "SELECT id, kind, name, economy_kind, markup_percent, created_at, archived_at \
             FROM protection_sets WHERE id = ?1",
            [id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<f64>>(4)?,
                    row.get::<_, f64>(5)?,
                    row.get::<_, Option<f64>>(6)?,
                ))
            },
        )
        .optional()?;
    let Some((id, kind, name, economy, markup, created_at, archived_at)) = row else {
        return Ok(None);
    };
    let latest_observation = read_latest_observation(conn, id)?;
    let pending_reconciliations = conn.query_row(
        "SELECT COUNT(*) FROM protection_reconciliations \
         WHERE set_id = ?1 AND status = 'pending'",
        [id],
        |row| row.get(0),
    )?;
    Ok(Some(ProtectionSet {
        id,
        kind: ProtectionSetKind::parse(&kind)?,
        name,
        economy_kind: ProtectionEconomyKind::parse(&economy)?,
        markup_percent: markup,
        created_at,
        archived_at,
        latest_observation,
        pending_reconciliations,
    }))
}

fn read_loadout(
    conn: &rusqlite::Connection,
    id: i64,
) -> Result<Option<ProtectionLoadout>, ProtectionError> {
    let row = conn
        .query_row(
            "SELECT id, name, armour_set_id, plate_set_id, archived_at \
             FROM protection_loadouts WHERE id = ?1",
            [id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, Option<f64>>(4)?,
                ))
            },
        )
        .optional()?;
    row.map(|(id, name, armour, plates, archived_at)| {
        Ok(ProtectionLoadout {
            id,
            name,
            armour: armour.map(|id| read_set_ref(conn, id)).transpose()?,
            plates: plates.map(|id| read_set_ref(conn, id)).transpose()?,
            archived_at,
        })
    })
    .transpose()
}

fn read_set_ref(conn: &rusqlite::Connection, id: i64) -> Result<ProtectionSetRef, ProtectionError> {
    let row = conn
        .query_row(
            "SELECT id, name, economy_kind, markup_percent FROM protection_sets WHERE id = ?1",
            [id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<f64>>(3)?,
                ))
            },
        )
        .optional()?;
    let (id, name, economy, markup) = row.ok_or(ProtectionError::Stored("missing loadout set"))?;
    Ok(ProtectionSetRef {
        id,
        name,
        economy_kind: ProtectionEconomyKind::parse(&economy)?,
        markup_percent: markup,
    })
}

fn read_selection(
    conn: &rusqlite::Connection,
    loadout_id: i64,
) -> Result<ProtectionSelection, ProtectionError> {
    let loadout = read_loadout(conn, loadout_id)?
        .filter(|loadout| loadout.archived_at.is_none())
        .ok_or(ProtectionError::NotFound("Protection loadout not found"))?;
    Ok(ProtectionSelection {
        loadout_id: loadout.id,
        loadout_name: loadout.name,
        armour: loadout.armour,
        plates: loadout.plates,
    })
}

fn read_observation(
    conn: &rusqlite::Connection,
    id: i64,
) -> Result<ProtectionObservation, ProtectionError> {
    let row = conn
        .query_row(
            "SELECT id, set_id, tt_value_ped, source, raw_text, observed_at, reset_reason \
             FROM protection_observations WHERE id = ?1",
            [id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, f64>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            },
        )
        .optional()?
        .ok_or(ProtectionError::Stored("missing protection observation"))?;
    Ok(ProtectionObservation {
        id: row.0,
        set_id: row.1,
        tt_value_ped: row.2,
        source: ObservationSource::parse(&row.3)?,
        raw_text: row.4,
        observed_at: row.5,
        reset_reason: row.6,
    })
}

fn read_latest_observation(
    conn: &rusqlite::Connection,
    set_id: i64,
) -> Result<Option<ProtectionObservation>, ProtectionError> {
    let id = conn
        .query_row(
            "SELECT id FROM protection_observations WHERE set_id = ?1 \
             ORDER BY observed_at DESC, id DESC LIMIT 1",
            [set_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    id.map(|id| read_observation(conn, id)).transpose()
}

fn read_reconciliation(
    conn: &rusqlite::Connection,
    id: i64,
) -> Result<ProtectionReconciliation, ProtectionError> {
    let row = conn
        .query_row(
            "SELECT id, set_id, opening_observation_id, closing_observation_id, \
                    consumed_tt_ped, markup_percent, cost_ped, status, session_id, reason, created_at \
             FROM protection_reconciliations WHERE id = ?1",
            [id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, f64>(4)?,
                    row.get::<_, f64>(5)?,
                    row.get::<_, f64>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, f64>(10)?,
                ))
            },
        )
        .optional()?
        .ok_or(ProtectionError::Stored("missing protection reconciliation"))?;
    Ok(ProtectionReconciliation {
        id: row.0,
        set_id: row.1,
        opening_observation_id: row.2,
        closing_observation_id: row.3,
        consumed_tt_ped: row.4,
        markup_percent: row.5,
        cost_ped: row.6,
        status: ReconciliationStatus::parse(&row.7)?,
        session_id: row.8,
        reason: row.9,
        created_at: row.10,
    })
}

fn read_observation_outcome(
    conn: &rusqlite::Connection,
    observation_id: i64,
) -> Result<ObservationOutcome, DbError> {
    let observation = read_observation(conn, observation_id).map_err(protection_decode)?;
    let reconciliation_id = conn
        .query_row(
            "SELECT id FROM protection_reconciliations WHERE closing_observation_id = ?1",
            [observation_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    let reconciliation = reconciliation_id
        .map(|id| read_reconciliation(conn, id).map_err(protection_decode))
        .transpose()?;
    Ok(ObservationOutcome {
        observation,
        reconciliation,
    })
}

fn protection_decode(error: ProtectionError) -> DbError {
    match error {
        ProtectionError::Db(error) => error,
        other => DbError::Decode {
            context: "stored protection record",
            source: serde_json::Error::io(std::io::Error::other(other.to_string())),
        },
    }
}

fn read_overview(conn: &rusqlite::Connection) -> Result<ProtectionOverview, DbError> {
    let mut set_stmt = conn.prepare(
        "SELECT id FROM protection_sets WHERE archived_at IS NULL ORDER BY kind, lower(name), id",
    )?;
    let set_ids = set_stmt
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let sets = set_ids
        .into_iter()
        .map(|id| {
            read_set(conn, id)
                .map_err(protection_decode)?
                .ok_or_else(|| protection_decode(ProtectionError::Stored("missing set")))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut loadout_stmt = conn.prepare(
        "SELECT id FROM protection_loadouts WHERE archived_at IS NULL ORDER BY created_at, id",
    )?;
    let loadout_ids = loadout_stmt
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let loadouts = loadout_ids
        .into_iter()
        .map(|id| {
            read_loadout(conn, id)
                .map_err(protection_decode)?
                .ok_or_else(|| protection_decode(ProtectionError::Stored("missing loadout")))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let active_loadout_id = conn
        .query_row(
            "SELECT active_loadout_id FROM protection_state WHERE singleton = 1",
            [],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()?
        .flatten();

    let mut rec_stmt = conn.prepare(
        "SELECT id FROM protection_reconciliations ORDER BY created_at DESC, id DESC LIMIT 12",
    )?;
    let rec_ids = rec_stmt
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let recent_reconciliations = rec_ids
        .into_iter()
        .map(|id| read_reconciliation(conn, id).map_err(protection_decode))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ProtectionOverview {
        sets,
        loadouts,
        active_loadout_id,
        recent_reconciliations,
    })
}

fn resolve_single_session(
    tx: &rusqlite::Transaction<'_>,
    set_id: i64,
    opening_at: f64,
    closing_at: f64,
) -> Result<(ReconciliationStatus, Option<String>, Option<String>), rusqlite::Error> {
    let mut stmt = tx.prepare(
        "SELECT id FROM tracking_sessions \
         WHERE started_at >= ?1 AND ended_at IS NOT NULL AND ended_at <= ?2 \
         ORDER BY started_at, id",
    )?;
    let sessions = stmt
        .query_map(rusqlite::params![opening_at, closing_at], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if sessions.len() != 1 {
        return Ok((
            ReconciliationStatus::Pending,
            None,
            Some(if sessions.is_empty() {
                "No complete recorded session falls inside this observation window".to_string()
            } else {
                "This observation window spans more than one recorded session".to_string()
            }),
        ));
    }
    let session_id = &sessions[0];
    let (distinct_armour, distinct_plates, armour_match, plates_match): (i64, i64, i64, i64) = tx
        .query_row(
        "SELECT \
                 COUNT(DISTINCT COALESCE(sp.armour_set_id, -1)), \
                 COUNT(DISTINCT COALESCE(sp.plate_set_id, -1)), \
                 MAX(CASE WHEN sp.armour_set_id = ?1 THEN 1 ELSE 0 END), \
                 MAX(CASE WHEN sp.plate_set_id = ?1 THEN 1 ELSE 0 END) \
             FROM session_intervals si \
             JOIN session_protection_intervals sp ON sp.interval_id = si.id \
             WHERE si.session_id = ?2 AND si.kind = 'protection'",
        rusqlite::params![set_id, session_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    let kind: String = tx.query_row(
        "SELECT kind FROM protection_sets WHERE id = ?1",
        [set_id],
        |row| row.get(0),
    )?;
    let unambiguous = match kind.as_str() {
        "armour" => distinct_armour == 1 && armour_match == 1,
        "plates" => distinct_plates == 1 && plates_match == 1,
        _ => false,
    };
    if !unambiguous {
        return Ok((
            ReconciliationStatus::Pending,
            None,
            Some(
                "The measured layer changed or was not declared throughout the session".to_string(),
            ),
        ));
    }
    Ok((ReconciliationStatus::Booked, Some(session_id.clone()), None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::MockClock;

    async fn harness() -> (tempfile::TempDir, Db, Arc<MockClock>, ProtectionService) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = Db::open(&dir.path().join("test.db"))
            .await
            .expect("open db");
        let clock = Arc::new(MockClock::new(None, 0.0));
        let service = ProtectionService::new(db.clone(), clock.clone());
        (dir, db, clock, service)
    }

    async fn limited_armour(
        service: &ProtectionService,
        name: &str,
        markup: f64,
    ) -> (ProtectionSet, ProtectionLoadout) {
        let set = service
            .create_set(
                ProtectionSetKind::Armour,
                name,
                ProtectionEconomyKind::Limited,
                Some(markup),
            )
            .await
            .expect("create set");
        let loadout = service
            .create_loadout(name, Some(set.id), None)
            .await
            .expect("create loadout");
        (set, loadout)
    }

    async fn seed_completed_session(
        db: &Db,
        session_id: &str,
        loadout: &ProtectionLoadout,
        set: &ProtectionSet,
        started_at: f64,
        ended_at: f64,
    ) {
        let session_id = session_id.to_string();
        let loadout_id = loadout.id;
        let loadout_name = loadout.name.clone();
        let set_id = set.id;
        let set_name = set.name.clone();
        let markup = set.markup_percent;
        db.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO tracking_sessions (id, started_at, ended_at, is_active) \
                 VALUES (?1, ?2, ?3, 0)",
                rusqlite::params![session_id, started_at, ended_at],
            )?;
            conn.execute(
                "INSERT INTO session_intervals \
                 (session_id, kind, label, ref_id, started_at, ended_at) \
                 VALUES (?1, 'protection', ?2, ?3, ?4, ?5)",
                rusqlite::params![session_id, loadout_name, loadout_id, started_at, ended_at],
            )?;
            let interval_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO session_protection_intervals \
                 (interval_id, loadout_id, loadout_name, armour_set_id, armour_set_name, \
                  armour_economy_kind, armour_markup_percent) \
                 VALUES (?1, ?2, ?3, ?4, ?5, 'limited', ?6)",
                rusqlite::params![
                    interval_id,
                    loadout_id,
                    loadout_name,
                    set_id,
                    set_name,
                    markup
                ],
            )?;
            Ok(())
        })
        .await
        .expect("seed session");
    }

    #[tokio::test]
    async fn a_single_completed_session_books_markup_adjusted_decay_once() {
        let (_dir, db, clock, service) = harness().await;
        let (set, loadout) = limited_armour(&service, "Adjusted Pixie", 125.0).await;
        let opening = service
            .confirm_observation(set.id, "open", 10.0, ObservationSource::Manual, None, None)
            .await
            .expect("opening observation");
        assert!(opening.reconciliation.is_none());
        let opening_at = opening.observation.observed_at;

        seed_completed_session(
            &db,
            "session-1",
            &loadout,
            &set,
            opening_at + 1.0,
            opening_at + 5.0,
        )
        .await;
        clock.advance(10.0).expect("advance");

        let closing = service
            .confirm_observation(set.id, "close", 8.0, ObservationSource::Manual, None, None)
            .await
            .expect("closing observation");
        let reconciliation = closing.reconciliation.expect("reconciliation");
        assert_eq!(reconciliation.status, ReconciliationStatus::Booked);
        assert_eq!(reconciliation.session_id.as_deref(), Some("session-1"));
        assert!((reconciliation.cost_ped - 2.5).abs() < 1e-9);

        let repeated = service
            .confirm_observation(set.id, "close", 7.0, ObservationSource::Manual, None, None)
            .await
            .expect("idempotent repeat");
        assert_eq!(repeated, closing);
        let booked: f64 = db
            .with_reader(|conn| {
                conn.query_row(
                    "SELECT armour_cost FROM tracking_sessions WHERE id = 'session-1'",
                    [],
                    |row| row.get(0),
                )
                .map_err(Into::into)
            })
            .await
            .expect("booked cost");
        assert!((booked - 2.5).abs() < 1e-9);
    }

    #[tokio::test]
    async fn ambiguous_windows_stay_pending_and_increases_require_a_reset() {
        let (_dir, db, clock, service) = harness().await;
        let (measured, _) = limited_armour(&service, "Measured armour", 110.0).await;
        let (other, other_loadout) = limited_armour(&service, "Other armour", 120.0).await;
        let opening = service
            .confirm_observation(
                measured.id,
                "pending-open",
                20.0,
                ObservationSource::Ocr,
                Some("20.00"),
                None,
            )
            .await
            .expect("opening observation");
        let opening_at = opening.observation.observed_at;
        seed_completed_session(
            &db,
            "session-other",
            &other_loadout,
            &other,
            opening_at + 1.0,
            opening_at + 4.0,
        )
        .await;
        clock.advance(5.0).expect("advance");

        let closing = service
            .confirm_observation(
                measured.id,
                "pending-close",
                19.0,
                ObservationSource::Manual,
                None,
                None,
            )
            .await
            .expect("pending observation");
        let reconciliation = closing.reconciliation.expect("reconciliation");
        assert_eq!(reconciliation.status, ReconciliationStatus::Pending);
        assert!(reconciliation.session_id.is_none());

        let increase = service
            .confirm_observation(
                measured.id,
                "increase",
                19.5,
                ObservationSource::Manual,
                None,
                None,
            )
            .await;
        assert!(matches!(increase, Err(ProtectionError::Conflict(_))));
        let reset = service
            .confirm_observation(
                measured.id,
                "reset",
                19.5,
                ObservationSource::Manual,
                None,
                Some("Replaced a piece"),
            )
            .await
            .expect("reset baseline");
        assert!(reset.reconciliation.is_none());
    }
}
