"""openagent.providers — LLM provider factory."""

from __future__ import annotations

import logging

from openagent.observability import get_logger, log_event

from .agno import Agno, agno_available
from .anthropic import AnthropicProvider
from .base import Message, Provider
from .config import ProviderConfig, load_provider_config
from .openai import OpenAIProvider
from .openai_compat import OpenAICompatProvider

logger = get_logger(__name__)
_AGNO_AVAILABLE = agno_available()


def get_provider(
    cfg: ProviderConfig,
) -> OpenAICompatProvider | AnthropicProvider | OpenAIProvider | Agno:
    """Return a provider instance for the given config."""
    if _AGNO_AVAILABLE:
        log_event(
            logger,
            logging.INFO,
            "using agno-backed provider",
            component="providers",
            provider_kind=cfg.kind,
        )
        return Agno(cfg)

    log_event(
        logger,
        logging.WARNING,
        "agno unavailable, using legacy provider",
        component="providers",
        provider_kind=cfg.kind,
    )
    match cfg.kind:
        case "anthropic":
            return AnthropicProvider(cfg)
        case "openai":
            return OpenAIProvider(cfg)
        case _:  # "openai_compat" or unknown
            return OpenAICompatProvider(cfg)


__all__ = [
    "Message",
    "Provider",
    "ProviderConfig",
    "load_provider_config",
    "get_provider",
    "OpenAICompatProvider",
    "AnthropicProvider",
    "OpenAIProvider",
    "Agno",
]
