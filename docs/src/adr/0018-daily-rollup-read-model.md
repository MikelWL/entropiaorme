# ADR-0018: Daily-rollup read model for the analytics Overview

- Status: Accepted
- Context: the analytics Overview recomputed every aggregate from the raw tracking tables on each request. On a real-world database approaching half a gigabyte that read took over ten seconds, and the analytical indexes ([ADR-0007](0007-sqlite-wal.md) covers the storage base; migration `0002` added the indexes) were measured to leave that floor intact: the grouped and windowed breakdown passes re-scan history by construction, so the cost is architectural, not a planner problem.

## Context and problem statement

The Overview answers day-granular questions: totals per named period, a per-day timeline, a monthly breakdown, and two trend windows. Every one of those was computed by aggregating raw rows (kills, tool stats, sessions, skill gains, codex and quest claims, ledger entries) at request time, making the read O(all recorded history) forever. Indexes help exactly where a predicate is selective; the all-time period has no predicate, and the per-day grouping passes visit every row regardless. A database that grows with play therefore degrades linearly, and the measured cost had already passed the ten-second mark on real usage.

The codebase already carried the answer in miniature: `session_summaries` materialises per-session aggregates once at session end, heals lazily on read behind a version stamp, and the reads that adopted it dropped from re-aggregating a million rows to reading a few hundred. What was missing was the same discipline at the granularity the Overview actually consumes: days.

## Decision

Materialise a per-UTC-day rollup of every aggregate family the Overview and its breakdowns read (`daily_rollups`, with `daily_ledger_rollups` carrying the per-day ledger sums by entry type and tag), and serve the Overview from it. The projection follows and generalises the `session_summaries` discipline:

- **The raw tables remain the source of truth.** Rollups are a rebuildable projection: deleting every rollup row and healing regenerates identical content, and a single recompute implementation is shared by the eager write-path maintenance, the lazy heal, and the rebuild.
- **A watermark is the single split boundary.** A one-row `daily_rollup_meta.rolled_through` day marks how far the projection is current. Healing advances it to yesterday, so the in-flight day is always served from the raw tables, and every day is re-verified once after it completes. Reads aggregate whole days at or below the watermark from the projection and touch the raw tables only for the partial edge days of now-relative windows and the un-rolled tail.
- **Writes maintain their own days.** Every path that mutates an aggregate family refreshes the affected day inside its writing transaction; a mutation dated past the watermark is a no-op, so live tracking pays nothing. A dirty flag written with the raw rows survives a crash between the write and its recompute, and the next heal repairs it.
- **Byte fidelity is a design constraint, not an aspiration.** Family columns store each day's `SUM` verbatim (`NULL` preserved) alongside a row-membership bit, so the `NULL`-versus-zero engine typing SQLite puts on the wire survives the rewire, and ledger date strings that do not name a canonical calendar day keep their own buckets. The rewired reads reproduce the raw-only responses byte for byte; no golden moved.

## Consequences

The Overview scales O(days), not O(rows): measured on a copy of a real 456 MiB database, the all-time read went from ~11.4 s to 74 ms median, with every period under 300 ms warm. The one-time backfill on the first read after upgrade walks the database's history once (~3 s on that copy) and never recurs.

The projection adds a maintenance surface: every future write path that touches an aggregate family must refresh its days. Three mechanisms bound the blast radius of a missed hook: the current day is never served from the projection, every day is recomputed once after it completes, and the version stamp forces a full lazy rebuild whenever the rollup format changes. The rebuild path doubles as the standing proof that the projection stays a pure function of the raw tables.

This record also fixes the direction for the remaining scalability work: read models over SQLite, not a second analytics engine. Responses being cheap removes any case for an HTTP response cache, and later read-side work (connection topology, list pagination, further consolidation) builds on this projection rather than revisiting the decision.

See [ADR-0007](0007-sqlite-wal.md) for the WAL storage base, [ADR-0010](0010-loose-response-models.md) for the read-model posture on the response shapes, [ADR-0017](0017-behavioural-contract-ownership.md) for the golden contract this change held to zero movement, and the [ADR index](index.md).

## Evidence

- `frontend/src-tauri/eo-services/migrations/0004_daily_rollups.sql`
- `frontend/src-tauri/eo-services/src/daily_rollup.rs`
- `frontend/src-tauri/eo-http/src/analytics_routes.rs`
- `frontend/src-tauri/eo-http/tests/perf_fixture_bench.rs`
