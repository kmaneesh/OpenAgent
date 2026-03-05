"""ToolRegistry — maps tool names to MCP-lite clients from ServiceManager.

The agent loop calls ``ToolRegistry.call(name, args)`` without knowing
which Go/Rust service owns the tool.  The registry handles dispatch.

Tool schemas are in OpenAI function-calling format so they can be passed
directly to ``provider.chat(tools=[...])`` regardless of provider.

Native tools
------------
Python-native tools (not backed by a Go service) can be registered via
``register_native(name, description, params_schema, fn)``.  Their handler
signature is ``async fn(session_key: str, args: dict) -> str``.  Native
registrations survive ``rebuild()`` calls.
"""

from __future__ import annotations

import json
import logging
from collections.abc import Awaitable, Callable
from typing import Any

from openagent.services.manager import ServiceManager
from openagent.services import protocol as proto

NativeHandler = Callable[[str, dict[str, Any]], Awaitable[str]]

logger = logging.getLogger(__name__)

_TOOL_CALL_TIMEOUT = 30.0  # seconds


class ToolRegistry:
    """Discovers tools from all running services and dispatches tool calls.

    Built once per ``AgentLoop`` from the ``ServiceManager``.  Refreshed on
    ``rebuild()`` if services restart and expose new tools.
    """

    def __init__(self, service_manager: ServiceManager) -> None:
        self._mgr = service_manager
        # tool_name → service_name for routing (Go service tools)
        self._tool_to_service: dict[str, str] = {}
        # OpenAI-format schemas from Go services (rebuilt on every rebuild())
        self._schemas: list[dict[str, Any]] = []
        # Native Python tools — survive rebuild()
        self._native_handlers: dict[str, NativeHandler] = {}
        self._native_schemas: list[dict[str, Any]] = []

    # ------------------------------------------------------------------
    # Build / rebuild
    # ------------------------------------------------------------------

    async def rebuild(self) -> None:
        """Discover all tools currently exposed by running services.

        Called at startup and whenever a service restarts (watchdog can
        notify the agent loop to rebuild).
        """
        self._tool_to_service.clear()
        self._schemas.clear()

        for svc in self._mgr.list_services():
            client = self._mgr.get_client(svc.name)
            if client is None:
                continue
            try:
                frame = await client.request({"type": "tools.list"}, timeout_s=5.0)
            except Exception:
                logger.warning("Could not list tools from service %s", svc.name)
                continue

            if not isinstance(frame, proto.ToolListResponse):
                continue

            for tool in frame.tools:
                if tool.name in self._tool_to_service:
                    logger.warning(
                        "Duplicate tool name %r — service %s overrides %s",
                        tool.name, svc.name, self._tool_to_service[tool.name],
                    )
                self._tool_to_service[tool.name] = svc.name
                self._schemas.append(_to_openai_schema(tool))

        logger.info(
            "ToolRegistry: %d tools from %d services",
            len(self._schemas),
            len({s for s in self._tool_to_service.values()}),
        )

    # ------------------------------------------------------------------
    # Native tool registration
    # ------------------------------------------------------------------

    def register_native(
        self,
        name: str,
        description: str,
        params_schema: dict[str, Any],
        fn: NativeHandler,
    ) -> None:
        """Register a Python-native tool alongside Go service tools.

        ``fn`` must be ``async fn(session_key: str, args: dict) -> str``.
        Native registrations survive ``rebuild()`` calls.
        """
        self._native_handlers[name] = fn
        self._native_schemas.append({
            "type": "function",
            "function": {
                "name": name,
                "description": description,
                "parameters": params_schema or {"type": "object", "properties": {}},
            },
        })
        logger.debug("ToolRegistry: registered native tool %r", name)

    # ------------------------------------------------------------------
    # Query
    # ------------------------------------------------------------------

    def schemas(self) -> list[dict[str, Any]]:
        """Return all tool schemas (Go services + native) in OpenAI format."""
        return list(self._schemas) + list(self._native_schemas)

    def has_tools(self) -> bool:
        return bool(self._schemas) or bool(self._native_schemas)

    # ------------------------------------------------------------------
    # Dispatch
    # ------------------------------------------------------------------

    async def call(
        self,
        name: str,
        arguments: dict[str, Any],
        *,
        session_key: str = "",
    ) -> str:
        """Invoke a tool — native Python tools first, then Go service tools.

        Returns the result string.  On error returns an error description
        instead of raising — the agent loop injects this into the conversation
        so the LLM can react gracefully.
        """
        # Native tools take priority (no network hop)
        if name in self._native_handlers:
            try:
                return await self._native_handlers[name](session_key, arguments)
            except Exception as exc:
                msg = f"[tool error] {name!r}: {exc}"
                logger.error(msg)
                return msg

        service_name = self._tool_to_service.get(name)
        if service_name is None:
            msg = f"[tool error] unknown tool: {name!r}"
            logger.warning(msg)
            return msg

        client = self._mgr.get_client(service_name)
        if client is None:
            msg = f"[tool error] service {service_name!r} not running"
            logger.warning(msg)
            return msg

        try:
            frame = await client.request(
                {"type": "tool.call", "tool": name, "params": arguments},
                timeout_s=_TOOL_CALL_TIMEOUT,
            )
        except TimeoutError:
            msg = f"[tool error] {name!r} timed out after {_TOOL_CALL_TIMEOUT}s"
            logger.error(msg)
            return msg
        except Exception as exc:
            msg = f"[tool error] {name!r}: {exc}"
            logger.error(msg)
            return msg

        if isinstance(frame, proto.ToolResultResponse):
            if frame.error:
                return f"[tool error] {frame.error}"
            return frame.result or ""

        return f"[tool error] unexpected frame type: {type(frame).__name__}"


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _to_openai_schema(tool: proto.ToolDefinition) -> dict[str, Any]:
    """Convert a MCP-lite ToolDefinition to OpenAI function-calling format."""
    return {
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.params or {"type": "object", "properties": {}},
        },
    }
