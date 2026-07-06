# ADR-0021: A synchronous database core, adopted against a measured null result

- Status: Accepted
- Context: with the HTTP transport retired ([ADR-0019](0019-typed-command-facade.md)) and both stateful services re-founded as single-owner actors ([ADR-0020](0020-tracker-actor-refounding.md)), the async database driver was the last piece of the persistence stack whose shape was inherited rather than chosen. The writer/reader topology it served ([ADR-0007](0007-sqlite-wal.md)) had already been settled on measurement.

## Context and problem statement

For an embedded, single-user SQLite database inside a desktop application, a synchronous driver on dedicated threads is the field's natural default: SQLite is a synchronous C library, and async buys concurrency over waiting that a local file rarely offers. This codebase instead carried an async driver as an artefact of its porting history, with every query hop passing through an executor. Two questions needed answers before v0.3.0: does the async core cost anything real at this workload, and should the actors' call-semantics messaging decouple producers from persistence latency (the question ADR-0020 deliberately left open)?

Both questions were put to measurement before any decision, on committed, re-runnable instruments over a copy of a real 456 MiB database: a producer-blocking event-path bench (the exact bus-event stream a long hunt produces, republished against the full consumer set) and a fixture-backed read bench through the typed facade.

**The measurements found no performance case for changing anything.** The event pipeline absorbed roughly 8,000 events per second sustained where real gameplay produces perhaps ten; producer blocking at a paced realistic rate was below one millisecond at the 95th percentile against a chat-log watcher that polls every 100 ms; loot persistence cost around 0.3 ms per event inline. On latency evidence alone, the honest verdict was "no change, measured and recorded".

## Decision

Adopt the synchronous core anyway, as a deliberate design decision made with the null result on the table: the platform-native shape for an embedded database is worth having when its cost is proven to be nothing. Concretely:

- **One writer thread and a small pool of reader threads**, each owning its own rusqlite connection with the established session configuration. Exclusive access is the owning thread: there is no pool checkout, no lock order, and no executor between a caller and SQLite. The ADR-0007 topology (one serialised writer, concurrent WAL readers, 64 MB page cache) is preserved exactly, re-expressed in threads.
- **Callers submit closures** and await a reply (or block, on plain producer threads that have no async context, which deleted the last runtime-handle bridges). A multi-statement transaction is a single closure, so it can never be interleaved or left half-open across an await point.
- **The projection write-hooks take a bare connection reference**, so a caller's transaction passes through them by deref and a raw write commits atomically with its projection refresh, structurally.
- **An embedded migration runner** replaces the driver's, inheriting the on-disk ledger byte for byte: the same table, the same version and description derivation, the same SHA-384-over-file-bytes checksums, validated as a contiguous prefix of the embedded chain on every open. Every database in the wild reconciles without noticing the driver changed.
- **Call semantics stay.** The mailbox question closed with the same measurement: producers wait on sub-millisecond absorbs against a 100 ms poll interval, so a free-running mailbox would rescue a wait nobody experiences at the price of the replay-determinism contract the frozen fingerprints pin.

## Consequences

The swap was proven behaviour-invariant by the strongest oracle the codebase has: every golden set (replay-corpus fingerprints and database state, wire JSON, typed-command byte-parity pins, the demo body) reproduced unchanged across every increment of the landing, with zero regenerations, on the full suite.

The matched after-measurement told a better story than the null hypothesis predicted. The event path sat within noise of the baseline on every row, as expected. The multi-statement read flows got materially faster: the Activity aggregate moved from about 19 ms to about 5 ms at the median and the Overview windows roughly halved, because a read flow that was previously a dozen awaited round-trips through an executor and a connection pool is now one closure running to completion on one connection. The per-event write path and the throughput ceiling were unchanged. These gains were not the reason for the swap and would not alone have justified it; they are recorded as its measured price, which turned out to be negative.

Two smaller improvements rode along: the startup corruption probe now runs on a throwaway read-only connection with an interrupt handle, so its time budget genuinely cancels the scan rather than abandoning a still-running query; and the query-latency metric observes the core's job execution directly instead of the retired driver's statement events, with an unchanged metric surface.

What was given up: compile-time-checked query macros are no longer reachable (the option existed and was deliberately unused), and any future move to an out-of-process or networked store would reopen the async question on its own evidence.

See [ADR-0007](0007-sqlite-wal.md) for the WAL topology this core re-expresses, [ADR-0018](0018-daily-rollup-read-model.md) for the read models it serves, [ADR-0020](0020-tracker-actor-refounding.md) for the actors that own it, and the [ADR index](index.md).

## Evidence

- `frontend/src-tauri/eo-services/src/db/` (the core: `mod.rs`, `pool.rs`, `migrate.rs`)
- `frontend/src-tauri/eo-services/tests/event_path_bench.rs` (the producer-blocking instrument; before and after legs)
- `frontend/src-tauri/eo-api/tests/facade_fixture_bench.rs` (the read-path instrument; before and after legs)
- `frontend/src-tauri/eo-services/tests/corpus_replay_oracle.rs` (the byte-frozen invariance floor the swap held against)
