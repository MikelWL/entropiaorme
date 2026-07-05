# Demo typed-command contract prune

Adversarial ratification of the OpenAPI contract prune and demo-golden movements that accompany migrating the guide-mode demo read namespace (`/api/demo/*`) off the in-process HTTP router onto typed Tauri IPC commands (ADR-0019). Re-derived against the current working tree; the change author's rationale is accepted nowhere on trust, because a self-approved contract move carries a structural conflict of interest.

## Change under review

The eight guide-mode demo reads serve over `eo_api::demo` (`eo-api/src/demo.rs`) as eight typed `demo_*` commands on `impl Api`, dispatched by shell commands in `entropia-orme/src/commands.rs`, sharing the live analytics and tracking DTOs. The parallel demo state (a writable clone of the bundled demo database plus a synthetic mid-hunt session) moved from `eo-http/src/demo.rs` into the facade crate and now reuses the same session-list, session-detail, and snapshot computation the live tracking commands use (`list_sessions_impl`, `get_session_impl`, and the now-free `build_snapshot_value` in `eo-api/src/tracking.rs`) plus `eo_services::analytics::AnalyticsService`, so that computation no longer lives in two places. The eight demo routes and the read/response scaffolding they were the last consumer of are deleted from the HTTP layer (`eo-http/src/demo.rs`, `tracking_routes.rs`, `producer_routes.rs`, `analytics_routes.rs`, `native.rs`; `hydration.rs` shrinks to the database handle the shutdown-optimise and dev-maintenance routes still reach through). Byte-parity of the reused computation is pinned by `eo-api/src/demo.rs`'s golden test. The OpenAPI snapshot (`contracts/openapi.snapshot.json`) and the regenerated `frontend/src/lib/api/schema.d.ts` are pruned accordingly. Two contract movements ride along, each matching a movement already ratified for the live surface.

## Oracle ratification audit

Range: `fc89f04..beca2a15` (`refactor/demo-typed-commands`). Every claim re-derived from source: the snapshot parsed as JSON and diffed object-by-object against `HEAD` with the full `$ref` reachability closure computed on both sides; the demo goldens diffed against their pre-move path under `HEAD`; the two governing tests run.

### Findings

**OR1, the snapshot delta is a structurally strict pure deletion: genuine-spec-move.**
Parsed-structure diff (canonicalised deep-equal, not raw lines): exactly eight `/api/demo/*` path objects removed and zero added; exactly twenty-one schemas removed (`AnalyticsActivity`, `AnalyticsOverview`, `InventoryItemModel`, `LedgerItem`, `LedgerPresetItem`, `LootItem`, `LossesBreakdown`, `MobBreakdownRow`, `MobComparison`, `MonthlyPoint`, `ReturnsBreakdown`, `SessionDetail`, `SessionNotableEvent`, `SessionSkillGain`, `SessionSummary`, `TagComparison`, `TimelinePoint`, `ToolStat`, `TrackingCostBreakdown`, `TrackingSession`, `WeaponComparison`) and zero added; zero surviving schema or path operation mutated. The surviving paths are exactly `/api/health` and the `/api/tracking/session/{session_id}` delete orphan. The corresponding live handlers leave the HTTP layer with the deletion, so the routes genuinely move off HTTP rather than being deleted from the contract while still served.

**OR2, no dangling `$ref`, no surviving reference to a removed schema: genuine-spec-move.**
All five `$ref` targets in the pruned snapshot resolve to existing schemas (`HTTPValidationError`, `HealthStatus`, `NotableEvent`, `SessionDeletedResult`, `ValidationError`); none of the twenty-one removed schemas is referenced anywhere in the new document. The prune closed cleanly on both sides.

**OR3, registry-root retention is correct, not a hidden live orphan: genuine-spec-move.**
`eo-wire/src/models.rs::registered_contracts()` is `[HealthStatus, NotableEvent, TrackingSnapshot]`; all three are retained and unmutated. `TrackingSnapshot` survives as a registry root (no longer path-referenced once the demo-snapshot path is removed) and its closure legitimately keeps `NotableEvent`. `eo-wire/tests/openapi_conformance.rs` passes against the pruned snapshot, so the retention (whose full retirement is deferred to a later change that retires the conformance apparatus wholesale) is asserted, not a swept orphan. The reachability roots for this prune are therefore the surviving paths plus the registered contracts, and the removed twenty-one are exactly those orphaned under that root set.

