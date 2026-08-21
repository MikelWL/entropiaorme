# Ratification: armour policy in the demo tracking snapshot

Independent adversarial review of the demo tracking snapshot after active sessions began publishing their stamped armour-cost policy.

## Change under review

`demo_goldens/tracking_snapshot.txt` gains exactly two fields:

- `"trackProtectionCosts":false`
- `"trackProtectionBySegment":false`

They appear after `endOfSessionArmourReminderEnabled`, the first available position from the typed snapshot field order because the demo has no session name or definition. Removing these two fields reproduces the previous golden exactly. No existing value, count, ordering, or nested payload changes.

## Findings

- **OR1**
  - **Path:** `app/src-tauri/eo-api/resources/demo_goldens/tracking_snapshot.txt:1`
  - **Dimension:** Delta accountability and minimality
  - **Severity:** `genuine-spec-move`
  - **Description:** The proposed delta contains only the two newly exposed active-session armour-policy booleans. No event frequency, economic value, series element, or unrelated field moves.
  - **Suggested resolution:** Update the golden with exactly these two fields.

- **OR2**
  - **Path:** `code:app/src-tauri/eo-api/src/demo.rs:583`
  - **Dimension:** Intended behaviour and fix-versus-adapt classification
  - **Severity:** `genuine-spec-move`
  - **Description:** The curated demo deliberately primes a definition-free session with `SessionFacets::default()`. Its fixture has zero armour cost and replays no protection intervals or defensive evidence, so both policy facets are false. This is distinct from normal session creation, which explicitly resolves the selected or protected definition and retains the product defaults of armour costs enabled and segment attribution disabled.
  - **Suggested resolution:** Adapt the oracle. Changing the demo facets to enabled would fabricate armour capture that the fixture does not contain.

- **OR3**
  - **Path:** `code:app/src-tauri/eo-api/src/tracking.rs:1664`
  - **Dimension:** Determinism
  - **Severity:** `genuine-spec-move`
  - **Description:** Both values are copied from immutable in-memory session facets. They do not depend on time, environment, collection order, randomness, or scheduling. The typed projection fixes their wire order, while other newly supported optional fields remain absent because the demo supplies no corresponding value.
  - **Suggested resolution:** No remediation required.

## Judgement

The old golden became obsolete when the tracking snapshot began exposing the active session's stamped armour-cost policy. The two false values truthfully describe the curated definition-free demo session and do not alter the enabled-by-default policy used for normal authored sessions. The delta is exact, deterministic, and contains no swept regression.

```text
ORACLE-RATIFICATION
range: 8b9be05..HEAD
goldens: tracking_snapshot
VERDICT: ratification-sound
```
