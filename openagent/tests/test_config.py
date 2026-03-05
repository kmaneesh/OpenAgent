"""Tests for the full OpenAgent config schema and loader."""

from __future__ import annotations

import os
import textwrap
from pathlib import Path

import pytest

from openagent.config import (
    AgentConfig,
    PlatformsConfig,
    DiscordPlatformConfig,
    OpenAgentConfig,
    SessionConfig,
    SlackPlatformConfig,
    TelegramPlatformConfig,
    ToolsConfig,
    build_service_env_extras,
    load_config,
)


# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------


def test_defaults():
    cfg = OpenAgentConfig()
    assert cfg.provider.kind == "openai_compat"
    assert len(cfg.agents) == 1
    assert cfg.default_agent.name == "default"
    assert cfg.session.summarise_after == 40
    assert cfg.platforms.discord is None
    assert cfg.tools.filesystem is None


def test_default_agent_fallback_empty_list():
    cfg = OpenAgentConfig(agents=[])
    agent = cfg.default_agent
    assert agent.name == "default"


# ---------------------------------------------------------------------------
# YAML loading
# ---------------------------------------------------------------------------


def test_load_config_minimal_yaml(tmp_path: Path):
    yaml_file = tmp_path / "openagent.yaml"
    yaml_file.write_text(textwrap.dedent("""\
        provider:
          kind: anthropic
          api_key: sk-ant-test
          model: claude-sonnet-4-6
    """))
    cfg = load_config(yaml_file)
    assert cfg.provider.kind == "anthropic"
    assert cfg.provider.api_key == "sk-ant-test"
    assert cfg.provider.model == "claude-sonnet-4-6"
    # Unspecified sections keep defaults
    assert cfg.session.summarise_after == 40
    assert cfg.platforms.discord is None


def test_load_config_with_all_sections(tmp_path: Path):
    yaml_file = tmp_path / "openagent.yaml"
    yaml_file.write_text(textwrap.dedent("""\
        provider:
          kind: openai_compat
          base_url: http://localhost:1234/v1
        agents:
          - name: mybot
            system_prompt: "You are a pirate."
            max_iterations: 10
        session:
          summarise_after: 20
          db_path: data/test.db
        platforms:
          discord:
            token: tok-discord
            guild_ids: [12345, 67890]
          telegram:
            app_id: 999
            app_hash: abc123
            bot_token: tok-tg
          slack:
            bot_token: xoxb-slack
            app_token: xapp-slack
        tools:
          filesystem:
            allowed_paths:
              - /tmp
              - ~/docs
    """))
    cfg = load_config(yaml_file)
    assert cfg.default_agent.name == "mybot"
    assert cfg.default_agent.system_prompt == "You are a pirate."
    assert cfg.default_agent.max_iterations == 10
    assert cfg.session.summarise_after == 20
    assert cfg.session.db_path == "data/test.db"

    assert cfg.platforms.discord is not None
    assert cfg.platforms.discord.token == "tok-discord"
    assert cfg.platforms.discord.guild_ids == [12345, 67890]

    assert cfg.platforms.telegram is not None
    assert cfg.platforms.telegram.app_id == 999
    assert cfg.platforms.telegram.bot_token == "tok-tg"

    assert cfg.platforms.slack is not None
    assert cfg.platforms.slack.bot_token == "xoxb-slack"

    assert cfg.tools.filesystem is not None
    assert "/tmp" in cfg.tools.filesystem.allowed_paths


def test_load_config_missing_file():
    cfg = load_config(Path("/nonexistent/openagent.yaml"))
    # Should return defaults without raising
    assert cfg.provider.kind == "openai_compat"


def test_load_config_empty_file(tmp_path: Path):
    yaml_file = tmp_path / "openagent.yaml"
    yaml_file.write_text("")
    cfg = load_config(yaml_file)
    assert cfg.provider.kind == "openai_compat"


# ---------------------------------------------------------------------------
# Environment variable overrides
# ---------------------------------------------------------------------------


