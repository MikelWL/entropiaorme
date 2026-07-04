# Ratification: settings HTTP-contract pruning (typed-command migration)

Adversarial review of the OpenAPI contract deletion that accompanies migrating
the settings route family off the in-process HTTP router onto typed Tauri IPC
commands (ADR-0019). The verdict is re-derived against the current tree rather
than accepting the change author's rationale, because a self-approved golden
move carries a structural conflict of interest.

## Change under review

The settings family is now served by four typed commands
(`entropia-orme/src/commands.rs`, registered in `entropia-orme/src/lib.rs`
`generate_handler!` as `settings_get`, `settings_overlay_position`,
`settings_set_overlay_position`, `settings_update`) dispatching into the
`eo-api` facade (`eo-api/src/settings.rs`). The family's HTTP read handlers
(`eo-http/src/settings_routes.rs`), its overlay and partial-update write
adapters (`native.rs`), the producer-spine write methods and chat-log-path
validation (`producer_routes.rs`), and their route registrations are deleted,
along with the now-dead chat-log-watcher slot on the HTTP app state. The
migrated family's contract lives in Rust DTOs with generated TypeScript
(`frontend/src/lib/api/commands.gen.ts`) narrowing onto the hand-written
`$lib/types/settings` interfaces, and its behaviour is pinned by
`eo-api/tests/settings_facade.rs`, including a byte-for-byte
transport-invariance pin on the overlay-position read.

## Oracle delta reviewed

Exactly two files move, both pure deletion (`git diff HEAD` = `0 578` and
`0 324`; zero added lines in either).

- `frontend/src-tauri/contracts/openapi.snapshot.json`: the removed path-key
  set is exactly `/api/settings`, `/api/settings/overlay-position`,
  `/api/settings/reset`, and the removed component set is exactly the seven
  types reachable only from them (`AppSettings`, `GameConnection`,
  `OverlayPosition`, `OverlayPositionPatch`, `SettingsPatch`, `TrifectaPreset`,
  `TrifectaSettings`). The prune was computed by reachability closure
  (`components reachable from settings paths` minus `components reachable from
  every other path`), so no component still reachable from a surviving path was
  touched. Post-prune the snapshot holds zero `$ref` to any removed component
  and zero occurrences of the string `settings`: no surviving path is orphaned
  of a component and no dangling reference is introduced. There were zero
  pre-existing orphans before the prune.
- `frontend/src/lib/api/schema.d.ts`: the faithful `npm run gen:api`
  regeneration. The only path keys and named types deleted are the three
  settings paths and the settings-scoped `operations`/`components` types; zero
  non-settings `/api/…` paths touched; zero remaining references to any removed
  type.

## Findings

- **Delta accountability + minimality: genuine-spec-move.** Every deleted path
  resolves to a typed command that exists and is registered; every deleted
  component was reachable only from a deleted path. Pure deletion, zero semantic
  insertions, no surviving path/component/ordering altered, no other family
  moved.
- **No dangling references.** Zero `$ref` to any of the seven removed components
  survives, and no removed component name survives even as a substring, so no
  surviving path or schema points at a retired model.
- **Conformance gate: unaffected.** `eo-wire::models::registered_contracts()`
  registers only the spine models (`HealthStatus`, `NotableEvent`,
  `TrackingSnapshot`); no settings model was ever registered, so deleting these
  unregistered components neither breaks `openapi_conformance.rs` nor removes
  protection it provided. The gate stays green (verified: 2 passed).
- **Reset retirement: genuine-spec-move.** `POST /api/settings/reset` has zero
  callers (`resetSettings`, `settings/reset`, `reset_settings` all absent from
  `frontend/src`) and no behaviour test; it retires unconverted with no caller
  to strand, mirroring the sanctioned character-`codex` / equipment
  `cost/calculate` precedent under ADR-0019.
- **Reads' byte-parity: substantiated.** The `AppSettings` DTO field order
  reproduces the removed schema's wire order field-for-field;
  `settings_facade.rs` pins that camelCase key order, the nested
  `gameConnection`/`trifecta` orders, the hotbar stored slot order `1..9,0`
  (`preserve_order`, not sorted), and the overlay read byte-exact at
  `{"x":null,"y":null}` (the HTTP-era bytes).
- **Retired 422/500 envelopes: framework-shape retirements, not dropped rules.**
  What retires is only framework-level shape/type validation: the typed
  signatures (`settings_set_overlay_position(x: i64, y: i64)`, the all-`Option`
  `SettingsPatch` with per-field serde types) reject a non-integer coordinate, a
  beyond-`i64` value, a surrogate-tainted string, and a structurally-malformed
  `hotbar`/`trifecta_presets` container at the deserialiser boundary rather than
  emitting a pydantic `HTTPValidationError`/`500`. The business validation
  ladder is fully ported and pinned (empty-patch 400, the three-step chat-log
  path chain, the mob-mode gate), and present/absent + present-null/absent
  semantics are preserved (the `double_option` on `active_trifecta_preset_id`).
  Every surviving operation routes through typed `invokeCommand`, so the retired
  envelopes are never emitted and no consumer depends on their detail body.
- **Determinism: clean.** Both goldens are pure deletions against an existing
  baseline; no new expected output is pinned, so no ambient input can have
  leaked in.

No suspected regressions swept into the goldens and no non-deterministic pins.
The delta is a structurally-proven pure deletion of a migrated family's HTTP
contract description, with the retired capability caller-free and the new
surface covered by its own typed tests.

## Verdict

```text
ORACLE-RATIFICATION
range: ec095cf..HEAD
goldens: frontend/src-tauri/contracts/openapi.snapshot.json, frontend/src/lib/api/schema.d.ts
VERDICT: ratification-sound
```
