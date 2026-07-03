# Ratification: equipment HTTP-contract pruning (typed-command migration)

Adversarial review of the OpenAPI contract deletion that accompanies migrating
the equipment route family off the in-process HTTP router onto typed Tauri IPC
commands (ADR-0019). The review re-derives the verdict against the current
tree rather than accepting the change author's rationale, because a
self-approved golden move carries a structural conflict of interest.

## Change under review

The equipment family (search, library CRUD, item detail) is now served by six
typed commands (`entropia-orme/src/commands.rs`) dispatching into the `eo-api`
facade, with the family's HTTP routes, adapters, and registrations deleted
(`eo-http/src/equipment_routes.rs` removed, `eo-http/src/native.rs` -283
lines). The OpenAPI snapshot describes the HTTP surface, and per ADR-0019 it
is pruned per migrated family so it stays true to exactly the families still
served over HTTP. The migrated family's contract lives in Rust DTOs
(`eo-api/src/equipment.rs`) with generated TypeScript
(`frontend/src/lib/api/commands.gen.ts`), and its behaviour is pinned by
`eo-api/tests/equipment_facade.rs`, including byte-identical response
serialisation to the HTTP-era bodies and unchanged stored `properties_json`
bytes.

## Oracle delta reviewed

Exactly two files move, both pure deletion (`git diff --numstat`: `0 817` and
`0 551`):

- `frontend/src-tauri/contracts/openapi.snapshot.json`: the five
  `/api/equipment/*` paths and the nine components reachable only from them
  (`AbsorberComponent`, `AddWeaponRequest`, `CalculateCostRequest`,
  `CostBreakdownLine`, `CostResult`, `Equipment`, `EquipmentDetail`,
  `EquipmentSearchResult`, `WeaponComponent`).
- `frontend/src/lib/api/schema.d.ts`: the faithful `npm run gen:api`
  regeneration, deleting only the five equipment path keys.

## Findings

- **Delta accountability: genuine-spec-move.** Every deleted path and
  component maps to the migration; five paths resolve to typed commands that
  provably exist and are tested, and the deleted registrations are genuinely
  gone from the router.
- **Minimality: genuine-spec-move.** Pure deletion, zero insertions, no
  reformatting, no other family moved; the residual `CostResult` substring
  hits are the distinct, still-served `ArmourCostResult` component.
- **No dangling references.** No remaining snapshot content `$ref`s a deleted
  component; `eo-wire::models::registered_contracts()` never contained
  equipment models, so `openapi_conformance` neither breaks nor was
  protecting these entries; no frontend consumer references the deleted
  types.
- **The cost/calculate retirement: genuine-spec-move.** The one capability
  deletion (no typed replacement) clears scrutiny: zero frontend callers
  (verified by grep), the cost logic survives in
  `eo-services/src/cost_engine.rs` and is exercised via the facade's library
  and detail cost shaping, and ADR-0017/ADR-0019 sanction per-family
  behaviour retirements as deliberate ratified contract changes.
- **Determinism: clean.** Pure deletion against an existing baseline; no new
  expected output pinned, so no ambient input can have leaked in.

Zero suspected-swept-regression and zero nondeterministic-pin findings.

## Verdict

```
ORACLE-RATIFICATION
range: worktree@refactor/equipment-typed-commands (merge-base 1dd0d94)
goldens: frontend/src-tauri/contracts/openapi.snapshot.json, frontend/src/lib/api/schema.d.ts
VERDICT: ratification-sound
```
