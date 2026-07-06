# Ratification: retire the OpenAPI snapshot with the HTTP transport

## Change under review

Every backend route family now serves over typed IPC commands, so the
in-process HTTP transport has no remaining tenant and is deleted wholesale.
This retires the last of the OpenAPI golden surface with it:

- `contracts/openapi.snapshot.json` (the `openapi` golden set) is deleted. At
  the base it held exactly two paths: `GET /api/health` and
  `DELETE /api/tracking/session/{session_id}`, plus the six schemas they close
  over.
- The regenerated frontend client type `frontend/src/lib/api/schema.d.ts` and
  the `openapi-typescript` / `openapi-fetch` toolchain that consumed the
  snapshot are deleted.
- The wire-model conformance registry that pinned the snapshot
  (`eo-wire/src/models.rs`: `registered_contracts` / `WireModel` /
  `ModelContract` and the `HealthStatus` / `NotableEvent` / `TrackingSnapshot`
  models) and its test `eo-wire/tests/openapi_conformance.rs` are deleted.

## Why this is a sound retirement, not a dropped contract

Both surviving snapshot paths are accounted for:

- `GET /api/health` had no remaining caller. The only frontend reference to it
  was the client-seam test's path literal, exercising the generated client
  rather than a live route. It retires with the transport.
- `DELETE /api/tracking/session/{session_id}` is migrated to a working typed
  command `tracking_session_delete` (`eo-api/src/tracking.rs`), registered in
  the manifest and shell and pinned by `eo-api/tests/tracking_facade.rs`. The
  command performs a real cascade delete (the kill-scoped rows, the kills, the
  skill gains, notable events, the summary, and the session) in one
  transaction and repairs the affected daily rollups, restoring behaviour that
  the old path silently dropped (no DELETE route was ever registered, so the
  action returned a not-found and left the session in place). An active session
  is a conflict and a missing one a not-found, matching the retired contract.

No live consumer of the snapshot remains: the frontend speaks only typed
commands (`commands.gen.ts`), and the retired conformance models had no
consumer beyond the deleted health route and their own test (the typed facade
carries its own `TrackingSnapshot` DTO). The frozen equivalence evidence
(`eo-wire/tests/emitters_proof.rs`, `eo-wire/src/http_fingerprint.rs`, and the
corpus goldens) is independent of the snapshot and the deleted registry, and
stays intact and passing. The sibling `event_schemas.snapshot.json` golden is
untouched.

## Verdict

```text
ORACLE-RATIFICATION
range: 2a34fc0..HEAD
goldens: openapi
VERDICT: ratification-sound
```
