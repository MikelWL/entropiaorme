# Ratification: analytics HTTP-contract pruning (typed-command migration)

Adversarial review of the OpenAPI contract deletion that accompanies migrating
the analytics route family (the Overview and Activity aggregates + the ledger,
preset, and inventory CRUD) off the in-process HTTP router onto typed Tauri IPC
commands (ADR-0019). The verdict is re-derived against the current tree rather
than accepting the change author's rationale, because a self-approved golden
move carries a structural conflict of interest.

## Change under review

The analytics computation is extracted into `eo_services::analytics::AnalyticsService`
(the domain home alongside `daily_rollup` / `session_summary`), shared by both
the typed `eo-api` facade (`eo-api/src/analytics.rs`, bridging `serde_json::Value`
→ typed DTO via `serde_json::from_value`) and the guide-mode demo surface
(`eo-http/src/analytics_routes.rs`, now thin demo-serving delegators). The real
`/api/analytics/*` route registrations + adapters (`native.rs`), the
`Handler::AnalyticsOverview/AnalyticsActivity` metrics variants, and the
`classify_read_handler` arms are deleted. The ledger list folds its keyset
cursor into the return DTO (`LedgerPage { entries, nextCursor }`). The delete
operations return no body. The `/api/demo/analytics/*` namespace stays (its own
migration is deferred), so the demo read schemas are retained. Behaviour is
pinned by `eo-api/tests/analytics_facade.rs` (byte-for-byte DTO serialisation)
and the extracted `eo-services::analytics` unit tests.

## Oracle ratification audit

Range: `2323aa80..HEAD`. Every claim re-derived from source; the parent's
rationale accepted nowhere on trust.

### Findings

**OR1, snapshot delta is a strict pure deletion: genuine-spec-move.**
Parsed-structure diff (not raw lines): exactly the 9 `/api/analytics/*`
non-demo paths removed, zero added; exactly `DeletedStatus`,
`InventoryItemCreate`, `InventoryItemPatch`, `InventoryItemSell`,
`InventorySellResult`, `LedgerEntryCreate`, `LedgerPresetCreate` (7) removed,
zero added; every surviving path object and schema deep-equal (canonicalised)
between parent and working tree: zero altered survivors; the top-level document
minus `paths` / `components.schemas` is byte-identical after canonicalisation.
The `0 777` line-diff is Myers re-anchoring only; the structure is a strict
in-order subsequence.

**OR2, orphan reachability is exact and analytics-caused: clean.**
In the parent, each of the 7 removed schemas was referenced only by removed
`/api/analytics/*` paths and by no surviving schema (`DeletedStatus` by exactly
the 3 analytics delete legs). In the pruned document each removed schema has 0
surviving `$ref`; the read schemas the demo GETs consume (`AnalyticsOverview`,
`AnalyticsActivity`, `LedgerItem`, `LedgerPresetItem`, `InventoryItemModel`,
`ReturnsBreakdown`, `LossesBreakdown`, `TimelinePoint`, `MonthlyPoint`) each
retain exactly one ref (the surviving `/api/demo/analytics/*` paths). Zero
dangling refs, zero orphan schemas. Prune removed only write-request/response +
delete-status schemas with no demo consumer.

**OR3, f64 zero-coercion movement (rides this ratification, no golden of its own): genuine-spec-move.**
The load-bearing judgement, verified at both layers. The raw service `Value`
keeps integer zeros (`cycledBreakdown:{"weapon":0,...}` from empty
`COALESCE(SUM,0)`, engine behaviour unchanged, pinned by
`eo-services/src/analytics.rs::empty_overview_emits_the_engine_typed_zeros`);
the `f64` DTO coerces on the empty-window edge only
(`{"weapon":0.0,...}`, pinned by
`eo-api/tests/analytics_facade.rs::the_empty_overview_serialises_to_the_float_typed_zeros`).
Decisive contract evidence: in the snapshot `LossesBreakdown.cycledBreakdown`
is an untyped `Any` slot (no `"type"`, rendered `unknown` in `schema.d.ts`), so
the `0`→`0.0` change sits inside an already-`Any`/`unknown` field: requiring no
golden edit and invisible to the JS consumer (both parse to `number`). The
divergence is confined to the no-sessions-in-window case; a populated
`COALESCE(SUM,...)` returns a real, so populated windows are byte-identical,
confirmed against the demo golden `analytics_overview_all.txt`
(`cycledBreakdown:{"weapon":3696.74,...}`, no bare integers). A genuine, benign
contract simplification (uniform float rendering, retiring the pydantic-`Any`
integer-literal leak), not a laundered regression.

**OR4, retired transport envelopes are genuinely unrepresentable: clean.**
The create/patch 422 field-validation + `binding_taint` deferred-500 were
pre-service HTTP request-build errors (malformed body / encoding depth), rejected
by Tauri IPC before dispatch over typed args: unrepresentable, not dropped. The
`{"status":"deleted"}` / `DeletedStatus` body is referenced by no surviving path
and no code reads it (the frontend delete wrappers return `void`): retirement
with no consumer. ETag/conditional-GET: analytics was explicitly outside the
ETag middleware prefixes (only `/api/tracking`, `/api/scan`): nothing to retire.

**OR5, error-class mapping preserved, no silent reclassification: clean.**
Facade vs the deleted handlers at `2323aa80`, verbatim: invalid cursor →
`bad_request("Invalid cursor")`; preset type →
`bad_request("type must be 'expense' or 'markup'")`; entry / preset / inventory
not-found → `not_found` with the identical messages; driver / rollup → the
framework-500-equivalent `ApiError::Internal`. Every 400 stays 400, every 404
stays 404, same messages. A sound adapt of a legitimately-obsolete pin.

**OR6, no corpus / frozen-golden interplay: clean.**
Analytics is absent from the replay `endpoint_table`
(`http_consistency_replay.rs`) and the frozen curated set (`emitters_proof.rs`);
`git status` shows zero `fixtures/corpus/**` byte changes. No symbol-table /
cardinality interplay for this family.

**OR7, determinism: clean.**
The golden delta is pure static-schema deletion; nothing ambient added. The
facade pin uses `RealClock`, but over an empty DB the `"all"` window has no
sessions, so the emitted bytes are wall-clock-invariant (all zeros); the
eo-services twin uses a fixed epoch. No ambient input leaks into any pin.

## Summary judgement

The snapshot delta is a structurally strict pure deletion (9 analytics paths + 7
exactly-orphaned schemas removed, zero added, zero survivor mutated, top-level
identical); `schema.d.ts` is a clean regen of the same prune (0 added lines, only
analytics operations/schemas removed). Orphan reachability is exact and caused by
this prune alone, the demo read-schemas correctly retained. The one riding
behaviour change (empty-window `cycledBreakdown` rendering `0.0` instead of `0`)
is a genuine, consumer-invisible contract simplification landing inside an
already-`Any`/`unknown` field. Retired transport envelopes are genuinely
unrepresentable or had no consumer; every error class maps verbatim; no
corpus/frozen/ETag interplay exists; no ambient input enters any pin. Nothing in
the delta is unaccounted for. This is a genuine spec move.

```
ORACLE-RATIFICATION
range: 2323aa80..HEAD
goldens: contracts/openapi.snapshot.json, frontend/src/lib/api/schema.d.ts
VERDICT: ratification-sound
```
