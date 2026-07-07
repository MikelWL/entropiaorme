# Service and crate map

The desktop backend is a Rust workspace that runs inside the Tauri shell process and is dispatched in-process over a single seam: typed Tauri IPC commands that dispatch into the `eo-api` facade ([ADR-0019](../adr/0019-typed-command-facade.md)), which reads through the domain services. The command DTOs are typed structs whose TypeScript bindings are generated from the Rust source by `cargo xtask gen-ts`; the sole non-typed IPC command is `capture_png`, which returns raw image bytes. This page enumerates the workspace crates and the services they own, then catalogues the operations the facade exposes.

The backend was ported from a Python FastAPI sidecar one route at a time behind a strangler-fig proxy seam; once the port was complete the sidecar and the proxy were removed and the backend collapsed into the shell. That history is recorded in [ADR-0001](../adr/0001-strangler-fig-port.md) (superseded) and [ADR-0013](../adr/0013-in-process-collapse.md). Throughout the port the Rust output was graded against a Python reference implementation that served as a cross-language equivalence oracle ([ADR-0005](../adr/0005-cross-language-equivalence-oracle.md)); that oracle has since been retired ([ADR-0016](../adr/0016-retire-equivalence-oracle.md)), leaving the Rust workspace as the sole implementation. For the surrounding runtime topology see the [System overview](overview.md).

## The Rust workspace

The workspace manifest is `app/src-tauri/Cargo.toml`. It declares five members with `resolver = "2"` (the four below plus the `xtask` guard runner). Three of them (the `eo-*` crates) are deliberately free of any dependency on the Tauri toolchain, so continuous integration can build and test them on a runner without the GUI system stack; that structurally prevents a window-system dependency from creeping into backend code. Only `entropia-orme` is coupled to Tauri.

| Crate | Responsibility | Depends on |
|---|---|---|
| `entropia-orme` | The Tauri shell and composition root: window chrome, the typed IPC commands, the domain-event bridge, and the wiring that constructs the native services and publishes them to the running substrate. | `eo-api`, `eo-services`, `eo-wire` (and Tauri) |
| `eo-api` | The typed IPC facade: the request/response DTO types (the single Rust source the TypeScript bindings generate from, via `cargo xtask gen-ts`), the typed error contract, and one async method per operation. | `eo-services`, `eo-wire` |
| `eo-services` | The domain service layer: the tracker, quests, codex, character and cost calculators, the scan and OCR services, the chat-log and input listeners, the persistence handle, and the supporting stores. | `eo-wire` |
| `eo-wire` | The wire-format contracts: the typed domain-event union, the domain-event broadcast channel, and the canonicalising emitters that produce the frozen equivalence goldens. | (leaf) |

The dependency direction is strictly one way: `eo-wire` is a leaf, `eo-services` builds on it, `eo-api` builds on both, and `entropia-orme` sits at the top as the composition root. No `eo-*` crate depends on `entropia-orme`, which is what keeps the backend layers Tauri-free.

## Per-crate detail

### `entropia-orme`: the Tauri shell and composition root

Defined by `app/src-tauri/entropia-orme/src/lib.rs` (with the composition logic in `app/src-tauri/entropia-orme/src/composition.rs`). The library target builds as `lib`, `cdylib`, and `staticlib`. Its public entry point is `run()`, the Tauri application bootstrap. This crate is the only member coupled to the Tauri toolchain, and it owns three concerns:

- **Window and overlay chrome.** The Tauri command handlers `toggle_overlay`, `show_scan_overlay`, and `hide_scan_overlay` manage the hidden overlay windows; on Windows a runtime-icon installer sets DPI-appropriate window icons.
- **The IPC dispatch and the event bridge.** The backend surface is typed commands (`app/src-tauri/entropia-orme/src/commands.rs`), one thin `#[tauri::command]` per operation delegating to the composed `eo_api::Api` against the published substrate state; the one exception is `capture_png`, which returns raw image bytes rather than a typed DTO. The companion `spawn_domain_event_bridge` subscribes to the event-stream hub and re-emits each frame onto the Tauri event system, the in-process replacement for the frontend's former HTTP event stream.
- **Native-service composition.** `composition.rs` resolves the data directory, opens the application database, loads the game-data snapshot, constructs the ported services over the real clock, and discharges the ONNX Runtime obligations for the OCR recogniser (pinning the bundled dynamic library to an absolute path and configuring the execution-provider ladder). `compose_substrate` (`lib.rs`) runs this publish-last: it builds the state, installs the composed services, and only then publishes the composed `eo_api::Api` to the command layer, emitting `substrate:native-installed` so the frontend re-drives its initial reads. Until that point the commands answer a not-ready error; if composition declines, the facade is never published.

