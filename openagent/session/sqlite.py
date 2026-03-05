"""SQLite session backend — aiosqlite, no ORM, atomic writes."""

from __future__ import annotations

import logging
import uuid
from datetime import datetime
from pathlib import Path
from typing import Literal

import aiosqlite

from .backend import Turn

logger = logging.getLogger(__name__)

_CREATE_SQL = """
CREATE TABLE IF NOT EXISTS turns (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    session_key TEXT    NOT NULL,
    role        TEXT    NOT NULL,
    content     TEXT    NOT NULL,
    tool_call_id TEXT   NOT NULL DEFAULT '',
    tool_name   TEXT    NOT NULL DEFAULT '',
    ts          TEXT    NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_turns_session ON turns (session_key, id);

CREATE TABLE IF NOT EXISTS identity_links (
    channel     TEXT NOT NULL,
    channel_id  TEXT NOT NULL,
    user_key    TEXT NOT NULL,
    last_active TEXT NOT NULL,
    PRIMARY KEY (channel, channel_id)
);
CREATE INDEX IF NOT EXISTS idx_identity_links_user_key ON identity_links (user_key);

CREATE TABLE IF NOT EXISTS link_pins (
    pin        TEXT PRIMARY KEY,
    user_key   TEXT NOT NULL,
    expires_at TEXT NOT NULL
);
"""


