"""Synchronous in-process event dispatch."""

from __future__ import annotations

import logging
import threading
from collections.abc import Callable
from typing import Any

log = logging.getLogger(__name__)


class EventBus:
    """Thread-safe pub/sub for app services that share process memory."""

    def __init__(self):
        self._subscribers: dict[str, list[Callable[[Any], None]]] = {}
        self._lock = threading.RLock()

    def subscribe(self, event_type: str, callback: Callable[[Any], None]) -> None:
        with self._lock:
            callbacks = self._subscribers.setdefault(event_type, [])
            if callback not in callbacks:
                callbacks.append(callback)

    def unsubscribe(self, event_type: str, callback: Callable[[Any], None]) -> None:
        with self._lock:
            callbacks = self._subscribers.get(event_type)
            if not callbacks:
                return
            try:
                callbacks.remove(callback)
            except ValueError:
                return
            if not callbacks:
                self._subscribers.pop(event_type, None)

    def has_subscribers(self, event_type: str) -> bool:
        with self._lock:
            return bool(self._subscribers.get(event_type))

    def publish(self, event_type: str, data: Any = None) -> None:
        with self._lock:
            callbacks = tuple(self._subscribers.get(event_type, ()))

        for callback in callbacks:
            try:
                callback(data)
            except Exception:
                log.exception("Event subscriber failed for %s", event_type)