### `eo-api`: the typed IPC facade

Defined by `app/src-tauri/eo-api/src/lib.rs`. The facade is the application boundary the typed Tauri commands call into: `Api` is built whole from the composed services after the database opens (construct-then-share; no lock-guarded optional slots) and published to the IPC layer in one step. Each operation is one async method taking and returning the DTO types defined beside it (plain `serde` structs with JSON-Schema derives), and `manifest` describes the full command surface machine-readably; `cargo xtask gen-ts` walks it to emit the committed TypeScript bindings (`app/src/lib/api/commands.gen.ts`), held in lock-step by a CI check. Errors cross the boundary as the typed `ApiError` contract (`kind` + `message`). The facade owns the whole backend operation surface: every route family that once rode the in-process HTTP router now resides here, the router and its `eo-http` crate having been deleted as the migration completed ([ADR-0019](../adr/0019-typed-command-facade.md)).

### `eo-services`: the domain services

Defined by `app/src-tauri/eo-services/src/lib.rs`. This crate carries the service layer behind the command facade, built up service by service over the course of the migration. Its modules group as follows:

- **Live tracking.** `tracker` (the `HuntTracker` producer spine), `chatlog_watcher` and `chatlog_parser` (the tailing watcher and line grammar), `session_summary`, `tracking_reads` (the session list/detail reads and the post-hoc session edits), `tracking_models`, `loot_filter`, and `mob_lookup_service`.
- **Quests and codex.** `quests`, `codex`, and `codex_categories`.
- **Ledger analytics.** `analytics` (the Overview/Activity aggregates and the ledger, preset, and inventory CRUD) and `daily_rollup` (the read-model projection those aggregates scale on).
- **Character and cost analytics.** `character_calc`, `cost_engine` (the pure-arithmetic leaf service), `equipment_pricing` (the per-shot and per-use cost lookups over the equipment library), `trifecta_service`, `tt_value_curve`, and `tool_inference`.
- **Configuration.** `config_service` (the settings reader and writer).
- **Scanning and OCR.** `ocr_engine` (the recogniser, EP-agnostic; the runtime wiring is a composition-root concern), `screen_capture`, `skill_scan_manual`, `skill_panel`, `scan_completion`, `scan_drift`, `scan_presets`, and `repair_ocr`. The fuzzy text matching used by these services lives in `fuzzy_match` and `difflib`.
- **Input listeners.** `hotbar_listener` and `spacebar_capture_listener` (the two OS keyboard hooks), behind the `keystroke_source` seam that filters keys at the hook boundary and provides an injectable mock for tests.
- **Skill tracking.** `skill_tracker`.
- **Infrastructure.** `game_data_store` (the bundled game-data snapshot), `db` (the persistence handle), `clock` (the injected-clock seam), `event_bus`, and `eu_window`/`paths` (Windows window-enumeration and path resolution). The `fingerprint_recorder` captures the runtime state that the Rust-side equivalence goldens are emitted from.

The crate is Windows-aware: the input listeners and window enumeration compile platform bindings under `cfg(windows)`. The OCR recogniser binds the ONNX Runtime dynamically, so a host without the runtime skips the engine-running tests honestly rather than failing to build.

### `eo-wire`: the wire contracts

Defined by `app/src-tauri/eo-wire/src/lib.rs`. This leaf crate carries the byte-level contracts. It splits into two groups:

