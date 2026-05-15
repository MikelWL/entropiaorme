# Testing

EntropiaOrme ships with an integrated Python test suite under `backend/tests/`. The suite exercises the tracker pipeline, chat.log parsing, cost / character / codex math, skill tracking, and quest automation end-to-end.

## Running the suite

From the repo root:

```bash
.venv/Scripts/python.exe -m pytest backend/tests/ -q
```

The Linux / macOS invocation is `.venv/bin/python -m pytest backend/tests/ -q`.

Expected: 319 tests pass in roughly 3 seconds. The suite is deterministic; no flaky tests, no network access, no on-disk state outside `tmp_path` fixtures.

## Test layout

| File | Coverage |
| ---- | -------- |
| `test_codex_formulas.py` | Codex rank multipliers, category cycling, cat4 bonus shape, rank cost and reward PED, inverse TT lookup. |
| `test_chatlog_parser.py` | Every EventType has at least one parametrised case. Critical-hit vs damage-dealt, HoF vs Global, quantity extraction, verbose vs direct skill formats. |
| `test_cost_engine.py` | Per-shot cost breakdown: weapon, amp, scope, absorber, damage enhancers, markups. Heal cost, heal range, damage range, weapon total damage. |
| `test_character_calc.py` | TT value curve anchors, profession level math, skill rank lookup, codex category resolution, HP formula, HP optimiser, profession path optimiser (target and budget modes). |
| `test_codex_service.py` | Species listing with dedup and progress cross-ref, rank breakdowns, claim recording, calibrate, skill-option ranking by profession or HP target, meta claims. |
| `test_skill_tracker.py` | Session-scoped recording, TT-value computation, codex claim suppression (one-shot, with-observation, expiry handling). |
| `test_scan_completion.py` | Scan-time anchor archival: prior `scan` rows move to the archive table, non-`scan` rows (codex / chatlog) stay live. |
| `test_chatlog_watcher.py` | Tick buffering, loot grouping by timestamp, quest-reward suppression (PED / zero-PED / skill), enhancer-break shrapnel matching. |
| `test_quests.py` | Quest CRUD, cooldown, completion routing (ledger vs `quest_claims`), playlist grouping and reorder, mission-name fuzzy matching, session-link suggestions, curated analytics. |
| `test_tracker_integration.py` | Full pipeline via the event bus: kills model, dangling cost, tool-stats merge, manual mob and tag modes, global / HoF correlation, crash recovery on orphaned sessions. |

## Adding tests

The suite uses pytest. No fixtures live outside the test files themselves; each test sets up an in-memory `sqlite3` database via `AppDatabase(tmp_path / "test.db")` or `sqlite3.connect(":memory:")`. The event bus is constructed per test.

Conventions:

- One module per system under test, located at `backend/tests/test_<module>.py`.
- Helpers are file-local (`_make_*`, `_seed_*`) rather than shared fixtures, to keep each file self-contained.
- Use `tmp_path` for any test that touches `AppDatabase`; do not use a persistent location.
- Avoid wall-clock dependencies; use explicit timestamps passed to `bus.publish` or seeded directly into rows.

## Posture

This suite is v0.1.0's core integrated coverage: the pipelines, formulas, and database contracts the rest of the app composes against. Router-level tests, OCR-pipeline tests, and broader unit coverage are deliberately scoped for post-launch expansion. Coverage extension is part of the project's post-1.0 work, not a launch blocker.
