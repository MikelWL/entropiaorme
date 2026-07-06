# Ratification: character HTTP-contract pruning (typed-command migration)

Adversarial review of the OpenAPI contract deletion that accompanies migrating
the character route family off the in-process HTTP router onto typed Tauri IPC
commands (ADR-0019). The review re-derives the verdict against the current
tree rather than accepting the change author's rationale, because a
self-approved golden move carries a structural conflict of interest.

## Change under review

The character family (calibration, stats, skills, professions, the Prospect
forecast family, and the profession / path / HP optimisers) is now served by
nine typed commands (`entropia-orme/src/commands.rs`, registered in
`entropia-orme/src/lib.rs` `generate_handler!` as `character_calibration`,
`character_stats`, `character_skills`, `character_professions`,
`character_prospect_options`, `character_prospect`,
`character_profession_optimizer`, `character_path_optimizer`,
`character_hp_optimizer`) dispatching into the `eo-api` facade
(`eo-api/src/character.rs`). The family's HTTP routes and registrations are
deleted (`eo-http/src/character_routes.rs` absent from the working tree,
`native.rs` handlers removed). The migrated family's contract lives in Rust
DTOs with generated TypeScript (`frontend/src/lib/api/commands.gen.ts`, the
nine `characterXxx` signatures present) narrowing onto the hand-written
`$lib/types/analytics` interfaces, and its behaviour is pinned by
`eo-api/tests/character_facade.rs`, including a byte-for-byte transport-invariance
pin against the HTTP-era bodies and the two ratified not-found convergences.

## Oracle delta reviewed

Exactly two files move, both pure deletion. The `git diff --numstat` figures
(`31 1255` and `2 806`) overstate insertions: structural JSON comparison of
`HEAD:` against the working tree proves zero semantic additions.

- `frontend/src-tauri/contracts/openapi.snapshot.json`: parsed key-by-key,
  the tree removes exactly the ten `/api/character/*` paths (calibration,
  codex, hp-optimizer, profession-optimizer, profession-path-optimizer,
  professions, prospect, prospect-options, skills, stats) and the eighteen
  components reachable only from them (`CalibrationStatus`,
  `CharacterCodexProgress`, `CharacterProspect`, `CharacterProspectOptions`,
  `CharacterStats`, `ExcludedSkill`, `HpOptimizerAttribute`,
  `HpOptimizerResult`, `HpOptimizerSkill`, `OptimizerAttribute`,
  `OptimizerSkill`, `PathAllocation`, `PathOptimizerResult`,
  `ProfessionLevel`, `ProfessionOptimizerResult`, `ProspectOption`,
  `SkillLevel`, `TopProfession`). Zero paths added, zero schemas added.
  Every surviving path and every surviving schema is byte-identical to `HEAD:`
  (JSON-value equality with sorted keys); `info` and `openapi` top-level
  blocks identical. The `+` lines git renders (e.g. `OverlayPosition`,
  `PlaylistAnalyticsRow`) are diff-realignment pairings with identical `-`
  counterparts, not content changes.
- `frontend/src/lib/api/schema.d.ts`: the faithful `npm run gen:api`
  regeneration. The only `/api/` path keys deleted are the ten character
  paths (no non-character path touched); the only named schema interfaces
  deleted are the eighteen character components (`parameters` / `responses`
  sub-blocks in the deleted set belong to the deleted path operations). The
  two `+` lines (509-510) are the `PlaylistAnalyticsRow` comment block
  re-paired verbatim after the character block above it was removed
  (occurrence count 3 -> 3, text byte-identical).

## Findings

- **Delta accountability: genuine-spec-move.** Every deleted path resolves to
  a typed command that provably exists and is registered (`generate_handler!`
  lists all nine), and every deleted component was reachable only from a
  deleted path. The deleted set matches the migrated family exactly with
  nothing left over.
- **Minimality: genuine-spec-move.** A structural parse confirms pure
  deletion: zero semantic insertions, no surviving path or schema value
  altered, no reformatting, no other family moved. The line-diff cosmetics are
  git realignment, not content.
- **No dangling references.** All 92 distinct `$ref` targets in the pruned
  snapshot resolve to surviving components; zero dangling refs. No deleted
  component name survives even as a substring anywhere in the snapshot, so no
  surviving path or schema still points at a retired model, and there is no
  shared component wrongly deleted (each of the eighteen was
  character-exclusive).
- **Conformance gate: unaffected.** `eo-wire::models::registered_contracts()`
  registers only the spine models (`HealthStatus`, `NotableEvent`,
  `TrackingSnapshot`); no character model was ever registered.
  `openapi_conformance.rs` asserts the registry is a subset of the snapshot
  (it fails on a *registered* model missing from the snapshot), so deleting
  these unregistered components neither breaks the gate nor removes protection
  it was providing. The gate stays green.
- **No frontend consumers.** No frontend module imports the deleted generated
  types from `$lib/api` / the schema; every character type reference in
  `routes/character/CharacterView.svelte` and `lib/guide/fixtures/character.ts`
  resolves to the hand-authored `$lib/types/analytics` (`CalibrationStatus`,
  `SkillLevel`, `PathOptimizerResult`, `HpOptimizerSkill`, `ProspectOption`
  are all defined there). No `components["schemas"][<deleted>]` usage remains,
  and no `/api/character/*` path string survives in `frontend/src` or
  `frontend/e2e`. Deleting the generated entries cannot break the frontend
  typecheck.
- **Codex retirement: genuine-spec-move.** `GET /api/character/codex`
  (`CharacterCodexProgress`) has zero callers (`getCharacterCodex`,
  `character/codex`, `CharacterCodexProgress` all absent from `frontend/src`
  and `frontend/e2e`); the still-live `/api/codex/*` family in `index.ts` is
  the unrelated codex-claim family, not this endpoint. The retirement is a
  documented decision in the `character.rs` header, the endpoint is a
  read-only derived projection so no stored data is lost, and it mirrors the
  sanctioned equipment `cost/calculate` precedent. The one capability deletion
  clears scrutiny.
- **The two ride-along behaviour movements sit outside this golden delta.**
  The not-found soft-error convergence (Prospect and profession-path-optimizer
  adopting their family's full error shape) and the Prospect `sample`
  narrowing (dropping the internal `skillShares`/`attributeRates`
  intermediates) are typed-response behaviours; the character family never had
  HTTP body goldens, and the snapshot describes only schema *shapes*, all of
  which are deleted here. So these movements do not appear in either golden
  file under review and cannot be laundered through this delta. They are
  documented in the `character.rs` header, pinned by `character_facade.rs`
  (the transport-invariance test plus the two named not-found convergences),
  invisible to the `.error`-first frontend, and sanctioned under
  ADR-0017/0019. Recorded here as positive evidence; the facade behaviour's
  correctness is out of scope for this contract-deletion review and rests on
  the facade tests.
- **Determinism: clean.** Both goldens are pure deletions against an existing
  baseline; no new expected output is pinned in either file, so no ambient
  input (clock, randomness, environment) can have leaked in.

No suspected regressions swept into the goldens and no non-deterministic pins.
The delta is a structurally-proven pure deletion of a migrated family's HTTP
contract description, with the retired capability caller-free and the new
surface covered by its own typed tests.

## Verdict

```
ORACLE-RATIFICATION
range: 83bdf7f..HEAD
goldens: frontend/src-tauri/contracts/openapi.snapshot.json, frontend/src/lib/api/schema.d.ts
VERDICT: ratification-sound
```
