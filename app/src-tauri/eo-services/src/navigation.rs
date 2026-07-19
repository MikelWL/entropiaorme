//! Persisted map-route navigation and radar guidance.
//!
//! Pins are immutable route inputs. Operational progress belongs to a
//! navigation run, so reaching a tree never mutates the pin itself. All
//! state-changing operations serialise through `operation`, while the
//! synchronous database core remains behind its async reader/writer seam.

use std::sync::{Arc, Mutex};

use rusqlite::OptionalExtension;

use crate::clock::Clock;
use crate::coord_capture::{
    CoordBounds, CoordCaptureService, CoordConfirmListener, CoordRead, CoordScanOutcome,
};
use crate::db::{Db, DbError};
use crate::keystroke_source::{KeystrokeKind, KeystrokeSource};
use crate::time::naive_to_epoch;

pub const ARRIVAL_TOLERANCE_UNITS: f64 = 2.0;
pub const DUPLICATE_TOLERANCE_UNITS: f64 = 2.0;
/// A confirmed visit puts its tree on cooldown for this long, so a freshly
/// regenerated route excludes trees that were just harvested. Two hours is
/// the initial default; a configurable per-species respawn is deferred work.
pub const COOLDOWN_SECONDS: f64 = 2.0 * 60.0 * 60.0;
pub const DEFAULT_NAVIGATION_HOTKEY: &str = "f8";
pub const NAVIGATION_HOTKEYS: [&str; 7] = ["f6", "f7", "f8", "f9", "f10", "f11", "f12"];

pub type BoundsProvider = Arc<dyn Fn(&str) -> Option<CoordBounds> + Send + Sync>;
pub type ChangedSink = Arc<dyn Fn() + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Active,
    Paused,
    Completed,
    Ended,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Ended => "ended",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "paused" => Some(Self::Paused),
            "completed" => Some(Self::Completed),
            "ended" => Some(Self::Ended),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopStatus {
    Pending,
    Active,
    Visited,
    Skipped,
}

