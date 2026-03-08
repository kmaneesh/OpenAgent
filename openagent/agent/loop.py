"""AgentLoop — custom ReAct loop. No framework dependency.

Architecture
------------
The loop reads from the MessageBus per-session queue, runs one full
ReAct iteration (LLM → tool calls → LLM → ... → final reply), saves turns
to the SessionManager, then dispatches the reply via the bus.

One coroutine per active session runs concurrently; they share the provider
and tool registry but each owns its own history slice.

Middleware
----------
Middlewares are split into two flat chains based on ``direction``:

* ``"inbound"``  — run before the LLM; receive and mutate ``InboundMessage``
* ``"outbound"`` — run after  the LLM; receive and mutate ``OutboundMessage``

Each middleware is a simple async callable that modifies the message in-place.
The loop owns the chaining; no NextCall / chain-of-responsibility needed.

Iteration limit
---------------
``MAX_ITERATIONS = 40`` prevents infinite loops when a model keeps calling
tools without converging.  When the limit is hit the loop returns whatever
partial content the model last produced (or a timeout notice).

Tool output truncation
----------------------
``MAX_TOOL_OUTPUT = 500`` characters.  Keeps the context window bounded on
low-RAM hardware (Pi 5 8 GB).  The model sees the truncated result; the full
output is logged at DEBUG level.

Session key
-----------
``InboundMessage.session_key`` is used throughout — it already handles
cross-platform identity (user_key → "user:<hex>") and per-chat fallback.
"""

from __future__ import annotations

import asyncio
import logging
from typing import Any

from openagent.bus.bus import MessageBus
from openagent.bus.events import InboundMessage, OutboundMessage
from openagent.providers.base import LLMResponse, Message, Provider, StreamEvent
from openagent.session.manager import SessionManager
from openagent.agent.tools import ToolRegistry
from openagent.agent.middlewares import AgentMiddleware

logger = logging.getLogger(__name__)

MAX_ITERATIONS = 40
MAX_TOOL_OUTPUT = 500   # chars; truncate beyond this
_SYSTEM_PROMPT = (
    "You are a helpful assistant. "
    "Use tools only when necessary. "
    "Be concise."
)


