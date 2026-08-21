# Ratification: expected hunting economics in demo contracts

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

The live `tracking_snapshot.txt` also gains exactly two fields after correcting
its typed projection allowlist:

- `expectedReturnCoverage: 0.0`
- `expectedReturnModel: "community_v1"`

The demo's legacy offensive phases contain positive shots and cost but no
immutable efficiency evidence. Zero coverage and an explicit model identity
therefore describe that incomplete basis honestly. `expectedTtRate` remains
absent because its service value is null and this response uses exclude-none
projection. No other snapshot field changed.

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
- Each of the nine analytics additions is a constant null derived from persisted
  absence. No clock, randomness, environment value, collection ordering, or
  floating-point calculation enters that delta.
- The live snapshot's two values derive from fixed demo evidence and the stable
  Community Model v1 identifier. They introduce no ambient input or fabricated
  numeric return.

## Judgement

The old goldens became obsolete when the response contract gained required,
nullable expected-economics fields and the tracking projection omitted fields
that its typed contract already populated. Both new pins are minimal,
deterministic, and semantically correct for bundled demo history without
immutable efficiency and looter evidence. They expose missing basis honestly
and do not absorb a behavioural regression.

```text
ORACLE-RATIFICATION
range: 37d8275..HEAD
goldens: analytics_hunting_activity, tracking_snapshot
VERDICT: ratification-sound
```
