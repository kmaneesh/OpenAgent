from __future__ import annotations

import json
from unittest.mock import AsyncMock

import pytest

from openagent.llm import Message
from openagent.providers.cortex import CortexProvider, _latest_user_input
from openagent.services.protocol import ToolResultResponse


def test_latest_user_input_uses_only_most_recent_user_turn() -> None:
    user_input = _latest_user_input([
        Message("system", "ignored"),
        Message("user", "hello"),
        Message("assistant", "hi"),
        Message("tool", "42", tool_name="calculator"),
        Message("user", "latest"),
    ])
    assert user_input == "latest"


@pytest.mark.asyncio
async def test_cortex_provider_chat_routes_to_cortex_step() -> None:
    client = AsyncMock()
    client.request = AsyncMock(
        return_value=ToolResultResponse(
            id="1",
            type="tool.result",
            result=json.dumps({"response_text": "world"}),
            error=None,
        )
    )
    provider = CortexProvider(
        get_client=lambda: client,
        default_agent_name="AgentM",
        timeout_s=305.0,
    )

    result = await provider.chat(
        [Message("system", "sys"), Message("user", "hello")],
        session_key="web:123",
    )

    assert result.content == "world"
    payload = client.request.await_args.args[0]
    assert payload["tool"] == "cortex.step"
    assert payload["params"]["session_id"] == "web:123"
    assert payload["params"]["agent_name"] == "AgentM"
    assert payload["params"]["user_input"] == "hello"
    assert client.request.await_args.kwargs["timeout_s"] == 305.0


@pytest.mark.asyncio
async def test_cortex_provider_translates_tool_call() -> None:
    client = AsyncMock()
    client.request = AsyncMock(
        return_value=ToolResultResponse(
            id="1",
            type="tool.result",
            result=json.dumps(
                {
                    "response_type": "tool_call",
                    "tool_call": {
                        "tool": "browser.open",
                        "arguments": {"url": "https://weather.com"},
                    },
                }
            ),
            error=None,
        )
    )
    provider = CortexProvider(
        get_client=lambda: client,
        default_agent_name="AgentM",
        timeout_s=305.0,
    )

    result = await provider.chat(
        [Message("system", "sys"), Message("user", "weather in florida")],
        session_key="web:123",
    )

    assert result.content == ""
    assert len(result.tool_calls) == 1
    assert result.tool_calls[0].name == "browser.open"
    assert result.tool_calls[0].arguments == {"url": "https://weather.com"}
