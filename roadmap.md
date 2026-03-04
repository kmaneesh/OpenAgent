# OpenAgent Roadmap

Consolidated comparison of **Nanobot** and **Picoclaw** with **OpenAgent**. Assumes channels and providers stay at current count (Discord, WhatsApp, Telegram, MCP-lite; openai_compat, anthropic, openai).

---

## Picoclaw vs OpenAgent — What's There, What's Not

### 1. Message Bus & Event Types

| Aspect | Picoclaw | OpenAgent |
|--------|----------|-----------|
| **Message bus** | ✅ `MessageBus` with `inbound`, `outbound`, `outboundMedia` channels | ❌ None |
| **InboundMessage** | ✅ `Channel`, `SenderID`, `Sender` (SenderInfo), `ChatID`, `Content`, `Media`, `Peer`, `SessionKey`, `Metadata` | ❌ None |
| **OutboundMessage** | ✅ `Channel`, `ChatID`, `Content` | ❌ None |
| **OutboundMediaMessage** | ✅ Separate channel for media (`Parts` with `Type`, `Ref`, `Caption`) | ❌ None |
| **Bus lifecycle** | ✅ `Close()`, drain on shutdown | ❌ N/A |
| **Context-aware publish** | ✅ `ctx` for cancellation | ❌ N/A |

**OpenAgent gap:** No message bus or event types.

---

### 2. Routing & Multi-Agent

| Aspect | Picoclaw | OpenAgent |
|--------|----------|-----------|
| **Agent registry** | ✅ `AgentRegistry` with multiple agents | ❌ None |
| **Route resolver** | ✅ 7-level cascade: peer → parent_peer → guild → team → account → channel_wildcard → default | ❌ None |
| **Bindings** | ✅ `AgentBinding` with `Match` (channel, account_id, peer, guild_id, team_id) | ❌ None |
| **Session key** | ✅ `BuildAgentPeerSessionKey`, `BuildAgentMainSessionKey`, `DMScope` | ❌ None |
| **Subagent spawn** | ✅ `CanSpawnSubagent(parent, target)` | ❌ None |

**OpenAgent gap:** No routing or multi-agent support.

---

### 3. Session Management

| Aspect | Picoclaw | OpenAgent |
|--------|----------|-----------|
| **Session struct** | ✅ `Key`, `Messages`, `Summary`, `Created`, `Updated` | ❌ None |
| **SessionManager** | ✅ `GetOrCreate`, `AddFullMessage`, `GetHistory`, `SetSummary`, `TruncateHistory`, `Save`, `SetHistory` | ❌ None |
| **Storage** | ✅ JSON per session in `workspace/sessions/` | ❌ None |
| **Persistence** | ✅ Atomic write (temp + rename) | ❌ N/A |

**OpenAgent gap:** No session or history.

---

### 4. Agent Instance & Loop

| Aspect | Picoclaw | OpenAgent |
|--------|----------|-----------|
| **AgentInstance** | ✅ Per-agent workspace, model, fallbacks, sessions, context, tools | ❌ None |
| **Agent loop** | ✅ `Process()`, tool iteration, fallback chain | ❌ None |
| **Context builder** | ✅ Per-agent, workspace-based | ❌ None |
| **Tool registry** | ✅ Per-agent, shared tools (web, message, spawn, MCP) | ❌ None |
| **Fallback chain** | ✅ Provider fallback with cooldown | ❌ None |
| **Summarization** | ✅ `SummarizeMessageThreshold`, `SummarizeTokenPercent` | ❌ None |

**OpenAgent gap:** No agent instance or loop.

---

### 5. Channel Manager

| Aspect | Picoclaw | OpenAgent |
|--------|----------|-----------|
| **Channel interface** | ✅ `Name`, `Start`, `Stop`, `Send`, `IsRunning`, `IsAllowed`, `IsAllowedSender`, `ReasoningChannelID` | ❌ No shared interface |
| **Channel manager** | ✅ Config-driven init, outbound dispatch, per-channel workers | ❌ None |
| **Rate limiting** | ✅ Per-channel `rate.Limiter` | ❌ None |
| **Message splitting** | ✅ `MaxMessageLength`, split by runes | ❌ None |
| **Placeholder editing** | ✅ `RecordPlaceholder`, `RecordTypingStop`, `RecordReactionUndo`, `preSend` | ❌ None |
| **Media queue** | ✅ Separate `outboundMedia` channel | ❌ None |
| **Group trigger** | ✅ `GroupTriggerConfig` per channel | ❌ None |

**OpenAgent gap:** No channel manager or shared channel contract.

---

### 6. Identity & Allow-List

| Aspect | Picoclaw | OpenAgent |
|--------|----------|-----------|
| **SenderInfo** | ✅ `Platform`, `PlatformID`, `CanonicalID`, `Username`, `DisplayName` | ❌ None |
| **Canonical ID** | ✅ `platform:id` format | ❌ None |
| **MatchAllowed** | ✅ Supports `123456`, `@alice`, `123456|alice`, `telegram:123456` | ❌ None |

