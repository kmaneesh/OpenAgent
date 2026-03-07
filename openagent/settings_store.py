"""Persistent key-value settings store backed by the shared openagent.db SQLite file."""

from __future__ import annotations

from pathlib import Path

import aiosqlite


class SettingsStore:
    """Async key-value store in the `settings` table of openagent.db.

    Keys are plain strings (e.g. ``"connector.slack.enabled"``).
    Values are stored as TEXT; callers convert to the appropriate type.

    The store opens its own connection to the DB file.  SQLite handles
    concurrent access from the SessionManager's connection via WAL journal.
    """

    def __init__(self, db_path: Path) -> None:
        self._path = db_path
        self._db: aiosqlite.Connection | None = None

    async def start(self) -> None:
        self._db = await aiosqlite.connect(self._path)
        self._db.row_factory = aiosqlite.Row
        await self._db.execute("PRAGMA journal_mode=WAL")
        await self._db.execute("""
            CREATE TABLE IF NOT EXISTS settings (
                key        TEXT PRIMARY KEY,
                value      TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        """)
        await self._db.commit()

    async def stop(self) -> None:
        if self._db:
            await self._db.close()
            self._db = None

    async def get(self, key: str, default: str = "") -> str:
        assert self._db, "SettingsStore not started"
        async with self._db.execute(
            "SELECT value FROM settings WHERE key = ?", (key,)
        ) as cur:
            row = await cur.fetchone()
        return row[0] if row else default

    async def set(self, key: str, value: str) -> None:
        assert self._db, "SettingsStore not started"
        await self._db.execute(
            """
            INSERT INTO settings (key, value, updated_at)
            VALUES (?, ?, datetime('now'))
            ON CONFLICT(key) DO UPDATE
                SET value = excluded.value,
                    updated_at = excluded.updated_at
            """,
            (key, value),
        )
        await self._db.commit()

    async def get_all(self, prefix: str = "") -> dict[str, str]:
        assert self._db, "SettingsStore not started"
        if prefix:
            async with self._db.execute(
                "SELECT key, value FROM settings WHERE key LIKE ?",
                (f"{prefix}%",),
            ) as cur:
                rows = await cur.fetchall()
        else:
            async with self._db.execute("SELECT key, value FROM settings") as cur:
                rows = await cur.fetchall()
        return {row[0]: row[1] for row in rows}
