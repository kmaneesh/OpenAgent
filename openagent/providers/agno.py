"""Agno-backed provider adapter for OpenAgent provider interface."""

from __future__ import annotations

import inspect
from collections.abc import AsyncIterator
from typing import Any

from .base import Message
from .config import ProviderConfig


def agno_available() -> bool:
    try:
        import agno  # noqa: F401
    except Exception:
        return False
    return True


class Agno:
    """Adapter that routes provider calls through Agno models/agent."""

    def __init__(self, cfg: ProviderConfig) -> None:
        self._cfg = cfg
        self._agent = self._build_agent(cfg)

    def _build_agent(self, cfg: ProviderConfig):
        try:
            from agno.agent import Agent
            from agno.models.anthropic import Claude
            from agno.models.openai import OpenAIChat
            from agno.models.openai_like import OpenAILike
        except Exception as exc:  # pragma: no cover - environment dependent
            raise RuntimeError(
                "Agno provider requested but 'agno' package is not installed. "
                "Install with: uv add agno"
            ) from exc

        model_id = cfg.model or _default_model(cfg.kind)
        if cfg.kind == "anthropic":
            model = Claude(
                id=model_id,
                api_key=cfg.api_key or None,
                max_tokens=cfg.max_tokens,
            )
        elif cfg.kind == "openai":
            model = OpenAIChat(
                id=model_id,
                api_key=cfg.api_key or None,
                base_url=cfg.base_url or None,
                max_tokens=cfg.max_tokens,
            )
        else:
            model = OpenAILike(
                id=model_id,
                base_url=cfg.base_url,
                api_key=cfg.api_key or "",
                max_tokens=cfg.max_tokens,
            )

        return Agent(model=model, markdown=False)

    async def stream(self, messages: list[Message], **kwargs) -> AsyncIterator[str]:
        prompt = _messages_to_prompt(messages)
        run_result = self._agent.run(prompt, stream=True, **kwargs)

        if hasattr(run_result, "__aiter__"):
            async for chunk in run_result:  # type: ignore[misc]
                text = _chunk_to_text(chunk)
                if text:
                    yield text
            return

        if hasattr(run_result, "__iter__"):
            for chunk in run_result:  # type: ignore[misc]
                text = _chunk_to_text(chunk)
                if text:
                    yield text
            return

        # Fallback: single completion path
        if inspect.isawaitable(run_result):
            run_result = await run_result
        text = _chunk_to_text(run_result)
        if text:
            yield text

    async def complete(self, messages: list[Message], **kwargs) -> str:
        prompt = _messages_to_prompt(messages)
        result = self._agent.arun(prompt, **kwargs)
        if inspect.isawaitable(result):
            result = await result
        return _chunk_to_text(result)



def _default_model(kind: str) -> str:
    if kind == "anthropic":
        return "claude-sonnet-4-6"
    if kind == "openai":
        return "gpt-4o"
    return "default"



def _messages_to_prompt(messages: list[Message]) -> str:
    # Preserve role order deterministically for provider-agnostic usage.
    lines: list[str] = []
    for msg in messages:
        lines.append(f"{msg.role}: {msg.content}")
    return "\n".join(lines).strip()



def _chunk_to_text(chunk: Any) -> str:
    if chunk is None:
        return ""
    if isinstance(chunk, str):
        return chunk
    content = getattr(chunk, "content", None)
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        parts = [str(p) for p in content if p is not None]
        return "".join(parts)
    text = getattr(chunk, "text", None)
    if isinstance(text, str):
        return text
    return str(chunk)
