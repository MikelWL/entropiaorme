# Database schema reference

This page documents the on-disk persistence layer: the application's SQLite
database, the storage configuration applied to its connections, its tables, the
forward-only migration mechanism, and the bundled game-data snapshot that lives
outside SQLite entirely.

The authoritative schema is the migration set under
`app/src-tauri/eo-services/migrations/`, applied by the embedded runner in
`app/src-tauri/eo-services/src/db/`. The set holds a version-33 baseline
migration, `0001_schema_baseline.sql`, which creates the complete base schema
(tables, indexes, and the timestamp-back-fill triggers) and stamps the
schema-version row, followed by forward-only additions:
`0002_analytical_indexes.sql` (analytical read-path indexes plus a one-time
`ANALYZE`), `0003_session_summary_read_columns.sql` (extra
`session_summaries` columns for the Activity and session-list reads),
`0004_daily_rollups.sql` (the per-day analytics rollup projection behind the
Overview, plus a ledger date index), `0005_market_observations.sql` (the
market-markup observation feed), `0006_harvest_events.sql` (the
harvesting activity tables plus the harvest columns on the summary and
rollup projections), `0007_map_pins.sql` (the cartography pins),
`0008_map_views.sql` (independent named pin sets over each planet map),
`0009_map_navigation.sql` (persisted routes, stop progress, pin visits, radar
calibration, and cartography spatial indexes),
`0010_navigation_runtime_fields.sql` (the last-position timestamp and
flow-scoped route hotkey), `0011_pin_configs.sql` (the per-preset pin
palette, with each placed pin referencing one configuration),
`0012_harvest_stock_removed.sql` (an interim per-item current-stock overlay,
since retired), `0013_harvest_yield_tier.sql` (durable Tree Cutting yield
activity plus its historical backfill), `0014_auction_sales.sql` (the auction
listing lifecycle, stock conversions, and the signed stock-movement ledger
that replaced the overlay above, whose rows it carries across before dropping
it), `0015_stock_movement_tool.sql` (the tool a movement's stock was produced
with, recorded but not currently reported on), `0016_stock_opening_balance.sql` (a movement kind for stock an outflow
proves was held but that was never recorded, rebuilding the movement table
because SQLite cannot widen a CHECK constraint in place), and
`0017_undone_entries.sql` (the marker that keeps an undone listing or
conversion on file with its effects reversed). The
`Db::open` path opens the database, configures its session pragmas, adopts or
refuses any pre-existing schema, and then runs the migrator (`MIGRATOR` in
`db.rs`).

The column descriptions below reflect the schema after the full migration set,
save for the stock and auction tables `0014` onward introduce, which are
described by the migrations themselves. The canonical table set is otherwise
the one the baseline defines; the later migrations add indexes and the
`session_summaries` read columns noted in place.

## Overview

The application persists user-owned data to a single SQLite database, kept under
the application data directory:

| Database | File | Role |
| --- | --- | --- |
| Application database | `entropia_orme.db` | Long-lived, user-owned data: equipment, calibrations, the ledger, codex and quest tracking, recorded hunting sessions, and the derived analytics caches. |

The database runs in write-ahead-logging (WAL) mode. The rationale for that
choice (concurrent reads alongside a single writer, with the database opened
once and shared across the in-process request handlers and background worker
threads) is covered in [ADR 0007: SQLite in WAL mode](../adr/0007-sqlite-wal.md). The
services that own and query the database are catalogued in the
[service map](service-map.md).

