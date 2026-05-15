"""Helpers for comparing tracked levels against fresh scan results."""

from collections.abc import Mapping
from typing import Any


def summarize_level_drift(
    tracked_levels: Mapping[str, float],
    scanned_levels: Mapping[str, float],
) -> dict[str, Any] | None:
    """Summarise drift between the app's tracked levels and a fresh scan."""
    tracked_names = set(tracked_levels)
    scanned_names = set(scanned_levels)
    shared_names = tracked_names & scanned_names
    if not shared_names:
        return None

    total_abs_diff = 0.0
    total_signed_diff = 0.0
    total_abs_pct = 0.0
    worst_name = ""
    worst_tracked = 0.0
    worst_scanned = 0.0
    worst_signed_diff = 0.0
    worst_abs_diff = -1.0

    for name in sorted(shared_names):
        tracked = float(tracked_levels[name])
        scanned = float(scanned_levels[name])
        signed_diff = scanned - tracked
        abs_diff = abs(signed_diff)
        abs_pct = abs_diff / max(abs(scanned), 1.0) * 100.0

        total_abs_diff += abs_diff
        total_signed_diff += signed_diff
        total_abs_pct += abs_pct

        if abs_diff > worst_abs_diff:
            worst_name = name
            worst_tracked = tracked
            worst_scanned = scanned
            worst_signed_diff = signed_diff
            worst_abs_diff = abs_diff

    compared_count = len(shared_names)
    return {
        "compared_count": compared_count,
        "tracked_only_count": len(tracked_names - shared_names),
        "scan_only_count": len(scanned_names - shared_names),
        "total_abs_diff": total_abs_diff,
        "avg_abs_diff": total_abs_diff / compared_count,
        "total_signed_diff": total_signed_diff,
        "avg_abs_pct": total_abs_pct / compared_count,
        "worst_name": worst_name,
        "worst_tracked": worst_tracked,
        "worst_scanned": worst_scanned,
        "worst_signed_diff": worst_signed_diff,
        "worst_abs_diff": worst_abs_diff,
    }
