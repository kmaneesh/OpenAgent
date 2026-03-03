"""Abstract interface for async TTS providers."""

from __future__ import annotations

from abc import ABC, abstractmethod
from collections.abc import AsyncIterator


class TTSProvider(ABC):
    @abstractmethod
    async def generate(self, text: str, **kwargs) -> bytes:
        """Generate audio bytes from text."""

    async def generate_stream(self, text: str, **kwargs) -> AsyncIterator[bytes]:
        """Optional streaming API; default wraps aggregate generation."""
        yield await self.generate(text, **kwargs)