The game-fact data the application reasons over (weapons, mobs, skills,
professions, and so on) is **not** stored in SQLite. It ships as a bundled,
read-only snapshot loaded from per-endpoint JSON files; see
[Bundled game-data snapshot](#bundled-game-data-snapshot) below.

## Storage configuration

Every connection is configured identically by the connect options assembled in
`Db::open` (`eo-services/src/db.rs`). The pragmas are applied as the connection
is opened, before adoption and the migrator run:

| Pragma | Value | Effect |
| --- | --- | --- |
| `journal_mode` | `WAL` | Write-ahead logging: readers do not block the single writer and the writer does not block readers. |
| `synchronous` | `NORMAL` | Reduced fsync frequency, the standard companion to WAL: durable across application crashes, with a small exposure to a power-loss truncation of the most recent WAL frames. |
| `busy_timeout` | `5000` | Wait up to 5000 ms for a contended lock before raising `SQLITE_BUSY`. |
| `cache_size` | `-8000` | Negative value: an 8 MB page cache (SQLite reads a negative `cache_size` as a kibibyte budget rather than a page count). |
| `foreign_keys` | `OFF` | Referential enforcement is left off, so the schema's `REFERENCES` clauses are declarative; this matches the effective pragma surface the schema was authored against, where an overlay write for a session id with no surviving session row must be accepted. |

### Single-connection model

The handle is a `SqlitePool` capped at a single connection
(`max_connections(1)` in `Db::open`), so every statement serialises on one
underlying connection. Cloning a `Db` shares that one pool rather than opening a
second; the composition root opens the application database exactly once. The
single-writer model is intentional, and relaxing to multiple reader connections
would be a later, benchmark-justified change.

The data directory is created on demand: the connect options set
`create_if_missing(true)`, and the composition root ensures the parent directory
exists before opening.

## Application database tables

All tables described here live in `entropia_orme.db`. The complete schema,
including the tracking tables, is created in one shot by the baseline migration
`0001_schema_baseline.sql`. Several `REAL` timestamp columns default to
`unixepoch('now')`; where a column is instead back-filled by an `AFTER INSERT`
trigger when the caller leaves it `NULL`, that is noted.

### Metadata

#### `db_metadata`

Key/value store for the schema version counter. Created by the baseline
migration; the version row it carries is read by the adoption logic described
later.

| Column | Type | Notes |
| --- | --- | --- |
| `key` | TEXT | Primary key. The schema version is stored under the key `version`. |
| `value` | TEXT | Stored as text; the version value is parsed back to an integer on read. |

### User data

#### `equipment_library`

The user's saved equipment definitions (weapons, amplifiers, and other gear),
with type-specific attributes held as a JSON blob.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | INTEGER | Primary key, autoincrement. |
| `name` | TEXT | Not null. |
| `item_type` | TEXT | Not null. |
| `catalog_id` | TEXT | Optional link to a game-data catalogue entry. |
| `properties_json` | TEXT | Not null; type-specific attributes serialised as JSON. |
| `created_at` | REAL | Not null; defaults to `unixepoch('now')`. |
| `updated_at` | REAL | Back-filled by an `AFTER INSERT` trigger when left null. |

#### `skill_calibrations`

Calibration points for the skill curve: an observed skill level at a point in
time, attributed to a source.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | INTEGER | Primary key, autoincrement. |
| `skill_name` | TEXT | Not null. Indexed (`idx_skill_cal_name`), and indexed with `scanned_at DESC` (`idx_skill_cal_name_scanned`) for latest-per-skill lookups. |
| `level` | REAL | Not null. |
| `source` | TEXT | Not null. |
| `scanned_at` | REAL | Not null; defaults to `unixepoch('now')`. |

#### `skill_calibrations_archive`

Superseded calibration rows, retained for history when a newer calibration
replaces an earlier one.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | INTEGER | Primary key, autoincrement. |
| `original_id` | INTEGER | Not null; the `id` of the archived `skill_calibrations` row. |
| `skill_name` | TEXT | Not null; indexed (`idx_skill_cal_arch_name`). |
| `level` | REAL | Not null. |
| `source` | TEXT | Not null. |
| `scanned_at` | REAL | Not null; the original scan time, carried over. |
| `archived_at` | REAL | Not null; defaults to `unixepoch('now')`. |

#### `inventory_items`

User-tracked inventory entries with TT value and markup paid.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | TEXT | Primary key (caller-supplied identifier). |
| `name` | TEXT | Not null; indexed (`idx_inventory_items_name`). |
| `tt_value` | REAL | Not null. |
| `markup_paid` | REAL | Not null. |
| `notes` | TEXT | Optional. |
| `acquired_at` | TEXT | Not null; stored as a text date. |
| `updated_at` | REAL | Back-filled by an `AFTER INSERT` trigger when left null. |

### Ledger

#### `ledger_entries`

The cost/sale ledger: dated, tagged, signed amounts that feed profit-and-loss
accounting. This table is shared between the user-data services and the tracking
layer: the baseline migration declares it once, and the tracker writes
shrapnel-conversion entries into it.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | TEXT | Primary key (caller-supplied identifier). |
| `date` | TEXT | Not null; stored as a text date. Indexed (`idx_ledger_entries_date`, migration `0004`) for the windowed analytics reads and the rollup heal's stray-key sweep. |
| `type` | TEXT | Not null. |
| `description` | TEXT | Not null. |
| `amount` | REAL | Not null; signed. |
| `tag` | TEXT | Not null. |

#### `ledger_presets`

Reusable templates for common ledger entries.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | TEXT | Primary key (caller-supplied identifier). |
| `name` | TEXT | Not null. |
| `type` | TEXT | Not null. |
| `description` | TEXT | Not null. |
| `amount` | REAL | Not null. |
| `tag` | TEXT | Not null. |
| `created_at` | REAL | Not null; defaults to `unixepoch('now')`. |
| `updated_at` | REAL | Back-filled by an `AFTER INSERT` trigger when left null. |

### Skill gains

#### `skill_gains`

Individual skill-gain events recorded during a session, optionally valued in
PED.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | INTEGER | Primary key, autoincrement. |
| `session_id` | TEXT | Not null; indexed (`idx_skill_gains_session`). |
| `timestamp` | REAL | Not null; indexed (`idx_skill_gains_timestamp`). |
| `skill_name` | TEXT | Not null; indexed (`idx_skill_gains_skill`). |
| `amount` | REAL | Not null. |
| `ped_value` | REAL | Optional PED valuation of the gain. |
| `created_at` | REAL | Not null; defaults to `unixepoch('now')`. |

### Codex

#### `codex_progress`

Current codex rank reached per species.

| Column | Type | Notes |
| --- | --- | --- |
| `species_name` | TEXT | Primary key. |
| `current_rank` | INTEGER | Not null; defaults to 0. |
| `updated_at` | REAL | Not null; defaults to `unixepoch('now')`. |

#### `codex_claims`

A log of codex reward claims (rank rewards and, where applicable, attribute
rewards).

| Column | Type | Notes |
| --- | --- | --- |
| `id` | INTEGER | Primary key, autoincrement. |
| `species_name` | TEXT | Not null; indexed (`idx_codex_claims_species`). |
| `rank` | INTEGER | Not null. |
| `skill_name` | TEXT | Not null. |
| `ped_value` | REAL | Not null. |
| `claimed_at` | REAL | Not null; defaults to `unixepoch('now')`. Indexed (`idx_codex_claims_claimed_at`). |
| `kind` | TEXT | Not null; defaults to `'rank'`. |
| `attribute_name` | TEXT | Optional; set for attribute claims. |

### Quests

#### `quests`

Quest definitions, including rewards, chain position, and activation state.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | INTEGER | Primary key, autoincrement. |
| `name` | TEXT | Not null. |
| `planet` | TEXT | Not null; defaults to `'Calypso'`. |
| `waypoint` | TEXT | Optional. |
| `cooldown_hours` | REAL | Optional. |
| `reward_ped` | REAL | Optional. |
| `reward_is_skill` | INTEGER | Not null; defaults to 0 (boolean flag). |
| `expected_reward_markup_percent` | REAL | Optional. |
| `notes` | TEXT | Optional. |
| `chain_name` | TEXT | Optional. |
| `chain_position` | INTEGER | Optional. |
| `chain_total` | INTEGER | Optional. |
| `started_at` | REAL | Optional. |
| `is_active` | INTEGER | Not null; defaults to 1. |
| `created_at` | REAL | Not null; defaults to `unixepoch('now')`. |
| `category` | TEXT | Optional. |
| `reward_description` | TEXT | Optional. |
| `updated_at` | REAL | Back-filled by an `AFTER INSERT` trigger when left null. |

#### `quest_mobs`

The mobs associated with a quest. Composite-keyed join table.

| Column | Type | Notes |
| --- | --- | --- |
| `quest_id` | INTEGER | Not null; references `quests(id)`. Part of the composite primary key. |
| `mob_name` | TEXT | Not null. Part of the composite primary key. |

#### `quest_playlists`

User-defined ordered collections of quests, with an estimated duration.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | INTEGER | Primary key, autoincrement. |
| `name` | TEXT | Not null. |
| `planet` | TEXT | Not null; defaults to `'Calypso'`. |
| `estimated_minutes` | INTEGER | Not null; defaults to 30. |
| `is_active` | INTEGER | Not null; defaults to 1. |
| `created_at` | REAL | Not null; defaults to `unixepoch('now')`. |
| `updated_at` | REAL | Back-filled by an `AFTER INSERT` trigger when left null. |

#### `quest_playlist_items`

The ordered members of a playlist.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | INTEGER | Primary key, autoincrement. |
| `playlist_id` | INTEGER | Not null; references `quest_playlists(id)`. Indexed (`idx_qpi_playlist`). |
| `quest_id` | INTEGER | Not null; references `quests(id)`. |
| `sort_order` | INTEGER | Not null; defaults to 0. |
| `description` | TEXT | Optional. |
| `group_type` | TEXT | Not null; defaults to `'immediate'`. |
| `updated_at` | REAL | Back-filled by an `AFTER INSERT` trigger when left null. |

#### `quest_claims`

A log of quest reward claims.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | INTEGER | Primary key, autoincrement. |
| `quest_id` | INTEGER | Optional; indexed (`idx_quest_claims_quest`). Nullable so a claim can survive deletion of its quest definition. |
| `quest_name` | TEXT | Not null. |
| `ped_value` | REAL | Not null. |
| `claimed_at` | REAL | Not null; defaults to `unixepoch('now')`. Indexed (`idx_quest_claims_claimed_at`). |

#### `session_quest_completions`

Records that a given quest was completed during a given session. The
`(session_id, quest_id)` pair is unique.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | INTEGER | Primary key, autoincrement. |
| `session_id` | TEXT | Not null; indexed (`idx_sqc_session`). |
| `quest_id` | INTEGER | Not null; indexed (`idx_sqc_quest`). |
| `completed_at` | REAL | Not null; defaults to `unixepoch('now')`. |

A `UNIQUE(session_id, quest_id)` constraint prevents duplicate completion rows.

#### `session_quest_analytics_links`

Associates a session with a single quest or playlist for analytics attribution.
Keyed by session, so each session has at most one link.

| Column | Type | Notes |
| --- | --- | --- |
| `session_id` | TEXT | Primary key. |
| `link_type` | TEXT | Not null. |
| `quest_id` | INTEGER | Optional; indexed (`idx_sqal_quest`). |
| `playlist_id` | INTEGER | Optional; indexed (`idx_sqal_playlist`). |
| `linked_at` | REAL | Not null; defaults to `unixepoch('now')`. Back-filled by an `AFTER INSERT` trigger when left null. |

### Tracking

These tables are created by the baseline migration alongside the rest of the
schema. They carry no separate creation step or version counter of their own:
the single baseline reproduces the complete version-33 surface, the tracking
tables included.

#### `tracking_sessions`

One row per recorded hunting session, with session-level cost buckets.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | TEXT | Primary key. |
| `started_at` | REAL | Not null; indexed (`idx_tracking_sessions_started_at`). |
| `ended_at` | REAL | Null while the session is open. |
| `is_active` | INTEGER | Not null; defaults to 1. |
| `armour_cost` | REAL | Defaults to 0. |
| `heal_cost` | REAL | Defaults to 0. |
| `dangling_cost` | REAL | Defaults to 0. |
| `mob_tracking_mode` | TEXT | Not null; defaults to `'mob'`. Records the attribution input mode (`'mob'` or `'tag'`); a presentation hint only, since the data semantics are identical. |
| `updated_at` | REAL | Back-filled by an `AFTER INSERT` trigger when left null. |

#### `kills`

One row per kill, which is also one loot group with accumulated combat stats. A
denormalised `loot_total_ped` is maintained alongside the per-item loot rows so
analytics queries can read the total directly.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | TEXT | Primary key. |
| `session_id` | TEXT | Not null; references `tracking_sessions(id)`. Indexed (`idx_kill_session`). |
| `mob_name` | TEXT | Optional. In tag-mode sessions the tag string is persisted here. |
| `mob_species` | TEXT | Defaults to `''`. |
| `mob_maturity` | TEXT | Defaults to `''`. |
| `timestamp` | REAL | Not null; indexed (`idx_kills_timestamp`) for the analytics time-window reads. |
| `shots_fired` | INTEGER | Defaults to 0. |
| `damage_dealt` | REAL | Defaults to 0. |
| `damage_taken` | REAL | Defaults to 0. |
| `critical_hits` | INTEGER | Defaults to 0. |
| `cost_ped` | REAL | Defaults to 0. |
| `enhancer_cost` | REAL | Defaults to 0. |
| `loot_total_ped` | REAL | Defaults to 0; denormalised per-kill loot total, mutated atomically with loot-item changes. |
| `is_global` | INTEGER | Defaults to 0 (boolean flag). |
| `is_hof` | INTEGER | Defaults to 0 (boolean flag). |
| `original_mob_name` | TEXT | Null until the session's attributed mob is renamed; preserves the first pre-rename value so a rename can be reverted. |

#### `kill_tool_stats`

Per-tool combat statistics within a single kill. The
`(kill_id, tool_name, cost_per_shot)` triple is unique.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | INTEGER | Primary key, autoincrement. |
| `kill_id` | TEXT | Not null; references `kills(id)`. |
| `tool_name` | TEXT | Not null. |
| `shots_fired` | INTEGER | Defaults to 0. |
| `damage_dealt` | REAL | Defaults to 0. |
| `critical_hits` | INTEGER | Defaults to 0. |
| `cost_per_shot` | REAL | Defaults to 0. |

A `UNIQUE(kill_id, tool_name, cost_per_shot)` constraint keeps one row per
tool-and-cost combination per kill. A covering index
`idx_kill_tool_stats_covering(kill_id, cost_per_shot, shots_fired, tool_name)`
(migration `0002`) carries `shots_fired` and `tool_name` alongside the join key,
so the weapon-cost aggregate resolves from the index without a per-row table
fetch.

#### `kill_loot_items`

The individual loot items dropped by a kill.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | INTEGER | Primary key, autoincrement. |
| `kill_id` | TEXT | Not null; references `kills(id)`. Indexed (`idx_kill_loot_items_kill_id`). |
| `item_name` | TEXT | Not null. |
| `quantity` | INTEGER | Defaults to 1. |
| `value_ped` | REAL | Not null. |
| `is_enhancer_shrapnel` | INTEGER | Not null; defaults to 0 (boolean flag). |
| `deactivated_at` | REAL | Null when active (included in aggregates); a Unix-epoch timestamp when the entry has been deactivated by a post-hoc edit. Recoverable: clearing the timestamp reactivates the entry. |

#### `harvest_events`

One row per harvesting (tree cutting) swing, the second tracked activity
beside hunting (migration `0006`). A successful swing arrives as a wood loot
group; a failed swing arrives as the explicit "Harvest attempt failed" chat
line, so every swing is directly counted rather than inferred. The tool
identity and per-swing cost are captured at swing time, so later equipment
edits cannot rewrite history; `tool_name` is null when no harvesting tool was
known and the swing recorded at zero cost. The effective yield tier is stored
independently: board evidence classifies the swing as short, long, or huge,
while a boardless swing may inherit a tier from direct board evidence on the same session and tool within 30 seconds. Live attribution takes the nearest preceding evidence in the same keypress; the historical backfill, and any later restamp when fresh direct evidence arrives, compare both temporal sides and infer only when one side carries evidence or the two agree. Direct evidence is never overwritten. Unsupported
evidence remains explicitly unknown.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | TEXT | Primary key. |
| `session_id` | TEXT | Not null; references `tracking_sessions(id)`. Indexed alone (`idx_harvest_session`) and with tool and timestamp (`idx_harvest_session_tool_time`). |
| `timestamp` | REAL | Not null. Indexed with yield tier and tool (`idx_harvest_time_tier_tool`) for period-scoped analytics. |
| `success` | INTEGER | Not null; defaults to 1 (boolean flag). |
| `tool_name` | TEXT | Optional; the harvesting tool equipped via the hotbar at swing time. |
| `yield_tier` | TEXT | Not null; `short`, `long`, `huge`, or `unknown` (migration `0013`). This is the effective board yield, not a physical-tree observation. |
| `yield_tier_source` | TEXT | Optional; `board` when the swing's own loot named the class, or `inferred` when it was taken from adjacent direct evidence under the bound above. Null when the tier remains unknown. |
| `cost_ped` | REAL | Defaults to 0; the tool's per-use decay (markup-weighted) at swing time. |
| `loot_total_ped` | REAL | Defaults to 0; denormalised per-swing loot total beside the per-item rows. |

#### `harvest_loot_items`

The individual wood items a successful swing dropped.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | INTEGER | Primary key, autoincrement. |
| `harvest_id` | TEXT | Not null; references `harvest_events(id)`. Indexed (`idx_harvest_loot_items_harvest`). |
| `item_name` | TEXT | Not null. |
| `quantity` | INTEGER | Defaults to 1. |
| `value_ped` | REAL | Defaults to 0. |
| `deactivated_at` | REAL | Null when active; mirrors `kill_loot_items` so the loot-edit flow can extend to harvest loot without a schema move. |

#### `harvest_stock_removed` (retired)

An interim per-item overlay of quantity already removed from the recorded
harvest position (migration `0012`). Migration `0014` retired it: its rows were
carried into `stock_movements` as explicitly unattributed adjustments and the
table was dropped, so current position derives from recorded loot plus that
signed movement ledger rather than from two sources of the same quantity. No
current database carries it.

#### `notable_events`

Notable in-session events (for example globals and Hall-of-Fame drops).

| Column | Type | Notes |
| --- | --- | --- |
| `id` | INTEGER | Primary key, autoincrement. |
| `session_id` | TEXT | Not null; references `tracking_sessions(id)`. Indexed (`idx_notable_session`). |
| `kill_id` | TEXT | Optional; the kill the event is associated with. |
| `event_type` | TEXT | Not null. |
| `mob_or_item` | TEXT | Not null. |
| `value_ped` | REAL | Not null. |
| `timestamp` | REAL | Not null. |

### Market data

The manually-fed market-markup layer: the user pastes the game's auction-house
market-ledger export (a tab-separated table of per-item markup and sales
volume over five aggregation horizons), and each accepted paste is stored as
one submission with its per-item, per-horizon observations. This layer is
informational only: estimated markup never contributes to the ledger, the
analytics aggregates, or any realised profit-and-loss figure (a boundary the
`market-isolation` CI guard enforces mechanically).

#### `market_submissions`

One accepted paste.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | INTEGER | Primary key, autoincrement. |
| `submitted_at` | REAL | Not null; Unix-epoch seconds. |
| `source` | TEXT | Not null; `paste` (room for future feeds). |
| `item_count` | INTEGER | Not null; the number of items the paste carried. |

#### `market_observations`

The per-item, per-horizon readings of a submission (five rows per item, one
per aggregation horizon).

| Column | Type | Notes |
| --- | --- | --- |
| `id` | INTEGER | Primary key, autoincrement. |
| `submission_id` | INTEGER | Not null; references `market_submissions(id)`. Indexed (`idx_market_observations_submission`). |
| `item_name` | TEXT | Not null. Indexed with `horizon` and `submission_id` (`idx_market_observations_item`). |
| `tier` | INTEGER | Not null; the item tier the export reported. |
| `horizon` | TEXT | Not null; one of `day`, `week`, `month`, `year`, `decade`. |
| `markup_pct` | REAL | Null where the export reported `N/A` (no sales in that horizon). |
| `sales_ped` | REAL | Not null; TT turnover over the horizon, normalised to PED. |

### Cartography

#### `map_views`

User-named pin sets over a bundled planet map (migration `0008`). The
permanent Default set is represented by a null `map_view_id` on `map_pins`,
so it needs no row and every pin created before this migration remains on
Default. Names are unique per planet without regard to case. Deleting a
named map and its pins is an explicit transaction in `map_pins`, because
foreign-key enforcement is intentionally disabled for this database.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | INTEGER | Primary key, autoincrement. |
| `planet` | TEXT | Not null; the bundled planet map this pin set overlays. Indexed with `created_at` and `id` (`idx_map_views_planet`). |
| `name` | TEXT | Not null, case-insensitive; unique with `planet`. |
| `created_at` | REAL | Not null; Unix-epoch seconds. |

#### `map_pins`

User-authored pins on the bundled planet maps (migration `0007`): a named,
icon-carrying location, either an exact point or an area of a given radius.
Coordinates are game units on the game's global tile grid; the write path
refuses coordinates outside the selected planet's calibrated map bounds, so
an implausible pin is never stored. `kind` and `icon` are user-shaped
presentation vocabulary (the pin palette is user-configured), deliberately
open text rather than a closed set.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | INTEGER | Primary key, autoincrement. |
| `planet` | TEXT | Not null; the bundled map the pin sits on. Indexed (`idx_map_pins_planet`), indexed with `map_view_id` and `created_at` (`idx_map_pins_planet_view`), and indexed with map identity plus coordinates (`idx_map_pins_spatial`) for viewport and proximity reads. |
| `lon` | REAL | Not null; game units (longitude grows eastward). |
| `lat` | REAL | Not null; game units (latitude grows northward). |
| `altitude` | REAL | Optional; carried when the source read included one. |
| `name` | TEXT | Not null. |
| `icon` | TEXT | Not null; the pin's icon identifier. |
| `kind` | TEXT | Not null; the user-shaped pin category. |
| `radius_m` | REAL | Null for an exact point; an area pin's radius in metres. |
| `notes` | TEXT | Optional free text. |
| `session_id` | TEXT | Optional; references `tracking_sessions(id)`, the session the pin was dropped during. |
| `map_view_id` | INTEGER | Optional; references `map_views(id)`. Null means the permanent Default map. |
| `pin_config_id` | INTEGER | Optional; references `pin_configs(id)` (migration `0011`, indexed `idx_map_pins_config`). The palette configuration the pin instantiates: its colour, category, and special behaviour derive from that row, so `name`/`icon`/`radius_m` are a snapshot at drop time. Deleting the configuration deletes its pins (explicit cascade; foreign keys are disabled). |
| `created_at` | REAL | Not null; Unix-epoch seconds. |

#### `pin_configs`

The per-preset pin palette (migration `0011`): each row is a *type* of pin
scoped to one `(planet, map_view_id)` preset, and placed pins are instances of
it. Editing a configuration restyles its placed pins; deleting one removes them.
`category` is `generic` (no behaviour) or `special`; the only special `kind` so
far is `tree`, which carries a distinct on-cooldown colour and is the sole pin
kind the navigation router treats as a route stop.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | INTEGER | Primary key, autoincrement. |
| `planet` | TEXT | Not null; the bundled map this palette entry belongs to. Indexed with `map_view_id` and `ordinal` (`idx_pin_configs_scope`). |
| `map_view_id` | INTEGER | Optional; references `map_views(id)`. Null is the permanent Default map. |
| `label` | TEXT | Not null; the palette label, snapshotted onto placed pins as their name. |
| `category` | TEXT | Not null; `generic` or `special` (checked). |
| `special_kind` | TEXT | Null for generic; `tree` for the special tree kind (checked). |
| `icon` | TEXT | Not null; the palette emoji. |
| `radius_m` | REAL | Null for an exact point; an area radius in metres. |
| `colour` | TEXT | Not null; the generic colour, or a tree's available colour (hex). |
| `cooldown_colour` | TEXT | Null for generic; a tree's on-cooldown colour (hex). |
| `ordinal` | INTEGER | Not null; palette display order within the preset. |
| `created_at` | REAL | Not null; Unix-epoch seconds. |

### Map navigation

#### `navigation_runs`

One persisted traversal over a planet and named map. A partial unique index
(`idx_navigation_one_live_run`) permits at most one `active` or `paused` run.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | INTEGER | Primary key, autoincrement. |
| `planet` | TEXT | Not null; the bundled planet map. |
| `map_view_id` | INTEGER | Optional reference to `map_views(id)`; null selects Default. |
| `status` | TEXT | One of `active`, `paused`, `completed`, or `ended`. |
| `start_lon`, `start_lat` | REAL | The route's fixed starting position. |
| `current_lon`, `current_lat` | REAL | The most recently captured position. |
| `last_position_at` | REAL | Optional Unix-epoch time of the latest manual, hotkey, or harvesting coordinate sample. |
| `hop_count` | INTEGER | Not null; number of stops admitted to the route. |
| `hotkey` | TEXT | Not null; the F6 through F12 position-update key selected for this run. |
| `created_at`, `updated_at` | REAL | Unix-epoch seconds. |

#### `navigation_stops`

The ordered pins selected for one run. The run and pin pair and the run and
ordinal pair are both unique. `idx_navigation_stops_run_status` serves the
active and pending traversal reads.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | INTEGER | Primary key, autoincrement. |
| `run_id`, `pin_id` | INTEGER | Not-null references to the run and map pin. |
| `ordinal` | INTEGER | Stable route order within the run. |
| `status` | TEXT | One of `pending`, `active`, `visited`, or `skipped`. |
| `completed_at` | REAL | Optional Unix-epoch completion time. |
| `completion_source` | TEXT | Optional source such as `manual` or `harvest`. |
| `observed_lon`, `observed_lat` | REAL | Optional captured completion position. |
| `observed_distance` | REAL | Optional Euclidean distance from the pin. |

#### `map_pin_visits`

Append-only visit evidence for manual and harvesting-driven completion.
`idx_map_pin_visits_pin_time` serves newest-first history for a pin.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | INTEGER | Primary key, autoincrement. |
| `pin_id` | INTEGER | Not-null reference to `map_pins(id)`. |
| `run_id` | INTEGER | Optional reference to the navigation run. |
| `visited_at` | REAL | Not-null Unix-epoch time. |
| `source`, `outcome` | TEXT | Not-null operational provenance. |
| `observed_lon`, `observed_lat`, `observed_distance` | REAL | Not-null capture and match evidence. |

#### `radar_calibration`

A singleton physical-screen calibration. The centre and north-edge points
define a circular radar and its northbound bearing frame.

| Column | Type | Notes |
| --- | --- | --- |
| `singleton` | INTEGER | Primary key constrained to `1`. |
| `centre_x`, `centre_y` | INTEGER | Physical-screen centre coordinates. |
| `north_x`, `north_y` | INTEGER | Physical-screen north-edge coordinates. |
| `radius_px` | REAL | Not null and at least eight physical pixels. |
| `display_scale` | REAL | Positive display scale, currently `1.0`. |
| `updated_at` | REAL | Not-null Unix-epoch time. |

### Derived caches

#### `session_summaries`

Per-session aggregates that back the character/prospect path and, since the
read-model work, the Activity and session-list reads as well. This is a derived
cache rather than a source of truth: a row is filled when a session ends and is
lazily rebuilt on read when missing or below the current `summary_version`.

The base columns are created by the baseline; the read columns below the
`computed_at` row are added by migration `0003`, and the harvest columns by
migration `0006`, each healed in by a version bump (the code's
`SUMMARY_VERSION` is 3).

| Column | Type | Notes |
| --- | --- | --- |
| `session_id` | TEXT | Primary key. |
| `summary_version` | INTEGER | Not null; column default 1. Cache-format version for invalidation; the code writes the current `SUMMARY_VERSION` and rebuilds any lower-versioned row on read. |
| `started_at` | REAL | Not null. |
| `ended_at` | REAL | Not null. |
| `duration_hours` | REAL | Not null. |
| `kills` | INTEGER | Not null. |
| `loot_tt` | REAL | Not null. |
| `weapon_cost` | REAL | Not null. |
| `enhancer_cost` | REAL | Not null. |
| `armour_cost` | REAL | Not null. |
| `heal_cost` | REAL | Not null. |
| `dangling_cost` | REAL | Not null. |
| `cycled_ped` | REAL | Not null. |
| `regular_skill_ped_json` | TEXT | Not null; per-skill PED breakdown serialised as JSON. |
| `attribute_levels_json` | TEXT | Not null; per-attribute level breakdown serialised as JSON. |
| `regular_skill_tt` | REAL | Not null. |
| `attribute_levels_total` | REAL | Not null. |
| `dominant_mob` | TEXT | Optional. |
| `dominant_tag` | TEXT | Optional. |
| `dominant_weapon` | TEXT | Optional. |
| `computed_at` | REAL | Not null; defaults to `unixepoch('now')`. |
| `dominant_mob_kills` | INTEGER | Not null; defaults to 0 (migration `0003`). The dominant mob's kill count, summed by the Activity read. |
| `dominant_tag_kills` | INTEGER | Not null; defaults to 0 (migration `0003`). The dominant tag's kill count. |
| `activity_skill_tt` | REAL | Not null; defaults to 0 (migration `0003`). The raw session `SUM(ped_value)` the Activity read uses (distinct from the per-skill `regular_skill_tt`). |
| `primary_mobs_json` | TEXT | Not null; defaults to `'[]'` (migration `0003`). The session-list top-three mob names as a JSON array. |
| `primary_weapons_json` | TEXT | Not null; defaults to `'[]'` (migration `0003`). The session-list top-three weapon names as a JSON array. |
| `globals` | INTEGER | Not null; defaults to 0 (migration `0003`). The session's global count. |
| `hofs` | INTEGER | Not null; defaults to 0 (migration `0003`). The session's Hall-of-Fame count. |
| `harvest_swings` | INTEGER | Defaults to 0 (migration `0006`). Harvesting swings, successes plus explicit fails. |
| `harvest_successes` | INTEGER | Defaults to 0 (migration `0006`). |
| `harvest_loot_tt` | REAL | Defaults to 0 (migration `0006`). Wood loot TT; also included in `loot_tt`. |
| `harvest_cost` | REAL | Defaults to 0 (migration `0006`). Swing decay; also included in `cycled_ped`. |

#### `daily_rollups`

Per-UTC-day sums of every aggregate family the analytics Overview and its
breakdowns read (migration `0004`; see
[ADR-0018](../adr/0018-daily-rollup-read-model.md)). Like `session_summaries`,
this is a rebuildable projection over the raw tracking tables: rows are
maintained eagerly at the write points, healed lazily on read (dirty flag,
version bump, and the watermark walk described under `daily_rollup_meta`), and
regenerate identically from scratch.

Column nullability is load-bearing: each family column stores the raw per-day
`SUM` verbatim, `NULL` when the day had no contributing rows, so aggregates over
rollups reproduce the engine typing the raw queries put on the wire. `has_rows`
carries the one distinction `NULL` sums erase: whether any source rows existed
that day at all, which decides the day's membership in the timeline and monthly
point sets.

| Column | Type | Notes |
| --- | --- | --- |
| `day` | TEXT | Primary key; ISO `YYYY-MM-DD`, UTC. Ledger dates that do not name a canonical calendar day get rows keyed by their literal text. |
| `rollup_version` | INTEGER | Not null. Projection-format version; a below-version row heals on the next read. |
| `dirty` | INTEGER | Not null; defaults to 0. Set inside a writing transaction when a backdated mutation touches the day; the eager recompute clears it, and the next heal repairs a crash between the two. |
| `has_rows` | INTEGER | Not null; defaults to 0. Day-membership bit (see above). |
| `loot_tt` | REAL | Nullable; `SUM(kills.loot_total_ped)` for the day. |
| `weapon_cost` | REAL | Nullable; `SUM(cost_per_shot * shots_fired)` over the day's kill tool stats. |
| `enhancer_cost` | REAL | Nullable; `SUM(kills.enhancer_cost)`. |
| `armour_cost` | REAL | Nullable; `SUM(tracking_sessions.armour_cost)` by session start day. |
| `heal_cost` | REAL | Nullable; `SUM(tracking_sessions.heal_cost)`. |
| `dangling_cost` | REAL | Nullable; `SUM(tracking_sessions.dangling_cost)`. |
| `skill_tt` | REAL | Nullable; `SUM(skill_gains.ped_value)`. |
| `codex_pes` | REAL | Nullable; `SUM(codex_claims.ped_value)`. |
| `quest_pes` | REAL | Nullable; `SUM(quest_claims.ped_value)`. |
| `computed_at` | REAL | Not null; defaults to `unixepoch('now')`. |
| `harvest_loot_tt` | REAL | Nullable (migration `0006`); `SUM(harvest_events.loot_total_ped)`. |
| `harvest_cost` | REAL | Nullable (migration `0006`); `SUM(harvest_events.cost_ped)`. |

#### `daily_ledger_rollups`

The per-day ledger sums by entry type and tag (migration `0004`), normalised so
the window totals, per-day maps, and monthly merges each stay one SQL pass.
Amounts are stored unrounded; rounding stays at response-build time.

| Column | Type | Notes |
| --- | --- | --- |
| `day` | TEXT | Part of the primary key; the `ledger_entries.date` text verbatim. |
| `entry_type` | TEXT | Part of the primary key. |
| `tag` | TEXT | Part of the primary key. |
| `amount` | REAL | Not null; the unrounded per-day sum. |

#### `daily_rollup_meta`

The rollup heal watermark (migration `0004`): a single row whose
`rolled_through` day is the split boundary the reader uses. Every day from the
earliest data day up to and including it has a `daily_rollups` row; healing
advances it to yesterday, so the in-flight day is always served from the raw
tables and every day is re-verified once after it completes.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | INTEGER | Primary key; constrained to the single row `1`. |
| `rolled_through` | TEXT | Not null; the inclusive ISO day the rollups are current through. |

## Migration mechanism

Schema application is handled by the embedded migration runner (`MIGRATIONS`
in `eo-services/src/db/migrate.rs`) over the migration set in
`eo-services/migrations/`, whose files are compiled into the binary (a unit
test pins the embedded chain to the directory's contents). The set carries the
version-33 baseline (`0001_schema_baseline.sql`) followed by forward-only
additions (`0002_analytical_indexes.sql`,
`0003_session_summary_read_columns.sql`, `0004_daily_rollups.sql`,
`0005_market_observations.sql`, `0006_harvest_events.sql`,
`0007_map_pins.sql`, `0008_map_views.sql`, `0009_map_navigation.sql`,
`0010_navigation_runtime_fields.sql`, `0011_pin_configs.sql`,
`0012_harvest_stock_removed.sql`, `0013_harvest_yield_tier.sql`,
`0014_auction_sales.sql`, `0015_stock_movement_tool.sql`,
`0016_stock_opening_balance.sql`, `0017_undone_entries.sql`); the runner
records applied migrations in the `_sqlx_migrations` ledger (the table name,
column shapes, and SHA-384 checksum accounting are inherited unchanged from
the previous runner, so existing databases reconcile byte for byte) and never
runs a down-migration. Applied rows must form a contiguous, checksum-identical
prefix of the embedded chain; any drift refuses loudly before anything
applies. The version the baseline reproduces is pinned by the
`BASELINE_SCHEMA_VERSION` constant (33); the post-baseline migrations extend
the schema in place and do not change that version row.

Migration files are immutable once they can have been applied. A later schema
refinement always receives the next version, including during feature-branch
development against a persistent dogfood database. This preserves checksum
validation as a corruption and provenance guard rather than turning the ledger
into mutable development state.

### The version-33 baseline

The baseline is the schema as it stands at version 33, written out statement for
statement. It creates every table, index, and timestamp-back-fill trigger in one
migration and stamps the `db_metadata` version row to `33`. The version number
is the cumulative result of the schema's earlier evolution; that incremental
history is folded into the single baseline rather than replayed, so a freshly
migrated database lands directly on the current surface.

### Open paths: fresh, adoption, and first-launch upgrade

On open, `Db::open` configures the connection, then calls `adopt_or_refuse`
before running the migration chain. This reconciles the on-disk schema with the
baseline and takes one of these paths:

- **Fresh:** an empty (or absent) database is created and the runner applies
  the baseline directly, landing it at version 33.
- **Adoption:** a database already at version 33 that carries no migration
  ledger is adopted in place. The baseline is marked applied (the ledger row is
  written with the baseline's own checksum) without re-running any DDL, and the
  post-adoption runner pass then validates that row.
- **Native first-launch upgrade:** a database at **version 32**, the version an
  installed v0.1.0-lineage database occupies, is upgraded in place by
  `upgrade_to_baseline` (dropping the retired write-only `tt_curve_observations`
  table and bumping the version row to 33) and then adopted, exactly as a fresh
  version-33 database is. The upgrade and the baseline stamp share one
  transaction, so a failure rolls the file back to exactly as it was found.

A database older than version 32 is declined rather than upgraded
(`DbError::UnsupportedSchemaVersion`): no installed database occupies those
versions, so the earlier upgrade steps are deliberately not carried natively.
The user's file is left untouched on a decline, and the composition root
(`Db::open_adopted`) treats a pre-existing-but-unadoptable file as a quarantine
signal rather than a bare error.

## Bundled game-data snapshot

The game-fact data the application reasons over (weapons, mobs, skills,
professions, and the rest) ships as a snapshot that is **not** stored in SQLite.
`GameDataStore` (`eo-services/src/game_data_store.rs`) loads it once at startup
from per-endpoint JSON files under
`app/src-tauri/entropia-orme/resources/snapshot/` and serves all queries
from memory. Each file is named for its endpoint (the file stem becomes the
endpoint key); most files hold a JSON list, while `skill_ranks` holds a single
object that the store wraps in a one-element list.

The bundled snapshot files are:

| File | Endpoint |
| --- | --- |
| `absorbers.json` | `absorbers` |
| `enhancers.json` | `enhancers` |
| `harvesting_tools.json` | `harvesting_tools` |
| `medical_tools.json` | `medical_tools` |
| `mobs.json` | `mobs` |
| `professions.json` | `professions` |
| `skill_ranks.json` | `skill_ranks` (single object) |
| `skills.json` | `skills` |
| `stimulants.json` | `stimulants` |
| `weapon_amplifiers.json` | `weapon_amplifiers` |
| `weapon_vision_attachments.json` | `weapon_vision_attachments` |
| `weapons.json` | `weapons` |

This JSON snapshot is the read-only, in-memory source of truth for game facts.
It is a maintained static asset that ships with the build and holds no
user-authored data.
