"""SQLite session backend — aiosqlite, no ORM, atomic writes.

Schema versioning
-----------------
A ``schema_version`` table tracks applied migrations.  On every startup:

1. ``_SCHEMA_SQL`` creates all tables that do not yet exist (idempotent).
2. ``_apply_migrations`` checks the current version and runs any pending
   ``_MIGRATIONS`` entries in order, bumping the version after each one.

Adding a new migration:  append a callable to ``_MIGRATIONS``.  Each callable
receives the open ``aiosqlite.Connection`` and must not commit — the caller
commits after recording the new version number.
"""

from __future__ import annotations

import logging
import uuid
from collections.abc import Awaitable, Callable
from datetime import datetime
from pathlib import Path
from typing import Literal

import aiosqlite

from .backend import Turn

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Base schema — all tables in their final form.
# New databases are created with this schema and immediately stamped at the
# latest migration version so no migration callbacks need to run.
# ---------------------------------------------------------------------------

_SCHEMA_SQL = """
CREATE TABLE IF NOT EXISTS schema_version (
    id      INTEGER PRIMARY KEY CHECK (id = 1),
    version INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS users (
    user_key    TEXT PRIMARY KEY,
    name        TEXT NOT NULL DEFAULT '',
    email       TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL,
    last_seen   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS turns (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    session_key  TEXT    NOT NULL,
    role         TEXT    NOT NULL,
    content      TEXT    NOT NULL,
    tool_call_id TEXT    NOT NULL DEFAULT '',
    tool_name    TEXT    NOT NULL DEFAULT '',
    ts           TEXT    NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_turns_session ON turns (session_key, id);

CREATE TABLE IF NOT EXISTS identity_links (
    platform     TEXT NOT NULL,
    platform_id  TEXT NOT NULL,
    user_key     TEXT NOT NULL REFERENCES users(user_key) ON DELETE CASCADE,
    channel_id   TEXT NOT NULL DEFAULT '',
    last_active  TEXT NOT NULL,
    PRIMARY KEY (platform, platform_id)
);
CREATE INDEX IF NOT EXISTS idx_identity_links_user_key ON identity_links (user_key);

CREATE TABLE IF NOT EXISTS link_pins (
    pin        TEXT PRIMARY KEY,
    user_key   TEXT NOT NULL,
    expires_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS session_metadata (
    session_key         TEXT PRIMARY KEY,
    hidden_at           TEXT,  -- NULL = visible; ISO timestamp = soft-deleted
    browser_session_id  TEXT,  -- NULL = no active browser session
    browser_last_active TEXT   -- ISO timestamp of last browser tool call
);

CREATE TABLE IF NOT EXISTS whitelist (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    platform    TEXT NOT NULL,
    channel_id  TEXT NOT NULL,
    label       TEXT NOT NULL DEFAULT '',
    added_by    TEXT NOT NULL DEFAULT '',
    added_at    TEXT NOT NULL,
    UNIQUE(platform, channel_id)
);

CREATE TABLE IF NOT EXISTS blacklist (
    platform      TEXT NOT NULL,
    channel_id    TEXT NOT NULL,
    first_seen    TEXT NOT NULL,
    last_seen     TEXT NOT NULL,
    message_count INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (platform, channel_id)
);
"""

# ---------------------------------------------------------------------------
# Migrations — ordered list of async callables.
# Each entry upgrades the schema from version N-1 → N.
# Migrations must be idempotent (use IF NOT EXISTS / PRAGMA checks).
# ---------------------------------------------------------------------------

_MigrationFn = Callable[[aiosqlite.Connection], Awaitable[None]]


async def _migration_1_add_browser_columns(db: aiosqlite.Connection) -> None:
    """v0 → v1: add browser session tracking columns to session_metadata."""
    async with db.execute("PRAGMA table_info(session_metadata)") as cur:
        cols = {row[1] async for row in cur}
    if "browser_session_id" not in cols:
        await db.execute(
            "ALTER TABLE session_metadata ADD COLUMN browser_session_id TEXT"
        )
    if "browser_last_active" not in cols:
        await db.execute(
            "ALTER TABLE session_metadata ADD COLUMN browser_last_active TEXT"
        )