class SqliteSessionBackend:
    """Async SQLite backend using aiosqlite.

    All writes use WAL journal mode for concurrency and fsync safety.
    When the Go session service is ready, swap this for GoSessionBackend —
    the SessionManager constructor is the only change required.
    """

    def __init__(self, db_path: Path | str) -> None:
        self._db_path = Path(db_path)
        self._db: aiosqlite.Connection | None = None

    # ------------------------------------------------------------------
    # Lifecycle
    # ------------------------------------------------------------------

    async def start(self) -> None:
        self._db_path.parent.mkdir(parents=True, exist_ok=True)
        self._db = await aiosqlite.connect(str(self._db_path))
        self._db.row_factory = aiosqlite.Row
        await self._db.executescript(_CREATE_SQL)
        await self._db.execute("PRAGMA journal_mode=WAL")
        await self._db.commit()
        logger.debug("SqliteSessionBackend opened %s", self._db_path)

    async def stop(self) -> None:
        if self._db:
            await self._db.close()
            self._db = None

    # ------------------------------------------------------------------
    # Session history
    # ------------------------------------------------------------------

    async def append(
        self,
        session_key: str,
        role: Literal["system", "user", "assistant", "tool"],
        content: str,
        *,
        tool_call_id: str = "",
        tool_name: str = "",
    ) -> None:
        assert self._db, "backend not started"
        ts = datetime.now().isoformat()
        await self._db.execute(
            "INSERT INTO turns (session_key, role, content, tool_call_id, tool_name, ts)"
            " VALUES (?, ?, ?, ?, ?, ?)",
            (session_key, role, content, tool_call_id, tool_name, ts),
        )
        await self._db.commit()

    async def get_history(
        self, session_key: str, *, limit: int = 100
    ) -> list[Turn]:
        assert self._db, "backend not started"
        async with self._db.execute(
            "SELECT role, content, tool_call_id, tool_name, ts FROM turns"
            " WHERE session_key = ?"
            " ORDER BY id DESC LIMIT ?",
            (session_key, limit),
        ) as cursor:
            rows = await cursor.fetchall()
        # Reverse so oldest-first
        return [
            Turn(
                role=r["role"],
                content=r["content"],
                tool_call_id=r["tool_call_id"],
                tool_name=r["tool_name"],
                timestamp=datetime.fromisoformat(r["ts"]),
            )
            for r in reversed(rows)
        ]

    async def set_summary(self, session_key: str, summary: str) -> None:
        """Atomically replace all turns with a single system summary."""
        assert self._db, "backend not started"
        ts = datetime.now().isoformat()
        async with self._db.execute("BEGIN"):
            await self._db.execute(
                "DELETE FROM turns WHERE session_key = ?", (session_key,)
            )
            await self._db.execute(
                "INSERT INTO turns (session_key, role, content, tool_call_id, tool_name, ts)"
                " VALUES (?, 'system', ?, '', '', ?)",
                (session_key, f"[Summary] {summary}", ts),
            )
        await self._db.commit()

    async def clear(self, session_key: str) -> None:
        assert self._db, "backend not started"
        await self._db.execute(
            "DELETE FROM turns WHERE session_key = ?", (session_key,)
        )
        await self._db.commit()

    async def list_sessions(self) -> list[str]:
        assert self._db, "backend not started"
        async with self._db.execute(
            "SELECT DISTINCT session_key FROM turns ORDER BY session_key"
        ) as cursor:
            rows = await cursor.fetchall()
        return [r["session_key"] for r in rows]

    # ------------------------------------------------------------------
    # Cross-channel identity
    # ------------------------------------------------------------------

    async def resolve_user_key(self, channel: str, channel_id: str) -> str:
        """Return (or create) the stable user_key for a channel identity.

        Uses INSERT OR IGNORE so concurrent calls within the same connection
        serialise safely — the second insert is silently dropped and the
        SELECT always returns the winner.
        """
        assert self._db, "backend not started"
        now = datetime.now().isoformat()

        # Fast path: known identity
        async with self._db.execute(
            "SELECT user_key FROM identity_links"
            " WHERE channel = ? AND channel_id = ?",
            (channel, channel_id),
        ) as cur:
            row = await cur.fetchone()

        if row:
            await self._db.execute(
                "UPDATE identity_links SET last_active = ?"
                " WHERE channel = ? AND channel_id = ?",
                (now, channel, channel_id),
            )
            await self._db.commit()
            return row["user_key"]

        # New identity — generate key and insert
        new_key = f"user:{uuid.uuid4().hex[:16]}"
        await self._db.execute(
            "INSERT OR IGNORE INTO identity_links"
            " (channel, channel_id, user_key, last_active) VALUES (?, ?, ?, ?)",
            (channel, channel_id, new_key, now),
        )
        await self._db.commit()

        # Re-select to get the actual winner (handles task-level concurrency)
        async with self._db.execute(
            "SELECT user_key FROM identity_links"
            " WHERE channel = ? AND channel_id = ?",
            (channel, channel_id),
        ) as cur:
            row = await cur.fetchone()
        return row["user_key"]

    async def link_user_keys(self, key_a: str, key_b: str) -> str:
        """Merge key_b into key_a atomically.

        All channel identities and conversation turns that belonged to key_b
        are reassigned to key_a.  key_b disappears from the database.
        Returns key_a.
        """
        assert self._db, "backend not started"
        await self._db.execute(
            "UPDATE identity_links SET user_key = ? WHERE user_key = ?",
            (key_a, key_b),
        )
        await self._db.execute(
            "UPDATE turns SET session_key = ? WHERE session_key = ?",
            (key_a, key_b),
        )
        await self._db.commit()
        return key_a

    async def store_link_pin(
        self, user_key: str, pin: str, expires_at: str
    ) -> None:
        """Persist a one-time link pin valid until ``expires_at`` (ISO string)."""
        assert self._db, "backend not started"
        await self._db.execute(
            "INSERT OR REPLACE INTO link_pins (pin, user_key, expires_at)"
            " VALUES (?, ?, ?)",
            (pin, user_key, expires_at),
        )
        await self._db.commit()

    async def redeem_link_pin(self, redeemer_key: str, pin: str) -> str | None:
        """Validate pin, merge the two sessions, return winning key.

        The generator's session absorbs the redeemer's history.
        Returns None if the pin is invalid, expired, already used, or
        both sides are already the same session.
        """
        assert self._db, "backend not started"
        now = datetime.now().isoformat()

        async with self._db.execute(
            "SELECT user_key FROM link_pins WHERE pin = ? AND expires_at > ?",
            (pin, now),
        ) as cur:
            row = await cur.fetchone()

        if row is None:
            return None

        generator_key = row["user_key"]
        if generator_key == redeemer_key:
            return None  # can't link a session to itself

        # Consume pin — one-time use
        await self._db.execute("DELETE FROM link_pins WHERE pin = ?", (pin,))
        await self._db.commit()

        # Generator's session wins; redeemer's history moves into it
        return await self.link_user_keys(generator_key, redeemer_key)
