"""Middleware interfaces bridging inbound messages to the ReAct core."""

from typing import Protocol, Callable, Awaitable

from openagent.bus.events import InboundMessage

NextCall = Callable[[InboundMessage], Awaitable[None]]

class AgentMiddleware(Protocol):
    """Protocol for intercepting inbound messages before or after the LLM executes."""
    
    async def __call__(self, msg: InboundMessage, next_call: NextCall) -> None:
        ...
