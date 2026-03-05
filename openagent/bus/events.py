"""Message bus event types — wire between channels and the agent loop."""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime
from typing import Any


@dataclass(slots=True)
class SenderInfo:
    """Identifies the sender across platforms.

    ``user_key`` enables cross-channel sessions: when set (e.g.
    ``"user:abc123"``), all channels for that user share one conversation.
    It is populated by the identity resolver before the message reaches the
    bus.  When absent, the session falls back to ``platform:user_id``.
    """

    platform: str           # "telegram" | "discord" | "whatsapp" | "slack"
    user_id: str            # platform-native identifier
    display_name: str = ""
    user_key: str = ""      # "user:<hex>" — stable cross-platform identity key


@dataclass
class InboundMessage:
    """Message received from any channel, ready for the agent loop.

    ``session_key`` determines which conversation this belongs to:
    1. ``session_key_override`` if explicitly set by the channel adapter
    2. ``sender.user_key`` if a cross-platform identity has been resolved
    3. ``channel:chat_id`` as the default (one conversation per chat)

    Cross-channel example::

        # WhatsApp message from Alice (user_key set by identity resolver)
        InboundMessage(channel="whatsapp", chat_id="+1234567890",
                       sender=SenderInfo("whatsapp", "+1234567890",
                                         user_key="user:abc123"), ...)
        # Telegram message from the same Alice
        InboundMessage(channel="telegram", chat_id="12345678",
                       sender=SenderInfo("telegram", "12345678",
                                         user_key="user:abc123"), ...)
        # Both route to session "user:abc123" — one shared conversation.
    """

    channel: str                           # originating channel
    sender: SenderInfo
    chat_id: str                           # channel-native chat/room identifier
    content: str
    timestamp: datetime = field(default_factory=datetime.now)
    media: list[str] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)
    session_key_override: str | None = None

    @property
    def session_key(self) -> str:
        """Stable key used to group messages into one conversation."""
        if self.session_key_override:
            return self.session_key_override
        if self.sender.user_key:
            return self.sender.user_key
        return f"{self.channel}:{self.chat_id}"


@dataclass
class OutboundMessage:
    """Reply from the agent loop, addressed to a specific channel chat.

    ``channel`` + ``chat_id`` always identify where to send.  The agent
    loop copies them from the ``InboundMessage`` it is responding to,
    ensuring the reply goes back to the originating channel.
    """

    channel: str
    chat_id: str
    content: str
    reply_to: str | None = None          # message-id for threaded replies (optional)
    media: list[str] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)
    session_key: str = ""                # informational; set by agent loop