class AgentLoop:
    """Orchestrates InboundMessage → LLM → tools → OutboundMessage.

    Parameters
    ----------
    bus:        MessageBus — publish/subscribe point for all platforms.
    provider:   LLM provider implementing ``chat(messages, tools)``.
    sessions:   SessionManager — history persistence.
    tools:      ToolRegistry — maps tool names to Go/Rust services.
    system_prompt:
        Override the default system prompt.
    middlewares:
        List of ``AgentMiddleware`` instances.  Split by ``direction``:
        ``"inbound"`` runs before the LLM; ``"outbound"`` runs after.
    """

    def __init__(
        self,
        bus: MessageBus,
        provider: Provider,
        sessions: SessionManager,
        tools: ToolRegistry,
        *,
        system_prompt: str = _SYSTEM_PROMPT,
        max_iterations: int = MAX_ITERATIONS,
        max_tool_output: int = MAX_TOOL_OUTPUT,
        middlewares: list[AgentMiddleware] | None = None,
    ) -> None:
        self._bus = bus
        self._provider = provider
        self._sessions = sessions
        self._tools = tools
        self._system_prompt = system_prompt
        self._max_iterations = max_iterations
        self._max_tool_output = max_tool_output
        mw = middlewares or []
        self._inbound_mw  = [m for m in mw if m.direction == "inbound"]
        self._outbound_mw = [m for m in mw if m.direction == "outbound"]
        self._tasks: dict[str, asyncio.Task[None]] = {}
        self._running = False

    # ------------------------------------------------------------------
    # Lifecycle
    # ------------------------------------------------------------------

    async def start(self) -> None:
        """Start the agent loop — register session callback with the bus."""
        self._running = True
        self._bus.on_new_session(self._on_new_session)
        logger.info("AgentLoop started")

    async def stop(self) -> None:
        """Cancel all per-session tasks and wait for them to finish."""
        self._running = False
        for task in self._tasks.values():
            task.cancel()
        if self._tasks:
            await asyncio.gather(*self._tasks.values(), return_exceptions=True)
        self._tasks.clear()
        logger.info("AgentLoop stopped")

    # ------------------------------------------------------------------
    # Session dispatch
    # ------------------------------------------------------------------

    def _on_new_session(self, session_key: str) -> None:
        """Called by the bus fanout the first time a session_key appears."""
        if not self._running:
            return
        task = asyncio.create_task(
            self._session_worker(session_key),
            name=f"agent.session.{session_key}",
        )
        self._tasks[session_key] = task
        task.add_done_callback(lambda _: self._tasks.pop(session_key, None))

    async def _session_worker(self, session_key: str) -> None:
        """Drain the per-session queue; process each message sequentially."""
        q = self._bus.session_queue(session_key)
        while self._running:
            msg = await q.get()
            if msg is None:  # shutdown sentinel
                break
            try:
                outbound = await self._process(msg)
                if outbound is not None:
                    await self._bus.dispatch(outbound)
            except asyncio.CancelledError:
                raise
            except Exception:
                logger.exception(
                    "Unhandled error processing message from session %s", session_key
                )

    # ------------------------------------------------------------------
    # Middleware chains + core ReAct
    # ------------------------------------------------------------------

    async def _process(self, msg: InboundMessage) -> OutboundMessage | None:
        """Run inbound chain → ReAct → outbound chain.

        Returns the final ``OutboundMessage`` to be dispatched, or ``None``
        if the reply was already streamed inline (no further dispatch needed).
        """
        # ── Inbound chain ──────────────────────────────────────────────
        for mw in self._inbound_mw:
            try:
                await mw(msg)
            except Exception:
                logger.exception("Inbound middleware %s raised", type(mw).__name__)

        # ── ReAct loop ─────────────────────────────────────────────────
        outbound = await self._run_react(msg)

        # ── Outbound chain ─────────────────────────────────────────────
        if outbound is not None:
            for mw in self._outbound_mw:
                try:
                    await mw(outbound)
                except Exception:
                    logger.exception("Outbound middleware %s raised", type(mw).__name__)

        return outbound

    async def _run_react(self, msg: InboundMessage) -> OutboundMessage | None:
        """Run one full ReAct turn.

        Returns ``OutboundMessage`` for the caller to dispatch (non-streaming),
        or ``None`` when streaming already sent everything inline.
        """
        session_key = msg.session_key

        # Save user turn
        await self._sessions.append(session_key, "user", msg.content)

        # Build message list: system + history
        history = await self._sessions.get_history(session_key)
        messages: list[Message] = [
            Message("system", self._system_prompt),
            *self._sessions.to_messages(history),
        ]

        tool_schemas = self._tools.schemas() if self._tools.has_tools() else None
        final_content = ""
        streamed = False  # True when we dispatched via stream_chunk (caller returns None)
        # Path A: stream_with_tools when provider supports it; else chat() when tools exist
        stream_with_tools = getattr(
            self._provider, "stream_with_tools", None
        ) if tool_schemas else None
        use_legacy_stream = tool_schemas is None and hasattr(self._provider, "stream")  # no tools + provider supports streaming

        for iteration in range(self._max_iterations):
            try:
                if use_legacy_stream:
                    # No tools — plain stream
                    accumulated = ""
                    first = True
                    async for chunk in self._provider.stream(messages):
                        accumulated += chunk
                        if first or len(accumulated) % 80 < len(chunk):
                            first = False
                            await self._bus.dispatch(OutboundMessage(
                                platform=msg.platform,
                                channel_id=msg.channel_id,
                                content=accumulated,
                                session_key=session_key,
                                metadata={
                                    **dict(msg.metadata),
                                    "stream_chunk": True,
                                    "stream_end": False,
                                },
                            ))
                    final_content = accumulated

                    # Final chunk — outbound middlewares see this via metadata
                    if final_content:
                        stream_end_msg = OutboundMessage(
                            platform=msg.platform,
                            channel_id=msg.channel_id,
                            content=final_content,
                            session_key=session_key,
                            metadata={
                                **dict(msg.metadata),
                                "stream_chunk": True,
                                "stream_end": True,
                            },
                        )
                        for mw in self._outbound_mw:
                            try:
                                await mw(stream_end_msg)
                            except Exception:
                                logger.exception(
                                    "Outbound middleware %s raised on stream_end",
                                    type(mw).__name__,
                                )
                        await self._bus.dispatch(stream_end_msg)
                    streamed = True
                    break

                elif stream_with_tools:
                    # Delta-based streaming with tools — content → UI, tool_calls → buffer
                    accumulated = ""
                    response_tool_calls: list = []
                    response_content = ""

                    async for event in stream_with_tools(messages, tools=tool_schemas):
                        if not isinstance(event, StreamEvent):
                            continue
                        if event.content:
                            accumulated += event.content
                            # Stream to UI immediately
                            if accumulated:
                                await self._bus.dispatch(OutboundMessage(
                                    platform=msg.platform,
                                    channel_id=msg.channel_id,
                                    content=accumulated,
                                    session_key=session_key,
                                    metadata={
                                        **dict(msg.metadata),
                                        "stream_chunk": True,
                                        "stream_end": False,
                                    },
                                ))
                        if event.tool_calls:
                            response_tool_calls = event.tool_calls
                            response_content = accumulated
                            # Optional: Nanobot-style tool hint
                            hint = ", ".join(
                                f"{tc.name}(...)" for tc in event.tool_calls
                            )
                            await self._bus.dispatch(OutboundMessage(
                                platform=msg.platform,
                                channel_id=msg.channel_id,
                                content=hint,
                                session_key=session_key,
                                metadata={
                                    **dict(msg.metadata),
                                    "_progress": True,
                                    "_tool_hint": True,
                                },
                            ))

                    final_content = accumulated
                    if response_tool_calls:
                        # Execute tools and continue loop
                        messages.append(Message(
                            "assistant",
                            response_content or "",
                            tool_calls=response_tool_calls,
                        ))
                        for tc in response_tool_calls:
                            logger.debug("Tool call: %s(%s)", tc.name, tc.arguments)
                            raw_result = await self._tools.call(
                                tc.name, tc.arguments, session_key=session_key
                            )
                            if len(raw_result) > self._max_tool_output:
                                logger.debug(
                                    "Truncating tool output for %s: %d → %d chars",
                                    tc.name, len(raw_result), self._max_tool_output,
                                )
                                raw_result = raw_result[: self._max_tool_output] + "…[truncated]"
                            await self._sessions.append(
                                session_key, "tool", raw_result,
                                tool_call_id=tc.id,
                                tool_name=tc.name,
                            )
                            messages.append(Message(
                                "tool", raw_result,
                                tool_call_id=tc.id,
                                tool_name=tc.name,
                            ))
                        continue  # next iteration
                    # No tool calls — stream ended with text
                    if final_content:
                        stream_end_msg = OutboundMessage(
                            platform=msg.platform,
                            channel_id=msg.channel_id,
                            content=final_content,
                            session_key=session_key,
                            metadata={
                                **dict(msg.metadata),
                                "stream_chunk": True,
                                "stream_end": True,
                            },
                        )
                        for mw in self._outbound_mw:
                            try:
                                await mw(stream_end_msg)
                            except Exception:
                                logger.exception(
                                    "Outbound middleware %s raised on stream_end",
                                    type(mw).__name__,
                                )
                        await self._bus.dispatch(stream_end_msg)
                    streamed = True
                    break

                else:
                    # Fallback: non-streaming chat when tools exist but no stream_with_tools
                    response: LLMResponse = await self._provider.chat(
                        messages, tools=tool_schemas
                    )
                    if response.content:
                        final_content = response.content
                    if not response.has_tool_calls:
                        break
                    messages.append(Message(
                        "assistant",
                        response.content or "",
                        tool_calls=response.tool_calls or None,
                    ))
                    for tc in response.tool_calls:
                        logger.debug("Tool call: %s(%s)", tc.name, tc.arguments)
                        raw_result = await self._tools.call(
                            tc.name, tc.arguments, session_key=session_key
                        )
                        if len(raw_result) > self._max_tool_output:
                            logger.debug(
                                "Truncating tool output for %s: %d → %d chars",
                                tc.name, len(raw_result), self._max_tool_output,
                            )
                            raw_result = raw_result[: self._max_tool_output] + "…[truncated]"
                        await self._sessions.append(
                            session_key, "tool", raw_result,
                            tool_call_id=tc.id,
                            tool_name=tc.name,
                        )
                        messages.append(Message(
                            "tool", raw_result,
                            tool_call_id=tc.id,
                            tool_name=tc.name,
                        ))
                    continue
            except Exception as exc:
                logger.error("LLM call failed on iteration %d: %s", iteration, exc)
                final_content = f"[error] LLM call failed: {exc}"
                break

        else:
            logger.warning(
                "Session %s hit max_iterations=%d; returning partial reply",
                session_key, self._max_iterations,
            )
            if not final_content:
                final_content = "[max iterations reached — partial response]"

        # Save assistant turn
        if final_content:
            await self._sessions.append(session_key, "assistant", final_content)

        logger.info(
            "Session %s → %s:%s (%d chars)",
            session_key, msg.platform, msg.channel_id, len(final_content),
        )

        # Streaming already dispatched everything — caller does nothing
        if streamed:
            return None

        # Non-streaming — return outbound for middleware chain + dispatch
        return OutboundMessage(
            platform=msg.platform,
            channel_id=msg.channel_id,
            content=final_content,
            session_key=session_key,
            metadata=dict(msg.metadata),
        )
