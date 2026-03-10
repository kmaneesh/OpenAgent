"""BrowserSessionManager — ties agent loop sessions to agent-browser CLI sessions.

Each agent loop session (e.g. ``"web:abc123"``, ``"discord:channel99"``) gets one
persistent browser context backed by ``agent-browser --session <id>``.  The ID is
derived deterministically from the session key so it survives service restarts.

The reaper task polls every ``reap_interval_s`` seconds and calls ``browser.close``
on any session that has not been touched for ``idle_timeout_s`` seconds (default
10 minutes), then clears the database record so a fresh session can be created next
time the agent uses the browser.

Usage (wired up in ``app/main.py``)::

    browser_sessions = BrowserSessionManager(session_backend)
    tool_registry = ToolRegistry(service_manager, browser_sessions=browser_sessions)
    reaper_task = browser_sessions.start_reaper(tool_registry.call)
    ...
    reaper_task.cancel()
"""

from __future__ import annotations

import asyncio
import logging
import re
from collections.abc import Awaitable, Callable
from datetime import datetime, timedelta
from typing import Any

logger = logging.getLogger(__name__)

# Seconds of browser inactivity before the reaper closes the session.
DEFAULT_IDLE_TIMEOUT_S = 600  # 10 minutes
# How often the reaper wakes up to check for stale sessions.
DEFAULT_REAP_INTERVAL_S = 60


def _derive_browser_session_id(session_key: str) -> str:
    """Return a stable, filesystem-safe browser session ID from an agent session key.

    ``"web:abc123def"``      → ``"webabc123def"``
    ``"discord:9876543210"`` → ``"disc9876543210"``

    The derived ID is deterministic across restarts: the same agent session always
    maps to the same Chromium context (cookies, storage, history preserved).
    """
    parts = session_key.split(":", 1)
    platform = re.sub(r"[^a-z0-9]", "", parts[0].lower())[:4] if len(parts) > 1 else ""
    raw = parts[1] if len(parts) > 1 else session_key
    safe = re.sub(r"[^a-z0-9]", "", raw.lower())
    combined = platform + safe
    return (combined or "default")[:20]


class BrowserSessionManager:
    """Maps agent session keys to persistent agent-browser session IDs.

    # Examples

    >>> mgr = BrowserSessionManager(backend)
    >>> sid = await mgr.get_or_create("web:abc123")
    >>> # sid == "webabc123"
    >>> await mgr.touch("web:abc123")
    >>> stale = await mgr.reap_stale(call_tool, idle_timeout_s=600)
    """

    def __init__(self, backend: Any) -> None:
        # Any SqliteSessionBackend — typed as Any to avoid import cycle.
        self._backend = backend
        # In-memory cache: session_key → browser_session_id.
        # Avoids a DB round-trip on every browser tool call.
        self._cache: dict[str, str] = {}

    # ------------------------------------------------------------------
    # Public API (called from ToolRegistry)
    # ------------------------------------------------------------------

    async def get_or_create(self, session_key: str) -> str:
        """Return the browser session ID for *session_key*, creating it if needed.

        The first call for a session key derives a stable ID and persists it to
        the database.  Subsequent calls return the cached value in O(1).
        """
        if session_key in self._cache:
            return self._cache[session_key]

        # Check DB first — covers restarts where the cache is cold.
        existing = await self._backend.get_browser_session(session_key)
        if existing:
            self._cache[session_key] = existing
            return existing

        browser_sid = _derive_browser_session_id(session_key)
        self._cache[session_key] = browser_sid
        asyncio.create_task(self._backend.set_browser_session(session_key, browser_sid))
        logger.debug(
            "browser_sessions: created session %r for agent session %r",
            browser_sid,
            session_key,
        )
        return browser_sid

    def record(self, session_key: str, browser_session_id: str) -> None:
        """Track an explicitly-provided browser session ID (from the LLM).

        Called when the LLM passes its own ``session_id`` to ``browser.open``.
        We honour it and store the association so the reaper can manage it.
        """
        if self._cache.get(session_key) == browser_session_id:
            return
        self._cache[session_key] = browser_session_id
        asyncio.create_task(
            self._backend.set_browser_session(session_key, browser_session_id)
        )

    def touch(self, session_key: str) -> None:
        """Record that *session_key* just used the browser (fire-and-forget DB write)."""
        asyncio.create_task(self._backend.touch_browser_session(session_key))

    # ------------------------------------------------------------------
    # Reaper
    # ------------------------------------------------------------------

    async def reap_stale(
        self,
        call_tool: Callable[[str, dict[str, Any]], Awaitable[str]],
        *,
        idle_timeout_s: int = DEFAULT_IDLE_TIMEOUT_S,
    ) -> int:
        """Close all browser sessions idle for more than *idle_timeout_s* seconds.

        Calls ``browser.close`` on each stale session, clears the database record,
        and evicts the cache entry.  Returns the number of sessions reaped.
        """
        cutoff = datetime.now() - timedelta(seconds=idle_timeout_s)
        stale = await self._backend.get_stale_browser_sessions(cutoff)
        count = 0
        for agent_session_key, browser_sid in stale:
            logger.info(
                "browser_sessions.reaper: closing idle session %r"
                " (agent session %r, idle > %ds)",
                browser_sid,
                agent_session_key,
                idle_timeout_s,
            )
            try:
                await call_tool("browser.close", {"session_id": browser_sid})
            except Exception as exc:
                # Log but continue — the Rust service may already have cleaned up.
                logger.warning(
                    "browser_sessions.reaper: browser.close failed for %r: %s",
                    browser_sid,
                    exc,
                )
            await self._backend.clear_browser_session(agent_session_key)
            self._cache.pop(agent_session_key, None)
            count += 1

        if count:
            logger.info("browser_sessions.reaper: reaped %d idle session(s)", count)
        return count

    def start_reaper(
        self,
        call_tool: Callable[[str, dict[str, Any]], Awaitable[str]],
        *,
        reap_interval_s: int = DEFAULT_REAP_INTERVAL_S,
        idle_timeout_s: int = DEFAULT_IDLE_TIMEOUT_S,
    ) -> asyncio.Task:
        """Start the background reaper loop and return the task.

        Cancel the task during shutdown::

            reaper_task.cancel()
            await asyncio.gather(reaper_task, return_exceptions=True)
        """
        return asyncio.create_task(
            self._reaper_loop(call_tool, reap_interval_s, idle_timeout_s),
            name="browser-session-reaper",
        )

    async def _reaper_loop(
        self,
        call_tool: Callable[[str, dict[str, Any]], Awaitable[str]],
        interval_s: int,
        idle_timeout_s: int,
    ) -> None:
        logger.info(
            "browser_sessions.reaper: started (interval=%ds, idle_timeout=%ds)",
            interval_s,
            idle_timeout_s,
        )
        while True:
            await asyncio.sleep(interval_s)
            try:
                await self.reap_stale(call_tool, idle_timeout_s=idle_timeout_s)
            except asyncio.CancelledError:
                raise
            except Exception as exc:
                logger.error("browser_sessions.reaper: unexpected error: %s", exc)
