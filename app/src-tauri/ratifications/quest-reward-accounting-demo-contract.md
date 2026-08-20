# Ratification: quest reward accounting fields in demo analytics

Independent adversarial review of the three demo golden movements caused by
making quest reward TT and realised reward markup explicit analytics fields.
The review compared the semantic JSON trees rather than relying on the
single-line text diff.

## Change under review

The two Overview fixtures now carry `questItemTt: 0.0` in every applicable
return breakdown:

- `analytics_overview_all.txt`: the top-level return breakdown, 63 timeline
  rows, and 5 monthly rows.
- `analytics_overview_30d.txt`: the top-level return breakdown, 1 timeline row,
  and 1 monthly row.

The Hunting activity fixture now carries
`realisedRewardMarkup: 0.0` on its single activity row.

## Evidence reviewed

- All 73 added leaves are stable zero values. No existing value, total, rate,
  row count, or ordering changed.
- Overview intentionally exposes the TT of confirmed, unreversed quest reward
  stock as a distinct liquid-return family. The curated demo database contains
  no quest reward item evidence, so each projected value is zero.
- Hunting intentionally projects later realised markup from quest-provenance
  stock movements into the originating activity context. The curated demo
  database contains no such sale, so the projected value is zero.
- Existing quest reward ledger entries are a separate accounting source. They
  do not imply either quest item stock or a later stock sale and therefore do
  not justify non-zero values in these new fields.
- The additions are derived from persisted demo data and contain no ambient
  clock, environment, or ordering input.

## Judgement

The previous fixtures are obsolete because the public response contracts now
make two existing accounting dimensions explicit. The regenerated output is
minimal, deterministic, and fully explained by the absence of those economic
events from the curated demo data. No regression has been absorbed into the
expected output.

```text
ORACLE-RATIFICATION
range: origin/main..HEAD
goldens: analytics_overview_all, analytics_overview_30d, analytics_hunting_activity
VERDICT: ratification-sound
```
