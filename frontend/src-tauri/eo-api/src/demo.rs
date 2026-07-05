//! Guide-mode demo playback: the typed `demo_*` read commands.
//!
//! The demo serves a curated, never-mutated dataset that drives the in-app
//! guide: a bundled demo database plus a synthetic "mid-hunt" active session.
//! The frontend's guide-mode read wrappers dispatch these typed commands
//! instead of the live ones, sharing the same DTOs; the parallel demo state
//! is built lazily on first access.
//!
//! - The bundled demo DB ships as a Tauri resource. On first demo access it is
//!   copied to a per-process working file and opened read/write, so the demo's
//!   priming writes never touch the bundled file. A parallel [`HuntTracker`]
//!   serves the live snapshot and an [`AnalyticsService`] over the same copy
//!   serves the analytics + session-read surface, both entirely separate from
//!   the live tracking state.
//! - The analytics / session-list reads need only the database; the tracker is
//!   primed lazily, and only by the snapshot read. Priming writes the mid-hunt
//!   session into the shared demo copy, so analytics reads taken after the
//!   snapshot reflect it.
//! - The mid-hunt session is synthesised relative to "now" (`started_at =
//!   now - elapsed`): its curated kill stream rides a committed fixture
//!   (`resources/mid_hunt_fixture.json`), replayed with every timestamp
//!   rebased onto the live clock. The fixture pins the data; the clock keeps
//!   the readout fresh.
//! - The reads share the live commands' computation: [`list_sessions_impl`],
//!   [`get_session_impl`], and [`build_snapshot_value`] (the tracking family),
//!   and [`AnalyticsService`] (the analytics family), each run over the demo's
//!   own parallel database and tracker rather than the live ones.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::TimeDelta;
use eo_services::analytics::AnalyticsService;
use eo_services::clock::Clock;
use eo_services::config_service::{AppConfig, TrifectaPresetConfig};
use eo_services::db::Db;
use eo_services::event_bus::EventBus;
use eo_services::ped::Ped;
use eo_services::time::{epoch_to_instant, naive_to_epoch};
use eo_services::tracker::{HuntTracker, MobSelection, Providers, TrackingMode};
use eo_services::tracking_models::{
    Kill, LootItem, ToolStats, TrackingSession as TrackingSessionModel,
};
use serde::Deserialize;
use sqlx::SqlitePool;
use tokio::sync::OnceCell;

use crate::analytics::{
    analytics_error, shape, shape_each, AnalyticsActivity, AnalyticsOverview, InventoryItem,
    LedgerItem, LedgerPage, LedgerPreset,
};
use crate::tracking::{
    build_snapshot_value, get_session_impl, list_sessions_impl, SessionDetail, TrackingSession,
    TrackingSnapshot,
};
use crate::{Api, ApiError};

/// The curated mid-hunt session. Timestamps are offsets from `started_at`;
/// every other value is replayed verbatim.
const MID_HUNT_FIXTURE: &str = include_str!("../resources/mid_hunt_fixture.json");

/// The demo's fixed mob lock and trifecta preset.
const DEMO_MOB: (&str, &str, &str) = ("Caboria Old", "Caboria", "Old");
const DEMO_PRESET_ID: &str = "demo_default";
const DEMO_PRESET_NAME: &str = "Calypso";
const DEMO_SMALL_WEAPON: &str = "Jester D-1";
const DEMO_BIG_WEAPON: &str = "Korss H400";
const DEMO_HEAL_TOOL: &str = "Vivo T1";

