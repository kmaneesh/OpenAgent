from __future__ import annotations

from datetime import datetime, timezone
from types import SimpleNamespace

import pytest

from discord_connector import DiscordConnector
from discord_plugin import DiscordExtension


@pytest.mark.asyncio
async def test_on_message_enqueues_openagent_message_with_attachment(monkeypatch, tmp_path):
    extension = DiscordExtension(token=None, data_dir=tmp_path)
    await extension.initialize()
    connector = DiscordConnector(extension)

    async def fake_fetch(url: str) -> bytes:
        assert url.startswith("https://")
        return b"abc"

    monkeypatch.setattr(connector, "_fetch_attachment", fake_fetch)

    attachment = SimpleNamespace(
        url="https://example.com/payload.bin",
        filename="payload.bin",
        content_type="application/octet-stream",
    )
    message = SimpleNamespace(
        id=123,
        content="analyze this",
        author=SimpleNamespace(id=7, bot=False, display_name="Bob", name="bob"),
        channel=SimpleNamespace(id=88, name="ops"),
        guild=SimpleNamespace(id=1, name="guild"),
        attachments=[attachment],
        reference=None,
        created_at=datetime(2026, 3, 3, tzinfo=timezone.utc),
    )

    await connector.on_message(message)
    items = await extension.pop_messages()
    assert len(items) == 1
    assert items[0].body == "analyze this"
    assert items[0].media_paths
    await extension.shutdown()
