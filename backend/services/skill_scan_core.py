"""Skill scan core — screenshot capture + local OCR extraction primitives.

Owns the mss capture path and the OpenOCR-backed local extraction. Skill
panel cells are sliced via the calibrated geometry in
``backend/data/panel_geometry.json`` and read per-cell by
:mod:`backend.services.local_ocr`. Names resolve through fuzzy match
against ``backend/data/snapshot/skills.json`` so what we persist is the
canonical vocab entry, not raw OCR text.
"""

import logging
from pathlib import Path
from typing import Any

from backend.services import local_ocr

log = logging.getLogger(__name__)

try:
    import mss
    import mss.tools
    MSS_AVAILABLE = True
except ImportError:
    mss = None
    MSS_AVAILABLE = False

PAGE_COUNT = 12


class SkillScanCore:
    """Screenshot + local OCR scan primitives for skill pages."""

    def __init__(self, config_service: Any, data_dir: Path):
        from backend.services.config_service import ConfigService
        self._config: ConfigService = config_service
        self._data_dir = data_dir

    @property
    def has_engine(self) -> bool:
        """Whether the local OCR engine can be loaded right now."""
        return local_ocr.is_engine_available()

    def capture_region(self, tl: list[int] | None, br: list[int] | None) -> bytes | None:
        """Capture the skill panel region as PNG bytes via mss."""
        if not MSS_AVAILABLE or not tl or not br:
            return None
        x1, y1 = tl
        x2, y2 = br
        monitor = {
            "left": min(x1, x2),
            "top": min(y1, y2),
            "width": abs(x2 - x1),
            "height": abs(y2 - y1),
        }
        if monitor["width"] <= 0 or monitor["height"] <= 0:
            return None
        try:
            with mss.mss() as sct:
                screenshot = sct.grab(monitor)
                return mss.tools.to_png(screenshot.rgb, screenshot.size)
        except Exception:
            return None

    def extract_page_levels(self, png_bytes: bytes) -> dict[str, float]:
        """Run local OCR on a single page PNG; return {canonical_name: level}."""
        try:
            panel_bgr = local_ocr.decode_panel_png(png_bytes)
        except ValueError as exc:
            log.warning("skill scan: PNG decode failed: %s", exc)
            return {}
        rows = local_ocr.read_skill_panel(panel_bgr)
        out: dict[str, float] = {}
        for row in rows:
            name = row.get("name")
            level = row.get("level")
            if name and level is not None:
                out[name] = float(level)
        return out
