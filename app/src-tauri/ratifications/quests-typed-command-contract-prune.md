# Ratification: quests + playlists HTTP-contract pruning (typed-command migration)

Adversarial review of the OpenAPI contract deletion and the corpus-golden
interplay that accompany migrating the quests + playlists route family off the
in-process HTTP router onto typed Tauri IPC commands (ADR-0019). The verdict is
re-derived against the current tree rather than accepting the change author's
rationale, because a self-approved golden move carries a structural conflict of
interest.

## Change under review

The quests + playlists family is now served by 15 typed commands
(`entropia-orme/src/commands.rs`, registered in `entropia-orme/src/lib.rs`
`generate_handler!`) dispatching into the `eo-api` facade
(`eo-api/src/quests.rs`), which types the boundary over the unchanged
`eo_services::quests::QuestService` (snake_case `Value` in, camelCase DTO out:
the facade ports the old `hydration.rs` `_format_quest` / `_format_playlist` /
analytics formatters into typed `from_service` builders). The family's HTTP
read handlers and write adapters (`eo-http/src/native.rs`), the hydration
read/write methods and their formatters (`eo-http/src/hydration.rs`), the route
registrations, and the quests-exclusive integer-path-id apparatus
(`PathId` / `path_id` / `PathParam` / `path_param`) are deleted. The two
`/api/tracking/session/{session_id}/quest-link{,-suggestion}` routes are
tracking-session-scoped and stay (a later family). Behaviour is pinned by
`eo-api/tests/quests_facade.rs`, including byte-for-byte transport-invariance
pins on a created quest and a created playlist.

## Oracle ratification audit

I re-derived every claim against the working tree rather than accepting the
rationale. All checks were run on the semantic JSON structure, not the git
line-diff (which is misleading here).

### Findings

**OR1, `contracts/openapi.snapshot.json`: genuine-spec-move.**
The git line-diff shows 973 "added" lines, which superficially contradicts the
"pure line-subsequence" claim. The claim holds anyway: a structural parse shows
exactly 10 paths removed (all `/api/quests*`), exactly the 12 named schemas
removed, **zero paths/schemas added**, and **zero surviving path or schema
altered** (deep-equal per key). An independent in-order line-subsequence check
returns **0 misses**: the new file is a strict subsequence of the parent. The
973 "additions" are pure Myers re-anchoring across the removed span (surviving
schemas `ProcessingProgress` / `QuestLinkDecisionResult` / `QuestLinkSuggestion`
sit alphabetically adjacent to the removed `Quest*` block). 6272→4740
confirmed. Pure prune.

**OR2, snapshot orphan/reachability: genuine-spec-move.**
Transitive `$ref` reachability from all surviving paths: 61 distinct refs,
**zero dangling**, **zero pointing at any removed schema**. `OkResponse`
specifically is referenced by **no surviving path**. The two quest-link routes
correctly survive. Orphan set is exact.

**OR3, `frontend/src/lib/api/schema.d.ts`: genuine-spec-move.**
Pure deletion: **0 added lines**, 957 removed. All removed content is
`/api/quests*` path scaffolding and the quests / playlist / `OkResponse` type
blocks. No surviving reference to any deleted type and no surviving `/api/quests`
path remain. Consistent with a clean `gen:api` regen off the pruned snapshot
(deterministic generator + unchanged surviving schemas ⇒ pure deletion is
exactly the expected output).

**OR4, corpus goldens / replay cardinality: genuine-spec-move.**
`git status` confirms **zero `fixtures/corpus/**/expected/**` byte changes**.
The replay `endpoint_table` dropped the four `GET_quests*` reads (guard 9→5).
The symbol-renumbering safety argument was verified empirically, not just
logically: surviving order is `snapshot, sessions, session_detail,
quest-link-suggestion, scan_skills_status`. The quests reads sat between
position 4 and the sole post-span endpoint `GET_scan_skills_status`, whose
golden carries **no UUID, timestamp, or `<...>` placeholder** (static
booleans / ints / `null`), so its fingerprint is invariant to symbol numbering.
`quest-link-suggestion` (position 4, before the span) carries
`<UUID_1>` / `<STRONG_ETAG>` assigned ahead of the removed span, so it is
unaffected. Removing the mid-order span renumbers nothing surviving. Goldens
unchanged on disk is the empirical confirmation.

