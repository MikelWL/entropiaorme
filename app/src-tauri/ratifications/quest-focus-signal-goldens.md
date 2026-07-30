# Ratification: quest focus and signal-quest golden growth

Independent semantic review of the pinned-output changes accompanying the
move to user-declared quest focus: the tracking snapshot demo golden gaining the
`questsInProgress` field, and the quest wire-shape pins gaining the
`signal_loot_item` column added by migration `0020_signal_quests.sql`.

## Change under review

- `demo_goldens/tracking_snapshot.txt`: one added key, `questsInProgress: 6`,
  at the projection position `SNAPSHOT_FIELDS` dictates.
- The quests facade wire-shape byte pin (`eo-api/tests/quests_facade.rs`)
  gains `"signalLootItem":null` at the tail.
- The quests service full-shape pin (`eo-services/src/quests/tests.rs`)
  gains `"signal_loot_item": null` between `updated_at` and
  `last_completed_at`.

## Evidence reviewed

- Structural diff of the demo golden: exactly one added key, zero removed
  keys, zero changed values across all 36 pre-existing keys including the
  100-element history arrays; pre-existing key order preserved verbatim.
- The pinned count is what the code should produce, not merely what it does:
  the predicate (`is_active` and in progress or signal-capable) matches
  exactly six quests in the bundled demo database, and it is the same
  predicate the focus picker filters on, so the cue cannot drift from the
  list it advertises. Completion and cancellation both clear `started_at`,
  so a completed quest cannot inflate it.
- Determinism: the demo runs under a mocked clock over a per-process copy of
  the committed database; the count takes no clock, environment, or
  randomness. Two consecutive runs agree byte-for-byte.
- Both wire-shape pins remain whole-object equality assertions, so an
  unaccounted extra field would fail them; placement matches serde
  declaration order and the row-projection column order respectively.
  `QuestInput` takes the new field with a serde default, so existing input
  payloads are unaffected, and the migration is additive and forward-only.
- The frozen replay corpus (including the mission-completion reward
  suppression scenario) passes byte-identical after the completion-ordering
  rework, evidence that the behaviour change is contained to the declared
  surfaces. `analytics_harvest.txt` and every other demo golden are
  unchanged.

## Judgement

All three deltas are shape growth recording an intended new capability, not
value drift and not a regression ratified as new truth. This review also
motivated extending the ratification guard to cover `demo_goldens/` paths,
landed alongside, so future demo-golden moves are mechanically forced
through this workflow.

```text
ORACLE-RATIFICATION
range: bc039e7..HEAD
goldens: demo_goldens/tracking_snapshot, quests_facade wire-shape pin, quests service full-shape pin
VERDICT: ratification-sound
```
