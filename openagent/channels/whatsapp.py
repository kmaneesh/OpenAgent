"""WhatsApp channel contract for backend implementations."""

from __future__ import annotations

from abc import ABC, abstractmethod
from typing import Any


class WhatsAppChannel(ABC):
    """Backend-agnostic WhatsApp channel interface."""

    @abstractmethod
    async def start(self) -> None:
        """Start the channel backend."""

    @abstractmethod
    async def stop(self) -> None:
        """Stop the channel backend."""

    @abstractmethod
    async def send_text(self, chat_id: str, text: str) -> Any:
        """Send a text message to a chat."""

    @abstractmethod
    def get_status(self) -> dict[str, Any]:
        """Return backend status details."""

    @abstractmethod
    def latest_qr(self) -> str | None:
        """Return latest QR payload, if available."""

    @abstractmethod
    def pop_messages(self) -> list[Any]:
        """Drain buffered inbound messages."""
