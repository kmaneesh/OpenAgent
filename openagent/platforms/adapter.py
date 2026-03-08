"""platform adapters — bridge Go service McpLiteClients to the MessageBus.

Design
------
Each ``platformAdapter`` wraps an existing ``McpLiteClient`` (owned by
``ServiceManager``) and registers an event handler on it via
``add_event_handler()``.  This keeps a **single socket connection** per
service — no event-stealing between two competing connections.

Inbound path:
    Go service → event frame → McpLiteClient._read_loop
    → on_event() → registered handler → platformAdapter._dispatch()
    → InboundMessage → bus.publish()

Outbound path:
    bus.outbound queue → platformManager._route_outbound()
    → adapter.send(OutboundMessage) → McpLiteClient.request(tool.call)
    → Go service sends reply.
"""

from __future__ import annotations

import asyncio
import json
import logging
import re
import time
from collections.abc import Awaitable, Callable
from typing import Any

from openagent.bus.bus import MessageBus
from openagent.bus.events import InboundMessage, OutboundMessage, SenderInfo
from openagent.observability.logging import get_logger
from openagent.services import protocol as proto

from .mcplite import McpLiteClient

# async fn(platform, platform_id, channel_id) -> user_key  (e.g. SessionManager.resolve_user_key)
IdentityResolver = Callable[[str, str, str], Awaitable[str]]

logger = get_logger(__name__)


# ---------------------------------------------------------------------------
# Base
# ---------------------------------------------------------------------------


class PlatformAdapter:
    """Composition wrapper that hooks a McpLiteClient into the MessageBus.

    Subclass and implement ``_to_inbound()`` and ``send()``.  The adapter
    registers itself on the client in ``__init__`` — no further wiring needed.
    """

    def __init__(
        self,
        *,
        platform_name: str,
        client: McpLiteClient,
        bus: MessageBus,
        resolver: IdentityResolver | None = None,
    ) -> None:
        self._platform_name = platform_name
        self._client = client
        self._bus = bus
        self._resolver = resolver
        client.add_event_handler(self._dispatch)

    @property
    def platform_name(self) -> str:
        return self._platform_name

    @property
    def client(self) -> McpLiteClient:
        """The underlying client — used for identity checks on restart."""
        return self._client

    # ------------------------------------------------------------------
    # Event dispatch (sync — called from async _read_loop via on_event)
    # ------------------------------------------------------------------

    def _dispatch(self, frame: proto.EventFrame) -> None:
        data = dict(frame.data)
        if "connection.status" in frame.event:
            self._on_connection_status(data)
            return
        inbound = self._to_inbound(data)
        if inbound is None:
            return
        if self._resolver:
            asyncio.ensure_future(self._enrich_and_publish(inbound))
        else:
            asyncio.ensure_future(self._bus.publish(inbound))

    async def _enrich_and_publish(self, inbound: InboundMessage) -> None:
        """Resolve user_key (+ store channel_id) before publishing."""
        try:
            inbound.sender.user_key = await self._resolver(
                inbound.platform, inbound.sender.user_id, inbound.channel_id
            )
        except Exception:
            logger.warning(
                "Identity resolution failed for %s:%s — falling back to platform:id",
                inbound.platform, inbound.sender.user_id,
            )
        await self._bus.publish(inbound)

    def _on_connection_status(self, data: dict[str, Any]) -> None:
        """Override to cache status fields."""

    def _to_inbound(self, data: dict[str, Any]) -> InboundMessage | None:
        """Map raw event data to an InboundMessage.  Return None to drop."""
        return None

    # ------------------------------------------------------------------
    # Outbound
    # ------------------------------------------------------------------

    async def send(self, msg: OutboundMessage) -> None:
        """Send a reply back through this platform.  Subclasses must override."""
        raise NotImplementedError(f"{type(self).__name__}.send() is not implemented")


# ---------------------------------------------------------------------------
# Discord
# ---------------------------------------------------------------------------


