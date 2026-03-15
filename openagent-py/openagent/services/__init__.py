"""Service-plane contracts and helpers."""

from .manager import (
    HealthConfig,
    ManagedService,
    ServiceManifest,
    ServiceManager,
    ServiceStatus,
)
from .protocol import (
    EventFrame,
    FrameModel,
    ProtocolErrorFrame,
    ProtocolPing,
    ProtocolPong,
    ToolCallRequest,
    ToolDefinition,
    ToolListRequest,
    ToolListResponse,
    ToolResultResponse,
    parse_frame,
)

__all__ = [
    # manager
    "HealthConfig",
    "ManagedService",
    "ServiceManifest",
    "ServiceManager",
    "ServiceStatus",
    # protocol
    "EventFrame",
    "FrameModel",
    "ProtocolErrorFrame",
    "ProtocolPing",
    "ProtocolPong",
    "ToolCallRequest",
    "ToolDefinition",
    "ToolListRequest",
    "ToolListResponse",
    "ToolResultResponse",
    "parse_frame",
]
