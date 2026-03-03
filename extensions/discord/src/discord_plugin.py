"""Discord extension entrypoint module."""

from __future__ import annotations

import asyncio
import contextlib
import os
from pathlib import Path
from time import time
from typing import Any

import aiohttp
import discord
from discord.ext import commands
from openagent.interfaces import BaseAsyncExtension

from discord_connector import DiscordConnector
from discord_schema import OpenAgentMessage


class OpenAgentDiscordBot(commands.Bot):
    def __init__(self, extension: "DiscordExtension"):
        intents = discord.Intents.all()
        super().__init__(command_prefix="!", intents=intents)
        self.extension = extension

    async def setup_hook(self) -> None:
        # Keep migrations non-blocking for future sync DB integrations.
        await asyncio.to_thread(self.extension.ensure_runtime_dirs)
        await self.add_cog(DiscordConnector(self.extension))
        if self.extension.sync_commands:
            await self.tree.sync()


class DiscordExtension(BaseAsyncExtension):
    def __init__(
        self,
        *,
        token: str | None = None,
        data_dir: str | Path = "data",
        account_id: str = "default",
        analyze_url: str | None = None,
    ):
        self.account_id = account_id
        self._token = token or os.getenv("OPENAGENT_DISCORD_TOKEN")
        self._analyze_url = analyze_url or os.getenv("OPENAGENT_ANALYZE_URL")
        self._data_dir = Path(data_dir)
        self.attachments_dir = self._data_dir / "discord" / account_id / "attachments"
        self.sync_commands = os.getenv("OPENAGENT_DISCORD_SYNC_COMMANDS", "false").lower() == "true"
        self._bot: OpenAgentDiscordBot | None = None
        self._bot_task: asyncio.Task[None] | None = None
        self._http: aiohttp.ClientSession | None = None
        self._messages: list[OpenAgentMessage] = []
        self._messages_lock = asyncio.Lock()
        self._status: dict[str, Any] = {
            "running": False,
            "connected": False,
            "messages_seen": 0,
            "last_message_at": None,
            "last_error": None,
        }

    @property
    def http_session(self) -> aiohttp.ClientSession:
        if self._http is None:
            raise RuntimeError("Discord HTTP session is not initialized.")
        return self._http

    async def initialize(self) -> None:
        self._status["running"] = True
        self._http = aiohttp.ClientSession()
        self._bot = OpenAgentDiscordBot(self)
        if not self._token:
            self._status["last_error"] = "OPENAGENT_DISCORD_TOKEN not configured"
            return
        self._bot_task = asyncio.create_task(self._run_bot(), name="openagent-discord-gateway")

    async def shutdown(self) -> None:
        self._status["running"] = False
        bot = self._bot
        if bot is not None:
            await bot.close()
        task = self._bot_task
        if task:
            task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await task
        if self._http is not None:
            await self._http.close()

    def get_status(self) -> dict[str, Any]:
        return dict(self._status)

    async def enqueue_message(self, message: OpenAgentMessage) -> None:
        async with self._messages_lock:
            self._messages.append(message)
            self._status["messages_seen"] += 1
            self._status["last_message_at"] = int(time() * 1000)

    async def pop_messages(self) -> list[OpenAgentMessage]:
        async with self._messages_lock:
            items = list(self._messages)
            self._messages.clear()
            return items

    def ensure_runtime_dirs(self) -> None:
        self.attachments_dir.mkdir(parents=True, exist_ok=True)

    def mark_connected(self) -> None:
        self._status["connected"] = True

    def mark_disconnected(self, reason: str | None = None) -> None:
        self._status["connected"] = False
        if reason:
            self._status["last_error"] = reason

    async def analyze_text(self, text: str) -> str:
        if not self._analyze_url:
            return f"Analysis unavailable (OPENAGENT_ANALYZE_URL not set). chars={len(text)}"
        session = self.http_session
        payload = {"text": text, "source": "discord"}
        async with session.post(self._analyze_url, json=payload) as response:
            response.raise_for_status()
            data = await response.json()
        return str(data.get("result") or data)

    async def _run_bot(self) -> None:
        assert self._bot is not None
        assert self._token is not None
        try:
            await self._bot.start(self._token)
        except asyncio.CancelledError:
            raise
        except Exception as exc:
            self._status["last_error"] = str(exc)
            self._status["connected"] = False
