from __future__ import annotations

from unittest.mock import AsyncMock

import pytest

from tts.plugin import TTSExtension


@pytest.mark.asyncio
async def test_tts_extension_defaults_to_edge():
    ext = TTSExtension(config={})
    await ext.initialize()
    assert ext.get_status()["provider"] == "edge"
    await ext.shutdown()


@pytest.mark.asyncio
async def test_tts_extension_selects_minimax_by_config():
    ext = TTSExtension(config={"provider": "minimax"})
    await ext.initialize()
    assert ext.get_status()["provider"] == "minimax"
    await ext.shutdown()


@pytest.mark.asyncio
async def test_tts_extension_speak_delegates_to_provider():
    ext = TTSExtension(config={})
    await ext.initialize()
    provider = ext._provider
    assert provider is not None
    provider.generate = AsyncMock(return_value=b"audio")  # type: ignore[method-assign]
    result = await ext.speak("hello")
    assert result == b"audio"
    await ext.shutdown()
