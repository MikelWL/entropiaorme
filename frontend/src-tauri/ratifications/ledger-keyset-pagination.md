# Ratification: ledger list keyset pagination

Adversarial review of the OpenAPI contract golden that accompanies serving the
activity ledger as bounded keyset (seek) pages. The verdict is re-derived
against the current tree rather than accepting the change author's rationale,
because a self-approved golden move carries a structural conflict of interest;
this review was produced by an independent oracle-ratification pass.

## Change under review

`/api/analytics/ledger` (and its demo mirror `/api/demo/analytics/ledger`)
previously read the whole `ledger_entries` table on every request. The handler
`list_ledger` (`eo-http/src/analytics_routes.rs`) now serves bounded keyset
pages: newest first, ordered `(date DESC, id DESC)`, seeking past an opaque
base64url `(date, id)` cursor, with the page size a `limit` query parameter
(default 50, capped at 200) and the next page's cursor returned in an
`X-Next-Cursor` response header (absent on the last page). The response **body
stays a JSON array of `LedgerItem`** — the contract's body shape is unchanged —
so single-page consumers and the demo body golden (`analytics_ledger.txt`, 40
entries, below the 50 default) are unaffected. `native.rs` and `demo.rs` thread
the `cursor`/`limit` query params through.

## Oracle delta reviewed

The only committed golden that moved in the range is
`frontend/src-tauri/contracts/openapi.snapshot.json` (set key `openapi`). For
each of the two ledger GET operations the diff ADDS: an optional `cursor` query
param (string, nullable, `required:false`), an optional `limit` query param
(integer, nullable, `required:false`), an optional `X-Next-Cursor` 200 response
header, and an expanded (additive) description on the real path. No existing
field, component schema, `operationId`, `summary`, tag, response-body schema, or
required-ness was changed or removed.

## Verdict

Every element of the delta maps one-to-one onto the keyset-pagination code in
the same commit, verified against the production handler rather than the diff
alone: the two query params bind `cursor: Option<&str>` / `limit: Option<i64>`
(with `LEDGER_PAGE_DEFAULT = 50` and `.clamp(1, 200)`), and the header is
emitted only when a further page exists (`response.headers_mut().insert(
"x-next-cursor", …)`), matching its `required:false` "absent on the last page"
contract. Nothing pre-existing was dropped or altered: all new inputs are
optional and the array-of-`LedgerItem` body is intact, so a single-page consumer
is unaffected (which is why the demo body golden legitimately did not move). The
added tests (`ledger_list_walks_every_entry_by_keyset_cursor`,
`ledger_list_rejects_a_malformed_cursor`) confirm the pinned contract describes
intended behaviour, not a snapshotted bug. This is a genuine, intended,
backward-compatible contract extension — not a regression laundered into the
golden.

```
ORACLE-RATIFICATION
goldens: openapi
range: c2c3efe..HEAD
VERDICT: ratification-sound
```