_MIGRATIONS: list[_MigrationFn] = [
    _migration_1_add_browser_columns,  # index 0 → schema version 1
]


# ---------------------------------------------------------------------------
# Backend
# ---------------------------------------------------------------------------


class SqliteSessionBackend:
    """Async SQLite backend using aiosqlite.

    All writes use WAL journal mode for concurrency and fsync safety.
    When a Rust session service is ready, swap this for a socket-backed
    backend — the SessionManager constructor is the only change required.
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
        await self._db.executescript(_SCHEMA_SQL)
        await self._db.execute("PRAGMA journal_mode=WAL")
        await self._db.execute("PRAGMA foreign_keys=ON")
        await self._apply_migrations()
        logger.debug("SqliteSessionBackend opened %s", self._db_path)

    async def stop(self) -> None:
        if self._db:
            await self._db.close()
            self._db = None

    async def _apply_migrations(self) -> None:
        """Run any pending schema migrations and stamp the version."""
        assert self._db
        async with self._db.execute(
            "SELECT version FROM schema_version WHERE id = 1"
        ) as cur:
            row = await cur.fetchone()
        current = row["version"] if row else 0

        for i, migration in enumerate(_MIGRATIONS[current:], start=current + 1):
            await migration(self._db)
            await self._db.execute(
                "INSERT INTO schema_version (id, version) VALUES (1, ?)"
                " ON CONFLICT(id) DO UPDATE SET version = excluded.version",
                (i,),
            )
            await self._db.commit()
            logger.info("SqliteSessionBackend: applied migration %d", i)

        if current == 0 and not _MIGRATIONS:
            # Fresh database — stamp at version 0 so future migrations skip correctly.
            await self._db.execute(
                "INSERT OR IGNORE INTO schema_version (id, version) VALUES (1, 0)"
            )
            await self._db.commit()

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
        """Return all visible (non-hidden) session keys."""
        assert self._db, "backend not started"
        async with self._db.execute(
            "SELECT DISTINCT t.session_key FROM turns t"
            " LEFT JOIN session_metadata m ON t.session_key = m.session_key"
            " WHERE m.hidden_at IS NULL"
            " ORDER BY t.session_key"
        ) as cursor:
            rows = await cursor.fetchall()
        return [r["session_key"] for r in rows]

    async def hide_session(self, session_key: str) -> None:
        """Soft-delete: mark a session hidden so it no longer appears in list_sessions.

        The turns remain in the database for logging and audit purposes.
        """
        assert self._db, "backend not started"
        ts = datetime.now().isoformat()
        await self._db.execute(
            "INSERT INTO session_metadata (session_key, hidden_at)"
            " VALUES (?, ?)"
            " ON CONFLICT(session_key) DO UPDATE SET hidden_at = excluded.hidden_at",
            (session_key, ts),
        )
        await self._db.commit()

    # ------------------------------------------------------------------
    # Browser session tracking
    # ------------------------------------------------------------------

    async def set_browser_session(
        self, session_key: str, browser_session_id: str | None
    ) -> None:
        """Associate (or clear) a browser session with an agent session."""
        assert self._db, "backend not started"
        ts = datetime.now().isoformat() if browser_session_id else None
        await self._db.execute(
            "INSERT INTO session_metadata"
            "  (session_key, browser_session_id, browser_last_active)"
            " VALUES (?, ?, ?)"
            " ON CONFLICT(session_key) DO UPDATE SET"
            "   browser_session_id  = excluded.browser_session_id,"
            "   browser_last_active = excluded.browser_last_active",
            (session_key, browser_session_id, ts),
        )
        await self._db.commit()

    async def get_browser_session(self, session_key: str) -> str | None:
        """Return the browser session ID for an agent session, or None."""
        assert self._db, "backend not started"
        async with self._db.execute(
            "SELECT browser_session_id FROM session_metadata WHERE session_key = ?",
            (session_key,),
        ) as cursor:
            row = await cursor.fetchone()
        return row["browser_session_id"] if row else None

    async def touch_browser_session(self, session_key: str) -> None:
        """Update the last-active timestamp for a session's browser context."""
        assert self._db, "backend not started"
        ts = datetime.now().isoformat()
        await self._db.execute(
            "INSERT INTO session_metadata (session_key, browser_last_active)"
            " VALUES (?, ?)"
            " ON CONFLICT(session_key) DO UPDATE SET"
            "   browser_last_active = excluded.browser_last_active",
            (session_key, ts),
        )
        await self._db.commit()

    async def clear_browser_session(self, session_key: str) -> None:
        """Remove the browser session association (session closed or reaped)."""
        assert self._db, "backend not started"
        await self._db.execute(
            "UPDATE session_metadata"
            " SET browser_session_id = NULL, browser_last_active = NULL"
            " WHERE session_key = ?",
            (session_key,),
        )
        await self._db.commit()

    async def get_stale_browser_sessions(
        self, cutoff: datetime
    ) -> list[tuple[str, str]]:
        """Return (session_key, browser_session_id) pairs inactive since cutoff."""
        assert self._db, "backend not started"
        async with self._db.execute(
            "SELECT session_key, browser_session_id FROM session_metadata"
            " WHERE browser_session_id IS NOT NULL"
            "   AND browser_last_active < ?",
            (cutoff.isoformat(),),
        ) as cursor:
            rows = await cursor.fetchall()
        return [(r["session_key"], r["browser_session_id"]) for r in rows]

    # ------------------------------------------------------------------
    # Cross-platform identity
    # ------------------------------------------------------------------

    async def resolve_user_key(
        self, platform: str, platform_id: str, *, channel_id: str = ""
    ) -> str:
        """Return (or create) the stable user_key for a platform identity.

        Uses INSERT OR IGNORE so concurrent calls within the same connection
        serialise safely — the second insert is silently dropped and the
        SELECT always returns the winner.  ``channel_id`` is stored so the
        operator can route direct replies back to the user's conversation.
        """
        assert self._db, "backend not started"
        now = datetime.now().isoformat()

        async with self._db.execute(
            "SELECT user_key FROM identity_links"
            " WHERE platform = ? AND platform_id = ?",
            (platform, platform_id),
        ) as cur:
            row = await cur.fetchone()

        if row:
            await self._db.execute(
                "UPDATE identity_links SET last_active = ?, channel_id = CASE"
                "  WHEN ? != '' THEN ? ELSE channel_id END"
                " WHERE platform = ? AND platform_id = ?",
                (now, channel_id, channel_id, platform, platform_id),
            )
            await self._db.commit()
            return row["user_key"]

        new_key = f"user:{uuid.uuid4().hex[:16]}"
        await self._db.execute(
            "INSERT OR IGNORE INTO users (user_key, name, email, created_at, last_seen)"
            " VALUES (?, '', '', ?, ?)",
            (new_key, now, now),
        )
        await self._db.execute(
            "INSERT OR IGNORE INTO identity_links"
            " (platform, platform_id, user_key, channel_id, last_active)"
            " VALUES (?, ?, ?, ?, ?)",
            (platform, platform_id, new_key, channel_id, now),
        )
        await self._db.commit()

        async with self._db.execute(
            "SELECT user_key FROM identity_links"
            " WHERE platform = ? AND platform_id = ?",
            (platform, platform_id),
        ) as cur:
            row = await cur.fetchone()

        actual_key = row["user_key"]
        await self._db.execute(
            "UPDATE users SET last_seen = ? WHERE user_key = ?", (now, actual_key)
        )
        await self._db.commit()
        return actual_key

    async def list_all_identities(self) -> list[dict]:
        """Return all identity_links rows, newest-active first."""
        assert self._db, "backend not started"
        async with self._db.execute(
            "SELECT platform, platform_id, user_key, channel_id, last_active"
            " FROM identity_links ORDER BY last_active DESC"
        ) as cur:
            rows = await cur.fetchall()
        return [
            {
                "platform": r["platform"],
                "platform_id": r["platform_id"],
                "user_key": r["user_key"],
                "channel_id": r["channel_id"],
                "last_active": r["last_active"],
            }
            for r in rows
        ]

    async def set_identity_link(
        self, user_key: str, platform: str, platform_id: str, channel_id: str = ""
    ) -> None:
        """Create or update a platform identity link for a given user_key."""
        assert self._db, "backend not started"
        now = datetime.now().isoformat()
        await self._db.execute(
            "INSERT OR IGNORE INTO users (user_key, name, email, created_at, last_seen)"
            " VALUES (?, '', '', ?, ?)",
            (user_key, now, now),
        )
        await self._db.execute(
            "INSERT OR REPLACE INTO identity_links"
            " (platform, platform_id, user_key, channel_id, last_active)"
            " VALUES (?, ?, ?, ?, ?)",
            (platform, platform_id, user_key, channel_id, now),
        )
        await self._db.commit()

    async def unlink_platform(self, platform: str, platform_id: str) -> None:
        """Remove a specific platform identity link."""
        assert self._db, "backend not started"
        await self._db.execute(
            "DELETE FROM identity_links WHERE platform = ? AND platform_id = ?",
            (platform, platform_id),
        )
        await self._db.commit()

    async def get_identity_links(self, user_key: str) -> list[dict]:
        """Return all platform links for a user_key, newest-active first."""
        assert self._db, "backend not started"
        async with self._db.execute(
            "SELECT platform, platform_id, channel_id, last_active"
            " FROM identity_links WHERE user_key = ?"
            " ORDER BY last_active DESC",
            (user_key,),
        ) as cur:
            rows = await cur.fetchall()
        return [
            {
                "platform": r["platform"],
                "platform_id": r["platform_id"],
                "channel_id": r["channel_id"],
                "last_active": r["last_active"],
            }
            for r in rows
        ]

    async def link_user_keys(self, key_a: str, key_b: str) -> str:
        """Merge key_b into key_a atomically.

        All platform identities and conversation turns that belonged to key_b
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
            return None

        await self._db.execute("DELETE FROM link_pins WHERE pin = ?", (pin,))
        await self._db.commit()
        return await self.link_user_keys(generator_key, redeemer_key)

    # ------------------------------------------------------------------
    # Users
    # ------------------------------------------------------------------

    async def list_users(self) -> list[dict]:
        """Return all users, newest-active first."""
        assert self._db, "backend not started"
        async with self._db.execute(
            "SELECT user_key, name, email, created_at, last_seen FROM users"
            " ORDER BY last_seen DESC"
        ) as cur:
            rows = await cur.fetchall()
        return [
            {
                "user_key": r["user_key"],
                "name": r["name"],
                "email": r["email"],
                "created_at": r["created_at"],
                "last_seen": r["last_seen"],
            }
            for r in rows
        ]

    async def get_user(self, user_key: str) -> dict | None:
        """Return a single user record or None."""
        assert self._db, "backend not started"
        async with self._db.execute(
            "SELECT user_key, name, email, created_at, last_seen"
            " FROM users WHERE user_key = ?",
            (user_key,),
        ) as cur:
            row = await cur.fetchone()
        if row is None:
            return None
        return {
            "user_key": row["user_key"],
            "name": row["name"],
            "email": row["email"],
            "created_at": row["created_at"],
            "last_seen": row["last_seen"],
        }

    async def upsert_user(self, user_key: str, name: str = "", email: str = "") -> None:
        """Create or update a user record."""
        assert self._db, "backend not started"
        now = datetime.now().isoformat()
        await self._db.execute(
            "INSERT INTO users (user_key, name, email, created_at, last_seen)"
            " VALUES (?, ?, ?, ?, ?)"
            " ON CONFLICT(user_key) DO UPDATE SET"
            "   name = CASE WHEN ? != '' THEN ? ELSE name END,"
            "   email = CASE WHEN ? != '' THEN ? ELSE email END,"
            "   last_seen = excluded.last_seen",
            (user_key, name, email, now, now, name, name, email, email),
        )
        await self._db.commit()

    async def delete_user(self, user_key: str) -> None:
        """Delete a user and all their identity links (CASCADE)."""
        assert self._db, "backend not started"
        await self._db.execute("DELETE FROM users WHERE user_key = ?", (user_key,))
        await self._db.commit()

    # ------------------------------------------------------------------
    # Whitelist / blacklist
    # ------------------------------------------------------------------

    async def get_whitelist(self) -> list[dict]:
        """Return all whitelist entries."""
        assert self._db, "backend not started"
        async with self._db.execute(
            "SELECT platform, channel_id, label, added_by, added_at"
            " FROM whitelist ORDER BY added_at DESC"
        ) as cur:
            rows = await cur.fetchall()
        return [
            {
                "platform": r["platform"],
                "channel_id": r["channel_id"],
                "label": r["label"],
                "added_by": r["added_by"],
                "added_at": r["added_at"],
            }
            for r in rows
        ]

    async def add_to_whitelist(
        self, platform: str, channel_id: str, *, label: str = "", added_by: str = ""
    ) -> None:
        """Insert or replace an entry."""
        assert self._db, "backend not started"
        now = datetime.now().isoformat()
        await self._db.execute(
            "INSERT INTO whitelist (platform, channel_id, label, added_by, added_at)"
            " VALUES (?, ?, ?, ?, ?)"
            " ON CONFLICT(platform, channel_id) DO UPDATE SET"
            "   label = excluded.label,"
            "   added_by = excluded.added_by,"
            "   added_at = excluded.added_at",
            (platform, channel_id, label, added_by, now),
        )
        await self._db.commit()

    async def remove_from_whitelist(self, platform: str, channel_id: str) -> None:
        """Delete an entry."""
        assert self._db, "backend not started"
        await self._db.execute(
            "DELETE FROM whitelist WHERE platform = ? AND channel_id = ?",
            (platform, channel_id),
        )
        await self._db.commit()

    async def is_whitelisted(self, platform: str, channel_id: str) -> bool:
        """Check if (platform, channel_id) is in the whitelist."""
        assert self._db, "backend not started"
        async with self._db.execute(
            "SELECT 1 FROM whitelist WHERE platform = ? AND channel_id = ?",
            (platform, channel_id),
        ) as cur:
            row = await cur.fetchone()
        return row is not None

    async def record_seen_sender(self, platform: str, channel_id: str) -> None:
        """Upsert a blocked-but-seen sender (called from WhitelistMiddleware)."""
        assert self._db, "backend not started"
        now = datetime.now().isoformat()
        await self._db.execute(
            "INSERT INTO blacklist (platform, channel_id, first_seen, last_seen, message_count)"
            " VALUES (?, ?, ?, ?, 1)"
            " ON CONFLICT(platform, channel_id) DO UPDATE SET"
            "   last_seen = excluded.last_seen,"
            "   message_count = message_count + 1",
            (platform, channel_id, now, now),
        )
        await self._db.commit()

    async def get_seen_senders(self) -> list[dict]:
        """Return all seen-but-not-whitelisted senders, most-recent first."""
        assert self._db, "backend not started"
        async with self._db.execute(
            "SELECT s.platform, s.channel_id, s.first_seen, s.last_seen, s.message_count"
            " FROM blacklist s"
            " WHERE NOT EXISTS ("
            "   SELECT 1 FROM whitelist w"
            "   WHERE w.platform = s.platform AND w.channel_id = s.channel_id"
            " )"
            " ORDER BY s.last_seen DESC"
        ) as cur:
            rows = await cur.fetchall()
        return [
            {
                "platform": r["platform"],
                "channel_id": r["channel_id"],
                "first_seen": r["first_seen"],
                "last_seen": r["last_seen"],
                "message_count": r["message_count"],
            }
            for r in rows
        ]