def test_env_overrides_provider(tmp_path: Path, monkeypatch):
    yaml_file = tmp_path / "openagent.yaml"
    yaml_file.write_text("provider:\n  kind: openai_compat\n")
    monkeypatch.setenv("OPENAGENT_PROVIDER_KIND", "anthropic")
    monkeypatch.setenv("OPENAGENT_API_KEY", "sk-from-env")
    cfg = load_config(yaml_file)
    assert cfg.provider.kind == "anthropic"
    assert cfg.provider.api_key == "sk-from-env"


def test_env_overrides_discord_token(tmp_path: Path, monkeypatch):
    yaml_file = tmp_path / "openagent.yaml"
    yaml_file.write_text("platforms:\n  discord:\n    token: yaml-token\n")
    monkeypatch.setenv("DISCORD_TOKEN", "env-discord-token")
    cfg = load_config(yaml_file)
    assert cfg.platforms.discord is not None
    assert cfg.platforms.discord.token == "env-discord-token"


def test_env_overrides_telegram(monkeypatch):
    monkeypatch.setenv("TELEGRAM_APP_ID", "42")
    monkeypatch.setenv("TELEGRAM_APP_HASH", "hashfromenv")
    monkeypatch.setenv("TELEGRAM_BOT_TOKEN", "tg-bot-env")
    cfg = load_config(None)
    assert cfg.platforms.telegram is not None
    assert cfg.platforms.telegram.app_id == 42
    assert cfg.platforms.telegram.app_hash == "hashfromenv"
    assert cfg.platforms.telegram.bot_token == "tg-bot-env"


def test_env_overrides_slack(monkeypatch):
    monkeypatch.setenv("SLACK_BOT_TOKEN", "xoxb-env")
    monkeypatch.setenv("SLACK_APP_TOKEN", "xapp-env")
    cfg = load_config(None)
    assert cfg.platforms.slack is not None
    assert cfg.platforms.slack.bot_token == "xoxb-env"
    assert cfg.platforms.slack.app_token == "xapp-env"


def test_env_wins_over_yaml(tmp_path: Path, monkeypatch):
    yaml_file = tmp_path / "openagent.yaml"
    yaml_file.write_text("platforms:\n  discord:\n    token: yaml-token\n")
    monkeypatch.setenv("DISCORD_TOKEN", "env-wins")
    cfg = load_config(yaml_file)
    assert cfg.platforms.discord.token == "env-wins"


# ---------------------------------------------------------------------------
# build_service_env_extras
# ---------------------------------------------------------------------------


def test_build_service_env_extras_discord():
    cfg = OpenAgentConfig(
        platforms=PlatformsConfig(discord=DiscordPlatformConfig(token="tok"))
    )
    extras = build_service_env_extras(cfg)
    assert extras.get("discord") == {"DISCORD_BOT_TOKEN": "tok"}


def test_build_service_env_extras_telegram():
    cfg = OpenAgentConfig(
        platforms=PlatformsConfig(
            telegram=TelegramPlatformConfig(app_id=1, app_hash="h", bot_token="t")
        )
    )
    extras = build_service_env_extras(cfg)
    tg = extras.get("telegram", {})
    assert tg["TELEGRAM_APP_ID"] == "1"
    assert tg["TELEGRAM_APP_HASH"] == "h"
    assert tg["TELEGRAM_BOT_TOKEN"] == "t"


def test_build_service_env_extras_slack():
    cfg = OpenAgentConfig(
        platforms=PlatformsConfig(
            slack=SlackPlatformConfig(bot_token="xoxb", app_token="xapp")
        )
    )
    extras = build_service_env_extras(cfg)
    sl = extras.get("slack", {})
    assert sl["SLACK_BOT_TOKEN"] == "xoxb"
    assert sl["SLACK_APP_TOKEN"] == "xapp"


def test_build_service_env_extras_empty_discord_token():
    cfg = OpenAgentConfig(
        platforms=PlatformsConfig(discord=DiscordPlatformConfig(token=""))
    )
    extras = build_service_env_extras(cfg)
    assert "discord" not in extras  # empty token → not injected


def test_build_service_env_extras_no_platforms():
    cfg = OpenAgentConfig()
    extras = build_service_env_extras(cfg)
    assert extras == {}