class DiscordPlatformAdapter(PlatformAdapter):
    """Adapter for the Discord Go service.

    Event data fields (from ``discord.message.received``):
        id, platform_id, guild_id, author_id, author, content, is_bot
    """

    def __init__(self, *, client: McpLiteClient, bus: MessageBus, resolver: IdentityResolver | None = None) -> None:
        super().__init__(platform_name="discord", client=client, bus=bus, resolver=resolver)
        self._status: dict[str, Any] = {"connected": False, "authorized": False}
        # stream_key -> {msg_id, last_edit} for throttled progressive edits
        self._stream_states: dict[str, dict] = {}

    def _on_connection_status(self, data: dict[str, Any]) -> None:
        self._status.update(data)

    def _to_inbound(self, data: dict[str, Any]) -> InboundMessage | None:
        if data.get("is_bot"):
            return None
        platform_id = str(data.get("platform_id", ""))
        content = str(data.get("content", ""))
        if not platform_id or not content:
            return None
        return InboundMessage(
            platform="discord",
            channel_id=platform_id,
            sender=SenderInfo(
                platform="discord",
                user_id=str(data.get("author_id", "")),
                display_name=str(data.get("author", "")),
            ),
            content=content,
            metadata={
                "message_id": data.get("id", ""),
                "guild_id": data.get("guild_id", ""),
            },
        )

    # Discord message limit (API rejects > 2000 chars)
    _MAX_MESSAGE_LEN = 1900
    # Minimum seconds between edit_message API calls during streaming (~5 edits/5s limit)
    _EDIT_MIN_INTERVAL_S = 1.2
    # Progressive delivery: show first chunk immediately, then edit.
    # Discord allows ~5 edits per 5 seconds — use 1s between edits to stay under limit.
    _PROGRESSIVE_CHUNK = 200
    _PROGRESSIVE_DELAY_S = 1.0

    def _split_content(self, text: str) -> list[str]:
        """Split text into chunks under Discord's limit, preferring newlines."""
        if len(text) <= self._MAX_MESSAGE_LEN:
            return [text] if text else []
        chunks: list[str] = []
        rest = text
        while rest:
            if len(rest) <= self._MAX_MESSAGE_LEN:
                chunks.append(rest)
                break
            cut = rest[: self._MAX_MESSAGE_LEN]
            # Prefer splitting at newline
            last_nl = cut.rfind("\n")
            if last_nl > self._MAX_MESSAGE_LEN // 2:
                cut, rest = cut[: last_nl + 1], rest[last_nl + 1 :]
            else:
                last_space = cut.rfind(" ")
                if last_space > self._MAX_MESSAGE_LEN // 2:
                    cut, rest = cut[: last_space + 1], rest[last_space + 1 :]
                else:
                    rest = rest[self._MAX_MESSAGE_LEN :]
                    cut = cut[: self._MAX_MESSAGE_LEN]
            chunks.append(cut)
        return chunks

    def _parse_message_id(self, result: object) -> str | None:
        """Extract message id from discord.send_message / edit_message JSON result."""
        if result is None:
            return None
        if isinstance(result, str):
            try:
                data = json.loads(result)
            except json.JSONDecodeError:
                return None
        elif isinstance(result, dict):
            data = result
        else:
            return None
        return data.get("id") if isinstance(data, dict) else None

    async def _send_progressive(self, platform_id: str, text: str) -> None:
        """Send content progressively so the user sees it as it arrives."""
        if not text:
            return
        if len(text) <= self._PROGRESSIVE_CHUNK:
            await self._client.request({
                "type": "tool.call",
                "tool": "discord.send_message",
                "params": {"platform_id": platform_id, "text": text},
            })
            return
        # Send first chunk immediately
        current = text[: self._PROGRESSIVE_CHUNK]
        frame = await self._client.request({
            "type": "tool.call",
            "tool": "discord.send_message",
            "params": {"platform_id": platform_id, "text": current},
        })
        msg_id = None
        if hasattr(frame, "result") and frame.result:
            msg_id = self._parse_message_id(frame.result)
        if not msg_id:
            # Fallback: send rest in one go (edit failed to get id)
            rest = text[self._PROGRESSIVE_CHUNK :]
            if rest:
                await self._client.request({
                    "type": "tool.call",
                    "tool": "discord.send_message",
                    "params": {"platform_id": platform_id, "text": rest},
                })
            return
        # Edit progressively (respect Discord ~5 edits/5s rate limit)
        pos = self._PROGRESSIVE_CHUNK
        while pos < len(text):
            await asyncio.sleep(self._PROGRESSIVE_DELAY_S)
            pos = min(pos + self._PROGRESSIVE_CHUNK, len(text))
            current = text[:pos]
            try:
                await self._client.request({
                    "type": "tool.call",
                    "tool": "discord.edit_message",
                    "params": {
                        "platform_id": platform_id,
                        "message_id": msg_id,
                        "text": current,
                    },
                })
            except Exception:
                # Rate limit or edit failed — send remainder as new message
                rest = text[pos:]
                if rest:
                    await self._client.request({
                        "type": "tool.call",
                        "tool": "discord.send_message",
                        "params": {"platform_id": platform_id, "text": rest},
                    })
                break

    def _process_think_blocks(self, text: str) -> str:
        """Convert <think>...</think> blocks to Discord spoiler format.

        Models like Qwen omit the opening tag — only </think> is emitted.
        Normalise first, then wrap thinking content in ||spoiler|| so users
        can click to reveal it, with a small label above.
        """
        # Normalise: implicit opener (only </think> present, no <think>)
        if "</think>" in text and "<think>" not in text:
            text = "<think>" + text

        def _replace(m: re.Match) -> str:
            inner = m.group(1).strip()
            if not inner:
                return ""
            return f"-# 💭 Thinking\n||{inner}||\n"

        return re.sub(r"<think>([\s\S]*?)</think>", _replace, text)

    def _stream_key(self, msg: OutboundMessage) -> str:
        return f"{msg.platform}:{msg.channel_id}:{msg.session_key}"

    async def send(self, msg: OutboundMessage) -> None:
        meta = msg.metadata or {}
        stream_chunk = meta.get("stream_chunk", False)
        stream_end = meta.get("stream_end", False)

        # Process think blocks on complete content only (stream_end or non-streaming).
        # Intermediate chunks are raw accumulated text — no </think> yet.
        raw = msg.content or ""
        content = self._process_think_blocks(raw) if (stream_end or not stream_chunk) else raw

        if stream_chunk:
            # True LLM streaming: create message on first chunk, throttle edits
            key = self._stream_key(msg)
            state = self._stream_states.get(key)
            if state is None:
                # First chunk: create the message
                if not raw:
                    return
                frame = await self._client.request({
                    "type": "tool.call",
                    "tool": "discord.send_message",
                    "params": {"platform_id": msg.channel_id, "text": content},
                })
                new_id = self._parse_message_id(
                    getattr(frame, "result", None) if hasattr(frame, "result") else None
                )
                if new_id:
                    self._stream_states[key] = {"msg_id": new_id, "last_edit": time.monotonic()}
            else:
                # Subsequent chunk: only edit if interval exceeded or this is the final chunk
                now = time.monotonic()
                elapsed = now - state["last_edit"]
                if raw and (stream_end or elapsed >= self._EDIT_MIN_INTERVAL_S):
                    try:
                        await self._client.request({
                            "type": "tool.call",
                            "tool": "discord.edit_message",
                            "params": {
                                "platform_id": msg.channel_id,
                                "message_id": state["msg_id"],
                                "text": content,
                            },
                        })
                        state["last_edit"] = time.monotonic()
                    except Exception:
                        pass  # Rate limit or edit failed
            if stream_end:
                self._stream_states.pop(key, None)
            return

        # Non-streaming: full message (fallback or when tools were used)
        # content already has think blocks processed above
        chunks = self._split_content(content)
        if not chunks:
            return
        if len(chunks) == 1 and len(content) <= self._MAX_MESSAGE_LEN:
            await self._send_progressive(msg.channel_id, content)
        else:
            for chunk in chunks:
                await self._client.request({
                    "type": "tool.call",
                    "tool": "discord.send_message",
                    "params": {"platform_id": msg.channel_id, "text": chunk},
                })