- **The wire-contract spine.** `domain_events` is the typed frontend-facing event union (closed in both directions, camelCase payload keys, a required ISO-8601 UTC `occurred_at`, declaration-order serialisation matching the Python `model_dump_json()` output). `bus` is the monomorphic domain-event broadcast channel that makes "a typed event on a domain topic" a compiler-checked invariant: a bounded tokio broadcast (capacity set at composition) with skip-to-live lag semantics for a slow receiver, which the shell's domain-event bridge consumes onto the Tauri event system.
- **The equivalence-golden emitters.** `normalizer` is the shared canonicaliser (UUIDs to sequential symbols, timestamps to symbols, floats rounded to four decimal places, keys sorted, serialised through a faithful reimplementation of Python's `json.dumps` including its float formatting). `fingerprint` emits the event-stream JSONL golden, `db_snapshot` emits the database-state snapshot golden, and `http_fingerprint` emits the HTTP-response golden. These emitters produce the frozen goldens that preserve the equivalence evidence; the project's hermetic tests assert the live Rust output against those committed goldens. The history behind them is recorded in [ADR-0005](../adr/0005-cross-language-equivalence-oracle.md) and [ADR-0016](../adr/0016-retire-equivalence-oracle.md).

## The equivalence oracle, retired

During the port the Rust workspace was graded against a cross-language equivalence oracle: a Python reference implementation whose output the native code was asserted against byte-for-byte. That oracle has been retired now that the port is complete, and the Python reference implementation no longer exists in the repository. The equivalence evidence it produced is preserved as frozen Rust-side goldens, which the project's hermetic tests assert the live Rust output against, so an unintended change to native behaviour fails a test rather than reaching a user. The retirement, and the move to self-contained Rust-side goldens, is recorded in ADR-0016.

## The operation surface

Every backend operation is a typed command dispatched into `eo-api::Api`; a command invoked before its backing service has composed answers a not-ready error until composition completes, after which it serves. The operations group by domain area as follows (each row's method/path notation denotes the operation's read/write semantics and its resource, the shape the facade preserves from the surface's origins):

| Area | Operations |
|---|---|
| Quests | quest list and create; `mobs`; `analytics`; playlists list/create, playlist analytics, playlist update/delete; quest read/update/delete; quest `start`, `complete`, `cancel` |
| Codex | `species`; species `ranks`; `recommend`; `calibrate`; `claim`; meta `claim`; meta `attributes` |
| Analytics | `overview`; `activity`; the ledger (list, create, entry delete), presets (list, create, delete), and inventory (list, create, item patch/delete, item `sell`) |
| Tracking (reads) | `sessions`; session read; `tag-suggestions`; `snapshot` |
| Tracking (producer) | `start`; `stop`; `manual-mob-suggestions`; `release-mob`, `manual-mob-lock`, `tag-lock` |
| Tracking (session edits) | session `rename-mob`, `restore-mob`, the loot-item flip, `armour-cost`, `quest-link`, `repair-scan`, session delete; `quest-link-suggestion` |
| Scan | skills `status`; `start`, `capture`, `cancel`, `undo`, `process`, `accept`, `reject`; skills capture read by page; skills `pending`; `spacebar-capture` |
| Settings | settings read/patch; overlay-position read/update |
| Character | `calibration`, `stats`, `skills`, `professions`, `prospect-options`, `prospect`, `profession-optimizer`, `profession-path-optimizer`, `hp-optimizer` |
| Demo (guide mode) | the guide-mode reads: analytics `overview`, `activity`, `ledger`, ledger `presets`, `inventory`; tracking `sessions`, session read, `snapshot` |

The settings patch and reset writes signal the live producers (restarting the chat-log watcher on a path change, toggling the hotbar hooks, and reloading the tracker configuration) so a settings change reconciles without a restart. The demo namespace is a parallel set of typed commands sharing the live commands' DTO types, backed by a lazily-built demo state over a writable clone of the bundled demo database. Domain events do not travel as commands; they reach the frontend over the Tauri event bridge described in the [System overview](overview.md). Beyond the areas above sit the developer-mode-gated dev-tools operations (the metrics snapshot and the crash-reporting toggle); these are gated on developer mode and decline when it is disabled, so they sit off the equivalence-covered surface.

For the event contract carried over the bridge, see the [Event taxonomy](event-taxonomy.md). For the OCR services behind the scan commands, see the [OCR pipeline](ocr-pipeline.md). For the shared SQLite database the read surface and the producer spine both use, see the [Database schema reference](database-schema.md).
