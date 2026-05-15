"""Base SQLite database with WAL mode and migration support."""

import sqlite3
import threading
from pathlib import Path


class BaseDatabase:
    """SQLite database with WAL mode, version-counter migrations.

    Subclasses define DB_VERSION and implement _migrate().

    The connection is opened with ``check_same_thread=False`` and is shared
    across the FastAPI threadpool plus background worker threads. SQLite's
    own serialisation protects single ``execute`` calls, but Python-level
    multi-step patterns (``execute`` + ``fetchall``, or batched writes
    inside a transaction) need external serialisation to keep cursor state
    coherent. Use ``with db.lock:`` as a re-entrant context manager around
    any compound DB operation that crosses threads.
    """

    DB_VERSION: int = 0

    def __init__(self, db_path: Path | str):
        self.db_path = Path(db_path)
        self.db_path.parent.mkdir(parents=True, exist_ok=True)

        self.lock = threading.RLock()
        self.conn = sqlite3.connect(
            str(self.db_path),
            check_same_thread=False,
        )
        self.conn.row_factory = sqlite3.Row
        self._configure_pragmas()
        self._ensure_meta_table()
        self._migrate_if_needed()

    def _configure_pragmas(self) -> None:
        self.conn.execute("PRAGMA journal_mode = WAL")
        self.conn.execute("PRAGMA synchronous = NORMAL")
        self.conn.execute("PRAGMA busy_timeout = 5000")
        self.conn.execute("PRAGMA cache_size = -8000")  # 8 MB page cache

    def _ensure_meta_table(self) -> None:
        self.conn.execute("""
            CREATE TABLE IF NOT EXISTS db_metadata (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )
        """)
        self.conn.commit()

    def _get_version(self) -> int:
        row = self.conn.execute(
            "SELECT value FROM db_metadata WHERE key = 'version'"
        ).fetchone()
        return int(row["value"]) if row else 0

    def _set_version(self, version: int) -> None:
        self.conn.execute(
            "INSERT OR REPLACE INTO db_metadata (key, value) VALUES ('version', ?)",
            (str(version),),
        )
        self.conn.commit()

    def _migrate_if_needed(self) -> None:
        current = self._get_version()
        if current < self.DB_VERSION:
            self._migrate(current)
            self._set_version(self.DB_VERSION)

    def _migrate(self, from_version: int) -> None:
        """Override in subclasses to apply migrations."""
        raise NotImplementedError

    def close(self) -> None:
        self.conn.close()
