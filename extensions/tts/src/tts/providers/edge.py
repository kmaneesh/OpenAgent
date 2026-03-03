"""Edge TTS provider (default, no API key required)."""

from __future__ import annotations

from collections.abc import AsyncIterator

import edge_tts

from .base import TTSProvider


class EdgeProvider(TTSProvider):
    async def generate(self, text: str, **kwargs) -> bytes:
        voice = str(kwargs.get("voice_id", "en-US-AriaNeural"))
        rate = str(kwargs.get("speed", "+0%"))
        volume = str(kwargs.get("vol", "+0%"))
        communicator = edge_tts.Communicate(text=text, voice=voice, rate=rate, volume=volume)
        chunks: list[bytes] = []
        async for item in communicator.stream():
            if item.get("type") == "audio":
                chunks.append(item["data"])
        return b"".join(chunks)

    async def generate_stream(self, text: str, **kwargs) -> AsyncIterator[bytes]:
        voice = str(kwargs.get("voice_id", "en-US-AriaNeural"))
        rate = str(kwargs.get("speed", "+0%"))
        volume = str(kwargs.get("vol", "+0%"))
        communicator = edge_tts.Communicate(text=text, voice=voice, rate=rate, volume=volume)
        async for item in communicator.stream():
            if item.get("type") == "audio":
                yield item["data"]
