# Ratification: codex mastery pins

Independent semantic review of the expected-output changes that accompany the
codex mastery feature (the repeatable post-rank-25 claim). The review
re-derives the verdict against the current tree rather than accepting the
change author's rationale, because a self-approved golden move carries a
structural conflict of interest.

## Change under review

The Codex gains the mastery level reward: once a species' 25 ranks are
complete, a repeatable claim into any cat1-cat3 skill for that skill's fixed
value, derived as a 5000 cost basis over the existing category divisors
(cat1 25, cat2 15.625, cat3 7.8125 PED; the in-game UI displays the 2dp
roundings 25 / 15.62 / 7.81). New service methods (`mastery_claim`,
`mastery_unclaim`, `get_mastery_skill_options`), three typed commands, an
additive `mastery_level` field on the species listing and rank breakdown
shapes, and a `kind = 'rank'` filter on the rank-claim overlay reader.

## Oracle deltas reviewed

1. **Adapted byte-shape pin** (`species_listing_dedupes_skips_and_sorts`):
   each species object gains a trailing `"masteryLevel": 0`; every
   pre-existing field value is byte-identical across the diff, and the zero is
   correct for a fixture with no mastery claims.
2. **First-generation pins**: the per-category mastery values
   (25 / 15.625 / 7.8125, reconciled against the divisor constants and the
   observed in-game display roundings), cat4/unknown-skill exclusion, sequence
   numbering (1/2/3, freed-number reuse via COUNT+1 inside the writer
   transaction), the calibration side-effect (the shared
   `write_codex_calibration` helper, so the silent-skip behaviour pinned by
   ADR-0017 is provably identical to rank claims), rollup landing
   (one claim = one `codex_pes` unit; unclaim relands the day), and the
   facade's error mapping and catalogue-independent 36-skill option set
   (15 + 11 + 10, counted against the category constants).
3. **Reader filter**: `get_species_ranks` gains `AND kind = 'rank'`. On the
   prior tree the writer and deleter already filtered by kind and the reader
   was safe only because meta rows use the `'__meta__'` sentinel species.
   Mastery rows deliberately carry real species names with the rank column as
   a sequence number, so the filter is necessary for the feature and provably
   a no-op for all pre-existing data. The new test pinning that mastery claims
   never read as rank claims guards exactly this.

## Adversarial review findings

- **Delta accountability.** Every changed or first-pinned value maps to the
  stated behaviour extension; nothing beyond it appears in the range.
- **Frequency/count discipline.** Double-fire is pinned against: one claim
  writes one rollup unit and at most one calibration row; sequences are
  contiguous.
- **Determinism.** All pinned timestamps derive from the injected mock clock;
  no wall-clock, randomness, or environment leak.
- **Frozen corpus goldens.** Untouched: the corpus pins only the
  meta-attributes read for codex, which this change does not disturb.

```text
ORACLE-RATIFICATION
range: 95065ee..4f40ee0
goldens: eo-services codex byte-shape pins, codex_categories mastery pins, codex mastery service tests, eo-api codex_facade tests
VERDICT: ratification-sound
```
