"""Session-aware message bus — routes inbound messages to per-session queues.

Architecture
------------
platform adapters call ``publish()`` to put an ``InboundMessage`` onto the
global inbound queue.  A background ``_fanout`` task reads it and routes to a
per-session ``asyncio.Queue`` keyed by ``msg.session_key``.

Cross-platform sessions work automatically: if WhatsApp and Telegram messages
both carry ``sender.user_key = "user:abc123"``, they share one queue and
therefore one agent loop invocation.

The agent loop calls ``session_queue(session_key)`` to get its dedicated queue
and ``dispatch()`` to put outbound replies on the shared outbound queue.
platform adapters drain ``outbound`` to send replies back.

Shutdown
--------
``close()`` puts a ``None`` sentinel on every live queue and waits for the
fanout task to finish.  Callers that iterate queues must treat ``None`` as a
stop signal.
"""

from __future__ import annotations

import asyncio
import logging
from asyncio import Queue, Task
from collections.abc import Callable
from typing import Any

from openagent.bus.events import InboundMessage, OutboundMessage

logger = logging.getLogger(__name__)

_SENTINEL = None  # type alias clarity


class MessageBus:
    """Async message bus with per-session fanout.

    Parameters
    ----------
    maxsize:
        Capacity of every queue (global inbound, per-session, outbound).
        When full, ``publish()`` blocks the caller until space is available.
        Keep this bounded so a busy session cannot exhaust memory on the Pi.
    """

    def __init__(self, maxsize: int = 256) -> None:
        self._maxsize = maxsize
        # Global inbound queue — platform adapters put here
        self._inbound: Queue[InboundMessage | None] = Queue(maxsize=maxsize)
        # Global outbound queue — platform adapters drain this
        self.outbound: Queue[OutboundMessage | None] = Queue(maxsize=maxsize)
        # Per-session queues — agent loop reads from these
        self._sessions: dict[str, Queue[InboundMessage | None]] = {}
        # Observer queues — SSE clients tap inbound+outbound per session_key
        self._observers: dict[str, list[Queue[dict | None]]] = {}
        # Optional callback: called once the first time a session_key is seen
        self._session_cb: Callable[[str], None] | None = None
        self._fanout_task: Task[Any] | None = None
        self._closed = False

    # ------------------------------------------------------------------
    # Lifecycle
    # ------------------------------------------------------------------

    async def start(self) -> None:
        """Start the background fanout loop."""
        if self._fanout_task is not None:
            return
        self._closed = False
        self._fanout_task = asyncio.create_task(self._fanout(), name="bus.fanout")
        logger.debug("MessageBus started")

    async def close(self) -> None:
        """Drain all queues and shut down.

        Sends a ``None`` sentinel to the global inbound queue so the fanout
        loop exits, then propagates sentinels to every open session queue and
        to the outbound queue.
        """
        if self._closed:
            return
        self._closed = True
        # Signal fanout loop to stop
        await self._inbound.put(None)
        if self._fanout_task is not None:
            try:
                await asyncio.wait_for(self._fanout_task, timeout=5.0)
            except (asyncio.TimeoutError, asyncio.CancelledError):
                self._fanout_task.cancel()
            self._fanout_task = None
        # Signal all session queues — drain first if full so sentinel always lands
        for q in self._sessions.values():
            while not q.empty():
                try:
                    q.get_nowait()
                except asyncio.QueueEmpty:
                    break
            q.put_nowait(None)
        # Signal all observer queues
        for queues in self._observers.values():
            for oq in queues:
                try:
                    oq.put_nowait(None)
                except asyncio.QueueFull:
                    pass
        # Signal outbound consumers
        try:
            self.outbound.put_nowait(None)
        except asyncio.QueueFull:
            pass
        logger.debug("MessageBus closed")

    # ------------------------------------------------------------------
    # Publishing / dispatching
    # ------------------------------------------------------------------

    async def publish(self, msg: InboundMessage) -> None:
        """platform adapter → bus.  Put an inbound message on the global queue."""
        if self._closed:
            raise RuntimeError("MessageBus is closed")
        await self._inbound.put(msg)

    async def dispatch(self, msg: OutboundMessage) -> None:
        """Agent loop → bus.  Put an outbound reply on the global outbound queue."""
        if self._closed:
            raise RuntimeError("MessageBus is closed")
        await self.outbound.put(msg)
        # Tap observers for this session so the UI can monitor in real-time
        if msg.session_key:
            self._notify_observers(msg.session_key, {
                "direction": "outbound",
                "platform": msg.platform,
                "content": msg.content,
                "session_key": msg.session_key,
                "stream_chunk": bool(msg.metadata.get("stream_chunk")),
                "stream_end": bool(msg.metadata.get("stream_end")),
            })

    # ------------------------------------------------------------------
    # Session access
    # ------------------------------------------------------------------

    def session_queue(self, session_key: str) -> Queue[InboundMessage | None]:
        """Return (creating if new) the per-session inbound queue.

        The agent loop calls this once per session_key, then reads from the
        returned queue in a loop until it receives ``None``.
        """
        if session_key not in self._sessions:
            self._sessions[session_key] = Queue(maxsize=self._maxsize)
        return self._sessions[session_key]

    def on_new_session(self, cb: Callable[[str], None]) -> None:
        """Register a callback invoked the first time a session_key appears.

        The agent loop uses this to start a per-session coroutine::

            def _on_session(key: str) -> None:
                asyncio.create_task(_run_agent_session(key))

            bus.on_new_session(_on_session)
        """
        self._session_cb = cb

    def active_sessions(self) -> list[str]:
        """Return a snapshot of all known session keys."""
        return list(self._sessions.keys())

    # ------------------------------------------------------------------
    # Observer queues — real-time session monitoring (SSE)
    # ------------------------------------------------------------------

    def subscribe(self, session_key: str) -> "Queue[dict | None]":
        """Register an observer queue for a session.

        Returns a queue that receives ``{"direction", "platform", "content",
        "session_key"}`` dicts for every inbound and outbound message routed
        through this session.  A ``None`` sentinel is sent on bus close.
        """
        q: Queue[dict | None] = Queue(maxsize=64)
        self._observers.setdefault(session_key, []).append(q)
        return q

    def unsubscribe(self, session_key: str, queue: "Queue[dict | None]") -> None:
        """Remove an observer queue when an SSE client disconnects."""
        bucket = self._observers.get(session_key)
        if bucket is None:
            return
        try:
            bucket.remove(queue)
        except ValueError:
            pass
        if not bucket:
            del self._observers[session_key]

    def _notify_observers(self, session_key: str, event: dict) -> None:
        """Push event to observer queues for this session; drop if full."""
        for oq in self._observers.get(session_key, []):
            try:
                oq.put_nowait(event)
            except asyncio.QueueFull:
                pass  # slow consumer; drop rather than block

    # ------------------------------------------------------------------
    # Internal fanout loop
    # ------------------------------------------------------------------

    async def _fanout(self) -> None:
        """Read from global inbound queue; route each message to its session queue."""
        while True:
            msg = await self._inbound.get()
            if msg is None:  # shutdown sentinel
                break
            key = msg.session_key
            is_new = key not in self._sessions
            q = self.session_queue(key)  # creates if new
            if is_new and self._session_cb is not None:
                try:
                    self._session_cb(key)
                except Exception:
                    logger.exception("on_new_session callback raised for key=%r", key)
            try:
                q.put_nowait(msg)
            except asyncio.QueueFull:
                logger.warning(
                    "Session queue full for key=%r; dropping message from %s",
                    key,
                    msg.platform,
                )
            # Tap observers so SSE clients see inbound messages too
            self._notify_observers(key, {
                "direction": "inbound",
                "platform": msg.platform,
                "content": msg.content,
                "session_key": key,
            })
        logger.debug("MessageBus fanout loop exited")
