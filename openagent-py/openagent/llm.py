"""Shared LLM message and response types for the Python control plane."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, AsyncIterator, Literal, Protocol, runtime_checkable


@dataclass
class StreamEvent:
    content: str | None = None
    tool_calls: list["ToolCall"] | None = None
    finish_reason: str | None = None


@dataclass
class Message:
    role: Literal["system", "user", "assistant", "tool"]
    content: str
    tool_call_id: str = ""
    tool_name: str = ""
    tool_calls: list["ToolCall"] | None = None


@dataclass
class ToolCall:
    id: str
    name: str
    arguments: dict[str, Any]


@dataclass
class LLMResponse:
    content: str
    tool_calls: list[ToolCall] = field(default_factory=list)

    @property
    def has_tool_calls(self) -> bool:
        return bool(self.tool_calls)


@runtime_checkable
class Provider(Protocol):
    async def stream(
        self, messages: list[Message], **kwargs
    ) -> AsyncIterator[str]:
        ...

    async def complete(self, messages: list[Message], **kwargs) -> str:
        ...

    async def chat(
        self,
        messages: list[Message],
        tools: list[dict[str, Any]] | None = None,
        **kwargs,
    ) -> LLMResponse:
        ...

    async def stream_with_tools(
        self,
        messages: list[Message],
        *,
        tools: list[dict[str, Any]] | None = None,
        **kwargs,
    ) -> AsyncIterator[StreamEvent]:
        ...
