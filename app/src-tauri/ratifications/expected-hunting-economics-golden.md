# Ratification: nullable expected hunting economics in demo analytics

Independent adversarial review of the Hunting activity demo golden after Expected
Hunting Economics became an always-present, nullable response field at the
overall, session-definition, declared-activity, and species grains.

## Change under review

`analytics_hunting_activity.txt` gains exactly nine `expected: null` leaves:

- one overall
- one definition
- one ambient activity
- six species

Removing those nine leaves produces a JSON tree exactly equal to the previous
golden. No existing numeric value, count, row, ordering, or nested payload moved.

## Evidence reviewed

- Every affected API DTO uses `Nullable<ExpectedHuntingEconomics>`, which means
  required on the wire and nullable when no model basis exists. The mapper
  preserves absent service evidence as explicit JSON `null` at all four grains.
- The generated TypeScript contract likewise requires
  `ExpectedHuntingEconomics | null`, and the frontend treats null as honest
  missing historical basis rather than inventing a projection.
- The service returns no expected aggregate unless positive immutable modelled
  raw TT and eligible offensive cost exist. Historical rows and the bundled
  demo do not acquire present-day equipment facts retroactively.
- Backend tests separately prove populated, partially covered, and absent
  aggregates; frontend fixtures cover the required-null input.
- Each added value is a constant null derived from persisted absence. No clock,
  randomness, environment value, collection ordering, or floating-point
  calculation enters the delta.

## Judgement

The old golden became obsolete when the response contract gained required,
nullable expected-economics fields. The new pin is minimal, deterministic, and
semantically correct for a bundled demo whose historical offensive phases do
not contain immutable efficiency and looter evidence. It exposes missing basis
honestly and does not absorb a behavioural regression.

```text
ORACLE-RATIFICATION
range: 37d8275..HEAD
goldens: analytics_hunting_activity
VERDICT: ratification-sound
```
