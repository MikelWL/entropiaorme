"""Demo data seeder for EntropiaOrme.

Produces a `data/demo/` directory (SQLite + settings.json) bundled with the
app for guide-mode playback. The seeded dataset describes a synthetic-but-
believable single-character career: the interactive user guide reads it
through the `/demo/*` API so every walkthrough surface has populated
content to render against.

Architecture:
- ``contract``: Seeder Protocol + CanonicalRefs dataclass.
- ``canonical``: Core seeder. Owns the canonical reference list every other
  seeder reads from. Writes foundational DB rows (settings, equipment library,
  quests, playlists, codex species placeholders) immediately.
- ``driver``: Topologically orders registered seeders, runs them in sequence
  against a target data dir, validates synthetic-data invariants.
- ``live_injection``: HuntTracker scenario loader. The module ships in
  frozen builds because the ``/demo/*`` router imports ``prime_tracker``
  from it to populate the in-memory demo tracker. The env-var-driven path
  (``ENTROPIAORME_DEMO_SCENARIO``) that primed the live tracker in dev is
  gated off in frozen builds at the call site in
  ``backend/tracking/tracker.py``; only the deliberate demo-router call
  reaches ``prime_tracker`` in production.
- ``__main__``: CLI entry point: ``python -m backend.scripts.demo_seed``.

Synthetic-only posture: every value the seeder writes is fictional, designed
to render plausibly in the guide's walkthrough surfaces. Synthetic character
name, fictional ledger numbers, synthetic timestamps relative to a "demo now"
anchor. The module synthesises values from the canonical refs in
``contract.py``; it does not import or reference live tracking data.
"""