# ---------------------------------------------------------------------------
# Telegram
# ---------------------------------------------------------------------------


class TelegramPlatformAdapter(PlatformAdapter):
    """Adapter for the Telegram Go service.

    Telegram replies require ``user_id`` + ``access_hash`` (the MTProto peer
    identifiers).  These are extracted from the inbound event and stored in
    ``InboundMessage.metadata`` so the agent loop can propagate them to the
    ``OutboundMessage.metadata`` that ``send()`` reads.

    Expected event data fields (from ``telegram.message.received``):
        from_id, access_hash, from_name, username, text, message_id
    """

    def __init__(self, *, client: McpLiteClient, bus: MessageBus, resolver: IdentityResolver | None = None) -> None:
        super().__init__(platform_name="telegram", client=client, bus=bus, resolver=resolver)
        self._status: dict[str, Any] = {"connected": False, "authorized": False}

    def _on_connection_status(self, data: dict[str, Any]) -> None:
        self._status.update(data)

    def _to_inbound(self, data: dict[str, Any]) -> InboundMessage | None:
        from_id = data.get("from_id")
        content = str(data.get("text", ""))
        if not from_id or not content:
            return None
        return InboundMessage(
            platform="telegram",
            channel_id=str(from_id),
            sender=SenderInfo(
                platform="telegram",
                user_id=str(from_id),
                display_name=str(data.get("from_name", "")),
            ),
            content=content,
            metadata={
                "access_hash": data.get("access_hash", 0),
                "message_id": data.get("message_id", 0),
                "username": data.get("username", ""),
            },
        )

    async def send(self, msg: OutboundMessage) -> None:
        meta = msg.metadata or {}
        if meta.get("stream_chunk") and not meta.get("stream_end"):
            return  # Skip intermediate chunks; send only the final complete message
        user_id = int(msg.channel_id)
        access_hash = int(meta.get("access_hash", 0))
        await self._client.request({
            "type": "tool.call",
            "tool": "telegram.send_message",
            "params": {
                "user_id": user_id,
                "access_hash": access_hash,
                "text": msg.content,
            },
        })


