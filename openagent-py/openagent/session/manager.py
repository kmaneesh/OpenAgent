"""SessionManager — wraps a SessionBackend with auto-summarisation.

Usage::

    backend = SqliteSessionBackend(db_path=root / "data" / "sessions.db")
    mgr = SessionManager(backend=backend, summarise_after=20)
    await mgr.start()

    # Agent loop:
    history = await mgr.get_history(session_key)
    await mgr.append(session_key, "user", msg.content)
    await mgr.append(session_key, "assistant", reply)

    # On shutdown:
    await mgr.stop()

Go/Rust migration
-----------------
Replace the constructor argument::

    # now
    backend = SqliteSessionBackend(db_path=...)
    # later — Go session service registered via ServiceManager
    backend = GoSessionBackend(socket_path=root / "data" / "sockets" / "session.sock")

No other code changes required.
"""

from __future__ import annotations

import logging
from typing import Any, Callable, Awaitable, Literal

from openagent.llm import Message

from .backend import SessionBackend, Turn

logger = logging.getLogger(__name__)

# Sentinel passed to the summarise callback so it can call the LLM
SummariseCallback = Callable[[list[Turn]], Awaitable[str]]


class SessionManager:
    """High-level session API with optional auto-summarisation.

    Parameters
    ----------
    backend:
        Any ``SessionBackend`` implementation (SQLite now, Go/Rust later).
    summarise_after:
        Number of turns after which auto-summarisation fires.
        Set to 0 to disable.
    summarise_fn:
        Async callable ``(turns) -> summary_str`` — called when threshold is
        hit.  Typically calls the LLM provider.  Required when
        ``summarise_after > 0``.
    """

    def __init__(
        self,
        backend: SessionBackend,
        *,
        summarise_after: int = 40,
        summarise_fn: SummariseCallback | None = None,
    ) -> None:
        self._backend = backend
        self._summarise_after = summarise_after
        self._summarise_fn = summarise_fn

    # ------------------------------------------------------------------
    # Lifecycle
    # ------------------------------------------------------------------

    async def start(self) -> None:
        await self._backend.start()
        logger.debug("SessionManager started")

    async def stop(self) -> None:
        await self._backend.stop()
        logger.debug("SessionManager stopped")

    # ------------------------------------------------------------------
    # Core API
    # ------------------------------------------------------------------

    async def get_history(
        self, session_key: str, *, limit: int = 100
    ) -> list[Turn]:
        """Return up to ``limit`` turns, oldest first."""
        return await self._backend.get_history(session_key, limit=limit)

    async def append(
        self,
        session_key: str,
        role: Literal["system", "user", "assistant", "tool"],
        content: str,
        *,
        tool_call_id: str = "",
        tool_name: str = "",
    ) -> None:
        """Append a turn and trigger summarisation if the threshold is reached."""
        await self._backend.append(
            session_key, role, content,
            tool_call_id=tool_call_id,
            tool_name=tool_name,
        )
        if self._summarise_after > 0:
            await self._maybe_summarise(session_key)

    async def clear(self, session_key: str) -> None:
        await self._backend.clear(session_key)

    async def list_sessions(self) -> list[str]:
        return await self._backend.list_sessions()

    async def hide_session(self, session_key: str) -> None:
        """Soft-delete: hide from list but keep turns for logs."""
        await self._backend.hide_session(session_key)

    # ------------------------------------------------------------------
    # Users
    # ------------------------------------------------------------------

    async def list_users(self) -> list[dict]:
        return await self._backend.list_users()

    async def get_user(self, user_key: str) -> dict | None:
        return await self._backend.get_user(user_key)

    async def upsert_user(self, user_key: str, name: str = "", email: str = "") -> None:
        await self._backend.upsert_user(user_key, name, email)

    async def delete_user(self, user_key: str) -> None:
        await self._backend.delete_user(user_key)

    # ------------------------------------------------------------------
    # Cross-platform identity (proxied from backend)
    # ------------------------------------------------------------------

    async def resolve_user_key(
        self, platform: str, platform_id: str, channel_id: str = ""
    ) -> str:
        """Return (or create) the stable user_key for a platform identity."""
        return await self._backend.resolve_user_key(
            platform, platform_id, channel_id=channel_id
        )

    async def list_all_identities(self) -> list[dict]:
        """Return all identity_links rows, newest-active first."""
        return await self._backend.list_all_identities()

    async def set_identity_link(
        self, user_key: str, platform: str, platform_id: str, channel_id: str = ""
    ) -> None:
        """Create or update a platform identity link for a given user_key."""
        await self._backend.set_identity_link(user_key, platform, platform_id, channel_id)

    async def unlink_platform(self, platform: str, platform_id: str) -> None:
        """Remove a specific platform identity link."""
        await self._backend.unlink_platform(platform, platform_id)

    async def get_identity_links(self, user_key: str) -> list[dict]:
        """Return all platform links for user_key, newest-active first."""
        return await self._backend.get_identity_links(user_key)

    async def link_user_keys(self, key_a: str, key_b: str) -> str:
        """Merge key_b into key_a.  Returns key_a."""
        return await self._backend.link_user_keys(key_a, key_b)

    async def store_link_pin(
        self, user_key: str, pin: str, expires_at: str
    ) -> None:
        """Persist a one-time link pin valid until expires_at (ISO string)."""
        await self._backend.store_link_pin(user_key, pin, expires_at)

    async def redeem_link_pin(self, redeemer_key: str, pin: str) -> str | None:
        """Validate pin and merge the two sessions.  Returns winning key or None."""
        return await self._backend.redeem_link_pin(redeemer_key, pin)

    # ------------------------------------------------------------------
    # History → provider.Message conversion
    # ------------------------------------------------------------------

    def to_messages(self, history: list[Turn]) -> list[Message]:
        """Convert Turn objects to provider Message objects for LLM input."""
        return [
            Message(
                role=t.role,
                content=t.content,
                tool_call_id=t.tool_call_id,
                tool_name=t.tool_name,
            )
            for t in history
        ]

    # ------------------------------------------------------------------
    # Auto-summarisation
    # ------------------------------------------------------------------

    async def _maybe_summarise(self, session_key: str) -> None:
        turns = await self._backend.get_history(session_key, limit=self._summarise_after + 5)
        if len(turns) < self._summarise_after:
            return
        if not self._summarise_fn:
            logger.warning(
                "summarise_after=%d reached for %s but no summarise_fn configured",
                self._summarise_after,
                session_key,
            )
            return
        logger.info(
            "Auto-summarising session %s (%d turns)", session_key, len(turns)
        )
        try:
            summary = await self._summarise_fn(turns)
            await self._backend.set_summary(session_key, summary)
            logger.info("Session %s summarised", session_key)
        except Exception:
            logger.exception("Failed to summarise session %s", session_key)
