# Ratification: healing attribution demo contracts

Independent adversarial review of the two demo golden additions caused by exposing
healing attribution in the active tracking snapshot and session detail response.
The review compared the semantic JSON trees, the production read paths, the demo
database, and the focused service tests.

## Change under review

`demo_goldens/tracking_snapshot.txt` gains one `healing` object describing the
active healing runtime. The untouched demo runtime has no healer intent, effect
window, activation, or output, so the tool and expiry fields are null, the state
is `passive`, and all counters are zero.

`demo_goldens/tracking_session_detail.txt` gains one `healing` evidence summary.
The demo database contains no healing activation, effect-window, or output rows,
so its activation list is empty and all evidence counters are zero.

## Evidence reviewed

- Removing the new `healing` member from each regenerated JSON value reproduces
  its previous golden exactly. No existing leaf, ordering, count, or economic
  value changed.
- The active snapshot is built from a fresh healing runtime. Its empty fallback
  contains no wall-clock or environment-dependent value.
- Session detail derives its evidence summary from the three additive healing
  tables. The demo fixture writes no rows to those tables and retains its
  existing zero healing cost.
- Populated and empty service-level tests cover the same read paths, so these
  zero values are evidence of the demo's data rather than a universally blank
  implementation.
- Neither addition fabricates an activation, passive output, cost, or provenance
  record for legacy history.

## Judgement

Both additions are minimal, deterministic representations of intentionally new
response fields. The previous fixtures are obsolete, and the regenerated values
do not absorb an unrelated behavioural or accounting regression.

```text
ORACLE-RATIFICATION
range: a50eae8d85488cac5fc251b51bc7979465ce825c..272a68775d41380582c8c25ab111dc1bd5eb558c
goldens: app/src-tauri/eo-api/resources/demo_goldens/tracking_snapshot.txt, app/src-tauri/eo-api/resources/demo_goldens/tracking_session_detail.txt
VERDICT: ratification-sound
```
