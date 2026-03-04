from __future__ import annotations

from openagent.providers import get_provider
from openagent.providers.config import ProviderConfig


def test_get_provider_falls_back_when_agno_unavailable(monkeypatch):
    import openagent.providers as providers

    monkeypatch.setattr(providers, "_AGNO_AVAILABLE", False)

    cfg = ProviderConfig(kind="openai_compat", base_url="http://localhost:1234/v1")
    provider = get_provider(cfg)

    from openagent.providers.openai_compat import OpenAICompatProvider

    assert isinstance(provider, OpenAICompatProvider)


def test_get_provider_prefers_agno_when_available(monkeypatch):
    import openagent.providers as providers

    class _FakeAgno:
        def __init__(self, cfg):
            self.cfg = cfg

    monkeypatch.setattr(providers, "_AGNO_AVAILABLE", True)
    monkeypatch.setattr(providers, "Agno", _FakeAgno)

    cfg = ProviderConfig(kind="openai")
    provider = get_provider(cfg)

    assert isinstance(provider, _FakeAgno)
    assert provider.cfg.kind == "openai"
