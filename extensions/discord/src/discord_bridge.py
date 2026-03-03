"""Bridge from discord.py message objects to OpenAgentMessage."""

from __future__ import annotations

from typing import Any

from discord_schema import OpenAgentMessage


def discord_message_to_openagent(message: Any, *, account_id: str = "default") -> OpenAgentMessage:
    channel = getattr(message, "channel", None)
    guild = getattr(message, "guild", None)
    author = getattr(message, "author", None)
    reference = getattr(message, "reference", None)
    created_at = getattr(message, "created_at", None)
    timestamp = int(created_at.timestamp() * 1000) if created_at else None

    chat_type = "direct" if guild is None else "group"
    channel_id = str(getattr(channel, "id", "")) or None
    sender_id = str(getattr(author, "id", "")) or None
    sender_name = getattr(author, "display_name", None) or getattr(author, "name", None)
    sender_username = getattr(author, "name", None)

    media_urls: list[str] = []
    media_types: list[str] = []
    for attachment in getattr(message, "attachments", []) or []:
        url = getattr(attachment, "url", None)
        if url:
            media_urls.append(str(url))
        ctype = getattr(attachment, "content_type", None)
        if ctype:
            media_types.append(str(ctype))

    return OpenAgentMessage(
        id=str(getattr(message, "id", "")) or None,
        from_id=channel_id,
        to_id=channel_id,
        account_id=account_id,
        body=getattr(message, "content", "") or "",
        timestamp=timestamp,
        chat_type=chat_type,
        chat_id=channel_id,
        sender_id=sender_id,
        sender_name=sender_name,
        sender_username=sender_username,
        media_urls=media_urls or None,
        media_types=media_types or None,
        reply_to_id=str(getattr(reference, "message_id", "")) or None,
        conversation_label=getattr(channel, "name", None) or channel_id,
        originating_to=channel_id,
        raw_event={"type": "discord.message"},
    )