impl StopStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Visited => "visited",
            Self::Skipped => "skipped",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "active" => Some(Self::Active),
            "visited" => Some(Self::Visited),
            "skipped" => Some(Self::Skipped),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NavigationStop {
    pub id: i64,
    pub pin_id: i64,
    pub ordinal: i64,
    pub status: StopStatus,
    pub name: String,
    pub icon: String,
    pub lon: f64,
    pub lat: f64,
    pub completed_at: Option<f64>,
    pub completion_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NavigationRun {
    pub id: i64,
    pub planet: String,
    pub map_view_id: Option<i64>,
    pub map_view_name: Option<String>,
    pub status: RunStatus,
    pub start_lon: f64,
    pub start_lat: f64,
    pub current_lon: f64,
    pub current_lat: f64,
    pub last_position_at: Option<f64>,
    pub hop_count: i64,
    pub hotkey: String,
    pub updated_at: f64,
    pub stops: Vec<NavigationStop>,
}

impl NavigationRun {
    pub fn active_stop(&self) -> Option<&NavigationStop> {
        self.stops
            .iter()
            .find(|stop| stop.status == StopStatus::Active)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadarGeometry {
    pub centre_x: i64,
    pub centre_y: i64,
    pub north_x: i64,
    pub north_y: i64,
    pub radius_px: f64,
    pub display_scale: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadarCalibrationPhase {
    Idle,
    AwaitCentre,
    AwaitNorthEdge { centre: (i64, i64) },
}

#[derive(Debug, Clone, PartialEq)]
pub enum PositionUpdate {
    Updated(NavigationRun),
    NoActiveRun,
    Paused(NavigationRun),
    NoRegion,
    CaptureFailed,
    EngineUnavailable,
    Unreadable,
    Implausible,
    Ambiguous(NavigationRun),
    /// A manual `Visited` whose observed position is outside the arrival
    /// tolerance. The run position is updated so distance/bearing are fresh,
    /// but no visit is recorded until the user confirms a forced visit.
    OutOfTolerance(NavigationRun),
}

/// The read-failure legs of a coordinate scan, kept small so the scan helper
/// does not carry a run-sized `Err` type. Each maps to its `PositionUpdate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanFailure {
    NoRegion,
    CaptureFailed,
    EngineUnavailable,
    Unreadable,
    Implausible,
}

impl From<ScanFailure> for PositionUpdate {
    fn from(failure: ScanFailure) -> Self {
        match failure {
            ScanFailure::NoRegion => PositionUpdate::NoRegion,
            ScanFailure::CaptureFailed => PositionUpdate::CaptureFailed,
            ScanFailure::EngineUnavailable => PositionUpdate::EngineUnavailable,
            ScanFailure::Unreadable => PositionUpdate::Unreadable,
            ScanFailure::Implausible => PositionUpdate::Implausible,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NavigationError {
    #[error("no active navigation route")]
    NoActiveRun,
    #[error("no eligible pins are available for this route")]
    NoPins,
    #[error("route hop count must be between 1 and 500")]
    InvalidHopCount,
    #[error("navigation hotkey must be F6 through F12")]
    InvalidHotkey,
    #[error("radar calibration needs a radius of at least 8 pixels")]
    InvalidRadarRadius,
    #[error(transparent)]
    Db(#[from] DbError),
}

pub struct NavigationService {
    db: Db,
    clock: Arc<dyn Clock>,
    coord_capture: Arc<CoordCaptureService>,
    bounds: BoundsProvider,
    changed: ChangedSink,
    operation: tokio::sync::Mutex<()>,
    radar_phase: Mutex<RadarCalibrationPhase>,
    radar_confirm_listener: Mutex<Option<std::sync::Weak<CoordConfirmListener>>>,
    input: Option<Arc<dyn KeystrokeSource>>,
    input_claimed: std::sync::atomic::AtomicBool,
    route_live: std::sync::atomic::AtomicBool,
    hotkey: Mutex<String>,
}

impl NavigationService {
    pub async fn new(
        db: Db,
        clock: Arc<dyn Clock>,
        coord_capture: Arc<CoordCaptureService>,
        bounds: BoundsProvider,
        changed: ChangedSink,
        input: Option<Arc<dyn KeystrokeSource>>,
    ) -> Arc<Self> {
        // An interrupted route is not restored as a unit: recovery is by
        // regenerating a route that excludes cooled-down trees (the per-tree
        // visit records are the recovery point). Any run left live by a crash
        // or a hard close is ended at startup rather than hydrated.
        let now = naive_to_epoch(clock.now());
        let _ = end_lingering_runs(&db, now).await;
        let service = Arc::new(Self {
            db,
            clock,
            coord_capture,
            bounds,
            changed,
            operation: tokio::sync::Mutex::new(()),
            radar_phase: Mutex::new(RadarCalibrationPhase::Idle),
            radar_confirm_listener: Mutex::new(None),
            input: input.clone(),
            input_claimed: std::sync::atomic::AtomicBool::new(false),
            route_live: std::sync::atomic::AtomicBool::new(false),
            hotkey: Mutex::new(DEFAULT_NAVIGATION_HOTKEY.to_string()),
        });
        if let Some(input) = input {
            let weak = Arc::downgrade(&service);
            let runtime = tokio::runtime::Handle::current();
            input.subscribe(Arc::new(move |event| {
                let Some(service) = weak.upgrade() else {
                    return;
                };
                if event.kind != KeystrokeKind::Press {
                    return;
                }
                if event.key == *service.hotkey.lock().expect("navigation hotkey")
                    && service.route_live.load(std::sync::atomic::Ordering::SeqCst)
                {
                    let service = service.clone();
                    runtime.spawn(async move {
                        let _ = service.update_position().await;
                    });
                }
            }));
        }
        service
    }

    fn claim_input(&self) {
        use std::sync::atomic::Ordering;
        if self.input_claimed.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Some(input) = &self.input {
            let _ = input.start();
        }
    }

    fn release_input_if_idle(&self) {
        use std::sync::atomic::Ordering;
        if self.route_live.load(Ordering::SeqCst) {
            return;
        }
        if !self.input_claimed.swap(false, Ordering::SeqCst) {
            return;
        }
        if let Some(input) = &self.input {
            input.stop();
        }
    }

    pub async fn snapshot(&self) -> Result<Option<NavigationRun>, DbError> {
        load_current_run(&self.db).await
    }

    pub async fn start(
        &self,
        planet: String,
        map_view_id: Option<i64>,
        start_lon: f64,
        start_lat: f64,
        hop_count: i64,
        hotkey: String,
    ) -> Result<NavigationRun, NavigationError> {
        if !(1..=500).contains(&hop_count) {
            return Err(NavigationError::InvalidHopCount);
        }
        if !NAVIGATION_HOTKEYS.contains(&hotkey.as_str()) {
            return Err(NavigationError::InvalidHotkey);
        }
        let selected_hotkey = hotkey.clone();
        let _guard = self.operation.lock().await;
        let now = naive_to_epoch(self.clock.now());
        let candidates = load_candidates(&self.db, planet.clone(), map_view_id, now).await?;
        let route = optimise_open_route((start_lon, start_lat), &candidates, hop_count as usize);
        if route.is_empty() {
            return Err(NavigationError::NoPins);
        }
        let run_id = self.db.with_writer(move |conn| {
            let tx = conn.transaction()?;
            tx.execute(
                "UPDATE navigation_runs SET status = 'ended', updated_at = ?1 WHERE status IN ('active', 'paused', 'completed')",
                [now],
            )?;
            tx.execute(
                "INSERT INTO navigation_runs (planet, map_view_id, status, start_lon, start_lat, current_lon, current_lat, hop_count, hotkey, created_at, updated_at) VALUES (?1, ?2, 'active', ?3, ?4, ?3, ?4, ?5, ?6, ?7, ?7)",
                rusqlite::params![planet, map_view_id, start_lon, start_lat, route.len() as i64, hotkey, now],
            )?;
            let run_id = tx.last_insert_rowid();
            for (index, pin) in route.iter().enumerate() {
                tx.execute(
                    "INSERT INTO navigation_stops (run_id, pin_id, ordinal, status) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![run_id, pin.id, index as i64, if index == 0 { "active" } else { "pending" }],
                )?;
            }
            tx.commit()?;
            Ok(run_id)
        }).await?;
        self.route_live
            .store(true, std::sync::atomic::Ordering::SeqCst);
        *self.hotkey.lock().expect("navigation hotkey") = selected_hotkey;
        self.claim_input();
        let run = load_run(&self.db, run_id)
            .await?
            .ok_or(NavigationError::NoActiveRun)?;
        (self.changed)();
        Ok(run)
    }

    /// Observe the current position. Strictly captures the coordinate and
    /// refreshes the run's position so distance and bearing to the active
    /// tree recompute; it never records a visit or advances the route. Both
    /// the `Update` button and the configured hotkey ride this path.
    pub async fn update_position(&self) -> Result<PositionUpdate, NavigationError> {
        let _guard = self.operation.lock().await;
        let Some(run) = load_live_run(&self.db).await? else {
            return Ok(PositionUpdate::NoActiveRun);
        };
        let read = match self.scan_position(&run.planet) {
            Ok(read) => read,
            Err(failure) => return Ok(failure.into()),
        };
        let now = naive_to_epoch(self.clock.now());
        update_run_position(&self.db, run.id, read.lon as f64, read.lat as f64, now).await?;
        let refreshed = load_run(&self.db, run.id)
            .await?
            .ok_or(NavigationError::NoActiveRun)?;
        (self.changed)();
        Ok(PositionUpdate::Updated(refreshed))
    }

    /// Record the active tree as visited. When the observed position is
    /// within the arrival tolerance (or `force` is set after the caller
    /// confirms), the active stop is completed, a durable per-pin visit is
    /// written (starting its cooldown), and the route advances. Outside the
    /// tolerance without `force`, the position is refreshed but no visit is
    /// recorded, and `OutOfTolerance` asks the caller to confirm.
    pub async fn mark_visited(&self, force: bool) -> Result<PositionUpdate, NavigationError> {
        let _guard = self.operation.lock().await;
        let Some(run) = load_live_run(&self.db).await? else {
            return Ok(PositionUpdate::NoActiveRun);
        };
        let read = match self.scan_position(&run.planet) {
            Ok(read) => read,
            Err(failure) => return Ok(failure.into()),
        };
        let lon = read.lon as f64;
        let lat = read.lat as f64;
        let now = naive_to_epoch(self.clock.now());
        let Some(active) = run.active_stop() else {
            update_run_position(&self.db, run.id, lon, lat, now).await?;
            (self.changed)();
            return Ok(PositionUpdate::NoActiveRun);
        };
        let observed_distance = distance((lon, lat), (active.lon, active.lat));
        if observed_distance > ARRIVAL_TOLERANCE_UNITS && !force {
            update_run_position(&self.db, run.id, lon, lat, now).await?;
            let refreshed = load_run(&self.db, run.id)
                .await?
                .ok_or(NavigationError::NoActiveRun)?;
            (self.changed)();
            return Ok(PositionUpdate::OutOfTolerance(refreshed));
        }
        let run_id = run.id;
        let stop_id = active.id;
        let pin_id = active.pin_id;
        self.db.with_writer(move |conn| {
            let tx = conn.transaction()?;
            tx.execute(
                "UPDATE navigation_runs SET current_lon = ?2, current_lat = ?3, last_position_at = ?4, updated_at = ?4 WHERE id = ?1",
                rusqlite::params![run_id, lon, lat, now],
            )?;
            tx.execute(
                "UPDATE navigation_stops SET status = 'visited', completed_at = ?2, completion_source = 'manual', observed_lon = ?3, observed_lat = ?4, observed_distance = ?5 WHERE id = ?1",
                rusqlite::params![stop_id, now, lon, lat, observed_distance],
            )?;
            tx.execute(
                "INSERT INTO map_pin_visits (pin_id, run_id, visited_at, source, outcome, observed_lon, observed_lat, observed_distance) VALUES (?1, ?2, ?3, 'manual', 'manual', ?4, ?5, ?6)",
                rusqlite::params![pin_id, run_id, now, lon, lat, observed_distance],
            )?;
            activate_next_or_complete(&tx, run_id, now)?;
            tx.commit()?;
            Ok(())
        }).await?;
        let refreshed = load_run(&self.db, run_id)
            .await?
            .ok_or(NavigationError::NoActiveRun)?;
        if refreshed.status == RunStatus::Completed {
            self.route_live
                .store(false, std::sync::atomic::Ordering::SeqCst);
            self.release_input_if_idle();
        }
        (self.changed)();
        Ok(PositionUpdate::Updated(refreshed))
    }

    /// Scan the calibrated coordinate region, mapping every read failure to
    /// its typed `PositionUpdate` so the position paths share one grammar.
    fn scan_position(&self, planet: &str) -> Result<CoordRead, ScanFailure> {
        match self.coord_capture.scan((self.bounds)(planet)) {
            CoordScanOutcome::Read(read) => Ok(read),
            CoordScanOutcome::NoRegion => Err(ScanFailure::NoRegion),
            CoordScanOutcome::CaptureFailed => Err(ScanFailure::CaptureFailed),
            CoordScanOutcome::EngineUnavailable => Err(ScanFailure::EngineUnavailable),
            CoordScanOutcome::Unreadable { .. } => Err(ScanFailure::Unreadable),
            CoordScanOutcome::Implausible { .. } => Err(ScanFailure::Implausible),
        }
    }

    async fn apply_position(
        &self,
        run: NavigationRun,
        lon: f64,
        lat: f64,
        source: &str,
        outcome: &str,
    ) -> Result<PositionUpdate, NavigationError> {
        let now = naive_to_epoch(self.clock.now());
        if source == "harvest" {
            let run_id = run.id;
            let recent = self
                .db
                .with_reader(move |conn| {
                    Ok(conn
                        .query_row(
                            "SELECT visited_at, observed_lon, observed_lat FROM map_pin_visits WHERE run_id = ?1 AND source = 'harvest' ORDER BY visited_at DESC, id DESC LIMIT 1",
                            [run_id],
                            |row| Ok((row.get::<_, f64>(0)?, row.get::<_, f64>(1)?, row.get::<_, f64>(2)?)),
                        )
                        .optional()?)
                })
                .await?;
            if recent.is_some_and(|(visited_at, previous_lon, previous_lat)| {
                now - visited_at <= 30.0
                    && distance((lon, lat), (previous_lon, previous_lat)) <= ARRIVAL_TOLERANCE_UNITS
            }) {
                update_run_position(&self.db, run.id, lon, lat, now).await?;
                let refreshed = load_run(&self.db, run.id)
                    .await?
                    .ok_or(NavigationError::NoActiveRun)?;
                return Ok(PositionUpdate::Updated(refreshed));
            }
        }
        let matches: Vec<_> = run
            .stops
            .iter()
            .filter(|stop| matches!(stop.status, StopStatus::Active | StopStatus::Pending))
            .filter_map(|stop| {
                let distance = distance((lon, lat), (stop.lon, stop.lat));
                (distance <= ARRIVAL_TOLERANCE_UNITS).then_some((stop.id, stop.pin_id, distance))
            })
            .collect();
        if matches.len() > 1 {
            update_run_position(&self.db, run.id, lon, lat, now).await?;
            let refreshed = load_run(&self.db, run.id)
                .await?
                .ok_or(NavigationError::NoActiveRun)?;
            return Ok(PositionUpdate::Ambiguous(refreshed));
        }
        let source = source.to_string();
        let outcome = outcome.to_string();
        let matched = matches.first().copied();
        let active_stop_id = run.active_stop().map(|stop| stop.id);
        let arrived_out_of_order = matched
            .map(|(stop_id, _, _)| Some(stop_id) != active_stop_id)
            .unwrap_or(false);
        let run_id = run.id;
        self.db.with_writer(move |conn| {
            let tx = conn.transaction()?;
            tx.execute(
                "UPDATE navigation_runs SET current_lon = ?2, current_lat = ?3, last_position_at = ?4, updated_at = ?4 WHERE id = ?1",
                rusqlite::params![run_id, lon, lat, now],
            )?;
            if let Some((stop_id, pin_id, observed_distance)) = matched {
                tx.execute(
                    "UPDATE navigation_stops SET status = 'visited', completed_at = ?2, completion_source = ?3, observed_lon = ?4, observed_lat = ?5, observed_distance = ?6 WHERE id = ?1",
                    rusqlite::params![stop_id, now, source, lon, lat, observed_distance],
                )?;
                tx.execute(
                    "INSERT INTO map_pin_visits (pin_id, run_id, visited_at, source, outcome, observed_lon, observed_lat, observed_distance) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    rusqlite::params![pin_id, run_id, now, source, outcome, lon, lat, observed_distance],
                )?;
                activate_next_or_complete(&tx, run_id, now)?;
            }
            tx.commit()?;
            Ok(())
        }).await?;
        if arrived_out_of_order {
            return Ok(PositionUpdate::Updated(self.replan_locked(run_id).await?));
        }
        let refreshed = load_run(&self.db, run.id)
            .await?
            .ok_or(NavigationError::NoActiveRun)?;
        if refreshed.status == RunStatus::Completed {
            self.route_live
                .store(false, std::sync::atomic::Ordering::SeqCst);
            self.release_input_if_idle();
        }
        Ok(PositionUpdate::Updated(refreshed))
    }

    pub async fn skip(&self) -> Result<NavigationRun, NavigationError> {
        self.complete_active("skipped", "manual", "unavailable")
            .await
    }

    async fn complete_active(
        &self,
        status: &str,
        source: &str,
        _outcome: &str,
    ) -> Result<NavigationRun, NavigationError> {
        let _guard = self.operation.lock().await;
        let run = load_live_run(&self.db)
            .await?
            .ok_or(NavigationError::NoActiveRun)?;
        let active = run.active_stop().ok_or(NavigationError::NoActiveRun)?;
        let now = naive_to_epoch(self.clock.now());
        let run_id = run.id;
        let stop_id = active.id;
        let status = status.to_string();
        let source = source.to_string();
        self.db.with_writer(move |conn| {
            let tx = conn.transaction()?;
            tx.execute(
                "UPDATE navigation_stops SET status = ?2, completed_at = ?3, completion_source = ?4 WHERE id = ?1",
                rusqlite::params![stop_id, status, now, source],
            )?;
            activate_next_or_complete(&tx, run_id, now)?;
            tx.commit()?;
            Ok(())
        }).await?;
        let refreshed = load_run(&self.db, run_id)
            .await?
            .ok_or(NavigationError::NoActiveRun)?;
        if refreshed.status == RunStatus::Completed {
            self.route_live
                .store(false, std::sync::atomic::Ordering::SeqCst);
            self.release_input_if_idle();
        }
        (self.changed)();
        Ok(refreshed)
    }

    pub async fn undo(&self) -> Result<NavigationRun, NavigationError> {
        let _guard = self.operation.lock().await;
        let run = load_live_run_or_latest(&self.db)
            .await?
            .ok_or(NavigationError::NoActiveRun)?;
        let run_id = run.id;
        let now = naive_to_epoch(self.clock.now());
        self.db.with_writer(move |conn| {
            let tx = conn.transaction()?;
            let completed: Option<i64> = tx.query_row(
                "SELECT id FROM navigation_stops WHERE run_id = ?1 AND status IN ('visited', 'skipped') ORDER BY completed_at DESC, ordinal DESC LIMIT 1",
                [run_id], |row| row.get(0)).optional()?;
            let Some(stop_id) = completed else { return Ok(()) };
            tx.execute("UPDATE navigation_stops SET status = 'pending' WHERE run_id = ?1 AND status = 'active'", [run_id])?;
            tx.execute("UPDATE navigation_stops SET status = 'active', completed_at = NULL, completion_source = NULL, observed_lon = NULL, observed_lat = NULL, observed_distance = NULL WHERE id = ?1", [stop_id])?;
            tx.execute("DELETE FROM map_pin_visits WHERE id = (SELECT id FROM map_pin_visits WHERE run_id = ?1 ORDER BY visited_at DESC, id DESC LIMIT 1)", [run_id])?;
            tx.execute("UPDATE navigation_runs SET status = 'active', updated_at = ?2 WHERE id = ?1", rusqlite::params![run_id, now])?;
            tx.commit()?;
            Ok(())
        }).await?;
        self.route_live
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.claim_input();
        let refreshed = load_run(&self.db, run_id)
            .await?
            .ok_or(NavigationError::NoActiveRun)?;
        (self.changed)();
        Ok(refreshed)
    }

    async fn replan_locked(&self, run_id: i64) -> Result<NavigationRun, NavigationError> {
        let run = load_run(&self.db, run_id)
            .await?
            .ok_or(NavigationError::NoActiveRun)?;
        let remaining: Vec<Candidate> = run
            .stops
            .iter()
            .filter(|stop| matches!(stop.status, StopStatus::Active | StopStatus::Pending))
            .map(|stop| Candidate {
                id: stop.pin_id,
                lon: stop.lon,
                lat: stop.lat,
            })
            .collect();
        let route = optimise_open_route(
            (run.current_lon, run.current_lat),
            &remaining,
            remaining.len(),
        );
        let now = naive_to_epoch(self.clock.now());
        self.db
            .with_writer(move |conn| {
                let tx = conn.transaction()?;
                let completed_ids = {
                    let mut statement = tx.prepare(
                        "SELECT id FROM navigation_stops WHERE run_id = ?1 AND status IN ('visited', 'skipped') ORDER BY completed_at, id",
                    )?;
                    let rows = statement.query_map([run_id], |row| row.get::<_, i64>(0))?;
                    rows.collect::<Result<Vec<_>, _>>()?
                };
                tx.execute(
                    "UPDATE navigation_stops SET ordinal = ordinal + 10000 WHERE run_id = ?1",
                    [run_id],
                )?;
                for (index, stop_id) in completed_ids.iter().enumerate() {
                    tx.execute(
                        "UPDATE navigation_stops SET ordinal = ?2 WHERE id = ?1",
                        rusqlite::params![stop_id, index as i64],
                    )?;
                }
                for (index, candidate) in route.iter().enumerate() {
                    tx.execute(
                        "UPDATE navigation_stops SET ordinal = ?3, status = ?4 WHERE run_id = ?1 AND pin_id = ?2",
                        rusqlite::params![run_id, candidate.id, (completed_ids.len() + index) as i64, if index == 0 { "active" } else { "pending" }],
                    )?;
                }
                tx.execute(
                    "UPDATE navigation_runs SET updated_at = ?2 WHERE id = ?1",
                    rusqlite::params![run_id, now],
                )?;
                tx.commit()?;
                Ok(())
            })
            .await?;
        let refreshed = load_run(&self.db, run_id)
            .await?
            .ok_or(NavigationError::NoActiveRun)?;
        Ok(refreshed)
    }

    pub async fn end(&self) -> Result<(), NavigationError> {
        let _guard = self.operation.lock().await;
        let now = naive_to_epoch(self.clock.now());
        self.db.with_writer(move |conn| {
            conn.execute("UPDATE navigation_runs SET status = 'ended', updated_at = ?1 WHERE status IN ('active', 'paused', 'completed')", [now])?;
            Ok(())
        }).await?;
        self.route_live
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.release_input_if_idle();
        (self.changed)();
        Ok(())
    }

    pub async fn on_harvest(&self, success: bool) {
        if !self.route_live.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let _ = self
            .harvest_position(if success { "success" } else { "failed" })
            .await;
    }

    /// The automatic arrival path: a harvest swing proves arrival. Scans the
    /// current position and runs full arrival matching (debounce, ambiguity
    /// safety, out-of-order replan) so a matched pending stop is recorded and
    /// the route advances without blocking tracker processing.
    async fn harvest_position(&self, outcome: &str) -> Result<PositionUpdate, NavigationError> {
        let _guard = self.operation.lock().await;
        let Some(run) = load_live_run(&self.db).await? else {
            return Ok(PositionUpdate::NoActiveRun);
        };
        let read = match self.scan_position(&run.planet) {
            Ok(read) => read,
            Err(failure) => return Ok(failure.into()),
        };
        let updated = self
            .apply_position(run, read.lon as f64, read.lat as f64, "harvest", outcome)
            .await?;
        (self.changed)();
        Ok(updated)
    }

    pub fn radar_calibration_start(&self) -> RadarCalibrationPhase {
        *self.radar_phase.lock().expect("radar phase") = RadarCalibrationPhase::AwaitCentre;
        self.set_radar_listener_enabled(true);
        RadarCalibrationPhase::AwaitCentre
    }

    pub fn radar_calibration_cancel(&self) {
        *self.radar_phase.lock().expect("radar phase") = RadarCalibrationPhase::Idle;
        self.set_radar_listener_enabled(false);
    }

    pub fn radar_calibration_phase(&self) -> RadarCalibrationPhase {
        *self.radar_phase.lock().expect("radar phase")
    }

    fn radar_calibration_active(&self) -> bool {
        !matches!(self.radar_calibration_phase(), RadarCalibrationPhase::Idle)
    }

    /// Attach radar calibration to the same gated, press-edge Enter listener
    /// used by coordinate-boundary calibration. Its callback explicitly
    /// re-enters the composition-time runtime from the plain dispatch thread.
    pub fn attach_radar_confirm_listener(
        self: &Arc<Self>,
        source: Option<Arc<dyn KeystrokeSource>>,
        runtime: tokio::runtime::Handle,
    ) -> Arc<CoordConfirmListener> {
        let active = Arc::downgrade(self);
        let confirm = Arc::downgrade(self);
        let listener = CoordConfirmListener::new_with_handler(
            Arc::new(move || {
                active
                    .upgrade()
                    .is_some_and(|service| service.radar_calibration_active())
            }),
            Arc::new(move || {
                let Some(service) = confirm.upgrade() else {
                    return;
                };
                runtime.spawn(async move { service.radar_confirm().await });
            }),
            source,
        );
        *self
            .radar_confirm_listener
            .lock()
            .expect("radar listener slot") = Some(Arc::downgrade(&listener));
        listener
    }

    fn set_radar_listener_enabled(&self, enabled: bool) {
        let listener = self
            .radar_confirm_listener
            .lock()
            .expect("radar listener slot")
            .as_ref()
            .and_then(std::sync::Weak::upgrade);
        if let Some(listener) = listener {
            listener.set_enabled(enabled);
        }
    }

    pub async fn radar_confirm(&self) {
        let Some(cursor) = self.coord_capture.cursor_position() else {
            self.radar_calibration_cancel();
            return;
        };
        let phase = self.radar_calibration_phase();
        match phase {
            RadarCalibrationPhase::Idle => {}
            RadarCalibrationPhase::AwaitCentre => {
                *self.radar_phase.lock().expect("radar phase") =
                    RadarCalibrationPhase::AwaitNorthEdge { centre: cursor };
            }
            RadarCalibrationPhase::AwaitNorthEdge { centre } => {
                let radius = distance(
                    (centre.0 as f64, centre.1 as f64),
                    (cursor.0 as f64, cursor.1 as f64),
                );
                if radius < 8.0 {
                    return;
                }
                let now = naive_to_epoch(self.clock.now());
                let _ = self.db.with_writer(move |conn| {
                    conn.execute(
                        "INSERT INTO radar_calibration (singleton, centre_x, centre_y, north_x, north_y, radius_px, display_scale, updated_at) VALUES (1, ?1, ?2, ?3, ?4, ?5, 1.0, ?6) ON CONFLICT(singleton) DO UPDATE SET centre_x = excluded.centre_x, centre_y = excluded.centre_y, north_x = excluded.north_x, north_y = excluded.north_y, radius_px = excluded.radius_px, display_scale = excluded.display_scale, updated_at = excluded.updated_at",
                        rusqlite::params![centre.0, centre.1, cursor.0, cursor.1, radius, now],
                    )?;
                    Ok(())
                }).await;
                *self.radar_phase.lock().expect("radar phase") = RadarCalibrationPhase::Idle;
                self.set_radar_listener_enabled(false);
            }
        }
    }

    pub async fn radar_geometry(&self) -> Result<Option<RadarGeometry>, DbError> {
        self.db.with_reader(|conn| {
            Ok(conn.query_row(
                "SELECT centre_x, centre_y, north_x, north_y, radius_px, display_scale FROM radar_calibration WHERE singleton = 1",
                [], |row| Ok(RadarGeometry { centre_x: row.get(0)?, centre_y: row.get(1)?, north_x: row.get(2)?, north_y: row.get(3)?, radius_px: row.get(4)?, display_scale: row.get(5)? }))
                .optional()?)
        }).await
    }
}

#[derive(Debug, Clone)]
struct Candidate {
    id: i64,
    lon: f64,
    lat: f64,
}

fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn optimise_open_route(start: (f64, f64), pins: &[Candidate], hop_count: usize) -> Vec<Candidate> {
    let mut remaining = pins.to_vec();
    remaining.sort_by_key(|pin| pin.id);
    let mut route: Vec<Candidate> = Vec::with_capacity(hop_count.min(remaining.len()));
    let mut current = start;
    while route.len() < hop_count && !remaining.is_empty() {
        let candidate_index = remaining
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| {
                distance(current, (left.lon, left.lat))
                    .total_cmp(&distance(current, (right.lon, right.lat)))
                    .then_with(|| left.id.cmp(&right.id))
            })
            .map(|(index, _)| index)
            .expect("remaining is non-empty");
        let next = remaining.remove(candidate_index);
        current = (next.lon, next.lat);
        route.push(next);
    }

    // Bounded best-improvement 2-opt. Reversing a segment preserves all
    // internal edge lengths, so only its two boundaries need evaluation.
    // The fixed start and open end are handled explicitly. Thirty-two passes
    // keep a 500-stop plan predictably interactive while admitting successive
    // improvements that a single pass would miss.
    for _ in 0..32 {
        let mut best: Option<(usize, usize, f64)> = None;
        for left in 0..route.len() {
            let previous = if left == 0 {
                start
            } else {
                (route[left - 1].lon, route[left - 1].lat)
            };
            for right in left + 1..route.len() {
                let old_left = distance(previous, (route[left].lon, route[left].lat));
                let new_left = distance(previous, (route[right].lon, route[right].lat));
                let (old_right, new_right) = if right + 1 < route.len() {
                    let next = (route[right + 1].lon, route[right + 1].lat);
                    (
                        distance((route[right].lon, route[right].lat), next),
                        distance((route[left].lon, route[left].lat), next),
                    )
                } else {
                    (0.0, 0.0)
                };
                let gain = old_left + old_right - new_left - new_right;
                if gain > 1e-9 && best.is_none_or(|(_, _, best_gain)| gain > best_gain) {
                    best = Some((left, right, gain));
                }
            }
        }
        let Some((left, right, _)) = best else { break };
        route[left..=right].reverse();
    }
    route
}

async fn load_candidates(
    db: &Db,
    planet: String,
    map_view_id: Option<i64>,
    now: f64,
) -> Result<Vec<Candidate>, DbError> {
    // A freshly regenerated route excludes trees whose latest confirmed visit
    // is still within its cooldown, so the durable per-tree visits are the
    // recovery point after an interruption.
    let cutoff = now - COOLDOWN_SECONDS;
    db.with_reader(move |conn| {
        // Only special-tree pins are route stops: generic markers (vendors,
        // bosses) are not harvestable targets. The inner join on the tree
        // configuration also excludes config-less pins.
        let mut stmt = conn.prepare("SELECT mp.id, mp.lon, mp.lat FROM map_pins mp JOIN pin_configs pc ON pc.id = mp.pin_config_id AND pc.special_kind = 'tree' WHERE mp.planet = ?1 AND ((?2 IS NULL AND mp.map_view_id IS NULL) OR mp.map_view_id = ?2) AND NOT EXISTS (SELECT 1 FROM map_pin_visits v WHERE v.pin_id = mp.id AND v.visited_at >= ?3) ORDER BY mp.id")?;
        let rows = stmt.query_map(rusqlite::params![planet, map_view_id, cutoff], |row| Ok(Candidate { id: row.get(0)?, lon: row.get(1)?, lat: row.get(2)? }))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }).await
}

async fn end_lingering_runs(db: &Db, now: f64) -> Result<(), DbError> {
    db.with_writer(move |conn| {
        conn.execute(
            "UPDATE navigation_runs SET status = 'ended', updated_at = ?1 WHERE status IN ('active', 'paused', 'completed')",
            [now],
        )?;
        Ok(())
    })
    .await
}

async fn update_run_position(
    db: &Db,
    id: i64,
    lon: f64,
    lat: f64,
    now: f64,
) -> Result<(), DbError> {
    db.with_writer(move |conn| { conn.execute("UPDATE navigation_runs SET current_lon = ?2, current_lat = ?3, last_position_at = ?4, updated_at = ?4 WHERE id = ?1", rusqlite::params![id, lon, lat, now])?; Ok(()) }).await
}

fn activate_next_or_complete(
    conn: &rusqlite::Connection,
    run_id: i64,
    now: f64,
) -> Result<(), rusqlite::Error> {
    let next: Option<i64> = conn.query_row("SELECT id FROM navigation_stops WHERE run_id = ?1 AND status = 'pending' ORDER BY ordinal LIMIT 1", [run_id], |row| row.get(0)).optional()?;
    if let Some(id) = next {
        conn.execute(
            "UPDATE navigation_stops SET status = 'active' WHERE id = ?1",
            [id],
        )?;
        conn.execute(
            "UPDATE navigation_runs SET updated_at = ?2 WHERE id = ?1",
            rusqlite::params![run_id, now],
        )?;
    } else {
        conn.execute(
            "UPDATE navigation_runs SET status = 'completed', updated_at = ?2 WHERE id = ?1",
            rusqlite::params![run_id, now],
        )?;
    }
    Ok(())
}

async fn load_live_run(db: &Db) -> Result<Option<NavigationRun>, DbError> {
    load_run_where(
        db,
        "WHERE status IN ('active', 'paused') ORDER BY updated_at DESC LIMIT 1",
    )
    .await
}

async fn load_current_run(db: &Db) -> Result<Option<NavigationRun>, DbError> {
    load_run_where(
        db,
        "WHERE status IN ('active', 'paused', 'completed') ORDER BY updated_at DESC LIMIT 1",
    )
    .await
}

async fn load_live_run_or_latest(db: &Db) -> Result<Option<NavigationRun>, DbError> {
    load_run_where(db, "ORDER BY updated_at DESC LIMIT 1").await
}

async fn load_run(db: &Db, id: i64) -> Result<Option<NavigationRun>, DbError> {
    load_run_where(db, &format!("WHERE id = {id}")).await
}

async fn load_run_where(db: &Db, clause: &str) -> Result<Option<NavigationRun>, DbError> {
    let clause = clause.to_string();
    let header = db.with_reader(move |conn| {
        Ok(conn.query_row(&format!("SELECT id, planet, map_view_id, (SELECT name FROM map_views WHERE id = navigation_runs.map_view_id), status, start_lon, start_lat, current_lon, current_lat, last_position_at, hop_count, hotkey, updated_at FROM navigation_runs {clause}"), [], |row| {
            let status: String = row.get(4)?;
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<i64>>(2)?, row.get::<_, Option<String>>(3)?, status, row.get::<_, f64>(5)?, row.get::<_, f64>(6)?, row.get::<_, f64>(7)?, row.get::<_, f64>(8)?, row.get::<_, Option<f64>>(9)?, row.get::<_, i64>(10)?, row.get::<_, String>(11)?, row.get::<_, f64>(12)?))
        }).optional()?)
    }).await?;
    let Some((
        id,
        planet,
        map_view_id,
        map_view_name,
        status,
        start_lon,
        start_lat,
        current_lon,
        current_lat,
        last_position_at,
        hop_count,
        hotkey,
        updated_at,
    )) = header
    else {
        return Ok(None);
    };
    let stops = db.with_reader(move |conn| {
        let mut stmt = conn.prepare("SELECT s.id, s.pin_id, s.ordinal, s.status, p.name, p.icon, p.lon, p.lat, s.completed_at, s.completion_source FROM navigation_stops s JOIN map_pins p ON p.id = s.pin_id WHERE s.run_id = ?1 ORDER BY s.ordinal")?;
        let rows = stmt.query_map([id], |row| {
            let status: String = row.get(3)?;
            Ok(NavigationStop { id: row.get(0)?, pin_id: row.get(1)?, ordinal: row.get(2)?, status: StopStatus::parse(&status).unwrap_or(StopStatus::Pending), name: row.get(4)?, icon: row.get(5)?, lon: row.get(6)?, lat: row.get(7)?, completed_at: row.get(8)?, completion_source: row.get(9)? })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }).await?;
    Ok(Some(NavigationRun {
        id,
        planet,
        map_view_id,
        map_view_name,
        status: RunStatus::parse(&status).unwrap_or(RunStatus::Ended),
        start_lon,
        start_lat,
        current_lon,
        current_lat,
        last_position_at,
        hop_count,
        hotkey,
        updated_at,
        stops,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::MockClock;
    use crate::coord_capture::{CoordCaptureProviders, CoordRegion};
    use crate::keystroke_source::MockKeystrokeSource;
    use crate::map_pins::{MapPinsService, NewMapPin};
    use crate::skill_panel::BgrImage;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    fn candidate(id: i64, lon: f64, lat: f64) -> Candidate {
        Candidate { id, lon, lat }
    }

    #[test]
    fn optimiser_is_deterministic_bounded_and_keeps_unique_stops() {
        let pins = vec![
            candidate(1, 10.0, 0.0),
            candidate(2, 2.0, 0.0),
            candidate(3, 4.0, 3.0),
            candidate(4, 5.0, 0.0),
        ];
        let a = optimise_open_route((0.0, 0.0), &pins, 3);
        let b = optimise_open_route((0.0, 0.0), &pins, 3);
        assert_eq!(
            a.iter().map(|p| p.id).collect::<Vec<_>>(),
            b.iter().map(|p| p.id).collect::<Vec<_>>()
        );
        assert_eq!(a.len(), 3);
        let ids: std::collections::BTreeSet<_> = a.iter().map(|pin| pin.id).collect();
        assert_eq!(ids.len(), a.len());
    }

    #[test]
    fn two_unit_arrival_policy_is_euclidean_and_inclusive() {
        assert_eq!(distance((0.0, 0.0), (1.2, 1.6)), ARRIVAL_TOLERANCE_UNITS);
        assert!(distance((0.0, 0.0), (1.2, 1.6)) <= ARRIVAL_TOLERANCE_UNITS);
        assert!(distance((0.0, 0.0), (2.01, 0.0)) > ARRIVAL_TOLERANCE_UNITS);
    }

    async fn navigation_fixture() -> (
        tempfile::TempDir,
        Arc<NavigationService>,
        Arc<Mutex<(i64, i64)>>,
        Arc<AtomicUsize>,
        Arc<MockKeystrokeSource>,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("navigation.db")).await.unwrap();
        let clock: Arc<dyn Clock> = Arc::new(MockClock::new(None, 0.0));
        let pins = MapPinsService::new(db.clone(), clock.clone());
        // Route stops are special-tree pins, so seed a tree configuration and
        // make every fixture pin an instance of it.
        let configs = crate::pin_configs::PinConfigsService::new(db.clone(), clock.clone());
        let tree = configs
            .create(crate::pin_configs::NewPinConfig {
                planet: "Calypso".into(),
                map_view_id: None,
                label: "Tree".into(),
                category: "special".into(),
                special_kind: Some("tree".into()),
                icon: "🌳".into(),
                radius_m: None,
                colour: "#22c55e".into(),
                cooldown_colour: Some("#f59e0b".into()),
            })
            .await
            .unwrap();
        for (name, lon, lat) in [
            ("A", 10.0, 0.0),
            ("D", 13.0, 0.0),
            ("B", 20.0, 3.0),
            ("C", 8.0, 12.0),
        ] {
            pins.create(NewMapPin {
                planet: "Calypso".into(),
                lon,
                lat,
                altitude: None,
                name: name.into(),
                icon: "🌳".into(),
                kind: "tree".into(),
                radius_m: None,
                notes: None,
                session_id: None,
                map_view_id: None,
                pin_config_id: Some(tree.id),
            })
            .await
            .unwrap();
        }
        let position = Arc::new(Mutex::new((0_i64, 0_i64)));
        let changes = Arc::new(AtomicUsize::new(0));
        let input = Arc::new(MockKeystrokeSource::new());
        let service =
            spawn_navigation(db, clock, position.clone(), changes.clone(), input.clone()).await;
        (dir, service, position, changes, input)
    }

    /// Build a navigation service reading its coordinates from `position`,
    /// so a second service can be spawned on the same database to exercise
    /// startup behaviour.
    async fn spawn_navigation(
        db: Db,
        clock: Arc<dyn Clock>,
        position: Arc<Mutex<(i64, i64)>>,
        changes: Arc<AtomicUsize>,
        input: Arc<MockKeystrokeSource>,
    ) -> Arc<NavigationService> {
        let read_position = position.clone();
        let cursor_position = position.clone();
        let capture = CoordCaptureService::new(CoordCaptureProviders {
            region: Arc::new(|| {
                Some(CoordRegion {
                    x: 0,
                    y: 0,
                    w: 8,
                    h: 8,
                })
            }),
            capture_region: Arc::new(|_, _, _, _| {
                Some(BgrImage {
                    data: vec![0; 3],
                    h: 1,
                    w: 1,
                })
            }),
            read_text: Arc::new(move |_| {
                let (lon, lat) = *read_position.lock().unwrap();
                Some((format!("{lon} {lat}"), 1.0))
            }),
            cursor_position: Arc::new(move || Some(*cursor_position.lock().unwrap())),
            ..CoordCaptureProviders::default()
        });
        let change_count = changes.clone();
        NavigationService::new(
            db,
            clock,
            capture,
            Arc::new(|_| {
                Some(CoordBounds {
                    lon_min: -100,
                    lon_max: 100,
                    lat_min: -100,
                    lat_max: 100,
                })
            }),
            Arc::new(move || {
                change_count.fetch_add(1, AtomicOrdering::SeqCst);
            }),
            Some(input),
        )
        .await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn radar_calibration_reuses_the_gated_listener_off_the_runtime_thread() {
        let (_dir, service, position, _changes, input) = navigation_fixture().await;
        let listener = service
            .attach_radar_confirm_listener(Some(input.clone()), tokio::runtime::Handle::current());
        *position.lock().unwrap() = (100, 100);
        service.radar_calibration_start();

        let first_input = input.clone();
        std::thread::spawn(move || {
            let now = chrono::Utc::now();
            first_input.inject("return", now, KeystrokeKind::Press);
            first_input.inject("return", now, KeystrokeKind::Release);
        })
        .join()
        .expect("off-runtime input dispatch");
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while !matches!(
                service.radar_calibration_phase(),
                RadarCalibrationPhase::AwaitNorthEdge { .. }
            ) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("centre confirmation");

        *position.lock().unwrap() = (100, 80);
        let second_input = input.clone();
        std::thread::spawn(move || {
            let now = chrono::Utc::now();
            second_input.inject("return", now, KeystrokeKind::Press);
            second_input.inject("return", now, KeystrokeKind::Release);
        })
        .join()
        .expect("off-runtime input dispatch");
        let geometry = tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if let Some(geometry) = service.radar_geometry().await.unwrap() {
                    break geometry;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("north-edge confirmation");
        assert_eq!((geometry.centre_x, geometry.centre_y), (100, 100));
        assert_eq!((geometry.north_x, geometry.north_y), (100, 80));
        assert_eq!(geometry.radius_px, 20.0);
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while service.radar_calibration_phase() != RadarCalibrationPhase::Idle {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("calibration teardown");
        listener.stop();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn navigation_hotkey_reenters_the_runtime_from_the_dispatch_thread() {
        let (_dir, service, position, _changes, input) = navigation_fixture().await;
        let run = service
            .start("Calypso".into(), None, 0.0, 0.0, 2, "f8".into())
            .await
            .unwrap();
        let first = run.active_stop().unwrap();
        *position.lock().unwrap() = (first.lon as i64, first.lat as i64);

        std::thread::spawn(move || {
            input.inject("f8", chrono::Utc::now(), KeystrokeKind::Press);
        })
        .join()
        .expect("off-runtime hotkey dispatch");

        // The hotkey observes only: it refreshes the position without ever
        // recording a visit, so no stop transitions to Visited.
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                let snapshot = service.snapshot().await.unwrap().unwrap();
                if snapshot.last_position_at.is_some() {
                    assert!(snapshot
                        .stops
                        .iter()
                        .all(|stop| stop.status != StopStatus::Visited));
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("hotkey position update");
    }

    #[tokio::test]
    async fn route_progress_persists_supports_undo_and_stays_visible_at_completion() {
        let (_dir, service, position, changes, _input) = navigation_fixture().await;
        let mut run = service
            .start("Calypso".into(), None, 0.0, 0.0, 2, "f8".into())
            .await
            .unwrap();
        assert_eq!(run.stops.len(), 2);
        assert_eq!(run.active_stop().unwrap().status, StopStatus::Active);

        let first = run.active_stop().unwrap().clone();
        *position.lock().unwrap() = (first.lon as i64, first.lat as i64);
        let PositionUpdate::Updated(updated) = service.mark_visited(false).await.unwrap() else {
            panic!("visited at the active tree")
        };
        run = updated;
        assert_eq!(
            run.stops
                .iter()
                .filter(|stop| stop.status == StopStatus::Visited)
                .count(),
            1
        );

        let second = run.active_stop().unwrap().clone();
        *position.lock().unwrap() = (second.lon as i64, second.lat as i64);
        let PositionUpdate::Updated(completed) = service.mark_visited(false).await.unwrap() else {
            panic!("visited at the active tree")
        };
        assert_eq!(completed.status, RunStatus::Completed);
        assert_eq!(
            service.snapshot().await.unwrap().unwrap().status,
            RunStatus::Completed
        );

        let restored = service.undo().await.unwrap();
        assert_eq!(restored.status, RunStatus::Active);
        assert!(restored.active_stop().is_some());
        assert!(changes.load(AtomicOrdering::SeqCst) >= 4);
    }

    #[tokio::test]
    async fn arriving_at_a_pending_stop_marks_it_and_replans_from_the_observed_position() {
        let (_dir, service, position, _changes, _input) = navigation_fixture().await;
        let run = service
            .start("Calypso".into(), None, 0.0, 0.0, 3, "f8".into())
            .await
            .unwrap();
        let pending = run
            .stops
            .iter()
            .find(|stop| stop.status == StopStatus::Pending)
            .unwrap()
            .clone();
        *position.lock().unwrap() = (pending.lon as i64, pending.lat as i64);

        let PositionUpdate::Updated(replanned) = service.harvest_position("success").await.unwrap()
        else {
            panic!("position updates")
        };
        assert_eq!(
            replanned
                .stops
                .iter()
                .find(|stop| stop.id == pending.id)
                .unwrap()
                .status,
            StopStatus::Visited,
        );
        assert_eq!(replanned.status, RunStatus::Active);
        assert_eq!(replanned.active_stop().unwrap().ordinal, 1);
        assert_eq!(
            (replanned.current_lon, replanned.current_lat),
            (pending.lon, pending.lat)
        );
    }

    #[tokio::test]
    async fn repeated_harvest_swings_near_the_same_tree_do_not_advance_the_next_stop() {
        let (_dir, service, position, _changes, _input) = navigation_fixture().await;
        let run = service
            .start("Calypso".into(), None, 0.0, 0.0, 2, "f8".into())
            .await
            .unwrap();
        let first = run.active_stop().unwrap().clone();
        *position.lock().unwrap() = (first.lon as i64, first.lat as i64);
        service.harvest_position("success").await.unwrap();

        // The next tree is three units away. A one-unit OCR shift is inside
        // both its arrival radius and the previous observation's debounce
        // radius, so this repeated swing must not consume the next stop.
        *position.lock().unwrap() = (first.lon as i64 + 1, first.lat as i64);
        let PositionUpdate::Updated(after_repeat) =
            service.harvest_position("success").await.unwrap()
        else {
            panic!("position updates")
        };
        assert_eq!(
            after_repeat
                .stops
                .iter()
                .filter(|stop| stop.status == StopStatus::Visited)
                .count(),
            1,
        );
        assert_eq!(after_repeat.status, RunStatus::Active);
    }

    #[tokio::test]
    async fn a_position_inside_two_pending_arrival_radii_is_ambiguous_and_mutates_no_stop() {
        let (_dir, service, position, _changes, _input) = navigation_fixture().await;
        let run = service
            .start("Calypso".into(), None, 0.0, 0.0, 2, "f8".into())
            .await
            .unwrap();
        let first = run.active_stop().unwrap();
        *position.lock().unwrap() = (first.lon as i64 + 1, first.lat as i64);
        let PositionUpdate::Ambiguous(ambiguous) =
            service.harvest_position("success").await.unwrap()
        else {
            panic!("overlapping arrival radii stay ambiguous")
        };
        assert!(ambiguous
            .stops
            .iter()
            .all(|stop| matches!(stop.status, StopStatus::Active | StopStatus::Pending)));
    }

    #[tokio::test]
    async fn update_position_observes_without_recording_a_visit() {
        let (_dir, service, position, _changes, _input) = navigation_fixture().await;
        let run = service
            .start("Calypso".into(), None, 0.0, 0.0, 2, "f8".into())
            .await
            .unwrap();
        let first = run.active_stop().unwrap().clone();
        *position.lock().unwrap() = (first.lon as i64, first.lat as i64);

        let PositionUpdate::Updated(observed) = service.update_position().await.unwrap() else {
            panic!("position observes")
        };
        // Standing exactly on the active tree still records nothing: observing
        // is strictly separate from completing.
        assert!(observed
            .stops
            .iter()
            .all(|stop| stop.status != StopStatus::Visited));
        assert_eq!(observed.active_stop().unwrap().id, first.id);
        assert_eq!(
            (observed.current_lon, observed.current_lat),
            (first.lon, first.lat)
        );
        assert!(observed.last_position_at.is_some());
    }

    #[tokio::test]
    async fn mark_visited_outside_tolerance_needs_force_then_completes_the_active_tree() {
        let (_dir, service, position, _changes, _input) = navigation_fixture().await;
        let run = service
            .start("Calypso".into(), None, 0.0, 0.0, 2, "f8".into())
            .await
            .unwrap();
        let first = run.active_stop().unwrap().clone();
        *position.lock().unwrap() = (50, 50);

        let PositionUpdate::OutOfTolerance(pending) = service.mark_visited(false).await.unwrap()
        else {
            panic!("an out-of-range visit asks for confirmation")
        };
        // Position refreshed, but nothing recorded until the user confirms.
        assert!(pending
            .stops
            .iter()
            .all(|stop| stop.status != StopStatus::Visited));
        assert_eq!((pending.current_lon, pending.current_lat), (50.0, 50.0));

        let PositionUpdate::Updated(forced) = service.mark_visited(true).await.unwrap() else {
            panic!("a forced visit completes the active tree")
        };
        assert_eq!(
            forced
                .stops
                .iter()
                .find(|stop| stop.id == first.id)
                .unwrap()
                .status,
            StopStatus::Visited,
        );
        assert_ne!(forced.active_stop().unwrap().id, first.id);
    }

    #[tokio::test]
    async fn a_regenerated_route_excludes_recently_visited_trees() {
        let (_dir, service, position, _changes, _input) = navigation_fixture().await;
        let run = service
            .start("Calypso".into(), None, 0.0, 0.0, 4, "f8".into())
            .await
            .unwrap();
        let first = run.active_stop().unwrap().clone();
        *position.lock().unwrap() = (first.lon as i64, first.lat as i64);
        service.mark_visited(false).await.unwrap();
        service.end().await.unwrap();

        let regenerated = service
            .start("Calypso".into(), None, 0.0, 0.0, 4, "f8".into())
            .await
            .unwrap();
        // The just-visited tree is on cooldown, so it never enters the new route.
        assert!(regenerated
            .stops
            .iter()
            .all(|stop| stop.pin_id != first.pin_id));
    }

    #[tokio::test]
    async fn a_lingering_run_is_ended_at_startup_not_resumed() {
        let (dir, service, position, _changes, _input) = navigation_fixture().await;
        service
            .start("Calypso".into(), None, 0.0, 0.0, 2, "f8".into())
            .await
            .unwrap();
        assert!(service.snapshot().await.unwrap().is_some());
        drop(service);

        // A fresh service on the same database does not restore the run: an
        // interrupted route is recovered by regeneration, not hydration.
        let db = Db::open(&dir.path().join("navigation.db")).await.unwrap();
        let clock: Arc<dyn Clock> = Arc::new(MockClock::new(None, 0.0));
        let restarted = spawn_navigation(
            db,
            clock,
            position.clone(),
            Arc::new(AtomicUsize::new(0)),
            Arc::new(MockKeystrokeSource::new()),
        )
        .await;
        assert!(restarted.snapshot().await.unwrap().is_none());
    }
}
