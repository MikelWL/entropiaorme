# Ratification: tracking HTTP-contract pruning (typed-command migration)

Adversarial review of the OpenAPI contract deletion and consistency-replay retirement that accompany migrating the tracking route family (the session list and detail reads, the consolidated snapshot, the tag and manual-mob suggestion lookups, the quest-link suggestion, the start/stop lifecycle, the release-mob and tag/manual-mob locks, the mob rename/restore and loot-item activate/deactivate edits, the armour-cost write, the quest-link decision, and the repair-scan read) off the in-process HTTP router onto typed Tauri IPC commands (ADR-0019). Re-derived against the current working tree; the change author's rationale is accepted nowhere on trust, because a self-approved contract move carries a structural conflict of interest.

## Change under review

The tracking family serves over `eo_api::tracking` (`eo-api/src/tracking.rs`) as typed DTOs on `impl Api`, dispatched by shell commands in `entropia-orme/src/commands.rs`. Byte-parity is pinned inline in `eo-api/tests/tracking_facade.rs`. The seventeen live `/api/tracking/*` route operations and their adapters are deleted from the HTTP router; the guide-mode demo reads (`/api/demo/tracking/*`) stay on the router and continue to reuse the shared read computation. The OpenAPI snapshot (`contracts/openapi.snapshot.json`) and the regenerated `frontend/src/lib/api/schema.d.ts` are pruned accordingly. The four tracking reads were the entire remaining live-replay set, so `eo-http/tests/http_consistency_replay.rs` retires wholesale; the curated corpus goldens stay frozen on disk (still exercised as frozen bytes by `eo-wire/tests/emitters_proof.rs`, which never builds the live router). One ratified contract movement rides along: the snapshot and quest-link-decision replies drop a present-null polymorphic field rather than serialise it null (the exclude-unset to exclude-none narrowing a single skip-none DTO entails).

## Oracle ratification audit

Range: working tree (`refactor/tracking-typed-commands`, uncommitted). Every claim re-derived from source: the snapshot parsed as JSON and diffed object-by-object against `HEAD`, with the full `$ref` reachability closure computed on both sides.

### Findings

**OR1, snapshot delta is a structurally strict pure deletion: genuine-spec-move.**
Parsed-structure diff (canonicalised deep-equal, not raw lines): exactly seventeen `/api/tracking/*` path objects removed, plus the `get` operation removed from `/api/tracking/session/{session_id}`; zero paths or operations added; zero surviving path object altered except that one. The `git` numstat "85 insertions" is Myers re-anchoring of survivor schemas around the deletion boundaries, not semantic additions: the histogram diff and the object-level equality both confirm zero added content and every survivor byte-identical to `HEAD`. The corresponding live HTTP handlers are removed from `eo-http/src/tracking_routes.rs` / `producer_routes.rs`, so the routes genuinely move off HTTP rather than being deleted from the contract while still served.

**OR2, orphan reachability is exact and tracking-caused; retentions correct: genuine-spec-move.**
Reachability closure: `HEAD` = 45 schemas, all reachable, 0 orphans; working tree = 27 schemas, all reachable, 0 orphans. The eighteen removed schemas (`ArmourCostBody`, `ArmourCostResult`, `LootItemEditResult`, `ManualMobLockRequest`, `ManualMobLockResult`, `ManualMobSuggestion`, `MobEditResult`, `QuestLinkDecisionResult`, `QuestLinkSuggestion`, `ReleaseMobResult`, `RenameMobRequest`, `RepairScanResult`, `RestoreMobRequest`, `SessionQuestLinkDecisionBody`, `TagLockRequest`, `TagLockResult`, `TrackingStartResult`, `TrackingStopResult`) each have zero surviving `$ref`, and no dangling `$ref` exists on either side. The shared response schemas the demo reads still reference (`TrackingSession`, `SessionDetail`, `TrackingSnapshot`, `ReturnsBreakdown`, and their transitive members) are correctly retained by the closure. `TrifectaAttribution` and `RecentEvent` are inlined shapes in the snapshot, never named components, so their absence from the delta is correct.

**OR3, schema.d.ts is a clean regen of the same prune: genuine-spec-move.**
The regenerated `frontend/src/lib/api/schema.d.ts` is `gen:api` re-run over the pruned snapshot: a deletion-dominant delta with no surviving type mutated. The removed tracking request/response type names are confirmed absent; the frontend type-checks clean against it.

