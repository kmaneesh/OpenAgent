"""Deepgram cloud STT provider using deepgram-sdk."""

from __future__ import annotations

import asyncio
import os
from typing import Any

from .base import STTProvider


class DeepgramProvider(STTProvider):
    def __init__(self, *, api_key: str | None = None, model: str = "nova-3"):
        self.api_key = api_key or os.getenv("DEEPGRAM_API_KEY")
        self.model = model

    async def transcribe(self, audio_data: bytes, **kwargs) -> str:
        if not self.api_key:
            raise RuntimeError("DEEPGRAM_API_KEY is required for Deepgram provider.")
        language = kwargs.get("language", "en")
        punctuate = bool(kwargs.get("punctuate", True))
        smart_format = bool(kwargs.get("smart_format", True))

        def _transcribe_sync() -> str:
            from deepgram import DeepgramClient

            client = DeepgramClient(self.api_key)
            payload = {"buffer": audio_data}
            options = {
                "model": self.model,
                "punctuate": punctuate,
                "smart_format": smart_format,
                "language": language,
            }

            response = client.listen.prerecorded.v("1").transcribe_file(payload, options)
            return self._extract_transcript(response)

        return (await asyncio.to_thread(_transcribe_sync)).strip()

    @staticmethod
    def _extract_transcript(response: Any) -> str:
        if hasattr(response, "to_dict"):
            data = response.to_dict()
        elif isinstance(response, dict):
            data = response
        else:
            data = getattr(response, "__dict__", {})
        try:
            return (
                data["results"]["channels"][0]["alternatives"][0].get("transcript", "") or ""
            ).strip()
        except Exception:
            return ""
