from __future__ import annotations

import asyncio

from plugin import WhatsAppExtension


def test_whatsapp_extension_initializes(capsys):
    extension = WhatsAppExtension()
    asyncio.run(extension.initialize())
    out = capsys.readouterr().out
    assert "WhatsApp extension initialized." in out
    asyncio.run(extension.shutdown())