**OpenAgent gap:** No identity model or allow-list logic.

---

### 7. State & Media

| Aspect | Picoclaw | OpenAgent |
|--------|----------|-----------|
| **State manager** | ✅ `LastChannel`, `LastChatID`, `Timestamp` | ❌ None |
| **Media store** | ✅ `media://` refs, `MediaPart` | ❌ None |
| **Media scope** | ✅ `MediaScope` in `InboundMessage` | ❌ None |

**OpenAgent gap:** No state or media handling.

---

### 8. Config

| Aspect | Picoclaw | OpenAgent |
|--------|----------|-----------|
| **Agents** | ✅ `agents.list`, `agents.defaults`, `AgentConfig` (ID, Model, Workspace, Skills, Subagents) | ❌ Only `provider` |
| **Bindings** | ✅ `bindings` with `Match` + `AgentID` | ❌ None |
| **Session** | ✅ `DMScope`, `IdentityLinks` | ❌ None |
| **Tools** | ✅ `AllowReadPaths`, `AllowWritePaths`, Web (Brave, Tavily, DuckDuckGo, Perplexity) | ❌ None |
| **Channels** | ✅ Per-channel config (allow_from, proxy, etc.) | ❌ None |

**OpenAgent gap:** No agents, bindings, session, or tools config.

---

### 9. Tools

| Aspect | Picoclaw | OpenAgent |
|--------|----------|-----------|
| **Tool registry** | ✅ Per-agent registry | ❌ None |
| **Filesystem** | ✅ read_file, write_file, edit_file, append_file, list_dir | ❌ None |
| **Shell** | ✅ exec with config | ❌ None |
| **Web** | ✅ search (Brave, Tavily, DuckDuckGo, Perplexity), fetch | ❌ None |
| **Message** | ✅ send to channel via bus | ❌ None |
| **Spawn** | ✅ subagent tool | ❌ None |
| **MCP** | ✅ MCP tool wrapper | ❌ None |
| **Cron** | ✅ cron tool | ❌ None |
| **Skills** | ✅ skills_install, skills_search | ❌ None |

**OpenAgent gap:** No tools or tool registry.

---

### 10. MCP & Cron

| Aspect | Picoclaw | OpenAgent |
|--------|----------|-----------|
| **MCP manager** | ✅ Connect, env file, tool registration | ❌ None |
| **Cron service** | ✅ `gronx` for cron expressions | ❌ None |
| **Cron tool** | ✅ add, list, enable, disable, remove | ❌ None |

**OpenAgent gap:** No MCP or cron.

---

## Architecture Principle: Python Brain, Go Muscles

**Python** = thin control plane only: LLM calls, orchestration decisions, routing logic, config.
**Go** = all heavy lifting: channel I/O, session storage, tool execution, web search, file ops.
**Migration** = incremental — start in Python, promote to Go service when load justifies it.
**Agno** = selective borrow only (agent loop patterns, OpenAILike wiring). No wholesale adoption.

---

## Consolidated: Nanobot + Picoclaw → OpenAgent

Assuming channels (Discord, WhatsApp, Telegram, MCP-lite) and providers (openai_compat, anthropic, openai) stay fixed.

---

### Tier 1 — Core Pipeline (Must Have)

| Component | Lang | Nanobot | Picoclaw | OpenAgent | Action |
|-----------|------|---------|----------|-----------|--------|
| **ServiceManager** | Python | N/A | N/A | ❌ | **Next**: spawn/watch Go binaries, manage sockets, health-check, restart. Wires `McpLiteClient` per service. |
| **Message bus** | Python | `asyncio.Queue` | `chan` + `Close()` | ❌ | `InboundMessage`, `OutboundMessage` dataclasses + asyncio queues. Python owns routing only. |
| **Event types** | Python | `session_key`, `metadata`, `media` | `SenderInfo`, `Peer`, `SessionKey` | ❌ | `session_key`, `SenderInfo`, `metadata`. Keep thin — Go services own their own events. |
| **Agent loop** | Python | `_process_message`, tool iteration | `Process`, fallback chain | ❌ | Thin loop: `InboundMessage` → LLM → tool dispatch via McpLiteClient → `OutboundMessage`. Borrow Nanobot shape selectively. |
| **Session manager** | Python→Go | JSONL, `get_history` | `GetHistory`, `Summary` | ❌ | Start Python/SQLite. Designed as Go migration target (Go service behind MCP-lite socket). |
| **Tool registry** | Python (thin) | `ToolRegistry`, `execute` | Per-agent registry | ❌ | Python names/dispatches tools. Go services execute them. No Python tool implementations for heavy ops. |

---

### Tier 2 — Channel Migration (Go Stubs Already Exist)

