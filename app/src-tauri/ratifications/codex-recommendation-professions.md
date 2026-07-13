# Ratification: codex recommendation multi-profession pins

Independent semantic review of the expected-output changes that accompany
multi-profession recommendation targets (profession families ranked as one
target). The review re-derives the verdict against the current tree rather
than accepting the change author's rationale, because a self-approved golden
move carries a structural conflict of interest.

## Change under review

The recommendation and mastery option reads take a list of professions
instead of a single optional name. The weight map sums per-skill weights
across the requested professions (deduplicated; within one profession a
duplicate skill entry's later weight still wins, resolved per-profession
before summing), and each option gains `professionContributions`: the
per-profession split, one entry per weighted requested profession in request
order, computed from the same unrounded level figures as the summed value.

## Oracle deltas reviewed

1. **Adapted byte-shape pins** (`profession_options_rank_by_contribution`,
   `hp_options_sort_by_gain_then_level_then_name`): each option object gains
   the additive `professionContributions` key (a one-entry split whose value
   byte-equals the summed field on the weighted options; empty on
   zero-weight options and in the no-professions read). The comparisons are
   full-object, so the passing pins prove every pre-existing field value is
   unchanged.
2. **First-generation pins** (`several_professions_rank_by_summed_weights`):
   summed weights (20 + 30 = 50), duplicate-request deduplication (a name
   requested twice never double-counts), the exact per-profession split
   (0.28749 / 0.431235, a 2:3 weight ratio to six decimals), and the rank
   ordering at equal summed weight (the lower-level skill's cheaper curve
   wins). Ordering is request-order over a vector, so deterministic.
3. **Equivalence**: a single requested profession reproduces the prior
   single-profession ranking byte-identically by construction (an empty map
   summed with one profession's map equals the old direct insertion); the
   adapted pins passing unchanged bar the additive key is the evidence.

## Adversarial review findings

- **Delta accountability.** Every changed or first-pinned value maps to the
  stated extension; nothing beyond it appears in the range.
- **Call-site adaptations** (facade tests, command wrappers, generated
  bindings, guide fixtures) are mechanical signature and shape follow-ons;
  the generated bindings landed in the same change.
- **Frozen corpus goldens.** Untouched across the range.
- **Design note (no defect).** The summed contribution rounds once from the
  summed weight while the split entries round independently; a future golden
  pinning both together should be authored aware the two figures can diverge
  by a rounding ULP (here they coincide exactly).

```text
ORACLE-RATIFICATION
range: 433dd39..d7f6670
goldens: eo-services codex option byte-shape pins (profession_options_rank_by_contribution, hp_options_sort_by_gain_then_level_then_name, several_professions_rank_by_summed_weights)
VERDICT: ratification-sound
```
