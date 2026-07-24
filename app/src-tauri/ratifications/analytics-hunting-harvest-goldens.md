# Ratification: analytics hunting and harvest demo goldens

Adversarial review of the demo response goldens accompanying the Analytics
profession split: the Activity aggregate becoming the Hunting aggregate
(per-weapon comparison retired) and the new Tree Cutting per-tool aggregate
gaining its first-generation golden.

## Change under review

The Analytics surface splits activity comparisons by profession. The
`analytics_activity` command slims to the per-mob and per-tag tables and
renames to `analytics_hunting`; the per-weapon comparison is retired as a
product decision (weapon deltas are explained by known item stats rather than
discovered from session data, and the comparison surface exists for
discovery). A new `analytics_harvest` command aggregates `harvest_events`
per tool: swings, cycled PED, and the loot-only rate, excluding swings with
no recorded tool.

## Oracle delta reviewed

- **`demo_goldens/analytics_hunting.txt`** (renamed from
  `analytics_activity.txt`): byte-level comparison against the pre-rename
  golden shows the only difference is the removed `weaponComparisons` key;
  the surviving twelve mob rows and the empty tag table are byte-identical.
  Reconstructing the old file with the weapon chunk excised reproduces the
  new file exactly. The removed key maps one-to-one onto the deleted
  `weapon_comparisons` DTO field. No count, value, or ordering moved.
- **`demo_goldens/analytics_harvest.txt`** (first generation, scrutinised
  hardest): pins `{"toolComparisons":[]}`. The bundled demo database was
  verified directly: it predates migration `0006_harvest_events.sql` and
  holds no harvest table at rest; the demo open path migrates it forward,
  creating the empty table, so the empty aggregate is the true output of a
  demo history containing no tree cutting. The aggregate's SQL selects only
  columns the migration defines, and the co-landed unit test seeds rows to
  prove grouping, the (-swings, -cycled, name) ordering, failed-swing
  counting, and NULL/empty-tool exclusion against real data. The golden
  test's expect-panic path rules out a silently absent table pinning empty.
- **Determinism**: the aggregate reads through the reader seam, groups in
  SQL, and sorts by a total order; no clock, randomness, or environment
  reaches the output.

## Judgement

The hunting delta is a clean, minimal field retirement realising a stated
product decision; the harvest first-pin is an honest empty snapshot with its
non-empty behaviour proven by unit tests rather than by the golden. Nothing
in the delta bears the shape of a regression being adopted as truth.

```text
ORACLE-RATIFICATION
range: 9eb481b..HEAD
goldens: demo_goldens (analytics_hunting, analytics_harvest)
VERDICT: ratification-sound
```