| Component | Lang | Status | Action |
|-----------|------|--------|--------|
| **Discord → Go** | Go | Stub in `services/discord/` | Flesh out Go service. Deprecate Python discord extension. First proof of channel migration. |
| **Telegram → Go** | Go | Stub in `services/telegram/` | After Discord is proven. |
| **Slack → Go** | Go | Stub in `services/slack/` | After Telegram. |
| **WhatsApp → Go** | Go | Stub in `services/whatsapp/` | Last — most complex (whatsmeow). |
| **Channel manager** | Python (thin) | ❌ | Config-driven init via ServiceManager. Outbound dispatch routes `OutboundMessage` to correct Go channel service via MCP-lite. |

---

### Tier 3 — Tool Services in Go

| Component | Lang | Action |
|-----------|------|--------|
| **Filesystem tool** | Go service | `read_file`, `write_file`, `list_dir`, `edit_file` — Go service behind MCP-lite |
| **Shell tool** | Go service | `exec` with config-driven allow-list |
| **Web search tool** | Go service | DuckDuckGo / Brave / Tavily — Go HTTP client |
| **Session store** | Go service | Promote Python SQLite session manager to Go service |
| **Cron service** | Go service | `gronx`-based, Picoclaw-style |

---

### Tier 4 — Routing & Multi-Agent (Nice to Have)

| Component | Lang | Action |
|-----------|------|--------|
| **Agent registry** | Python | Optional multi-agent (Picoclaw-style) |
| **Route resolver** | Python | 7-level cascade: peer → guild → team → account → channel → default |
| **Config schema** | Python | Extend `openagent.yaml` with `agents`, `session`, `tools`, `bindings` |
| **Slash commands** | Python | `/new`, `/stop`, `/help` in agent loop |
| **Identity / allow-list** | Python | `SenderInfo`, `allow_from` matching |
| **Rate limiting** | Go (per-channel service) | Each Go channel service owns its own rate limiter |
| **Memory consolidation** | Go service | Long-term — Go service with SQLite + LanceDB |

---

## Summary: What OpenAgent Has vs Needs

### ✅ Present

| Component | Lang | Where |
|-----------|------|-------|
| MCP-lite Go SDK | Go | `services/sdk-go/mcplite/` |
| Hello service (reference) | Go | `services/hello/` |
| Channel service stubs | Go | `services/discord/`, `telegram/`, `whatsapp/`, `slack/` |
| MCP-lite Python client | Python | `openagent/channels/mcplite.py` |
| Provider layer | Python | `openagent/providers/` |
| Extension manager | Python | `openagent/manager.py` |
| Heartbeat/health | Python | `openagent/heartbeat/` |
| Observability | Python + Go | `openagent/observability/`, `services/sdk-go/mcplite/observe.go` |
| Web UI | Python | `app/` |

### ❌ Missing (by layer)

| Layer | Missing |
|-------|---------|
| **Glue** | ServiceManager (spawns + watches Go binaries) |
| **Bus** | `InboundMessage`, `OutboundMessage`, asyncio queues |
| **Agent** | Agent loop, tool dispatch |
| **Session** | Session manager, history, `session_key` |
| **Tools** | Filesystem, shell, web Go services |
| **Channels** | Go channel services fleshed out (stubs only) |
| **Routing** | Agent registry, route resolver, bindings |
| **Config** | agents, bindings, session, tools sections |

---

## Recommended Build Order (Go-Heavy, Incremental)

1. **ServiceManager** (Python) — spawn/watch Go binaries via subprocess, manage Unix sockets, restart on crash. This unblocks everything.
2. **Message bus** (Python) — `InboundMessage`, `OutboundMessage` dataclasses + asyncio queues
3. **Agent loop** (Python) — thin: `InboundMessage` → LLM → McpLiteClient tool calls → `OutboundMessage`
4. **Session manager** (Python/SQLite) — designed as Go migration target from day one
5. **Discord service** (Go) — flesh out `services/discord/`, prove Python channel extension → Go migration
6. **Tool services** (Go) — filesystem, shell, web search as Go services behind MCP-lite
7. **Config schema** (Python) — extend `openagent.yaml`: agents, session, tools, bindings
8. **Remaining channels** (Go) — Telegram, Slack, WhatsApp in order of complexity
9. **Session → Go** (Go) — promote SQLite session to Go service when Python version is stable
10. **Optional** — multi-agent, bindings, memory consolidation, cron, state, media

---

## Reference Implementations

| Reference | Language | Key Files |
|-----------|----------|-----------|
| **Nanobot** | Python | `inspire/nanobot/nanobot/agent/loop.py`, `bus/events.py`, `bus/queue.py`, `session/manager.py` |
| **Picoclaw** | Go | `inspire/picoclaw/pkg/agent/loop.go`, `pkg/bus/bus.go`, `pkg/session/manager.go`, `pkg/routing/route.go` |
