# ADR-0024: Estimated market data as a quarantined informational layer

- Status: Accepted
- Context: the app's accounting rests on three classes that never blur: liquid trade-terminal value, the manually confirmed ledger, and non-liquid skill progress. Market markup estimates are a fourth kind of number: genuinely useful for direction, structurally untrustworthy as personal profit-and-loss.

## Context and problem statement

Knowing an item's typical auction markup answers real questions: whether an activity's loot composition trends above or below its sustainability bar, which of two hunts is on a markup upswing, whether a stack is worth listing rather than vendoring. The game exposes this data through a market-ledger export the player builds in-game and copies to the clipboard.

The danger is not collecting the data; it is what an analytics surface does with it. An estimated markup folded into a return rate turns a measured figure into a modelled one without changing its label: a losing week reads as profitable because a database said a drop "typically trades at 350%". The app's discipline is that unrealised profit is not profit; markup is booked only when a sale is confirmed and entered in the ledger. Any market feature therefore has to add value without giving estimates a path into realised figures.

## Decision

Market data enters the app as a manually fed, structurally separate **informational layer**:

- **Manual paste, no runtime fetching of game data.** The user pastes the game's market-ledger export into the Market page; `eo-services/src/market_paste.rs` parses it fail-soft (tab-separated primary, whitespace fallback; a no-sales window parses to null, never zero) and migration `0005_market_observations.sql` stores per-item observations across the export's five time horizons, with sales volume kept as a liquidity signal alongside each markup figure.
- **Reads stay on the market surface.** `eo-services/src/market_service.rs` serves the overview, per-item history, the modelled break-even readout, and the per-species ranking that TT-weights recorded loot compositions by latest observed markups. Every figure derived from an observation is presented as an estimate, never folded into a realised rate.
- **The boundary is mechanical, one-directional, and CI-bound.** The `market-isolation` guard (`app/src-tauri/xtask/src/market_isolation.rs`) forbids the accounting surfaces (the analytics aggregates, the cost engine, the rollups, the tracker's session accounting, the ledger, the quest reward accounting) from referencing the market modules or tables, down to raw SQL. The market layer may read accounting data; the accounting layer may never read estimated markup.

## Consequences

The app can say "this activity trends two points short of sustainable" while its profit-and-loss stays measured truth, and the two claims cannot contaminate each other by accident: a future change that tries to consume an estimate on an accounting surface fails CI rather than review. The cost is a little duplication at the seams (the market layer re-derives loot compositions through its own reads), accepted as the price of the wall.

The layer also fixes the integration recipe for any future markup-adjacent display: compute on the market side, join in the frontend, label as an estimate. Confirmed sales continue to enter the ledger exactly as before; nothing about realised markup accounting changed.

See [ADR-0018](0018-daily-rollup-read-model.md) for the accounting read models this layer is walled off from, [ADR-0025](0025-central-market-data-service.md) for the optional shared backend built on this foundation, and the [ADR index](index.md).
