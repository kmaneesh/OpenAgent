"""SessionBackend Protocol + Turn dataclass — stable interface for Go/Rust migration.

The agent loop and session manager only ever talk to ``SessionBackend``.
Swapping the underlying store (SQLite → Go service → Rust service) is a
one-line change in the constructor call site.

Wire protocol for future Go/Rust backend (MCP-lite frames):
  {"type": "session.append",   "session_key": "...", "role": "...", "content": "..."}
  {"type": "session.history",  "session_key": "...", "limit": 50}
  {"type": "session.summary",  "session_key": "...", "summary": "..."}
  {"type": "session.clear",    "session_key": "..."}
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime
from typing import Literal, Protocol, runtime_checkable


@dataclass
class Turn:
    """One exchange turn in a conversation."""
    role: Literal["system", "user", "assistant", "tool"]
    content: str
    timestamp: datetime = field(default_factory=datetime.now)
    tool_call_id: str = ""   # set for role=="tool"
    tool_name: str = ""      # set for role=="tool"


@runtime_checkable
class SessionBackend(Protocol):
    """Stable interface for session persistence.

    Implementations: SqliteSessionBackend (now), GoSessionBackend (later).
    The interface is intentionally narrow — no ORM leakage, no engine refs.
    """

    async def start(self) -> None:
        """Initialise resources (create tables, open connection, etc.)."""
        ...

    async def stop(self) -> None:
        """Release resources cleanly."""
        ...

    async def append(
        self,
        session_key: str,
        role: Literal["system", "user", "assistant", "tool"],
        content: str,
        *,
        tool_call_id: str = "",
        tool_name: str = "",
    ) -> None:
        """Append one turn to a session."""
        ...

    async def get_history(
        self, session_key: str, *, limit: int = 100
    ) -> list[Turn]:
        """Return the last ``limit`` turns, oldest first."""
        ...

    async def set_summary(self, session_key: str, summary: str) -> None:
        """Replace all turns with a single system summary turn.

        Called by the session manager when auto-summarization fires.
        """
        ...

    async def clear(self, session_key: str) -> None:
        """Delete all turns for a session."""
        ...

    async def list_sessions(self) -> list[str]:
        """Return all known session keys."""
        ...

    # ------------------------------------------------------------------
    # Cross-channel identity
    # ------------------------------------------------------------------

    async def resolve_user_key(self, channel: str, channel_id: str) -> str:
        """Return (or create) the stable ``user_key`` for a channel identity.

        First call for a new ``(channel, channel_id)`` pair generates a unique
        ``user:<hex>`` key and persists it.  Subsequent calls return the same
        key and refresh ``last_active``.
        """
        ...

    async def link_user_keys(self, key_a: str, key_b: str) -> str:
        """Merge key_b into key_a — redirect all channel IDs and move turns.

        Returns key_a (the winner).  key_b will no longer appear anywhere
        after this call.
        """
        ...

    async def store_link_pin(
        self, user_key: str, pin: str, expires_at: str
    ) -> None:
        """Persist a one-time link pin valid until ``expires_at`` (ISO string)."""
        ...

    async def redeem_link_pin(self, redeemer_key: str, pin: str) -> str | None:
        """Validate pin, merge the two sessions, return winning key.

        Returns None if the pin is invalid, expired, or already used.
        """
        ...
