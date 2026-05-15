"""Locate and measure the Entropia Universe game window.

Helpers used by the manual scan flow to derive capture regions
from the live game window rather than a fixed-resolution preset table.
On non-Windows platforms the helpers return ``None`` and callers must
handle the missing-window case.
"""

from __future__ import annotations

import sys

GAME_TITLE_PREFIX = "Entropia Universe Client"

if sys.platform == "win32":
    import ctypes
    import ctypes.wintypes

    _user32 = ctypes.windll.user32
else:
    _user32 = None


def find_game_window() -> int | None:
    """Return the HWND of the visible Entropia Universe window, or None."""
    if sys.platform != "win32" or _user32 is None:
        return None

    result: list[int] = []

    @ctypes.WINFUNCTYPE(ctypes.c_bool, ctypes.wintypes.HWND, ctypes.wintypes.LPARAM)
    def enum_callback(hwnd, _lparam):
        length = _user32.GetWindowTextLengthW(hwnd)
        if length == 0:
            return True
        buf = ctypes.create_unicode_buffer(length + 1)
        _user32.GetWindowTextW(hwnd, buf, length + 1)
        if buf.value.startswith(GAME_TITLE_PREFIX) and _user32.IsWindowVisible(hwnd):
            result.append(hwnd)
            return False
        return True

    _user32.EnumWindows(enum_callback, 0)
    return result[0] if result else None


def get_window_geometry(hwnd: int) -> tuple[int, int, int, int] | None:
    """Return (x, y, width, height) of the window's client area in screen coords."""
    if sys.platform != "win32" or _user32 is None:
        return None
    rect = ctypes.wintypes.RECT()
    _user32.GetClientRect(hwnd, ctypes.byref(rect))
    width = rect.right - rect.left
    height = rect.bottom - rect.top
    if width <= 0 or height <= 0:
        return None
    point = ctypes.wintypes.POINT(0, 0)
    _user32.ClientToScreen(hwnd, ctypes.byref(point))
    return (point.x, point.y, width, height)
