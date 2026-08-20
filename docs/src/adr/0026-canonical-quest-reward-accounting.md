# ADR-0026: Canonical quest reward accounting and session-owned quest rosters

- Status: Accepted
- Context: quest rewards can arrive through mission-log completion, signal items, reviewed loot evidence, or a manually confirmed hand-in clump. Their names, item mix, and value are not reliably authorable in advance. The retired quest-playlist feature also duplicated the ordered quest roster already owned by session definitions.

## Context and problem statement

The game does not emit one uniform quest-completion contract. Some missions
name themselves in chat, some complete when an item arrives, and AI-generated
dailies may expose only a reward clump. The reward can mix Universal Ammo with
tradable items or grant PES progression. Treating each detection path as its
own accounting path made Inventory, the ledger, Overview, and activity
attribution disagree.

Universal Ammo needs one explicit classification. It cannot be sold, traded,
listed, or converted onward, but every PED of it funds the same tracked cycle
cost that a PED balance would fund. Carrying it as stock would create an
Inventory position with no valid stock action. Carrying an ammo balance against
future costs would instead change cost accounting based on how the player paid,
which would make identical hunts incomparable.

Quest playlists created a second collection ontology beside session-definition
rosters. Both could name an ordered group of quests, but only the session
definition also owns runtime availability, session identity, family resolution,
and the overlay activity set.

## Decision

All confirmed reward evidence converges on one lifecycle, independent of how
completion was detected.

- Universal Ammo is liquid PED. Its full observed TT writes one positive
  `quest_reward` ledger entry and never enters Inventory. Later cycling records
  ordinary costs without suppression, balance drawdown, or amortisation.
- Shrapnel conversion removes the converted Shrapnel stock and records only the
  1 percent gain from the fixed 100:101 conversion. The original Shrapnel TT was
  already recognised as loot; Universal Ammo is not conversion output stock.
- Every other confirmed item is an Inventory acquisition at observed TT. Its
  reward-item, quest-run, quest, activity-context, and session-definition
  provenance survives listings, sales, conversions, removals, and their
  corrections. Zero-TT items allocate later markup by quantity.
- Fixed PES remains non-liquid progression. The active quest model has no
  authored direct-PED reward policy. Legacy stored `fixed_ped` values remain
  readable but normalise to no active economic claim.
- A run snapshots reward ownership across its exact declared contexts, weighted
  first by measured cycle PED and otherwise by duration. With neither basis, the
  reward remains globally and quest attributable without inventing an activity
  owner.
- Item TT enters realised returns at acquisition while staying outside
  Loot-Only. A later sale writes and attributes only markup or loss beyond TT.
  Estimated market value remains informational under ADR-0024.
- Resetting a cooldown is not an economic correction. It appends a cooldown
  reset and preserves the completion, evidence, stock, and progression.
  Undoing a reward appends a reversal and is refused while downstream stock
  transactions depend on the acquisition.
- Session-definition rosters are the sole active quest collection. Quest
  playlists, their commands, service paths, authoring UI, analytics, and legacy
  link suggestion retire. Their database tables remain dormant so the migration
  is non-lossy. The dashboard Quests widget reads the selected or running
  session definition's roster directly.

## Consequences

Every completion mechanism now produces the same durable reward facts, so the
ledger, Inventory, Overview, Quest Analytics, and Hunting can project one
consistent economy. Universal Ammo behaves exactly like PED income without a
parallel wallet, and ordinary cycle costs retain their meaning. Tradable reward
items can realise markup later and return it to the quest and activity that
earned them, including when the item's TT is zero.

Session authoring becomes the single place that defines a playable quest set.
Old playlist rows remain inspectable in database history but have no active
product behaviour. This accepts a deliberate command-contract contraction and
removes the possibility that dashboard, overlay, and analytics disagree about
which quest collection is current.

See [ADR-0018](0018-daily-rollup-read-model.md) for the rebuildable Overview
projection, [ADR-0024](0024-market-informational-layer.md) for the market-data
boundary, and the [database schema reference](../architecture/database-schema.md)
for the reward provenance and correction tables.
