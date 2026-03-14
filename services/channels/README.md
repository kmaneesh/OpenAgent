# Omnibus Channels Service (`services/channels`)

This service represents a major architectural shift from isolated per-platform binaries (`services/discord`, `services/slack`, `services/telegram`) to a **unified, multi-platform Omnibus daemon**.

## Why the Change?

1. **Standardization:** We ported upstream `zeroclaw`'s `Channel` trait (`src/traits.rs`), which abstracts platform-specific messaging mechanics into a unified interface (`send`, `listen`, `update_draft`, `finalize_draft`, `start_typing`).
2. **Resource Efficiency:** Running a single daemon conserves memory and CPU on low-power hardware (Raspberry Pi targets) compared to running ~6 dedicated platform services.
3. **Advanced Features Parity:** Implementing `zeroclaw` features like "draft" edits (streaming tool outputs directly into Discord/Telegram messages) and "typing" indicators is vastly simplified with a centralized trait dispatcher.

## URL-Based Routing

To handle message routing to specific platforms, workspaces, and threads seamlessly over MCP-lite, we use a custom **`ChannelAddress`** URI formatted like so:

- **Slack:** `slack://work_workspace_id/C123456?thread=123.456`
- **Discord:** `discord://guild_id/channel_id`
- **Telegram:** `telegram://bot_name/chat_id`

This ensures full type safety in Rust (via the `url` crate) and makes it easy for LLMs to target distinct threads or groups without complex JSON blobs.

## Retained Separations

- **WhatsApp:** Kept separate (`services/whatsapp/`) as it uniquely runs on Go (`whatsmeow`). It still communicates via standard MCP-lite.

## Supported Platforms (WIP)

- Discord
- Slack
- Telegram
- iMessage
- IRC
- Mattermost
- Signal

## Platform Setup & Credentials

Each platform requires specific environment variables to be set before it will boot successfully inside the omnibus daemon. 

### 1. Discord
Requires a bot token from the Discord Developer Portal.
- `DISCORD_BOT_TOKEN="MTEz..."`

### 2. Slack
Requires a bot token from the Slack API portal and optionally an app token if using socket mode.
- `SLACK_BOT_TOKEN="xoxb-..."`
- `SLACK_APP_TOKEN="xapp-..."` (If using socket mode)

### 3. Telegram
Requires a bot token from the BotFather.
- `TELEGRAM_BOT_TOKEN="123456789:ABCDef..."`

### 4. iMessage
Requires macOS. Works by interacting with the local Messages SQLite database and AppleScript. No specific auth token, but requires accessibility permissions and Full Disk Access for the terminal/daemon.

### 5. IRC
Requires standard IRC server details.
- `IRC_SERVER="irc.freenode.net"`
- `IRC_PORT="6667"`
- `IRC_NICKNAME="agentbot"`
- `IRC_CHANNEL="#openagent"`

### 6. Mattermost
Requires API keys generated from a Mattermost admin console.
- `MATTERMOST_URL="https://mattermost.example.com"`
- `MATTERMOST_TOKEN="xyz123..."`

### 7. Signal
Runs by interfacing with a local `signal-cli` REST API or DBus interface.
- `SIGNAL_CLI_URL="http://127.0.0.1:8080"` (if using the REST wrapper)
- `SIGNAL_NUMBER="+1234567890"`
