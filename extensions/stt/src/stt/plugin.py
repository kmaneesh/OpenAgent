"""STT extension entrypoint module."""

from __future__ import annotations

import asyncio
import os
from pathlib import Path
from typing import Any

from openagent.interfaces import BaseAsyncExtension

from .providers import DeepgramProvider, FasterWhisperProvider, STTProvider


class STTExtension(BaseAsyncExtension):
    def __init__(self, *, config: dict[str, Any] | None = None):
        self._config = config or {}
        self._provider_name = str(
            self._config.get("provider") or os.getenv("OPENAGENT_STT_PROVIDER", "faster-whisper")
        ).lower()
        self._provider: STTProvider | None = None
        self._status: dict[str, Any] = {
            "running": False,
            "provider": self._provider_name,
        }

    async def initialize(self) -> None:
        self._provider = self._build_provider(self._provider_name)
        self._status["running"] = True

    async def shutdown(self) -> None:
        self._status["running"] = False

    def get_status(self) -> dict[str, Any]:
        return dict(self._status)

    async def listen(
        self,
        *,
        stream=None,
        file: str | os.PathLike[str] | None = None,
        audio_data: bytes | None = None,
        chunk_bytes: int = 64000,
        **kwargs,
    ) -> str:
        provider = self._require_provider()

        if file is not None:
            file_path = Path(file)
            data = await asyncio.to_thread(file_path.read_bytes)
            return await provider.transcribe(data, **kwargs)

        if audio_data is not None:
            return await provider.transcribe(audio_data, **kwargs)

        if stream is not None:
            parts: list[str] = []
            buffer = bytearray()
            async for chunk in stream:
                if not chunk:
                    continue
                buffer.extend(chunk)
                if len(buffer) >= chunk_bytes:
                    text = await provider.transcribe(bytes(buffer), **kwargs)
                    if text:
                        parts.append(text)
                    buffer.clear()
            if buffer:
                text = await provider.transcribe(bytes(buffer), **kwargs)
                if text:
                    parts.append(text)
            return " ".join(parts).strip()

        raise ValueError("Provide one of: stream, file, or audio_data.")

    async def listen_stream(self, stream, *, chunk_bytes: int = 64000, **kwargs):
        provider = self._require_provider()
        buffer = bytearray()
        async for chunk in stream:
            if not chunk:
                continue
            buffer.extend(chunk)
            if len(buffer) >= chunk_bytes:
                yield await provider.transcribe(bytes(buffer), **kwargs)
                buffer.clear()
        if buffer:
            yield await provider.transcribe(bytes(buffer), **kwargs)

    def _require_provider(self) -> STTProvider:
        if self._provider is None:
            raise RuntimeError("STTExtension is not initialized.")
        return self._provider

    def _build_provider(self, provider_name: str) -> STTProvider:
        if provider_name in {"faster-whisper", "whisper", "local"}:
            model_size = str(self._config.get("whisper_model", "small"))
            return FasterWhisperProvider(model_size=model_size)
        if provider_name == "deepgram":
            return DeepgramProvider()
        raise ValueError(f"Unsupported STT provider '{provider_name}'.")
