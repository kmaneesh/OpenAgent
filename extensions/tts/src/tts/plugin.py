"""TTS extension entrypoint module."""

from __future__ import annotations

import os
from collections.abc import AsyncIterator
from typing import Any

from openagent.interfaces import BaseAsyncExtension

from .providers import EdgeProvider, MiniMaxProvider, TTSProvider


class TTSExtension(BaseAsyncExtension):
    def __init__(self, *, config: dict[str, Any] | None = None):
        self._config = config or {}
        self._provider_name = str(
            self._config.get("provider")
            or os.getenv("OPENAGENT_TTS_PROVIDER", "edge")
        ).lower()
        self._provider: TTSProvider | None = None
        self._status: dict[str, Any] = {"running": False, "provider": self._provider_name}

    async def initialize(self) -> None:
        self._provider = self._build_provider(self._provider_name)
        self._status["running"] = True

    async def shutdown(self) -> None:
        self._status["running"] = False

    def get_status(self) -> dict[str, Any]:
        return dict(self._status)

    async def speak(self, text: str, **kwargs) -> bytes:
        provider = self._require_provider()
        return await provider.generate(text, **kwargs)

    async def speak_stream(self, text: str, **kwargs) -> AsyncIterator[bytes]:
        provider = self._require_provider()
        async for chunk in provider.generate_stream(text, **kwargs):
            yield chunk

    def _require_provider(self) -> TTSProvider:
        if self._provider is None:
            raise RuntimeError("TTSExtension is not initialized.")
        return self._provider

    def _build_provider(self, provider_name: str) -> TTSProvider:
        if provider_name == "edge":
            return EdgeProvider()
        if provider_name == "minimax":
            return MiniMaxProvider()
        raise ValueError(f"Unsupported TTS provider '{provider_name}'.")