**OR4, schema.d.ts is a clean regen of the same prune: genuine-spec-move.**
The regenerated `frontend/src/lib/api/schema.d.ts` is a deletion-dominant delta with no surviving type mutated and the demo path operations gone, consistent with `npm run gen:api` over the pruned snapshot.

**OR5, the ledger golden is a wrapper-only movement, entries byte-identical: genuine-spec-move.**
`analytics_ledger.txt` moves from a bare array `[...]` to `{"entries":[...],"nextCursor":null}`; the forty entries are byte-identical (the extracted `entries` substring equals the old file exactly). This is the `LedgerPage` cursor-fold ratified for the live surface in the analytics family (`analytics-typed-command-contract-prune.md`): a typed command answers one structured payload, so the keyset cursor travels in the body rather than an `X-Next-Cursor` header. `frontend/src/lib/api/index.ts` `getLedgerEntries` adapts via `page.entries` and `page.nextCursor ?? null`.

**OR6, the snapshot golden's sole delta is the present-null `currentTool` removal, consumer-safe: genuine-spec-move.**
Structural key-diff of `tracking_snapshot.txt` (after normalising the now-relative datetimes and `elapsed`): exactly one key removed (`currentTool`, old value `null`), zero added, zero shared key with a differing value, order otherwise preserved. This is the exclude-unset to exclude-none narrowing that `TrackingSnapshot`'s `skip_serializing_if = "Option::is_none"` DTO entails, ratified for the live snapshot in the tracking family (`tracking-typed-command-contract-prune.md`, OR5). The overlay consumers read `currentTool` by truthiness (`OverlayStrip.svelte` renders it when truthy and a placeholder glyph otherwise) with no key-presence or `=== null` check, and the TypeScript type is optional-and-nullable, so present-null versus absent is inert.

**OR7, determinism holds via an injected clock with the ambient surface normalised: genuine-spec-move.**
The demo golden test (`eo-api/src/demo.rs::demo_reads_reproduce_the_curated_goldens`) injects a `MockClock` fixed at 2026-06-18 12:00:00. Its `normalise` helper masks the clock-relative surface (ISO-8601 datetimes to `<TS>`, `elapsed` to `<ELAPSED>`) on both the produced payload and the golden, and the deterministic values are pinned separately (`elapsed == 754`, `kill_count == 100`, `status == "active"`, the mob and attribution fields). No wall-clock, randomness, or environment input enters the equality. The test passes, so the migrated shared computation reproduces the curated goldens.

**OR8, the other seven demo goldens and the fixture are content-unchanged renames: inconsequential.**
`git` reports similarity index 100% for `analytics_activity`, `analytics_inventory`, `analytics_ledger_presets`, `analytics_overview_30d`, `analytics_overview_all`, `tracking_session_detail`, `tracking_sessions`, and `mid_hunt_fixture.json` (the test input): each is a `git mv` with no content change, so byte-parity of the reused computation is pinned unchanged for those seven surfaces.

**OR9, no unnamed golden in the range: inconsequential.**
A full scan for golden / snapshot / fixture / expected artefacts in the range yields only the named set. `commands.gen.ts` (+32) is generator output for the eight new `demo_*` commands, not an oracle pin; the modified `perf_*_bench.rs` files are source tests, not goldens.

## Summary judgement

A genuine spec move, not a laundered regression. The old pins (the bare ledger array and the null-serialised `currentTool`) were artefacts of the retired HTTP-router serialisation; the purpose of this family is to route the demo reads through the shared live analytics and tracking DTOs, so the two payload movements are the intended, already-live-ratified DTO contracts (the ledger cursor-fold from the analytics family; the exclude-none narrowing from the tracking family), and holding the old pins would contradict the migration. Every element of every golden diff is accounted for and minimal: the OpenAPI prune is provably pure deletion with no dangling or orphaned refs and the registered contracts plus closure correctly retained; `schema.d.ts` is a clean deletion-only regen; the ledger entries are byte-identical under the wrapper; the snapshot delta is a single consumer-safe null-field removal; seven goldens and the fixture are content-unchanged renames. Determinism holds via the injected clock with the ambient surface normalised on both sides. Both governing tests pass. Nothing in the delta is an unaccounted-for regression.

```
ORACLE-RATIFICATION
range: fc89f04..beca2a15 (refactor/demo-typed-commands)
goldens: contracts/openapi.snapshot.json, frontend/src/lib/api/schema.d.ts, frontend/src-tauri/eo-api/resources/demo_goldens/tracking_snapshot.txt, frontend/src-tauri/eo-api/resources/demo_goldens/analytics_ledger.txt
VERDICT: ratification-sound
```
