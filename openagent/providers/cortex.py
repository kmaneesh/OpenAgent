"""Provider adapter that routes LLM calls through the Cortex MCP-lite service."""

from __future__ import annotations

import json
from collections.abc import Callable
from typing import Any, AsyncIterator

from openagent.llm import LLMResponse, Message, StreamEvent, ToolCall
from openagent.platforms.mcplite import McpLiteClient
from openagent.services import protocol as proto


class CortexProvider:
    """Provider-compatible adapter that delegates chat completion to Cortex.

    Cortex returns structured final/tool-call output. The adapter converts
    tool calls into the existing Python ``LLMResponse`` contract so AgentLoop
    can execute them.
    """

    def __init__(
        self,
        *,
        get_client: Callable[[], McpLiteClient | None],
        default_agent_name: str = "",
        timeout_s: float = 90.0,
    ) -> None:
        self._get_client = get_client
        self._default_agent_name = default_agent_name
        self._timeout_s = timeout_s

    async def stream(
        self, messages: list[Message], **kwargs: Any
    ) -> AsyncIterator[str]:
        response = await self.chat(messages, **kwargs)
        if response.content:
            yield response.content

    async def complete(self, messages: list[Message], **kwargs: Any) -> str:
        response = await self.chat(messages, **kwargs)
        return response.content

    async def chat(
        self,
        messages: list[Message],
        tools: list[dict[str, Any]] | None = None,
        **kwargs: Any,
    ) -> LLMResponse:
        del tools  # Cortex owns the default tool set for this phase.

        client = self._get_client()
        if client is None:
            raise RuntimeError("cortex service is not running")

        session_key = str(kwargs.get("session_key") or "").strip()
        if not session_key:
            raise RuntimeError("cortex provider requires session_key")

        user_input = _latest_user_input(messages)
        payload: dict[str, Any] = {
            "type": "tool.call",
            "tool": "cortex.step",
            "params": {
                "session_id": session_key,
                "user_input": user_input,
            },
        }
        if self._default_agent_name:
            payload["params"]["agent_name"] = self._default_agent_name

        frame = await client.request(payload, timeout_s=self._timeout_s)
        if not isinstance(frame, proto.ToolResultResponse):
            raise RuntimeError(f"unexpected cortex response: {type(frame).__name__}")
        if frame.error:
            raise RuntimeError(frame.error)

        parsed = json.loads(frame.result or "{}")
        response_type = str(parsed.get("response_type") or "final").strip()
        if response_type == "tool_call":
            tool_call = _parse_tool_call(parsed)
            return LLMResponse(content="", tool_calls=[tool_call])
        return LLMResponse(content=str(parsed.get("response_text") or ""))

    async def stream_with_tools(
        self,
        messages: list[Message],
        *,
        tools: list[dict[str, Any]] | None = None,
        **kwargs: Any,
    ) -> AsyncIterator[StreamEvent]:
        response = await self.chat(messages, tools=tools, **kwargs)
        if response.tool_calls:
            yield StreamEvent(tool_calls=response.tool_calls, finish_reason="tool_calls")
        elif response.content:
            yield StreamEvent(content=response.content)


def _parse_tool_call(payload: dict[str, Any]) -> ToolCall:
    raw_tool = payload.get("tool_call") or {}
    if not isinstance(raw_tool, dict):
        raise RuntimeError("cortex tool_call payload must be an object")
    name = str(raw_tool.get("tool") or "").strip()
    if not name:
        raise RuntimeError("cortex tool_call payload missing tool")
    arguments = raw_tool.get("arguments") or {}
    if not isinstance(arguments, dict):
        raise RuntimeError("cortex tool_call arguments must be an object")
    return ToolCall(id=f"cortex-{name}", name=name, arguments=arguments)


def _latest_user_input(messages: list[Message]) -> str:
    """Return only the latest user turn for stateless Cortex Phase 1."""
    for message in reversed(messages):
        if message.role != "user":
            continue
        content = message.content.strip()
        if content:
            return content
    raise RuntimeError("cortex provider requires at least one user message")
