"""MCP-lite wire schema for Python control plane."""

from __future__ import annotations

from typing import Annotated, Any, Literal, TypeAlias

from pydantic import BaseModel, ConfigDict, Field, TypeAdapter


class _StrictModel(BaseModel):
    """Base model with deterministic validation rules."""

    model_config = ConfigDict(extra="forbid", strict=True)


class ToolDefinition(_StrictModel):
    """Tool descriptor exchanged during tools.list."""

    name: str
    description: str
    params: dict[str, Any]


class ToolListRequest(_StrictModel):
    """Agent request to enumerate tools."""

    id: str
    type: Literal["tools.list"]


class ToolCallRequest(_StrictModel):
    """Agent request to call one tool."""

    id: str
    type: Literal["tool.call"]
    tool: str
    params: dict[str, Any] = Field(default_factory=dict)


class ProtocolPing(_StrictModel):
    """Health-check ping from agent."""

    id: str
    type: Literal["ping"]


class ToolListResponse(_StrictModel):
    """Service response with tool schemas."""

    id: str
    type: Literal["tools.list.ok"]
    tools: list[ToolDefinition]


class ToolResultResponse(_StrictModel):
    """Service response with tool execution result."""

    id: str
    type: Literal["tool.result"]
    result: str | None = None
    error: str | None = None


class ProtocolPong(_StrictModel):
    """Health-check response from service."""

    id: str
    type: Literal["pong"]
    status: str


class ProtocolErrorFrame(_StrictModel):
    """Error response with stable code + message."""

    id: str
    type: Literal["error"]
    code: str
    message: str


class EventFrame(_StrictModel):
    """Unprompted service event, no request id."""

    type: Literal["event"]
    event: str
    data: dict[str, Any] = Field(default_factory=dict)


RequestFrame: TypeAlias = Annotated[
    ToolListRequest | ToolCallRequest | ProtocolPing, Field(discriminator="type")
]
ResponseFrame: TypeAlias = Annotated[
    ToolListResponse | ToolResultResponse | ProtocolPong | ProtocolErrorFrame,
    Field(discriminator="type"),
]
FrameModel: TypeAlias = RequestFrame | ResponseFrame | EventFrame

_REQUEST_ADAPTER: TypeAdapter[RequestFrame] = TypeAdapter(RequestFrame)
_RESPONSE_ADAPTER: TypeAdapter[ResponseFrame] = TypeAdapter(ResponseFrame)
_EVENT_ADAPTER: TypeAdapter[EventFrame] = TypeAdapter(EventFrame)


def parse_frame(payload: bytes | str | dict[str, Any]) -> FrameModel:
    """Parse one MCP-lite frame into the concrete pydantic model."""

    raw: Any
    if isinstance(payload, bytes):
        raw = payload.decode("utf-8")
    else:
        raw = payload

    if isinstance(raw, str):
        raw = raw.strip()
        if not raw:
            raise ValueError("empty frame")
        value: Any = TypeAdapter(dict[str, Any]).validate_json(raw)
    else:
        value = raw

    frame_type = value.get("type")
    if frame_type == "event":
        return _EVENT_ADAPTER.validate_python(value)
    if frame_type in {"tools.list", "tool.call", "ping"}:
        return _REQUEST_ADAPTER.validate_python(value)
    if frame_type in {"tools.list.ok", "tool.result", "pong", "error"}:
        return _RESPONSE_ADAPTER.validate_python(value)
    raise ValueError(f"unsupported frame type: {frame_type!r}")
