"""CLI entry point: ``python -m backend.scripts.demo_seed [--out DIR] [--reseed]``.

Defaults to seeding into ``<repo>/data/demo/`` (resolved against the project
root the same way ``backend/main.py`` resolves its data dir).
"""

from __future__ import annotations

import argparse
import logging
import shutil
import sys
from pathlib import Path

from backend.scripts.demo_seed.domain_codex import SEEDER as CODEX_SEEDER
from backend.scripts.demo_seed.domain_equipment import SEEDER as EQUIPMENT_SEEDER
from backend.scripts.demo_seed.domain_ledger import SEEDER as LEDGER_SEEDER
from backend.scripts.demo_seed.domain_quests import SEEDER as QUESTS_SEEDER
from backend.scripts.demo_seed.domain_sessions import SEEDER as SESSIONS_SEEDER
from backend.scripts.demo_seed.domain_skills import SEEDER as SKILLS_SEEDER
from backend.scripts.demo_seed.driver import format_report, run

# All six per-domain seeders. The driver topologically orders them via each
# Seeder's `depends_on`; declaration order here is irrelevant at runtime.
ALL_SEEDERS = [
    SKILLS_SEEDER,
    CODEX_SEEDER,
    EQUIPMENT_SEEDER,
    SESSIONS_SEEDER,
    LEDGER_SEEDER,
    QUESTS_SEEDER,
]


def _project_root() -> Path:
    """Resolve project root the same way backend/main.py does."""
    return Path(__file__).resolve().parents[3]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="python -m backend.scripts.demo_seed")
    parser.add_argument(
        "--out",
        default="data/demo",
        help="Target data dir (default: data/demo, resolved against project root if relative).",
    )
    parser.add_argument(
        "--reseed",
        action="store_true",
        help="Delete the target dir before seeding (otherwise rows accumulate on top of any existing DB).",
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="Verbose logging.",
    )
    args = parser.parse_args(argv)

    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(levelname)s %(name)s: %(message)s",
    )

    out = Path(args.out)
    if not out.is_absolute():
        out = _project_root() / out

    if args.reseed and out.exists():
        logging.info("Removing existing data dir at %s", out)
        shutil.rmtree(out)

    report = run(out, extra_seeders=ALL_SEEDERS)
    print(format_report(report))

    if report.violations:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