# ---------------------------------------------------------------------------
# Slack
# ---------------------------------------------------------------------------


class SlackPlatformAdapter(PlatformAdapter):
    """Adapter for the Slack Go service.

    Expected event data fields (from ``slack.message.received``):
        channel_id, user_id, text, ts, team_id
    """

    def __init__(self, *, client: McpLiteClient, bus: MessageBus, resolver: IdentityResolver | None = None) -> None:
        super().__init__(platform_name="slack", client=client, bus=bus, resolver=resolver)
        self._status: dict[str, Any] = {"connected": False}

    def _on_connection_status(self, data: dict[str, Any]) -> None:
        self._status.update(data)

    def _to_inbound(self, data: dict[str, Any]) -> InboundMessage | None:
        if data.get("bot_id"):
            return None
        channel_id = str(data.get("channel_id") or data.get("platform_id", ""))
        content = str(data.get("text", ""))
        user_id = str(data.get("user_id", ""))
        if not channel_id or not content:
            return None
        return InboundMessage(
            platform="slack",
            channel_id=channel_id,
            sender=SenderInfo(
                platform="slack",
                user_id=user_id,
                display_name=str(data.get("username", "")),
            ),
            content=content,
            metadata={"message_ts": data.get("ts", "")},
        )

    async def send(self, msg: OutboundMessage) -> None:
        meta = msg.metadata or {}
        if meta.get("stream_chunk") and not meta.get("stream_end"):
            return  # Skip intermediate chunks; send only the final complete message
        await self._client.request({
            "type": "tool.call",
            "tool": "slack.send_message",
            "params": {"channel_id": msg.channel_id, "text": msg.content},
        })