**OR4, the delete-kept / get-removed split on the session path is correct: genuine-spec-move.**
`/api/tracking/session/{session_id}` retains only `delete`, byte-identical to `HEAD` (the sole structural effect is the necessary loss of the trailing comma when `get` ceased to be a sibling). The delete operation is a live HTTP orphan with no typed-command replacement: `frontend/src/lib/api/index.ts` `deleteSession` still calls `client.DELETE('/api/tracking/session/{session_id}')` over the HTTP client, and `schema.d.ts` keeps the operation. Retaining its OpenAPI path is therefore correct and deliberate; migrating it is out of this family's scope.

**OR5, the exclude-unset to exclude-none movement is sound and consumer-safe: genuine-spec-move.**
The polymorphic snapshot and quest-link-decision replies were served with a projection that kept a field explicitly set to null on the wire; the typed DTOs (`TrackingSnapshot`, `QuestLinkDecision`) are single structs whose optional members skip when `None`, which cannot distinguish a present-null from an absent key, so a present-null polymorphic field is now dropped rather than serialised null. Pinned by `eo-api/tests/tracking_facade.rs::the_idle_snapshot_serialises_the_dashboard_way`: the idle wire string drops the present-null polymorphic fields (`currentMob` / `mobSource` / `currentTool` absent) while the fixed-shape `TrifectaAttribution` keeps `"smallWeapon":null,"bigWeapon":null,"healTool":null` and `SessionDetail`'s `originalName` keeps its present-null. Consumer safety re-derived at the dashboard: the snapshot type members are all optional-and-nullable and every consumer reads them by truthiness (`!data.currentMob`, a `currentTool || fallback` default, `data.weaponAttribution === 'trifecta'`), with no `=== null` or key-presence check that could distinguish present-null from absent. The fixed-shape `SessionQuestLinkSuggestion` (seven always-present fields) keeps its nulls, as does `SessionDetail`'s nullable members, so the narrowing is confined to the two genuinely polymorphic replies.

**OR6, consistency-replay retirement leaves every frozen golden byte-unchanged: genuine-spec-move.**
The four tracking reads were the whole live-replay `endpoint_table`, so retiring the family empties it and `http_consistency_replay.rs` retires wholesale (with its cardinality guard). No golden file under `fixtures/` is edited or deleted: the curated tracking goldens stay frozen on disk, and `emitters_proof.rs` still exercises them as frozen captured bytes through `eo_wire::http_fingerprint`, never building the live router, so deleting the live routes is decoupled from it (that file is unmodified and still passes).

**OR7, coverage observation on the quest-link-decision narrowing: inconsequential.**
The idle-snapshot byte pin covers the snapshot half of the exclude-none narrowing; the quest-link-decision reply (the `decline` branch, which drops the accept-only link fields) is not separately byte-pinned. This does not affect the snapshot delta (the `QuestLinkDecisionResult` schema removal is correct regardless) and the DTO uses the identical skip-when-`None` mechanism the snapshot pin already exercises, so it is a test-coverage observation, not a contract movement. A follow-up may add an explicit present-null byte pin for the quest-link-decision reply; not required here.

**OR8, determinism: clean.**
The golden is a static schema document with no ambient input. The supporting byte pin fixes `recentEvents` to the empty array (no timestamps) and injects the wall clock through `duration_seconds(..., now)`, so no clock, randomness, environment, or timing value enters any pinned byte.

## Summary judgement

The snapshot delta is a structurally strict pure deletion: exactly seventeen tracking path retirements plus one get-only removal, plus exactly the eighteen schemas those retirements orphan, with a mathematically clean reachability closure (0 orphans, 0 dangling refs) and every survivor byte-identical to `HEAD`; the numstat additions are Myers re-anchoring of unaltered survivors, histogram-confirmed. `schema.d.ts` is a clean regen of the same prune. The demo reads correctly retain the shared response schemas; the delete-only session path is correctly kept for its still-live HTTP caller. The single behavioural movement riding along, exclude-unset to exclude-none on the two polymorphic replies, is byte-pinned on the snapshot and consumer-safe against all-optional, defensively-read frontend types. The consistency-replay retirement changed no golden byte, and the frozen fingerprint evidence stays decoupled. Nothing in the delta is an unaccounted-for regression.

```
ORACLE-RATIFICATION
range: worktree (refactor/tracking-typed-commands, uncommitted)
goldens: contracts/openapi.snapshot.json, frontend/src/lib/api/schema.d.ts, eo-http/tests/http_consistency_replay.rs, eo-api/tests/tracking_facade.rs
VERDICT: ratification-sound
```
