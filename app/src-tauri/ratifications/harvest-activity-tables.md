# Ratification: harvest activity tables and response fields

Adversarial review of the testing-oracle output accompanying the tree
harvesting feature: the DB-state snapshot catalogue gaining the two harvest
tables, a first-generation replay scenario pinning the new routing behaviour,
and the demo response goldens gaining the harvest fields.

## Change under review

Tree harvesting became the second tracked activity beside hunting. Wood loot
groups (items named "Wood Shavings" or ending " Board") route to harvest
events instead of minting zero-shot kills; the explicit "Harvest attempt
failed to generate useable resources" chat line records a failed swing; swings
with no hotbar-equipped harvesting tool record at zero cost with a one-shot
session warning. Migration `0006_harvest_events.sql` adds `harvest_events` and
`harvest_loot_items`, and the snapshot catalogue in
`eo-wire/src/db_snapshot.rs` gains both tables. The tracking snapshot and
session detail responses gain harvest totals.

## Oracle delta reviewed

- **14 existing corpus `expected/db_state.json`**: a `--unified=0` diff over
  all 14 files yields nothing but the two alphabetically placed empty keys
  (`"harvest_events": []`, `"harvest_loot_items": []`). No existing
  `fingerprint.jsonl` moved. Zero collateral.
- **`tree_harvesting_session` (first generation, scrutinised hardest)**: a
  tick-by-tick trace of the committed 15-line `chat_replay.log` accounts for
  every row. Four wood groups become four `success:1` harvest events with the
  exact loot totals (0.098, 0.047, 0.1, 0.08) and six per-item rows; the two
  fail lines become two `success:0` events with no loot rows; all six carry
  `cost_ped: 0.0` and `tool_name: null` (no hotbar signal exists in replay,
  matching the no-tool branch). The non-wood Shrapnel group still creates the
  one kill, absorbing the two pending shots (55.0 damage), and the
  shrapnel-conversion ledger entry (0.025 = 1% of 2.5) derives from
  `kill_loot_items` only: the conversion query joins kills exclusively, so
  wood loot cannot contribute. The fingerprint stream fires `harvest_fail`
  exactly twice on its own topic, represents successful swings as their wood
  `loot_group` (routing happens at the tracker, not the bus), keeps one
  settled-tick heartbeat per content-bearing tick, and inflates no topic's
  frequency. UUID numbering follows the deterministic catalogue scan order.
- **Demo goldens (`tracking_snapshot.txt`, `tracking_session_detail.txt`)**:
  the post-normalisation delta is only the new harvest fields, all zero for
  the hunting-only demo fixture. The regeneration also refreshed now-relative
  time fields (`elapsed`, ISO instants) that live entirely inside the golden
  test's documented normalised-away surface (`MockClock` plus the `<TS>` /
  `<ELAPSED>` normaliser); recorded here so the churn is not mistaken for an
  unexplained content change.

## Verdict

Every element of the delta is accounted for by the stated additive extension
and was confirmed against the production code (the wood-vs-kill router, the
harvest fail handler, the no-tool zero-cost branch, the kill-only shrapnel
conversion), not merely against the regenerated output. Determinism is
intact: corpus replays run on the committed clock plan and the demo comparison
runs under an injected clock, so no ambient input reaches a pinned byte. This
is a genuine spec move, not a laundered regression.

```text
ORACLE-RATIFICATION
range: fc3036a..HEAD
goldens: basic_hunt_10_events, crit_dodge_evade_jam, defensive_combat_round, empty_session, enhancer_break_during_hunt, global_item_drop, global_kill_correlated, hof_item_drop, hof_kill_correlated, mission_completion_with_reward_suppression, multi_mob_hunt_loot_grouping, placeholder_recorded_hunt, single_mob_hunt, skill_gain_across_tick, tree_harvesting_session, demo_goldens
VERDICT: ratification-sound
```
