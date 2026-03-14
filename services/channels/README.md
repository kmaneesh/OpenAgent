# Omnibus Channels Service (`services/channels`)

Unified multi-platform messaging daemon for OpenAgent.  A single long-lived
Rust process handles all platform connectors via a shared `Channel` trait.

## Architecture

```
vendor/zeroclaw/          ← git subtree — canonical Channel trait + all connectors
services/channels/src/
  main.rs                 ← boot sequence + MCP-lite server
  config.rs               ← TOML load + ${VAR} env interpolation + dotenvy
  adapter.rs              ← ZeroClawChannel<T> — OTEL + metrics wrapper
  registry.rs             ← ChannelRegistry — platform instantiation + listeners
  address.rs              ← ChannelAddress — URL routing type
config/channels.toml      ← platform credentials + enable flags
```

### Why zeroclaw?

zeroclaw ships a production-hardened `Channel` trait and implementations for
20+ platforms.  Rather than maintaining our own platform code, we pull zeroclaw
as a **git subtree** (`vendor/zeroclaw/`) and depend on it as a Rust path dep.

Ingesting a new zeroclaw connector = `git subtree pull` + one `config/channels.toml`
section + one `ChannelRegistry::build` match arm.  Zero trait synchronization work.

### `ZeroClawChannel<T>`

Generic adapter that wraps any `T: zeroclaw::channels::Channel`:

- Records OTEL spans for `send`, `listen`, `update_draft`, etc.
- Emits per-call metrics to `sdk-rust::MetricsWriter` (daily JSONL)
- Delegates all platform logic to the inner `T`

New zeroclaw connectors are automatically instrumented.

## Configuration (`config/channels.toml`)

Credentials use `${VAR}` syntax resolved from environment variables.
A `.env` file in the project root is loaded automatically at startup.

Enable platforms by setting `enabled = true` and providing credentials:

```toml
[telegram]
enabled = true
token = "${TELEGRAM_BOT_TOKEN}"

[discord]
enabled = true
token = "${DISCORD_BOT_TOKEN}"

[slack]
enabled = true
bot_token = "${SLACK_BOT_TOKEN}"
app_token = "${SLACK_APP_TOKEN}"   # required for Socket Mode
```

See [config/channels.toml](../../config/channels.toml) for the full template.

## MCP-lite Tool Surface

| Tool | Description |
|---|---|
| `channel.send` | Send a message to a `ChannelAddress` URI |
| `channel.update_draft` | Update a streaming draft message |
| `channel.finalize_draft` | Finalize a draft with complete response |
| `channel.cancel_draft` | Cancel and remove a draft |
| `channel.react` | Add an emoji reaction to a message |
| `channel.typing_start` | Send typing indicator |
| `channel.typing_stop` | Stop typing indicator |
| `channel.list` | List enabled platform names |

## MCP-lite Events (pushed to Python control plane)

| Event | When |
|---|---|
| `message.received` | Inbound message from any platform |
| `channel.status` | On startup — lists enabled channels |

## URL-Based Routing (`ChannelAddress`)

```
telegram://bot_name/chat_id
discord://guild_id/channel_id
slack://workspace_id/C123456?thread=123.456
```

## Supported Platforms

All platforms listed below are provided by `vendor/zeroclaw/`.  Enable any
by adding a section to `config/channels.toml`.

- Telegram, Discord, Slack
- IRC, Mattermost, Signal
- iMessage (macOS only — requires Full Disk Access)
- WhatsApp, Email, DingTalk, Lark/Feishu, Matrix, Nostr, WeCom, QQ, and more

> **WhatsApp** remains in `services/whatsapp/` (Go/whatsmeow) — not part of
> this omnibus due to its unique Go dependency.

## Updating zeroclaw

```bash
git subtree pull --prefix=vendor/zeroclaw \
  https://github.com/zeroclaw-labs/zeroclaw.git master --squash
```

Then add a registry entry in `registry.rs` for any new connector and a
section in `config/channels.toml`.  The adapter and trait code need no changes.
