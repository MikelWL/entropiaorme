# ADR-0028: Versioned expected-hunting economics over immutable evidence

- **Status:** Accepted
- **Context:** Entropia Universe does not publish a complete loot-return formula. Community testing supports approximately linear Efficiency and hunting-looter effects, while the absolute return anchor and the way those effects combine remain a working theory. EntropiaOrme needs a useful long-run planning estimate without rewriting observed history when equipment, professions, or the theory later changes. It must also preserve the accounting boundary between modelled return, estimated market value, and realised PED.

## Decision

Expected hunting economics is a versioned domain model. Community Model v1 uses:

```text
TT return = 0.86 + 0.07 x Efficiency / 100 + 0.07 x Looter level / 100
```

The implementation clamps inputs to the model's declared 0 to 100 domain. This is not a claim that the game caps looter professions at level 100. The model version travels with every derived result, while persisted evidence remains model-neutral so a later model can replay the same history.

Weapon and amplifier streams retain their own Efficiency. Expected loot is calculated for each component from its raw loot-bearing TT, then summed. A displayed combined Efficiency is only a TT-weighted summary; an amplifier never changes the weapon's own Efficiency. Limited-item acquisition premium is consumed cost, not loot-bearing TT, so it enters the economic denominator without entering the expected-loot numerator. Unlimited-item purchase markup remains recoverable capital and stays outside per-use return.

Only weapon and amplifier streams with a known Efficiency and raw TT basis are eligible. Healing, armour and plates, harvesting, consumables, enhancers, scopes, implants, absorbers, and other ungrounded streams enter neither numerator nor denominator. Their exclusion does not assert zero return. A user-facing Expected Return figure therefore always carries an adjacent disclosure that it models offensive spend only and is not a whole-activity forecast.

The tracker snapshots Animal, Mutant, and Robot Looter levels at session start and persists the exact component evidence for each offensive phase. Target-specific looter selection is typed for future use. Until a trustworthy target class is available, v1 uses and labels the arithmetic mean of exactly those three professions. Legacy phases without captured evidence remain explicitly incomplete.

Equipment and historical Hunting analytics may invert premium-adjusted expected return into
Effective Efficiency: the Efficiency an otherwise identical unlimited offensive setup would
require at the same looter level. This is an economic comparison, not a change to any component's
in-game Efficiency. Equipment requires a complete setup basis. Analytics weights every captured
weapon and amplifier phase by raw TT, retains consumed premium in the combined denominator, and
may report the modelled subset while separately disclosing partial historical coverage. A result
outside v1's declared Efficiency domain is labelled as outside the model rather than clamped.

Loot Markup is an independent, 100%-anchored factor over observed loot composition:

```text
Loot Markup Factor = estimated market value / loot composition TT
Expected Market Rate = Expected TT Rate x Loot Markup Factor
```

Both quantities remain informational estimates. Neither may enter the confirmed ledger or realised profit and loss.

Historical activity breakdowns retain the immutable context stamp on offensive evidence. Exact
quest, segment, and joint-activity signatures therefore receive only the phases captured under
their own context; quest-family rows sum their variants. Quest rewards remain separate from
ordinary loot markup and expected offensive return.

## Consequences

- Equipment can explain component Efficiency, premium-adjusted Effective Efficiency, expected offensive return, and break-even loot markup without presenting a weapon comparator as a separate product.
- Live and historical hunting surfaces can replay one model over the loadout evidence that actually produced their offensive cost, including one aggregate unlimited-equivalent Efficiency across mixed setups.
- Missing Efficiency narrows labelled coverage instead of becoming a plausible zero-return input.
- A future model revision adds a new version and re-evaluates retained evidence. It does not mutate the meaning of historical observations.
- Target-resolved looter selection remains unavailable until mob classification is trustworthy. The explicit three-looter mean is the honest current fallback.
