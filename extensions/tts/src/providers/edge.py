"""Edge TTS provider (default, no API key required)."""

from __future__ import annotations

import asyncio
from collections.abc import AsyncIterator
import time

import edge_tts

from openagent.observability import log_event
from openagent.observability.logging import get_logger
from openagent.observability.metrics import PROVIDER_CALL_SECONDS

from .base import TTSProvider

logger = get_logger(__name__)


class EdgeProvider(TTSProvider):
    def __init__(
        self,
        *,
        voice: str = "en-US-AriaNeural",
        speed: str = "+0%",
        volume: str = "+0%",
    ):
        self._default_voice = voice
        self._default_speed = speed
        self._default_volume = volume

    async def generate(self, text: str, **kwargs) -> bytes:
        start = time.perf_counter()
        status = "ok"
        voice = str(kwargs.get("voice_id", self._default_voice))
        rate = str(kwargs.get("speed", self._default_speed))
        volume = str(kwargs.get("vol", self._default_volume))
        timeout_s = float(kwargs.get("timeout_s", 20.0))
        communicator = edge_tts.Communicate(text=text, voice=voice, rate=rate, volume=volume)
        chunks: list[bytes] = []
        try:
            async with asyncio.timeout(timeout_s):
                async for item in communicator.stream():
                    if item.get("type") == "audio":
                        chunks.append(item["data"])
            return b"".join(chunks)
        except Exception:
            status = "error"
            raise
        finally:
            elapsed = time.perf_counter() - start
            PROVIDER_CALL_SECONDS.labels(
                extension="tts",
                provider="edge",
                operation="generate",
                status=status,
            ).observe(elapsed)
            log_event(
                logger,
                20 if status == "ok" else 40,
                "edge tts generate complete",
                component="provider.tts",
                provider="edge",
                operation="generate",
                status=status,
                text_length=len(text),
                duration_ms=round(elapsed * 1000, 3),
            )

    async def generate_stream(self, text: str, **kwargs) -> AsyncIterator[bytes]:
        start = time.perf_counter()
        status = "ok"
        timeout_s = float(kwargs.get("timeout_s", 20.0))
        voice = str(kwargs.get("voice_id", self._default_voice))
        rate = str(kwargs.get("speed", self._default_speed))
        volume = str(kwargs.get("vol", self._default_volume))
        communicator = edge_tts.Communicate(text=text, voice=voice, rate=rate, volume=volume)
        try:
            async with asyncio.timeout(timeout_s):
                async for item in communicator.stream():
                    if item.get("type") == "audio":
                        yield item["data"]
        except Exception:
            status = "error"
            raise
        finally:
            elapsed = time.perf_counter() - start
            PROVIDER_CALL_SECONDS.labels(
                extension="tts",
                provider="edge",
                operation="generate_stream",
                status=status,
            ).observe(elapsed)
