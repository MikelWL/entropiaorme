# ADR-0027: Intent-led healing attribution with durable output evidence

- Status: Accepted
- Context: self-heal chat lines can describe a paid medical-tool activation,
  a restoration effect tick, weapon or clothing lifesteal, or another passive
  effect. Output alone cannot identify which economic action occurred.

## Context and problem statement

The game reports healing as a numeric self-heal line without naming its source.
The former tracker treated a compatible line as evidence that the selected
healing tool had been used. That inference becomes false when a weapon or
amplifier carries lifesteal, and it would charge every tick of a restoration
effect as another activation.

Chat timestamps are also insufficient for ordering. They have whole-second
precision, while a player can select a restoration chip, activate it, and
return to a weapon within half a second. Input resolution and chat delivery can
arrive at the tracker in either order even though both observations are valid.

Healing cost is liquid PED accounting. When evidence is ambiguous, recording a
charge would invent a loss. The system therefore needs an affirmative paid-use
boundary while retaining enough evidence to explain and improve attribution.

## Decision

A paid healing activation requires two compatible signals: a resolved healing
hotbar intent and a chat output matching that tool's immutable healing profile.

- The input hook records the operating-system occurrence time immediately and
  resolves the bound equipment on its worker. Database access never runs in the
  hook callback.
- Direct, over-time, and compound profiles declare direct output intervals,
  effect duration, effect tick intervals, and optional cadence. The profile is
  copied onto each activation so later equipment edits cannot rewrite history.
- A healer intent remains eligible through a short delivery tail after the
  player switches back to a weapon. An output delivered before its earlier
  intent is first persisted at zero cost and can be reconciled within the same
  bounded interval.
- Confirmation persists the activation, its confirming output, and the session's
  healing-cost increment in one database transaction before adding the cost to
  live actor state. A failed evidence write cannot create an unverifiable
  charge, and crash recovery retains every committed activation.
- A confirmed over-time or compound activation opens an effect window. Matching
  subsequent ticks are persisted as effect outputs and never add another cost.
  A pure over-time profile uses its first matching tick as its confirming
  output. A fresh healer edge plus compatible direct output can confirm another
  activation when the intervals overlap. After that edge ages out, the active
  effect receives the conservative claim over a merely held healer.
- Per-tool reload state is a second guard. Repeated input or output inside the
  cooldown cannot create another activation.
- Healing outside a compatible intent is persisted as passive when correlated
  with recent damage, or unattributed otherwise. Both classifications cost
  zero. Known lifesteal percentages improve explanation but never authorise or
  suppress billing.
- Health-capped direct output may fall below the configured minimum only for a
  fresh compatible intent with no competing damage correlation.
- The game timestamp remains raw provenance. Local monotonic observation and
  input occurrence times govern ordering, cooldowns, and effect windows.

Chat-only healing inference is retired as an accounting authority. The legacy
tool-change topics remain for weapon and harvesting compatibility while the
hotbar intent becomes the canonical healing boundary.

## Consequences

Lifesteal, clothing effects, and unknown passive sources cannot create healing
cost. Restoration tools charge exactly once per confirmed activation, while
their subsequent ticks remain visible evidence. A different healing tool can
be used during an active effect window because each incoming output is matched
against the active effect and the current intent profile before attribution.

The equipment library gains explicit healing profiles and descriptive
lifesteal metadata. Existing saved weapon setups are enriched at read time from
their catalogue component IDs, so a refreshed bundled snapshot improves their
explanation without rewriting stored cost inputs. User-authored passive-effect
configuration is not required for accounting correctness.

Conservative ambiguity can undercount a real activation when either signal is
missing, but it cannot overcharge one. The activation, effect-window, and output
tables preserve the audit trail needed to review that tradeoff and extend the
classifier without reinterpreting session totals.

See [ADR-0020](0020-tracker-actor-refounding.md) for the actor and monotonic-time
foundation, the [event taxonomy](../architecture/event-taxonomy.md) for the
intent path, and the [database schema reference](../architecture/database-schema.md)
for the evidence model.
