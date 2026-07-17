# Corpus DB-state snapshots: the map_pins catalogue section

An adversarial ratification review of the corpus DB-state snapshot
regeneration accompanying the cartography-pins schema extension
(migration `0007_map_pins.sql` plus the `map_pins` entry in the
`eo-wire` snapshot catalogue).

## Findings

- **Delta accountability.** All 15 changed goldens gain exactly one
  line, byte-identical across every file (`"map_pins": [],`), and zero
  lines are removed. This maps precisely to the additive
  schema-plus-catalogue extension: the migration is a pure
  `CREATE TABLE` plus one index, with no `ALTER` and no data mutation on
  any existing table, and the catalogue gains one `TableSpec`.
- **Minimality and ordering.** The new key lands in correct
  alphabetical position (after `ledger_entries`, before
  `notable_events`) in every file, confirming the snapshot serialises
  table sections by name. No existing section shifted, no value on any
  untouched field changed, and the `fingerprint.jsonl` goldens did not
  move.
- **Intended, not merely actual.** Empty `map_pins` is the intended
  contract: pins are created only through the typed map commands (user
  annotation actions); the corpus scenarios replay the chat.log
  tracking pipeline exclusively, and no corpus path invokes a pin
  write, so `[]` is what the code should produce for every scenario.
- **Determinism.** The `map_pins` schema carries a `created_at` time
  field, but no pin rows materialise in any scenario, so no ambient
  time value appears in any golden. Forward note: a future scenario
  that creates a pin must inject the clock seam before pinning.

## Summary judgement

The golden delta is a clean, minimal, fully-explained consequence of an
additive schema and snapshot-catalogue extension: one alphabetically
placed empty `map_pins` section per snapshot and nothing else. This is
a genuine spec move, not a laundered regression.

```text
ORACLE-RATIFICATION
range: eb1cb73..HEAD
goldens: basic_hunt_10_events, crit_dodge_evade_jam, defensive_combat_round, empty_session, enhancer_break_during_hunt, global_item_drop, global_kill_correlated, hof_item_drop, hof_kill_correlated, mission_completion_with_reward_suppression, multi_mob_hunt_loot_grouping, single_mob_hunt, skill_gain_across_tick, tree_harvesting_session, placeholder_recorded_hunt
VERDICT: ratification-sound
```