**OR5, error-class mapping (fix-vs-adapt): genuine-spec-move.**
The highest regression risk: mapping `QuestError → ApiError::Internal` would
launder a regression if the HTTP layer had returned a client error (400/422)
for `QuestError::Invalid`. It did not. At HEAD, `hydration.rs` `quest_error_response(_error: QuestError)`
**ignores the variant and unconditionally returns `internal_error()`** (500);
reads used `Err(_) => internal_error()`. So all three `QuestError` variants
(`Invalid`, `Db`, `Rollup`) were already 500s. The facade's catch-all
`quest_error` (`quests.rs`) mapping every variant to `ApiError::internal` is
byte-behaviour-preserving; no error class introduced or dropped, and no
`bad_request` arm existed to lose. Not-found is handled off the service
`Option` (None → `not_found`), matching the deleted `quest_not_found` /
`playlist_not_found` (`NOT_FOUND` + `"Quest not found"` / `"Playlist not
found"`), messages identical.

**OR6, retired transport behaviours: genuine-spec-move.**
- `{"ok":true}` delete body: deleted `delete_quest` / `delete_playlist` emitted
  `plain_json_response(json!({"ok":true}))`; facade returns `()`. Retirement of
  a body no consumer reads (equipment precedent). Genuine.
- ETag / conditional-GET: deleted reads threaded `if_none_match` /
  `json_response`; over IPC the body is returned directly. Genuine.
- 422 / surrogate-taint / beyond-`i64` deferred-500: these were pre-service
  request-*build* errors (`Built::Deferred500`, the 422 envelope), genuinely
  unrepresentable over typed DTO args (a typed field cannot carry a wrong type;
  an `i64` argument cannot overflow a parse). Genuine.
- Integer-path-id 422/404 apparatus (`PathId` / `PathParam`) retires with typed
  `i64` command args. Genuine.

**OR7, determinism: clean.**
Removed golden content is static schema; nothing ambient added (0 added lines
in `schema.d.ts`). The facade builds `QuestService` over an injected
`Arc<dyn Clock>` (`build_quests_service`); no wall-clock / random / env leak
into any pinned output. The facade contract is first-gen-pinned by
`eo-api/tests/quests_facade.rs` (8 substantive tests: wire-shape read-back,
present-null clearing, lifecycle, playlist membership derivation, typed
not-found legs, soft-delete), corroborating that the behaviour relocated to a
guarded facade rather than being lost.

**OR8, orphaned per-endpoint quests goldens: inconsequential.**
The `GET_quests*.json` golden files remain on disk after leaving the replay
`endpoint_table`. Not a regression: they are still referenced by
`eo-api/tests/facade_microbench.rs` and `raw_captures/http_responses.json`, and
are unchanged. No action required.

## Summary judgement

Every element of the two-golden delta is accounted for by the single intended
movement: the quests + playlists family relocating from the in-process HTTP
router to typed IPC (ADR-0019). The snapshot prune is a proven pure in-order
subsequence with an exact, fully-reachable orphan set and `OkResponse` safely
unreferenced; `schema.d.ts` is a consistent pure deletion; no corpus golden
byte changed and the replay cardinality drop is empirically symbol-neutral. The
fix-versus-adapt call is correct: the old pins were not right-and-regressed,
they described an HTTP surface that legitimately no longer exists, and the
surviving behaviour (error classes, not-found messages, retired ETag / 422 /
`ok:true`) is preserved or genuinely unrepresentable, verified against the
deleted production code (notably `quest_error_response` proving the
500-for-all contract was pre-existing). No unaccounted delta, no snapshot of a
bug, no ambient input, no counts/frequencies to launder. This is a genuine spec
move.

```
ORACLE-RATIFICATION
range: 5cf4148..HEAD
goldens: contracts/openapi.snapshot.json, frontend/src/lib/api/schema.d.ts
VERDICT: ratification-sound
```
