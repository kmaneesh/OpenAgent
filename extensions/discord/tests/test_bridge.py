from __future__ import annotations

from datetime import datetime, timezone
from types import SimpleNamespace

from discord_bridge import discord_message_to_openagent


def test_bridge_maps_discord_message_to_openagent_schema():
    author = SimpleNamespace(id=42, display_name="Alice", name="alice")
    channel = SimpleNamespace(id=77, name="general")
    guild = SimpleNamespace(id=1, name="Guild")
    attachment = SimpleNamespace(url="https://example.com/file.txt", content_type="text/plain")
    message = SimpleNamespace(
        id=1001,
        content="hello world",
        author=author,
        channel=channel,
        guild=guild,
        attachments=[attachment],
        reference=SimpleNamespace(message_id=999),
        created_at=datetime(2026, 3, 3, tzinfo=timezone.utc),
    )
    result = discord_message_to_openagent(message, account_id="work")
    assert result.account_id == "work"
    assert result.chat_type == "group"
    assert result.sender_id == "42"
    assert result.conversation_label == "general"
    assert result.media_urls == ["https://example.com/file.txt"]
