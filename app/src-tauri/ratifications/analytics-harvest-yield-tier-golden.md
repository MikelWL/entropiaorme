# Ratification: Tree Cutting yield-tier demo golden

Independent semantic review of the Tree Cutting demo response golden after
source activity moved from a tool-first aggregate to durable effective yield
tiers with nested tool strategies.

## Change under review

`demo_goldens/analytics_harvest.txt` changes from
`{"toolComparisons":[]}` to `{"tierComparisons":[]}`. This is the empty
database shape of the same typed command, now exposing Small, Long, Huge, and
Unclassified yield activities at the top level. Tool comparisons remain
available inside each populated tier.

## Evidence reviewed

- The service response, API DTO, generated TypeScript binding, and frontend
  model all use the same tier-first shape.
- Migration `0013_harvest_yield_tier.sql` durably records direct board
  evidence, bounded inference, and explicit unknowns.
- The migration test covers direct, inferred, conflicting, cross-session,
  cross-tool, and unknown historical rows.
- The non-empty analytics test proves tier and nested-tool grouping,
  preservation of unknown tools, and conservation of swings, cost, returns,
  and active loot composition.
- The empty façade serialisation and demo integration tests reproduce the new
  bytes exactly.

## Judgement

The new bytes pin the intended typed contract and do not conceal a data-loss
regression. The old top-level tool key would misrepresent the service and DTO
after the semantic change; the new empty tier table is the truthful result for
the bundled demo history, which contains no Tree Cutting events.

```text
ORACLE-RATIFICATION
range: c4fe937..HEAD
goldens: demo_goldens/analytics_harvest
VERDICT: ratification-sound
```
