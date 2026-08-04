# Ratification: the Activities block replaces the quest-focus facet in the demo tracking snapshot

Independent adversarial review of the single golden movement accompanying the
overlay's unified Activities control: the demo tracking snapshot losing
`questsInProgress` and gaining a nested `activities` block. Re-derived against
the tree at HEAD rather than from the change author's account.

## Change under review

`demo_goldens/tracking_snapshot.txt`, one contiguous substitution:

- Removed: `"questsInProgress":6`.
- Added, in the slot it vacated: `"activities":{"visible":false,
  "adHocSegments":false,"readyCount":0,"active":[]}`.

Two rationales, reviewed separately because they carry different risk:

1. **Shape.** The overlay's separate quest-focus picker and segment field became
   one control, so the snapshot publishes one `activities` block instead of the
   separate `questNames` / `segmentName` / `questsInProgress` keys.
   `SNAPSHOT_FIELDS` moved 45 -> 43 accordingly.
2. **Values.** The visibility rule tightened partway through this change: a
   session now offers only what its definition rostered, and the bundled
   demo's primed session is an instance of no definition, so it offers
   nothing.

## Evidence reviewed

- **Golden set completeness.** Every path `xtask/src/ratify.rs::is_golden_path`
  treats as a golden was diffed over the range: exactly one file moved. No
  corpus `expected/` artefact, no wire fixture, no visual baseline, not
  `contracts/event_schemas.snapshot.json`, no other demo golden.
- **Minimality.** Both revisions were JSON-walked and compared leaf by leaf.
  One removed path, four added paths, and **zero changed values on shared
  paths**: all 36 surviving top-level keys, both ~100-element series
  (`multiplierHistory`, `cumulativeNetHistory`), the nested
  `trifectaAttribution`, and every `recentEvents` entry are identical. Key
  order is preserved. At byte level the diff is a single substitution between
  a 173-character common prefix and a 2183-character common suffix; there is
  no second edit site in the payload.
- **The values are arithmetic over the demo's data, not a blanking defect.**
  The roster branch is unreachable twice over for data reasons rather than
  code-path ones: the demo assembly passes no definition service, and the
  primed session row carries no `definition_id` at all. The off-roster arm now
  admits only what is recording, and the demo writes no interval rows (the
  bundled database predates the table; it holds 20 quests, 6 in progress,
  which is what the retired `questsInProgress: 6` counted). So the options
  list is empty and the three pinned figures follow.
- **The same assembly still publishes a populated block**, which is the
  counter-evidence that this is not universal blanking: the facade tests drive
  `build_snapshot_value` with a definition service and a two-quest roster and
  assert `visible: true`, `readyCount: 2`, dropping to 1 after one
  declaration, and pin `visible` while idle as well. Executed at HEAD: pass.
- **Intended, not merely actual.** The tightened rule is stated independently
  in three places, including a purpose-built test that pins exactly the demo's
  own shape (an open mission log with no roster yields an absent control). The
  adapt-versus-fix call therefore lands on adapt: the old pin described the
  behaviour this change deliberately retired.
- **Determinism.** `activity_picture` takes a `now`, but it reaches only the
  cooldown predicates, which are unreachable from an empty roster with nothing
  standing; the demo additionally runs under a frozen mock clock at rate zero.
  Verified empirically under a timezone sweep (UTC, America/Anchorage,
  Asia/Kathmandu, Pacific/Kiritimati): the snapshot comparison holds at every
  offset.
- **Declared collateral confirmed inert.** `is_auto_numbered_segment` is gone
  with no surviving references; `signal_quest` survives only service-side and
  is absent from the wire type. Neither can reach this golden. Stated plainly:
  this golden is NOT evidence about the removal of `questNames` /
  `segmentName`, because the demo emitted neither key before the change; the
  evidence for those is the exact-string idle snapshot assertion in the
  facade tests, which passes.

## Judgement

The delta is fully accounted for and minimal: one key out, one block in, every
other leaf in a 2.4 KB payload provably unchanged. The half carrying real risk
is the values, and it survives adversarial pressure from two independent
directions. The frequency/count dimension this review exists to catch was
checked explicitly: the two counts are zero, and zero is what the code should
produce for a session that rostered nothing and is recording nothing. No
suspected regression swept into the pin, and no non-deterministic pin.

One finding from the review is fixed alongside: the `TrackingSnapshot::activities`
doc comment still claimed the idle branch never carries the key, which the
same change falsified.

```text
ORACLE-RATIFICATION
range: 1a17f87..HEAD
goldens: demo_goldens/tracking_snapshot
VERDICT: ratification-sound
```
