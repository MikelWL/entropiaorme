# Ratification: quest reward contracts and playlist retirement

Independent adversarial review of the expected-output changes accompanying
the unified quest reward lifecycle and retirement of quest playlists. The
review checked the absolute values, arithmetic, removed oracle ownership, and
the production paths that derive each field.

## Change under review

- The minimal Quest facade response adds `rewardUndoAvailable: false`.
- The analytics fixture adds the explicit `questItemTt` and
  `realisedRewardMarkup` fields required by the current DTOs.
- Universal Ammo reward rows now carry their observed TT once in confirmed
  and rewarded returns, with the corresponding rates and item evidence
  status.
- The dashboard playlist fixture member, playlist-only visual baseline, and
  playlist-matching YML mirror are removed with their retired product owners.

## Evidence reviewed

- A newly created quest has no completion or economic reward, so
  `rewardUndoAvailable: false` is the exact truthful initial value. The
  service derives the flag from the latest unreversed completion rather than
  from cooldown state.
- The Overview fixture contains no stock-class quest reward, so each of its
  seven `questItemTt` additions is deterministically zero.
- The six Hunting rows contain no realised sale of quest-provenance stock, so
  each `realisedRewardMarkup` addition is deterministically zero.
- The Universal Ammo family and variant arithmetic was independently checked:
  `695 + 4 = 699`, `430 + 2.5 = 432.5`, and `265 + 1.5 = 266.5`; dividing by
  the respective cycled values yields the pinned rates `0.9197`, `0.9202`,
  and `0.9190`.
- `rewardStatus: "item"` describes item-shaped completion evidence. Universal
  Ammo's independent liquid accounting kind sends its TT to the ledger and
  never to Inventory, so the status does not imply tradeable stock.
- No active playlist code remains in the frontend, API, or quest service.
  The removed mirrors asserted only retired playlist behaviour; quest
  lifecycle behaviour remains covered by service and facade tests, while
  dormant historical database tables remain intact.
- No unrelated fixture value, ordering, or count moved.

## Judgement

The previous expected outputs described retired playlist ownership and the
pre-overhaul reward contract. The new values and removals are minimal,
deterministic, and fully accounted for by the production semantics. No
regression has been absorbed into the oracle.

```text
ORACLE-RATIFICATION
range: origin/main..HEAD
goldens: quests_facade minimal Quest JSON, e2e analytics fixture, e2e dashboard fixture, quest_automation_with_playlist_match, quests playlists visual baseline
VERDICT: ratification-sound
```
