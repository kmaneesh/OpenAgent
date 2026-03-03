"""Discord Cog connector and slash command handlers."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

import aiohttp
import discord
from discord import app_commands
from discord.ext import commands

from discord_bridge import discord_message_to_openagent

if TYPE_CHECKING:
    from discord_plugin import DiscordExtension


class DiscordConnector(commands.Cog):
    def __init__(self, extension: "DiscordExtension"):
        self.extension = extension

    @commands.Cog.listener()
    async def on_ready(self) -> None:
        self.extension.mark_connected()

    @commands.Cog.listener()
    async def on_disconnect(self) -> None:
        self.extension.mark_disconnected("gateway-disconnected")

    @commands.Cog.listener()
    async def on_message(self, message: discord.Message) -> None:
        if message.author and getattr(message.author, "bot", False):
            return
        bridged = discord_message_to_openagent(message, account_id=self.extension.account_id)
        attachment_paths, attachment_types = await self._download_attachments(message)
        if attachment_paths:
            bridged.media_paths = attachment_paths
        if attachment_types:
            bridged.media_types = attachment_types
        await self.extension.enqueue_message(bridged)

    @app_commands.command(name="status", description="Show OpenAgent Discord connector status.")
    async def status(self, interaction: discord.Interaction) -> None:
        status = self.extension.get_status()
        content = (
            f"running={status.get('running')} "
            f"connected={status.get('connected')} "
            f"messages={status.get('messages_seen')}"
        )
        await interaction.response.send_message(content, ephemeral=True)

    @app_commands.command(name="analyze", description="Run analysis via configured async API.")
    @app_commands.describe(text="Text content to analyze")
    async def analyze(self, interaction: discord.Interaction, text: str) -> None:
        await interaction.response.defer(ephemeral=True, thinking=True)
        result = await self.extension.analyze_text(text)
        await interaction.followup.send(result, ephemeral=True)

    async def _download_attachments(self, message: discord.Message) -> tuple[list[str], list[str]]:
        attachments = list(getattr(message, "attachments", []) or [])
        if not attachments:
            return [], []

        saved_paths: list[str] = []
        media_types: list[str] = []
        for attachment in attachments:
            url = getattr(attachment, "url", None)
            if not url:
                continue
            content_type = getattr(attachment, "content_type", None) or "application/octet-stream"
            file_name = getattr(attachment, "filename", "attachment.bin")
            body = await self._fetch_attachment(url)
            path = await self._store_attachment(file_name, body)
            saved_paths.append(path)
            media_types.append(content_type)
        return saved_paths, media_types

    async def _fetch_attachment(self, url: str) -> bytes:
        session = self.extension.http_session
        async with session.get(url, timeout=aiohttp.ClientTimeout(total=30)) as response:
            response.raise_for_status()
            return await response.read()

    async def _store_attachment(self, file_name: str, body: bytes) -> str:
        path = self.extension.attachments_dir / file_name
        await asyncio.to_thread(path.parent.mkdir, parents=True, exist_ok=True)
        await asyncio.to_thread(path.write_bytes, body)
        return str(path)
