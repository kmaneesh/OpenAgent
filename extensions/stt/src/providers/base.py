"""Abstract async STT provider interface."""

from __future__ import annotations

from abc import ABC, abstractmethod


class STTProvider(ABC):
    @abstractmethod
    async def transcribe(self, audio_data: bytes, **kwargs) -> str:
        """Transcribe audio bytes into text."""
