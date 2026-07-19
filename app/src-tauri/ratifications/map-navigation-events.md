# Ratification: harvesting and navigation domain events

Adversarial review of the new `harvest.recorded` and `navigation.updated`
domain-event contracts and the harvesting corpus movement they intentionally
produce.

## Findings

The harvesting fingerprint adds exactly six `harvest.recorded` envelopes, one
for each harvest row committed by the scenario. Four carry `success: true` and
two carry `success: false`, matching the durable `harvest_events` rows exactly.
Each envelope carries the persisted harvest identity and appears only after the
harvest transaction succeeds.

The database snapshot has no semantic state change. Exposing harvest identities
earlier in the shared fingerprint-first normaliser reassigns the symbolic UUIDs:
the six harvest IDs move from `<UUID_3>` through `<UUID_8>` to `<UUID_2>`
through `<UUID_7>`, and the later kill ID moves from `<UUID_2>` to `<UUID_8>`.
After independent first-encounter UUID canonicalisation, the old and new
database snapshots are identical.

The event-schema snapshot adds exactly two closed envelopes and their payloads.
`HarvestRecordedPayload` requires `harvestId` and `success`.
`NavigationUpdatedPayload` is a closed empty object: a content-free push-to-pull
invalidation that prompts consumers to hydrate the persisted navigation
snapshot. Radar calibration uses its separate status read and does not emit the
route-state event. The discriminator and `oneOf` members match the four native
`DomainEvent` variants exactly, with no existing event definition changed.

Focused verification passed: event-schema conformance 7/7, domain-event unit
tests 8/8, and the tree-harvesting corpus replay 1/1. No ambient input enters
the pinned output, and no unrelated golden or database behaviour moved.

Nothing in the reviewed golden delta is unaccounted for. This is a genuine,
deterministic contract extension.

```text
ORACLE-RATIFICATION
range: b0393bd..HEAD
goldens: app/src-tauri/contracts/event_schemas.snapshot.json, app/src-tauri/fixtures/corpus/scripted/tree_harvesting_session/expected/fingerprint.jsonl, app/src-tauri/fixtures/corpus/scripted/tree_harvesting_session/expected/db_state.json
VERDICT: ratification-sound
```
