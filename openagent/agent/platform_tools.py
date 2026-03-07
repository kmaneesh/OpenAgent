"""Platform outbound tools — let the agent proactively send to any platform.

Exposes one native tool to the agent:

platform.send_message
    Send a message to a specific user/channel on Discord, Telegram,
    WhatsApp, or Slack without requiring an inbound message first.
    The agent decides when and where to send.

    params:
        platform  — "discord" | "telegram" | "whatsapp" | "slack"
        channel_id — platform-native identifier:
                       discord:  channel snowflake (string)
                       telegram: user numeric id (string)
                       whatsapp: JID, e.g. "15551234567@s.whatsapp.net"
                       slack:    channel id, e.g. "C01234ABCDE"
        text      — message content

The OutboundMessage is put on bus.outbound; PlatformManager routes it to
the correct PlatformAdapter which calls the Go service tool (e.g.
discord.send_message).  The current session_key is attached so the SSE
monitor can show the message in the web UI.
"""

from __future__ import annotations

import json
from typing import Any

from openagent.bus.bus import MessageBus
from openagent.bus.events import OutboundMessage

_SUPPORTED_PLATFORMS = {"discord", "telegram", "whatsapp", "slack"}


def make_platform_tools(
    bus: MessageBus,
) -> list[tuple[str, str, dict[str, Any], Any]]:
    """Return ``(name, description, params_schema, handler)`` tuples.

    Pass each tuple directly to ``ToolRegistry.register_native()``.
    """

    async def send_message(session_key: str, args: dict[str, Any]) -> str:
        platform = str(args.get("platform", "")).lower().strip()
        channel_id = str(args.get("channel_id", "")).strip()
        text = str(args.get("text", "")).strip()

        if platform not in _SUPPORTED_PLATFORMS:
            return json.dumps({
                "error": f"unsupported platform {platform!r}. Choose from: {sorted(_SUPPORTED_PLATFORMS)}",
            })
        if not channel_id:
            return json.dumps({"error": "channel_id is required"})
        if not text:
            return json.dumps({"error": "text is required"})

        msg = OutboundMessage(
            platform=platform,
            channel_id=channel_id,
            content=text,
            session_key=session_key,
        )
        await bus.dispatch(msg)
        return json.dumps({"ok": True, "platform": platform, "channel_id": channel_id})

    return [
        (
            "platform.send_message",
            (
                "Send a message to a user on Discord, Telegram, WhatsApp, or Slack. "
                "Use this to proactively reach out to a user without waiting for them "
                "to message first. "
                "platform: one of 'discord', 'telegram', 'whatsapp', 'slack'. "
                "channel_id: the platform-native identifier — "
                "Discord: channel snowflake; "
                "Telegram: numeric user id; "
                "WhatsApp: JID string (e.g. '15551234567@s.whatsapp.net'); "
                "Slack: channel id (e.g. 'C01234ABCDE'). "
                "text: the message to send."
            ),
            {
                "type": "object",
                "properties": {
                    "platform": {
                        "type": "string",
                        "enum": sorted(_SUPPORTED_PLATFORMS),
                        "description": "Target platform.",
                    },
                    "channel_id": {
                        "type": "string",
                        "description": "Platform-native channel or user identifier.",
                    },
                    "text": {
                        "type": "string",
                        "description": "Message text to send.",
                    },
                },
                "required": ["platform", "channel_id", "text"],
            },
            send_message,
        ),
    ]
