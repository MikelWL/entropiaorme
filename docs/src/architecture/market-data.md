# Market data pipeline

The market layer tracks what loot items actually trade for, without ever
letting an estimate masquerade as measured profit. It has two halves: a
local feed every installation owns, and an optional shared service that
pools observations between installations. Both land in the same
quarantined informational layer; the accounting boundary they sit behind
is [ADR-0024](../adr/0024-market-informational-layer.md), and the shared
service's architecture is
[ADR-0025](../adr/0025-central-market-data-service.md).

## The local feed

The game builds a market ledger in-game and copies it to the clipboard
as a tab-separated table: one row per item, five time horizons (day,
week, month, year, decade), each with a markup percentage and a sales
volume. The user pastes that export into the Market page, reviews the
parse, and commits it.

- `eo-services/src/market_paste.rs` parses the paste fail-soft: rows it
  cannot read are skipped and reported, a no-sales window (`N/A`)
  becomes a null markup rather than a zero, and sales volumes normalise
  to PED. The committed result is one `market_submissions` row and one
  `market_observations` row per item and horizon (migration
  `0005_market_observations.sql`; column detail in the
  [schema reference](database-schema.md)).
- `eo-services/src/market_service.rs` serves the reads: the tracked-item
  overview with staleness, per-item history, the modelled break-even
  readout, and a per-species ranking that TT-weights each species'
  recorded loot composition by the latest markup observations. The
  ranking reads the accounting tables directly; the reverse direction is
  forbidden (see the boundary below).
- `eo-api/src/market.rs` is the typed command family over those reads,
  consumed by the Market page (`app/src/lib/features/market/`).

## The shared service

The optional backend,
[market-data-service](https://github.com/entropiaorme/market-data-service),
pools observations between installations: authenticated submissions land
in an ingest API, a scheduled aggregation folds them per item and
horizon, and every client fetches the same versioned snapshot JSON from
a CDN-fronted bucket. Contracts are versioned JSON Schemas served at the
URLs their `$id` fields declare. The full pipeline and its rationale
live in [ADR-0025](../adr/0025-central-market-data-service.md).

Client-side, the flow is deliberately consent-shaped
(`app/src/lib/marketData.svelte.ts`):

- **Consuming** the shared snapshot is a first-run consent choice; when
  enabled, `app/src/lib/marketDataFetch.ts` fetches the published
  `latest.json` with ETag revalidation and caches it in preferences.
- **Contributing** is a second, independent, default-off opt-in in
  Settings that also requires a contributor token, and each send is an
  explicit button press sharing exactly the latest accepted paste;
  nothing uploads automatically.
- All of it routes through the app's one hardened outbound HTTP gateway
  (`app/src/lib/outboundHttp.ts`), and the service domain is pinned in
  the CSP `connect-src` beside the news feed. The app still fetches no
  game data at runtime; market observations always enter by a user's
  paste, locally or via another contributor's.

## The accounting boundary

Estimated markup is an informational data class. It never joins the
ledger, the analytics aggregates, or any realised rate; markup becomes
profit-and-loss only when a confirmed sale is entered in the ledger. The
boundary is one-directional (the market layer may read accounting data;
the accounting surfaces may never reference the market layer) and
mechanically enforced by the `market-isolation` CI guard
(`app/src-tauri/xtask/src/market_isolation.rs`), which scans down to raw
SQL so a table reference cannot slip past the module seam. Any future
markup display follows the same recipe: compute on the market side, join
in the frontend, label as an estimate.
