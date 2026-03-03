from __future__ import annotations

import asyncio

import pytest

from discord_plugin import DiscordExtension


@pytest.mark.asyncio
async def test_extension_initializes_without_token_sets_status(tmp_path):
    ext = DiscordExtension(token=None, data_dir=tmp_path)
    await ext.initialize()
    status = ext.get_status()
    assert status["running"] is True
    assert "OPENAGENT_DISCORD_TOKEN" in (status["last_error"] or "")
    await ext.shutdown()


@pytest.mark.asyncio
async def test_extension_uses_background_task_with_token(monkeypatch, tmp_path):
    started = {"value": False}

    async def fake_run_bot(self):
        started["value"] = True
        await asyncio.sleep(0)

    monkeypatch.setattr(DiscordExtension, "_run_bot", fake_run_bot)

    ext = DiscordExtension(token="dummy-token", data_dir=tmp_path)
    await ext.initialize()
    await asyncio.sleep(0)
    assert started["value"] is True
    await ext.shutdown()
