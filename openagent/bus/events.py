"""Message bus event types — wire between platforms and the agent loop."""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime
from typing import Any


@dataclass(slots=True)
class SenderInfo:
    """Identifies the sender across platforms.

    ``user_key`` enables cross-platform sessions: when set (e.g.
    ``"user:abc123"``), all platforms for that user share one conversation.
    It is populated by the identity resolver before the message reaches the
    bus.  When absent, the session falls back to ``platform:user_id``.
    """

    platform: str           # "telegram" | "discord" | "whatsapp" | "slack"
    user_id: str            # platform-native identifier
    display_name: str = ""
    user_key: str = ""      # "user:<hex>" — stable cross-platform identity key


@dataclass
class InboundMessage:
    """Message received from any platform, ready for the agent loop.

    ``session_key`` determines which conversation this belongs to:
    1. ``session_key_override`` if explicitly set by the platform adapter
    2. ``sender.user_key`` if a cross-platform identity has been resolved
    3. ``platform:channel_id`` as the default (one conversation per channel)

    Cross-platform example::

        # WhatsApp message from Alice (user_key set by identity resolver)
        InboundMessage(platform="whatsapp", channel_id="+1234567890",
                       sender=SenderInfo("whatsapp", "+1234567890",
                                         user_key="user:abc123"), ...)
        # Telegram message from the same Alice
        InboundMessage(platform="telegram", channel_id="12345678",
                       sender=SenderInfo("telegram", "12345678",
                                         user_key="user:abc123"), ...)
        # Both route to session "user:abc123" — one shared conversation.
    """

    platform: str                           # originating platform
    sender: SenderInfo
    channel_id: str                        # platform-native channel/room identifier
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
        return f"{self.platform}:{self.channel_id}"


@dataclass
class OutboundMessage:
    """Reply from the agent loop, addressed to a specific platform chat.

    ``platform`` + ``channel_id`` always identify where to send.  The agent
    loop copies them from the ``InboundMessage`` it is responding to,
    ensuring the reply goes back to the originating platform.
    """

    platform: str
    channel_id: str
    content: str
    reply_to: str | None = None          # message-id for threaded replies (optional)
    media: list[str] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)
    session_key: str = ""                # informational; set by agent loop
