# Activity analytics rate-precision golden ratification

An independent adversarial review examined the Hunting demo golden after activity and variant rates were brought onto the established four-decimal analytics precision.

The only golden movement is the ambient activity's `lootRate` and `rewardedRate`, each changing from the unrounded quotient `1.3926862314360504` to the half-even result `1.3927`. Both values are `5293.67 / 3801.05`; no economics, counts, ordering, or other payload fields changed. The shared deterministic normaliser now makes these recursively mapped rates consistent with overall, definition, species, and other analytics rates.

```text
ORACLE-RATIFICATION
range: bb2737c..HEAD
goldens: analytics_hunting_activity
VERDICT: ratification-sound
```
