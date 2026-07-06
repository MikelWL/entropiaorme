# Ratification: codex HTTP-contract pruning (typed-command migration)

Adversarial review of the OpenAPI contract deletion and the corpus-golden
interplay that accompany migrating the codex route family off the in-process
HTTP router onto typed Tauri IPC commands (ADR-0019). The verdict is
re-derived against the current tree rather than accepting the change author's
rationale, because a self-approved golden move carries a structural conflict
of interest.

## Change under review

The codex family is now served by eight typed commands
(`entropia-orme/src/commands.rs`, registered in `entropia-orme/src/lib.rs`
`generate_handler!` as `codex_species`, `codex_species_ranks`,
`codex_recommend`, `codex_meta_attributes`, `codex_calibrate`, `codex_claim`,
`codex_unclaim`, `codex_meta_claim`) dispatching into the `eo-api` facade
(`eo-api/src/codex.rs`), which types the boundary over the unchanged
`eo_services::codex::CodexService` (Value in, typed DTO out via
`serde_json::from_value`). The family's HTTP read handlers and write adapters
(`eo-http/src/native.rs`), the hydration read/write methods and the
`CodexService` field on `HydrationState` (`eo-http/src/hydration.rs`), the
route registrations, and the now-dead `skill_tracker` slot on the HTTP app
state (`eo-http/src/lib.rs`, `NativeServices`) are deleted. The facade re-owns
the skill tracker directly from the producer spine (`Api` gains
`skill_tracker: Arc<SkillTracker>` via `producers.skill_tracker_handle()`) so
the claim/meta-claim suppress-next behaviour is preserved. The migrated
family's contract lives in Rust DTOs with generated TypeScript
(`frontend/src/lib/api/commands.gen.ts`) narrowing onto the hand-written
`$lib/types` codex interfaces, and its behaviour is pinned by
`eo-api/tests/codex_facade.rs`, including byte-for-byte transport-invariance
pins on the meta-attributes read, the calibrate result, and the meta-claim
result.

## Oracle delta reviewed

### 1. OpenAPI contract (`contracts/openapi.snapshot.json` + `schema.d.ts`)

Both files move as pure deletion (verified structurally: the pruned file is an
exact line-subsequence of its committed parent, zero modified or inserted
lines; `git diff --numstat` insertion counts are diff-alignment noise around
the deletion boundaries, not content changes).

- `openapi.snapshot.json`: the removed path-key set is exactly the eight
  `/api/codex/*` paths (`calibrate`, `claim`, `meta/attributes`, `meta/claim`,
  `recommend`, `species`, `species/{name}/ranks`, `unclaim`), and the removed
  component set is exactly the twelve schemas reachable only from them
  (`CalibrateRequest`, `ClaimRequest`, `CodexCalibrateResult`,
  `CodexClaimResult`, `CodexMetaAttribute`, `CodexMetaClaimResult`,
  `CodexRankBreakdown`, `CodexRankItem`, `CodexSkillOption`, `CodexSpecies`,
  `MetaClaimRequest`, `UnclaimRequest`). The prune was computed by reachability
  closure (`components reachable from codex paths` minus `components reachable
  from every surviving path`), so the shared `HTTPValidationError` /
  `ValidationError` (reachable from many other paths) were correctly kept. The
  written file was verified to contain zero dangling `$ref` (every surviving
  `$ref` resolves to a surviving schema). The only surviving `codex`
  occurrences are the analytics economy field `codexPes`
  (`returnsBreakdown`, three schemas) and a ledger doc-comment referencing the
  `codex_claims` table: both are the liquid-economy analytics surface, not the
  retired codex CRUD routes.
- `schema.d.ts`: the faithful `npm run gen:api` regeneration (also a pure
  line-subsequence of its parent). Only codex path keys and codex-scoped
  `operations`/`components` types were removed; zero non-codex `/api/…` paths
  touched; the surviving `codexPes` / doc-comment references are the analytics
  ones.

### 2. Corpus-golden interplay (no golden bytes changed)

The `expected/http_responses/GET_codex_meta_attributes.json` goldens (×8,
byte-identical, state-invariant all-null across every scenario) double as two
independent consumers:

- **Frozen fingerprint evidence** (`eo-wire/tests/emitters_proof.rs`): computes
  the HTTP fingerprint over the Python-era raw captures and asserts these
  goldens. This is route-independent historical migration evidence; it is
  **left untouched** (the curated-ten cardinality pin stays ten, the codex
  golden and its `raw_captures` entry stay). Verified green (3 passed).
