"""Screen-region capture helper."""

from __future__ import annotations

import threading

import numpy as np

try:
    import mss
except ImportError:
    mss = None


__all__ = ["ScreenCapturer"]


class ScreenCapturer:
    """Capture rectangular screen regions as BGR uint8 arrays."""

    def __init__(self):
        if mss is None:
            raise ImportError("mss is required for screen capture")
        self._local = threading.local()

    def stop(self) -> None:
        sct = getattr(self._local, "sct", None)
        if sct is not None:
            close = getattr(sct, "close", None)
            if close is not None:
                close()
            del self._local.sct

    def _sct(self):
        sct = getattr(self._local, "sct", None)
        if sct is None:
            sct = mss.mss()
            self._local.sct = sct
        return sct

    def capture_region(self, x: int, y: int, width: int, height: int) -> np.ndarray:
        """Capture a screen rectangle and return a BGR uint8 image."""
        if width <= 0 or height <= 0:
            raise ValueError("capture dimensions must be positive")
        shot = self._sct().grab(
            {"left": int(x), "top": int(y), "width": int(width), "height": int(height)}
        )
        return np.asarray(shot, dtype=np.uint8)[:, :, :3]