/// Why the demo surface could not prime or serve. Log-facing only: the demo
/// commands collapse it to the generic internal error and log the failure.
#[derive(Debug, thiserror::Error)]
pub enum DemoError {
    #[error("demo data dir: {0}")]
    Io(#[from] std::io::Error),
    #[error("demo database: {0}")]
    Db(#[from] eo_services::db::DbError),
    #[error("demo seed write: {0}")]
    Sql(#[from] sqlx::Error),
    #[error("demo fixture: {0}")]
    Fixture(#[from] serde_json::Error),
    #[error("demo equipment {0:?} is not in the bundled library")]
    MissingEquipment(String),
}

// ── The committed fixture shape ──

#[derive(Deserialize)]
struct Fixture {
    elapsed_seconds: f64,
    session: FixtureSession,
    kills: Vec<FixtureKill>,
    skill_gains: Vec<FixtureSkillGain>,
    notable_events: Vec<FixtureNotable>,
}

#[derive(Deserialize)]
struct FixtureSession {
    id: String,
    is_active: i64,
    armour_cost: f64,
    heal_cost: f64,
    dangling_cost: f64,
}

#[derive(Deserialize)]
struct FixtureKill {
    id: String,
    mob_name: String,
    mob_species: String,
    mob_maturity: String,
    ts_offset: f64,
    shots_fired: i64,
    damage_dealt: f64,
    damage_taken: f64,
    critical_hits: i64,
    cost_ped: f64,
    enhancer_cost: f64,
    loot_total_ped: f64,
    is_global: i64,
    is_hof: i64,
    tool_stats: Vec<FixtureToolStat>,
    loot_items: Vec<FixtureLootItem>,
}

#[derive(Deserialize)]
struct FixtureToolStat {
    tool_name: String,
    shots_fired: i64,
    damage_dealt: f64,
    critical_hits: i64,
    cost_per_shot: f64,
}

#[derive(Deserialize)]
struct FixtureLootItem {
    item_name: String,
    quantity: i64,
    value_ped: f64,
    is_enhancer_shrapnel: i64,
}

#[derive(Deserialize)]
struct FixtureSkillGain {
    ts_offset: f64,
    skill_name: String,
    amount: f64,
    ped_value: f64,
}

#[derive(Deserialize)]
struct FixtureNotable {
    kill_id: String,
    event_type: String,
    mob_or_item: String,
    value_ped: f64,
    ts_offset: f64,
}

// ── The demo state ──

/// The parallel demo services over a writable clone of the bundled demo DB.
pub struct DemoState {
    db: Db,
    analytics: AnalyticsService,
    tracker: Arc<HuntTracker>,
    clock: Arc<dyn Clock>,
    fixture: Fixture,
    /// Snapshot-triggered, once: writes the mid-hunt session into the demo DB
    /// and primes the demo tracker.
    primed: OnceCell<()>,
}

impl DemoState {
    /// Build the demo services: copy the bundled demo DB to a per-process
    /// working file, open it, and stand up the parallel analytics + tracker.
    /// The tracker stays UNPRIMED until the first snapshot.
    pub async fn build(demo_db_path: &Path, clock: Arc<dyn Clock>) -> Result<DemoState, DemoError> {
        let work = working_copy_path();
        // A stale copy from a prior run of the same pid (rare) must not be
        // adopted; start from the bundled file each launch.
        let _ = std::fs::remove_file(&work);
        std::fs::copy(demo_db_path, &work)?;
        let db = Db::open(&work).await?;
        let analytics = AnalyticsService::new(db.clone(), clock.clone());
        let bus = Arc::new(EventBus::new());
        let tracker =
            HuntTracker::new(bus, db.clone(), clock.clone(), Providers::default()).await?;
        let fixture: Fixture = serde_json::from_str(MID_HUNT_FIXTURE)?;
        Ok(DemoState {
            db,
            analytics,
            tracker,
            clock,
            fixture,
            primed: OnceCell::new(),
        })
    }

    fn read(&self) -> &SqlitePool {
        self.db.read()
    }

    fn write(&self) -> &SqlitePool {
        self.db.write()
    }

    fn now_epoch(&self) -> f64 {
        naive_to_epoch(self.clock.now())
    }

    // ── Analytics reads (delegated to the parallel service; no prime) ──

    async fn analytics_overview(&self, period: &str) -> Result<AnalyticsOverview, ApiError> {
        let value = self
            .analytics
            .overview(period)
            .await
            .map_err(analytics_error("demo analytics overview"))?;
        shape(value, "demo analytics overview shaping")
    }

    async fn analytics_activity(&self) -> Result<AnalyticsActivity, ApiError> {
        let value = self
            .analytics
            .activity()
            .await
            .map_err(analytics_error("demo analytics activity"))?;
        shape(value, "demo analytics activity shaping")
    }

    async fn ledger_list(
        &self,
        cursor: Option<String>,
        limit: Option<i64>,
    ) -> Result<LedgerPage, ApiError> {
        let page = self
            .analytics
            .list_ledger(cursor.as_deref(), limit)
            .await
            .map_err(analytics_error("demo ledger list"))?;
        let entries = page
            .entries
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<Vec<LedgerItem>, _>>()
            .map_err(ApiError::internal("demo ledger list shaping"))?;
        Ok(LedgerPage {
            entries,
            next_cursor: page.next_cursor,
        })
    }

    async fn ledger_presets_list(&self) -> Result<Vec<LedgerPreset>, ApiError> {
        let rows = self
            .analytics
            .list_ledger_presets()
            .await
            .map_err(analytics_error("demo ledger presets list"))?;
        shape_each(rows, "demo ledger presets shaping")
    }

    async fn inventory_list(&self) -> Result<Vec<InventoryItem>, ApiError> {
        let rows = self
            .analytics
            .list_inventory()
            .await
            .map_err(analytics_error("demo inventory list"))?;
        shape_each(rows, "demo inventory list shaping")
    }

    // ── Session reads (the live tracking computation over the demo DB) ──

    async fn tracking_sessions(&self) -> Result<Vec<TrackingSession>, ApiError> {
        let value = list_sessions_impl(&self.db, self.now_epoch())
            .await
            .map_err(ApiError::internal("demo tracking sessions"))?;
        serde_json::from_value(value).map_err(ApiError::internal("demo tracking sessions shaping"))
    }

    async fn tracking_session_detail(&self, session_id: &str) -> Result<SessionDetail, ApiError> {
        match get_session_impl(self.read(), session_id, self.now_epoch())
            .await
            .map_err(ApiError::internal("demo session detail"))?
        {
            Some(value) => serde_json::from_value(value)
                .map_err(ApiError::internal("demo session detail shaping")),
            None => Err(ApiError::not_found("Session not found")),
        }
    }

    // ── The snapshot (assumes the primed state ensure_demo guarantees) ──

    async fn tracking_snapshot(&self) -> Result<TrackingSnapshot, ApiError> {
        let config = self
            .demo_config()
            .await
            .map_err(ApiError::internal("demo config"))?;
        // hotbar_active is fixed `true` (the demo reports the listener as
        // running); the snapshot assembly is the live one, reused verbatim.
        let value = build_snapshot_value(&self.db, &self.tracker, &config, true).await?;
        serde_json::from_value(value).map_err(ApiError::internal("demo snapshot shaping"))
    }

    async fn ensure_primed(&self) -> Result<(), DemoError> {
        // `get_or_try_init` runs the prime exactly once even under concurrent
        // first snapshots: the losers await the winner's result rather than
        // racing a second prime (a manual get-then-set would leave an await
        // between the check and the set, so two callers could both prime and
        // the second would hit the fixture's UNIQUE keys). A failed prime is
        // not cached, so a transient error can still retry.
        self.primed
            .get_or_try_init(|| self.prime())
            .await
            .map(|_| ())
    }

    /// Write the mid-hunt session into the demo DB and prime the demo tracker,
    /// rebasing every fixture timestamp onto the live clock.
    async fn prime(&self) -> Result<(), DemoError> {
        let started_naive = self.clock.now()
            - TimeDelta::milliseconds((self.fixture.elapsed_seconds * 1000.0).round() as i64);
        let started_epoch = naive_to_epoch(started_naive);
        let session = &self.fixture.session;

        let mut tx = self.write().begin().await?;
        sqlx::query(
            "INSERT INTO tracking_sessions \
             (id, started_at, ended_at, is_active, armour_cost, heal_cost, dangling_cost) \
             VALUES (?, ?, NULL, ?, ?, ?, ?)",
        )
        .bind(&session.id)
        .bind(started_epoch)
        .bind(session.is_active)
        .bind(session.armour_cost)
        .bind(session.heal_cost)
        .bind(session.dangling_cost)
        .execute(&mut *tx)
        .await?;

        for kill in &self.fixture.kills {
            sqlx::query(
                "INSERT INTO kills \
                 (id, session_id, mob_name, mob_species, mob_maturity, timestamp, \
                  shots_fired, damage_dealt, damage_taken, critical_hits, \
                  cost_ped, enhancer_cost, loot_total_ped, is_global, is_hof) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&kill.id)
            .bind(&session.id)
            .bind(&kill.mob_name)
            .bind(&kill.mob_species)
            .bind(&kill.mob_maturity)
            .bind(started_epoch + kill.ts_offset)
            .bind(kill.shots_fired)
            .bind(kill.damage_dealt)
            .bind(kill.damage_taken)
            .bind(kill.critical_hits)
            .bind(kill.cost_ped)
            .bind(kill.enhancer_cost)
            .bind(kill.loot_total_ped)
            .bind(kill.is_global)
            .bind(kill.is_hof)
            .execute(&mut *tx)
            .await?;

            for tool in &kill.tool_stats {
                sqlx::query(
                    "INSERT INTO kill_tool_stats \
                     (kill_id, tool_name, shots_fired, damage_dealt, critical_hits, cost_per_shot) \
                     VALUES (?, ?, ?, ?, ?, ?)",
                )
                .bind(&kill.id)
                .bind(&tool.tool_name)
                .bind(tool.shots_fired)
                .bind(tool.damage_dealt)
                .bind(tool.critical_hits)
                .bind(tool.cost_per_shot)
                .execute(&mut *tx)
                .await?;
            }

            for item in &kill.loot_items {
                sqlx::query(
                    "INSERT INTO kill_loot_items \
                     (kill_id, item_name, quantity, value_ped, is_enhancer_shrapnel) \
                     VALUES (?, ?, ?, ?, ?)",
                )
                .bind(&kill.id)
                .bind(&item.item_name)
                .bind(item.quantity)
                .bind(item.value_ped)
                .bind(item.is_enhancer_shrapnel)
                .execute(&mut *tx)
                .await?;
            }
        }

        for gain in &self.fixture.skill_gains {
            sqlx::query(
                "INSERT INTO skill_gains \
                 (session_id, timestamp, skill_name, amount, ped_value) \
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&session.id)
            .bind(started_epoch + gain.ts_offset)
            .bind(&gain.skill_name)
            .bind(gain.amount)
            .bind(gain.ped_value)
            .execute(&mut *tx)
            .await?;
        }

        for event in &self.fixture.notable_events {
            sqlx::query(
                "INSERT INTO notable_events \
                 (session_id, kill_id, event_type, mob_or_item, value_ped, timestamp) \
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&session.id)
            .bind(&event.kill_id)
            .bind(&event.event_type)
            .bind(&event.mob_or_item)
            .bind(event.value_ped)
            .bind(started_epoch + event.ts_offset)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;

        // Build the in-memory session and prime the parallel tracker (the
        // snapshot reads its computed readout). The kill values match the rows
        // just written, so the snapshot and the session-read surface agree.
        let kills: Vec<Kill> = self
            .fixture
            .kills
            .iter()
            .map(|kill| Kill {
                id: kill.id.clone(),
                session_id: session.id.clone(),
                mob_name: kill.mob_name.clone(),
                mob_species: kill.mob_species.clone(),
                mob_maturity: kill.mob_maturity.clone(),
                timestamp: started_epoch + kill.ts_offset,
                shots_fired: kill.shots_fired,
                damage_dealt: kill.damage_dealt,
                damage_taken: kill.damage_taken,
                critical_hits: kill.critical_hits,
                cost_ped: Ped(kill.cost_ped),
                enhancer_cost: Ped(kill.enhancer_cost),
                loot_total_ped: Ped(kill.loot_total_ped),
                loot_items: kill
                    .loot_items
                    .iter()
                    .map(|item| LootItem {
                        item_name: item.item_name.clone(),
                        quantity: item.quantity,
                        value_ped: item.value_ped,
                        is_enhancer_shrapnel: item.is_enhancer_shrapnel != 0,
                    })
                    .collect(),
                tool_stats: kill
                    .tool_stats
                    .iter()
                    .map(|tool| {
                        (
                            tool.tool_name.clone(),
                            ToolStats {
                                tool_name: tool.tool_name.clone(),
                                shots_fired: tool.shots_fired,
                                damage_dealt: tool.damage_dealt,
                                critical_hits: tool.critical_hits,
                                cost_per_shot: Ped(tool.cost_per_shot),
                            },
                        )
                    })
                    .collect(),
                is_global: kill.is_global != 0,
                is_hof: kill.is_hof != 0,
            })
            .collect();

        let demo_session = TrackingSessionModel {
            id: session.id.clone(),
            start_time: epoch_to_instant(started_epoch),
            end_time: None,
            kills,
            dangling_cost: Ped(session.dangling_cost),
        };
        self.tracker
            .prime_demo(
                demo_session,
                MobSelection::Manual {
                    name: DEMO_MOB.0.to_string(),
                    species: DEMO_MOB.1.to_string(),
                    maturity: DEMO_MOB.2.to_string(),
                },
                TrackingMode::Mob,
            )
            .await;
        Ok(())
    }

    /// The demo's config stub: trifecta mode with the curated "Calypso" preset,
    /// its weapon ids resolved by name from the demo equipment library.
    /// Everything else is the default config.
    async fn demo_config(&self) -> Result<AppConfig, DemoError> {
        let preset = TrifectaPresetConfig {
            id: DEMO_PRESET_ID.to_string(),
            name: DEMO_PRESET_NAME.to_string(),
            small_weapon_id: Some(self.lookup_equipment_id(DEMO_SMALL_WEAPON).await?),
            big_weapon_id: Some(self.lookup_equipment_id(DEMO_BIG_WEAPON).await?),
            heal_id: Some(self.lookup_equipment_id(DEMO_HEAL_TOOL).await?),
        };
        Ok(AppConfig {
            hotbar_hooks_enabled: false,
            repair_ocr_enabled: false,
            end_of_session_armour_reminder_enabled: false,
            mob_tracking_mode: "mob".to_string(),
            mob_tracking_tag: String::new(),
            manual_mob_species: String::new(),
            manual_mob_maturity: String::new(),
            trifecta_presets: vec![preset],
            active_trifecta_preset_id: Some(DEMO_PRESET_ID.to_string()),
            ..AppConfig::default()
        })
    }

    async fn lookup_equipment_id(&self, name: &str) -> Result<i64, DemoError> {
        sqlx::query_scalar::<_, i64>("SELECT id FROM equipment_library WHERE name = ?")
            .bind(name)
            .fetch_optional(self.read())
            .await?
            .ok_or_else(|| DemoError::MissingEquipment(name.to_string()))
    }
}

/// The per-process working copy of the demo DB (writable, never the bundle).
fn working_copy_path() -> PathBuf {
    std::env::temp_dir().join(format!("entropiaorme-demo-{}.db", std::process::id()))
}

// ── The typed demo commands (Api boundary) ──────────────────────────

impl Api {
    /// Resolve the lazily-built demo state, building it once on first demo
    /// access. A build that cannot be served (no demo DB bundled, or the
    /// build failed) collapses to the internal error, logged server-side; the
    /// demo DB is a shipped resource, so this is a defensive path.
    async fn ensure_demo(&self) -> Result<Arc<DemoState>, ApiError> {
        let demo = self
            .demo
            .get_or_init(|| async {
                let path = self.demo_db_path.clone()?;
                match DemoState::build(&path, self.clock.clone()).await {
                    Ok(demo) => Some(Arc::new(demo)),
                    Err(error) => {
                        tracing::warn!(target: "eo::api", "demo state build failed: {error:?}");
                        None
                    }
                }
            })
            .await
            .clone()
            .ok_or_else(|| ApiError::invalid_state("demo services unavailable"))?;
        // Prime the mid-hunt session up front, so every demo read sees the full
        // curated state regardless of which command the guide issues first. The
        // reads are otherwise order-dependent (only the snapshot needs the
        // primed tracker, but analytics and the session list/detail draw on the
        // primed session too). Priming is idempotent, so only the first demo
        // access in the process pays it.
        demo.ensure_primed()
            .await
            .map_err(ApiError::internal("demo prime"))?;
        Ok(demo)
    }

    /// The demo Overview aggregate for a named period.
    pub async fn demo_analytics_overview(
        &self,
        period: &str,
    ) -> Result<AnalyticsOverview, ApiError> {
        self.ensure_demo().await?.analytics_overview(period).await
    }

    /// The demo Activity aggregate.
    pub async fn demo_analytics_activity(&self) -> Result<AnalyticsActivity, ApiError> {
        self.ensure_demo().await?.analytics_activity().await
    }

    /// One demo ledger page plus the cursor for the next page.
    pub async fn demo_ledger_list(
        &self,
        cursor: Option<String>,
        limit: Option<i64>,
    ) -> Result<LedgerPage, ApiError> {
        self.ensure_demo().await?.ledger_list(cursor, limit).await
    }

    /// The demo ledger presets.
    pub async fn demo_ledger_presets_list(&self) -> Result<Vec<LedgerPreset>, ApiError> {
        self.ensure_demo().await?.ledger_presets_list().await
    }

    /// The demo inventory items.
    pub async fn demo_inventory_list(&self) -> Result<Vec<InventoryItem>, ApiError> {
        self.ensure_demo().await?.inventory_list().await
    }

    /// The demo recent sessions.
    pub async fn demo_tracking_sessions(&self) -> Result<Vec<TrackingSession>, ApiError> {
        self.ensure_demo().await?.tracking_sessions().await
    }

    /// One demo session's full detail; an absent session is a not-found.
    pub async fn demo_tracking_session_detail(
        &self,
        session_id: &str,
    ) -> Result<SessionDetail, ApiError> {
        self.ensure_demo()
            .await?
            .tracking_session_detail(session_id)
            .await
    }

    /// The demo dashboard snapshot (primes the mid-hunt session on first call).
    pub async fn demo_tracking_snapshot(&self) -> Result<TrackingSnapshot, ApiError> {
        self.ensure_demo().await?.tracking_snapshot().await
    }
}

// The goldens are the demo's exact command payloads (the DTOs serialised), a
// byte-faithful pin of the curated DATA and the shared native computation. The
// demo's now-relative session makes the absolute-datetime renderings (the
// snapshot `started_at` and `recentEvents[].timestamp`, the sessions-list
// `startTime`/`endTime`) clock/timezone-dependent; those are normalised before
// the comparison, so the test pins the curated data while treating the live
// time rendering as the known non-deterministic surface. UTC-stable date
// buckets (the analytics timeline/monthly keys) are NOT datetime strings and
// stay asserted. Set `UPDATE_DEMO_GOLDENS` to rewrite the golden files from the
// current output (a ratified regeneration when the typed contract moves).
#[cfg(test)]
mod tests {
    use super::*;
    use eo_services::clock::MockClock;
    use serde::Serialize;
    use serde_json::Value;

    /// The dev-tree bundled demo DB (the resource the app ships).
    fn demo_db_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../data/demo/entropia_orme.db")
    }

    fn golden_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources/demo_goldens")
            .join(format!("{name}.txt"))
    }

    fn to_json<T: Serialize>(value: &T) -> String {
        serde_json::to_string(value).expect("demo payload serialises")
    }

    /// Recursively replace the now-relative surface with placeholders: ISO-8601
    /// datetime strings (`YYYY-MM-DDThh:...`) and the snapshot's `elapsed`
    /// count. Date-only (`YYYY-MM-DD`) and month (`YYYY-MM`) bucket keys lack
    /// the `T` and stay, so the analytics timeline is still asserted.
    fn normalise(value: &mut Value) {
        match value {
            Value::String(text) => {
                if is_iso_datetime(text) {
                    *text = "<TS>".to_string();
                }
            }
            Value::Array(items) => items.iter_mut().for_each(normalise),
            Value::Object(map) => {
                for (key, child) in map.iter_mut() {
                    if key == "elapsed" {
                        *child = Value::String("<ELAPSED>".to_string());
                    } else {
                        normalise(child);
                    }
                }
            }
            _ => {}
        }
    }

    fn is_iso_datetime(text: &str) -> bool {
        let b = text.as_bytes();
        b.len() >= 19
            && b[..4].iter().all(u8::is_ascii_digit)
            && b[4] == b'-'
            && b[7] == b'-'
            && b[10] == b'T'
    }

    fn assert_matches_golden(name: &str, body: &str) {
        let path = golden_path(name);
        if std::env::var_os("UPDATE_DEMO_GOLDENS").is_some() {
            std::fs::write(&path, body).expect("golden writes");
            return;
        }
        let golden = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{name}: golden unreadable: {e}"));
        let mut got: Value = serde_json::from_str(body)
            .unwrap_or_else(|e| panic!("{name}: demo body is not JSON: {e}\n{body}"));
        let mut want: Value = serde_json::from_str(&golden)
            .unwrap_or_else(|e| panic!("{name}: golden not JSON: {e}"));
        normalise(&mut got);
        normalise(&mut want);
        assert_eq!(got, want, "{name}: demo output diverged from the golden");
    }

    // The parallel tracker bridges DB work onto the runtime via `block_on`,
    // which requires the multi-threaded flavour (as in production).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn demo_reads_reproduce_the_curated_goldens() {
        let clock = Arc::new(MockClock::new(
            Some(
                chrono::NaiveDateTime::parse_from_str("2026-06-18 12:00:00", "%Y-%m-%d %H:%M:%S")
                    .unwrap(),
            ),
            0.0,
        ));
        let demo = DemoState::build(&demo_db_path(), clock)
            .await
            .expect("demo state builds over the bundled demo DB");

        // Prime up front, exactly as `Api::ensure_demo` does before any demo
        // command runs (the reads no longer self-prime). Order-independence:
        // the session list reflects the primed mid-hunt session here, before the
        // snapshot has been requested at all, so a guide flow that reads any
        // command first sees the full curated state.
        demo.ensure_primed().await.expect("demo primes");
        assert_matches_golden(
            "tracking_sessions",
            &to_json(&demo.tracking_sessions().await.expect("sessions")),
        );

        let snapshot = to_json(&demo.tracking_snapshot().await.expect("snapshot"));
        assert_matches_golden("tracking_snapshot", &snapshot);
        // The now-relative readout: elapsed is the fixed mid-hunt window and the
        // session is active with the full kill stream.
        let snap: Value = serde_json::from_str(&snapshot).unwrap();
        assert_eq!(snap["status"], "active");
        assert_eq!(snap["elapsed"], 754);
        assert_eq!(snap["kill_count"], 100);
        assert_eq!(snap["currentMob"], "Caboria Old");
        assert_eq!(snap["mobSource"], "manual");
        assert_eq!(snap["weaponAttribution"], "trifecta");
        assert!(is_iso_datetime(snap["started_at"].as_str().unwrap()));

        assert_matches_golden(
            "analytics_overview_all",
            &to_json(&demo.analytics_overview("all").await.expect("overview all")),
        );
        assert_matches_golden(
            "analytics_overview_30d",
            &to_json(&demo.analytics_overview("30d").await.expect("overview 30d")),
        );
        assert_matches_golden(
            "analytics_activity",
            &to_json(&demo.analytics_activity().await.expect("activity")),
        );
        assert_matches_golden(
            "analytics_ledger",
            &to_json(&demo.ledger_list(None, None).await.expect("ledger")),
        );
        assert_matches_golden(
            "analytics_ledger_presets",
            &to_json(&demo.ledger_presets_list().await.expect("presets")),
        );
        assert_matches_golden(
            "analytics_inventory",
            &to_json(&demo.inventory_list().await.expect("inventory")),
        );

        // Session detail for the primed mid-hunt session (the fixture's id, the
        // same one the golden was captured against).
        let fixture: Value = serde_json::from_str(MID_HUNT_FIXTURE).unwrap();
        let session_id = fixture["session"]["id"].as_str().unwrap();
        assert_matches_golden(
            "tracking_session_detail",
            &to_json(
                &demo
                    .tracking_session_detail(session_id)
                    .await
                    .expect("detail"),
            ),
        );
    }
}