- **Live-route consistency replay** (`eo-http/tests/http_consistency_replay.rs`):
  replays scenarios through the live native router and fingerprints the live
  endpoints. With the live codex HTTP route deleted, `GET_codex_meta_attributes`
  (the last endpoint in the fixed capture order) is retired from this test's
  set (`the_endpoint_set_is_the_fixed_ten` → `_nine`). Because it was last in
  the shared-`Normalizer` order, the nine preceding endpoints' fingerprints are
  byte-unchanged (verified: the four scenario replays pass against the existing
  nine goldens). The codex golden files are **not deleted** (still consumed by
  the frozen fingerprint evidence, so not orphaned).

The codex read's live contract is re-pinned on the facade in
`codex_facade.rs`: the meta-attributes read serialises byte-for-byte to the
same all-null six-attribute body the HTTP golden carried
(`[{"name":"Agility","currentLevel":null},…]`).

## Findings

- **Delta accountability + minimality: genuine-spec-move.** Every deleted path
  resolves to a typed command that exists and is registered (parity test
  `the_registered_commands_match_the_manifest` green); every deleted component
  was reachable only from a deleted path. Pure deletion, zero semantic
  insertions, no surviving path/component/ordering altered, no other family
  moved.
- **No dangling references.** Programmatically verified: zero surviving `$ref`
  points at any removed component.
- **Conformance gate: unaffected.** `eo-wire::models::registered_contracts()`
  never registered any codex model (grep-confirmed absent), so deleting these
  unregistered components neither breaks `openapi_conformance.rs` nor removes
  protection it provided.
- **Reads' byte-parity: substantiated.** The codex DTOs' field order reproduces
  the service's `json!` wire order field-for-field (the byte-parity holds
  because `serde_json`'s `float_roundtrip` and the wire normaliser's
  `python_repr_f64` agree on all normal doubles, and codex values are rounded);
  `codex_facade.rs` pins the meta-attributes body, the calibrate result
  (`{"speciesName":"Sp","rank":7}`), and the meta-claim result
  (`{"attributeName":"Health","pedValue":1.0}`) byte-exact.
- **Retired framework envelopes: framework-shape retirements, not dropped
  rules.** What retires with the transport is only framework-level shape/type
  validation: the recommend rank 422 becomes a typed `bad_request` on the `i64`
  argument (bound `1..=25` preserved and pinned); the `target` out-of-vocabulary
  422 is unrepresentable (a closed `CodexRecommendTarget` enum); the calibrate
  surrogate-codec 400 and the claim/unclaim/meta body-taint + beyond-`i64`
  500/422 ceremony are unrepresentable over a `String`/`i64` typed command. The
  business validation ladder is fully ported and pinned (calibrate's
  `Rank must be 0-25`, claim's rank/skill validity, unclaim's
  nothing-to-unclaim, meta-claim's attribute membership), all mapping the
  service's `CodexError::Invalid` message onto `ApiError::bad_request`; the
  species-not-found read maps onto `ApiError::not_found` (the HTTP 404). The
  conditional-GET (ETag) contract retires with the transport.
- **Determinism: clean.** Both goldens are pure deletions against an existing
  baseline; no new expected output is pinned into the corpus (the frozen
  evidence is untouched), so no ambient input can have leaked in.

No suspected regressions swept into the goldens and no non-deterministic pins.
The delta is a structurally-proven pure deletion of a migrated family's HTTP
contract description, with the retired framework envelopes caller-free and the
new surface covered by its own typed byte-parity tests, and the frozen
migration evidence preserved intact.

## Verdict

Independently re-derived against the working tree (not accepting this
rationale): the snapshot and its regenerated TypeScript proven pure
order-preserving-subsequence deletions with zero surviving value mutated,
the removed set exactly the codex-only reachability
closure (`HTTPValidationError` / `ValidationError` correctly retained, no
dangling `$ref`, no orphan created), the frozen migration evidence
byte-untouched (`emitters_proof` 3/3 green, cardinality stays ten), the
live-replay ten→nine retirement sound (codex last in the fixed order, the nine
surviving fingerprints byte-unchanged, `http_consistency_replay` 5/5 green),
and the functionality genuinely relocated to eight registered typed commands
with byte-parity pins (`codex_facade` 5/5 green).

```text
ORACLE-RATIFICATION
range: ac61035..HEAD
goldens: frontend/src-tauri/contracts/openapi.snapshot.json, frontend/src/lib/api/schema.d.ts
VERDICT: ratification-sound
```
