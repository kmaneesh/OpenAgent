"""Web channel adapter — bridges /ws/chat WebSocket connections to the MessageBus.

Not backed by a Go service.  Pure Python.  Each browser tab that connects
to /ws/chat calls ``register_connection()`` with a unique ``chat_id`` and
an async send callback.  When the agent loop dispatches an OutboundMessage
with ``channel="web"``, ChannelManager calls ``adapter.send(msg)`` which
routes to the correct WebSocket.

Registration lifecycle
----------------------
1. WS connects  → ``register_connection(chat_id, send_fn)``
2. User message → ``bus.publish(InboundMessage(channel="web", chat_id=...))``
3. Agent reply  → ``bus.dispatch(OutboundMessage(channel="web", chat_id=...))``
4. ChannelManager → ``adapter.send(msg)`` → ``send_fn(msg.content)``
5. WS disconnects → ``unregister_connection(chat_id)``
"""

from __future__ import annotations

from collections.abc import Callable, Coroutine
from typing import Any

from openagent.bus.events import OutboundMessage
from openagent.observability.logging import get_logger

logger = get_logger(__name__)

# Type alias: async callable(content, stream_chunk=False) for delivering replies.
SendFn = Callable[..., Coroutine[Any, Any, None]]


class WebChannelAdapter:
    """Routes OutboundMessage → active WebSocket send callbacks.

    Registered with ChannelManager via ``channel_manager.register(adapter)``
    so the ChannelManager can dispatch ``channel="web"`` messages to it.
    """

    channel_name: str = "web"

    def __init__(self) -> None:
        self._connections: dict[str, SendFn] = {}

    # ------------------------------------------------------------------
    # Connection registry
    # ------------------------------------------------------------------

    def register_connection(self, chat_id: str, send_fn: SendFn) -> None:
        """Called by the WebSocket handler on connect."""
        self._connections[chat_id] = send_fn
        logger.info("WebChannel: registered — chat_id=%r  active=%d",
                    chat_id, len(self._connections))

    def unregister_connection(self, chat_id: str) -> None:
        """Called by the WebSocket handler on disconnect."""
        self._connections.pop(chat_id, None)
        logger.info("WebChannel: removed — chat_id=%r  active=%d",
                    chat_id, len(self._connections))

    def active_connections(self) -> list[str]:
        return list(self._connections)

    # ------------------------------------------------------------------
    # ChannelManager interface
    # ------------------------------------------------------------------

    async def send(self, msg: OutboundMessage) -> None:
        """Deliver msg.content to the WebSocket identified by msg.chat_id."""
        send_fn = self._connections.get(msg.chat_id)
        if send_fn is None:
            logger.warning("WebChannel: no connection for chat_id=%r", msg.chat_id)
            return
        try:
            meta = msg.metadata or {}
            stream_chunk = meta.get("stream_chunk", False)
            if stream_chunk:
                await send_fn(msg.content, stream_chunk=True)
            else:
                await send_fn(msg.content)
        except Exception as exc:
            logger.error("WebChannel: send error chat_id=%r: %s", msg.chat_id, exc)
            self._connections.pop(msg.chat_id, None)
