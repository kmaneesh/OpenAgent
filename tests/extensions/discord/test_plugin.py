from __future__ import annotations

import asyncio

from discord_plugin import DiscordExtension


def test_discord_extension_initializes(capsys):
    extension = DiscordExtension()
    asyncio.run(extension.initialize())
    out = capsys.readouterr().out
    assert "Discord extension initialized." in out
    asyncio.run(extension.shutdown())