# ---------------------------------------------------------------------------
# WhatsApp
# ---------------------------------------------------------------------------


class WhatsAppPlatformAdapter(PlatformAdapter):
    """Adapter for the WhatsApp Go service.

    Expected event data fields (from ``whatsapp.message.received``):
        chat_id, sender, text
    """

    def __init__(self, *, client: McpLiteClient, bus: MessageBus, resolver: IdentityResolver | None = None) -> None:
        super().__init__(platform_name="whatsapp", client=client, bus=bus, resolver=resolver)
        self._status: dict[str, Any] = {"connected": False}
        self._latest_qr: str | None = None

    def _dispatch(self, frame: proto.EventFrame) -> None:
        if frame.event == "whatsapp.qr":
            self._latest_qr = str(frame.data.get("qr") or "")
            return
        if frame.event == "whatsapp.call.received":
            self._on_call(dict(frame.data))
            return
        super()._dispatch(frame)

    def _on_call(self, data: dict) -> None:
        chat_id = str(data.get("chat_id", ""))
        is_video = bool(data.get("is_video", False))
        call_type = "video" if is_video else "voice"
        logger.info("WhatsApp %s call from %s (call_id=%s)", call_type, chat_id, data.get("call_id", ""))
        # Synthesise an inbound text message so the agent can respond
        inbound = InboundMessage(
            platform="whatsapp",
            channel_id=chat_id,
            sender=SenderInfo(
                platform="whatsapp",
                user_id=chat_id,
                display_name=chat_id,
            ),
            content=f"[Incoming {call_type} call]",
            metadata={"call_id": data.get("call_id", ""), "is_video": is_video},
        )
        asyncio.ensure_future(self._bus.publish(inbound))

    def latest_qr(self) -> str | None:
        """Return latest QR payload for linking (from whatsapp.qr event)."""
        return self._latest_qr

    def _on_connection_status(self, data: dict[str, Any]) -> None:
        self._status.update(data)

    def _to_inbound(self, data: dict[str, Any]) -> InboundMessage | None:
        chat_id = str(data.get("chat_id", ""))
        content = str(data.get("text", ""))
        sender = str(data.get("sender", chat_id))
        artifact_path = str(data.get("artifact_path", ""))
        kind = str(data.get("kind", ""))

        # Audio messages have an artifact_path but may have empty text.
        # Use a placeholder so the agent loop has something to route on;
        # STTMiddleware will replace it with the transcript if enabled.
        if not content and artifact_path:
            content = "[PTT]" if kind == "ptt" else "[Voice message]"

        if not chat_id or not content:
            return None

        media: list[str] = [artifact_path] if artifact_path else []
        return InboundMessage(
            platform="whatsapp",
            channel_id=chat_id,
            sender=SenderInfo(
                platform="whatsapp",
                user_id=chat_id,
                display_name=sender,
            ),
            content=content,
            media=media,
            metadata={"kind": kind, "is_ptt": data.get("is_ptt", False)},
        )

    @staticmethod
    def _strip_think(text: str) -> str:
        """Remove everything up to and including the last </think> tag."""
        idx = text.rfind("</think>")
        if idx == -1:
            return text
        return text[idx + len("</think>"):].lstrip()

    async def send(self, msg: OutboundMessage) -> None:
        meta = msg.metadata or {}
        # WhatsApp has no edit API — skip all intermediate stream chunks.
        # Only the final accumulated message (stream_end=True or non-streaming) is sent.
        if meta.get("stream_chunk") and not meta.get("stream_end"):
            return
        content = self._strip_think(msg.content or "")
        if not content:
            return
        await self._client.request({
            "type": "tool.call",
            "tool": "whatsapp.send_text",
            "params": {"chat_id": msg.channel_id, "text": content},
        })
