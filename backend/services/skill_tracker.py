"""Skill gain tracker — records chat.log skill events during tracking sessions.

Subscribes to EVENT_SKILL_GAIN on the event bus. During active sessions,
records each gain to the skill_gains table with TT value computation.
Also increments the calibrated skill level so TT values stay accurate
between full scans.
"""

import logging
import threading
import time as _time
from datetime import datetime

from backend.core.event_bus import EventBus
from backend.core.events import (
    EVENT_SKILL_GAIN,
    EVENT_SESSION_STARTED,
    EVENT_SESSION_STOPPED,
)
from backend.data.tt_value_curve import tt_value_of_gain
from backend.services.character_calc import ATTRIBUTE_SKILLS

log = logging.getLogger(__name__)


class SkillTracker:
    """Records skill gains from chat.log during active tracking sessions."""

    def __init__(self, event_bus: EventBus, app_db):
        self._event_bus = event_bus
        self._app_db = app_db
        self._db = app_db.conn
        self._db_lock: threading.RLock = app_db.lock
        self._active = False
        self._session_id: str | None = None

        # In-memory session totals
        self._session_skills: dict[str, float] = {}     # name → total amount
        self._session_skill_tt: dict[str, float] = {}   # name → total TT PED

        # Codex claim suppression: {skill_name: (ped_value, from_level, source, expiry_epoch)}
        self._suppressed_claims: dict[str, tuple[float, float | None, str, float]] = {}

        event_bus.subscribe(EVENT_SKILL_GAIN, self._on_skill_gain)
        event_bus.subscribe(EVENT_SESSION_STARTED, self._on_session_start)
        event_bus.subscribe(EVENT_SESSION_STOPPED, self._on_session_stop)

    def _on_session_start(self, data: dict) -> None:
        self._active = True
        self._session_id = data.get("session_id")
        self._session_skills.clear()
        self._session_skill_tt.clear()
        log.info("Skill tracking started for session %s", self._session_id[:8] if self._session_id else "?")

    def _on_session_stop(self, data: dict) -> None:
        if self._session_skills:
            total_exp = sum(self._session_skills.values())
            total_tt = sum(self._session_skill_tt.values())
            log.info(
                "Skill tracking stopped: %d skills, %.4f exp, %.4f PED TT",
                len(self._session_skills), total_exp, total_tt,
            )
        self._active = False
        self._session_id = None

    def _on_skill_gain(self, data: dict) -> None:
        if not self._active or not self._session_id:
            return

        skill_name: str = data["skill_name"]
        amount: float = data["amount"]
        timestamp: datetime = data["timestamp"]
        ts_epoch = timestamp.timestamp() if isinstance(timestamp, datetime) else float(timestamp)

        # Check codex claim suppression
        if skill_name in self._suppressed_claims:
            expected_ped, from_level, source, expiry = self._suppressed_claims[skill_name]
            if _time.time() < expiry:
                del self._suppressed_claims[skill_name]
                # Diagnostic: use the pre-claim level to compute TT round-trip
                if from_level is not None:
                    curve_ped = tt_value_of_gain(from_level, from_level + amount)
                    deviation = abs(curve_ped - expected_ped)
                    pct = deviation / expected_ped * 100 if expected_ped > 0 else 0
                    log.info(
                        "CODEX SUPPRESSED [%s]: %s +%.4f levels (from %.1f) | "
                        "TT curve says %.4f PED | codex formula predicted %.4f PED | "
                        "delta %.4f PED (%.1f%%)",
                        source, skill_name, amount, from_level,
                        curve_ped, expected_ped, deviation, pct,
                    )
                    # Store as a TT curve observation for ongoing calibration
                    with self._db_lock:
                        self._db.execute(
                            "INSERT INTO tt_curve_observations "
                            "(skill_name, from_level, level_gain, known_ped, curve_ped, deviation, source, observed_at) "
                            "VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                            (skill_name, from_level, amount, expected_ped, curve_ped, deviation, source, _time.time()),
                        )
                        self._db.commit()
                else:
                    log.info(
                        "CODEX SUPPRESSED [%s]: %s +%.4f levels (no calibration — can't verify TT) | "
                        "codex formula predicted %.4f PED",
                        source, skill_name, amount, expected_ped,
                    )
                return
            else:
                # Expired — remove and process normally
                del self._suppressed_claims[skill_name]
                log.info("Suppression for %s expired, processing normally", skill_name)

        # Get current calibrated level for TT computation
        old_level = self._get_current_level(skill_name)
        ped_value = None
        is_attribute = skill_name in ATTRIBUTE_SKILLS

        if old_level is not None:
            new_level = old_level + amount
            # Only compute TT value for regular skills — no attribute curve exists yet
            if not is_attribute:
                ped_value = tt_value_of_gain(old_level, new_level)
            # Insert incremental calibration point (for both skills and attributes)
            with self._db_lock:
                self._db.execute(
                    "INSERT INTO skill_calibrations (skill_name, level, source, scanned_at) VALUES (?, ?, 'chatlog', ?)",
                    (skill_name, new_level, ts_epoch),
                )

        # Insert skill gain record
        with self._db_lock:
            self._db.execute(
                "INSERT INTO skill_gains (session_id, timestamp, skill_name, amount, ped_value) VALUES (?, ?, ?, ?, ?)",
                (self._session_id, ts_epoch, skill_name, amount, ped_value),
            )
            self._db.commit()

        # Update in-memory session totals
        self._session_skills[skill_name] = self._session_skills.get(skill_name, 0.0) + amount
        if ped_value is not None:
            self._session_skill_tt[skill_name] = self._session_skill_tt.get(skill_name, 0.0) + ped_value

    def _get_current_level(self, skill_name: str) -> float | None:
        """Get the latest calibrated level for a skill."""
        with self._db_lock:
            row = self._db.execute(
                "SELECT level FROM skill_calibrations WHERE skill_name = ? ORDER BY scanned_at DESC LIMIT 1",
                (skill_name,),
            ).fetchone()
        return float(row[0]) if row else None

    def suppress_next(
        self, skill_name: str, ped_value: float,
        from_level: float | None = None, source: str = "codex",
        timeout: float = 30.0,
    ) -> None:
        """Register a pending codex claim — suppress the next matching skill gain.

        When the player claims a codex rank in-game, the resulting skill gain
        shows up in chat.log. This method marks that upcoming gain for
        suppression so it isn't double-counted alongside the ledger entry.

        from_level is the pre-claim calibrated level — used to verify the TT
        curve against the known codex PED value when the gain arrives.
        source distinguishes 'codex' (mob skill) from 'codex_meta' (attribute).
        """
        expiry = _time.time() + timeout
        self._suppressed_claims[skill_name] = (ped_value, from_level, source, expiry)
        log.info("Suppressing next %s gain (%.4f PED, from level %.1f, source=%s, expires in %.0fs)",
                 skill_name, ped_value, from_level or 0, source, timeout)
