"""Standardized OpenAgent message schema for Discord bridge events."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass(slots=True)
class OpenAgentMessage:
    id: str | None = None
    from_id: str | None = None
    to_id: str | None = None
    account_id: str = "default"
    body: str = ""
    timestamp: int | None = None
    chat_type: str = "direct"
    chat_id: str | None = None
    sender_id: str | None = None
    sender_name: str | None = None
    sender_username: str | None = None
    media_paths: list[str] | None = None
    media_types: list[str] | None = None
    media_urls: list[str] | None = None
    reply_to_id: str | None = None
    reply_to_body: str | None = None
    conversation_label: str | None = None
    provider: str = "discord"
    surface: str = "discord"
    originating_channel: str = "discord"
    originating_to: str | None = None
    raw_event: dict[str, Any] = field(default_factory=dict)
