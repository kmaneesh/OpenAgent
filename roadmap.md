# OpenAgent Roadmap

Consolidated comparison of **Nanobot** and **Picoclaw** with **OpenAgent**. Assumes channels and providers stay at current count (Discord, WhatsApp, Telegram, MCP-lite; openai_compat, anthropic, openai).

**Status:** OpenAgent has implemented many core components (message bus, session manager, agent loop, ServiceManager, tool registry). The tables below describe conceptual gaps from the reference implementations; the **Summary** and **Build Order** sections reflect current status.

---

## Picoclaw vs OpenAgent — What's There, What's Not

*Note: Conceptual comparison. OpenAgent has since implemented message bus, session, agent loop, ServiceManager, tool registry — see Summary section.*

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

## Architecture Principle: Python Brain, Go/Rust Muscles

**Python** = thin control plane only: LLM calls, orchestration decisions, routing logic, config.
**Go** = heavy lifting today: channel I/O, session storage, tool execution, web search, file ops.
**Rust** = performance-critical services, migrated from Go incrementally (per-service, no big bang).
**Migration** = incremental — Python first → Go → Rust where load justifies it.
**No framework dependency** — Agno is inspiration only; we do not use it. Own thin httpx-based provider layer + custom ReAct loop.

### Why not Agno
Agno is used as inspiration (see `.claude/agno_cheatsheet.md`), not as a dependency. By owning the loop we control:
- Exact tool schema format (models differ in what they were fine-tuned on)
- Retry logic for malformed tool call responses
- Per-model XML-style fallback if OpenAI function calling is not supported
- Iteration limit (40) and tool output truncation (500 chars) without monkey-patching

### Go → Rust migration strategy (incremental, socket-transparent)
The MCP-lite protocol is language-agnostic. Swapping a Go binary for a Rust binary is transparent
to Python — update `binary` field in `service.json` and recompile. No Python changes required.

**Priority order for Rust** (highest Pi ROI first):
1. Session store — `rusqlite` + `tokio` outperforms Go's `mattn/go-sqlite3` on arm64
2. Web search / HTTP fetch — `reqwest` + `tokio` is extremely memory-efficient
3. Discord channel — `serenity` is mature, GC-free, fits Pi 5 RAM
4. Telegram channel — `teloxide` is idiomatic and well-maintained
5. Filesystem tool — trivial port, big RSS savings

**Keep in Go**: anything already working or with no compelling Rust library. Don't port for
the sake of it — port when a specific service is the measured bottleneck.

**Python stays forever**: the control plane is already asyncio-thin. LLM latency (100ms+)
dwarfs Python overhead. Nothing to gain from porting the orchestration layer.

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
| **Agent loop** | Python | `_process_message`, tool iteration | `Process`, fallback chain | ❌ | Custom ReAct loop (no framework). `InboundMessage` → `provider.chat(tools=[...])` → McpLiteClient → `OutboundMessage`. Max 40 iters, 500-char truncation. |
| **Session manager** | Python→Go→Rust | JSONL, `get_history` | `GetHistory`, `Summary` | ❌ | `SessionBackend` Protocol frozen day 1. SQLite now (`aiosqlite`), Go or Rust service later — one constructor line to swap. |
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
| Hello, filesystem, shell services | Go | `services/hello/`, `filesystem/`, `shell/` |
| Channel service stubs | Go | `services/discord/`, `telegram/`, `whatsapp/`, `slack/` |
| MCP-lite Python client | Python | `openagent/channels/mcplite.py` |
| Provider layer (httpx) | Python | `openagent/providers/` — Anthropic, OpenAI, OpenAI-compat |
| ServiceManager + watchdog | Python | `openagent/services/manager.py` |
| Message bus + event types | Python | `openagent/bus/` — InboundMessage, OutboundMessage, SenderInfo |
| Session manager | Python | `openagent/session/` — SessionBackend protocol, SQLite impl |
| Agent loop + ToolRegistry | Python | `openagent/agent/loop.py`, `openagent/agent/tools.py` |
| Extension manager | Python | `openagent/manager.py` |
| Heartbeat/health | Python | `openagent/heartbeat/` |
| Observability | Python + Go | `openagent/observability/`, `services/sdk-go/mcplite/observe.go` |
| Web UI | Python | `app/` |

### ❌ Missing (by layer)

| Layer | Missing | Notes |
|-------|---------|-------|
| **Provider** | `chat(tools=[...])` returning `tool_calls` | Extend existing httpx providers if not done |
| **Tools** | Filesystem, shell, web Go services | MCP-lite Go services (filesystem, shell exist) |
| **Channels** | Go channel services fully fleshed | Stubs exist; Discord/Telegram/Slack/WhatsApp in progress |
| **Routing** | Agent registry, route resolver, bindings | Tier 4 — optional |
| **Config** | agents, bindings, session, tools sections | Extend `openagent.yaml` |
| **Chat path** | End-to-end wiring | Agent loop ↔ channel events ↔ web UI |

---

## Recommended Build Order (Go/Rust-heavy, Incremental)

1. ~~**ServiceManager**~~ (Python) ✅ DONE
2. ~~**Message bus + Event types**~~ (Python) ✅ DONE
3. ~~**Provider layer**~~ (Python) ✅ DONE — httpx-based
4. ~~**Session manager**~~ (Python) ✅ DONE — `SessionBackend` Protocol, SQLite impl
5. ~~**Agent loop**~~ (Python) ✅ DONE — custom ReAct, tool dispatch via MCP-lite
6. ~~**Tool services**~~ (Go) ✅ DONE — filesystem, shell
7. **Chat path wiring** — agent loop ↔ channel events ↔ web UI end-to-end
8. **Discord/Telegram/Slack/WhatsApp** (Go) — flesh out channel services
9. **Config schema** (Python) — extend `openagent.yaml`: agents, session, tools, bindings
10. **Optional** — multi-agent, bindings, memory consolidation, cron, state, media
11. **Go → Rust** (per-service) — when each is the measured bottleneck

---

## Reference Implementations

| Reference | Language | Key Files |
|-----------|----------|-----------|
| **Nanobot** | Python | `inspire/nanobot/nanobot/agent/loop.py`, `bus/events.py`, `bus/queue.py`, `session/manager.py` |
| **Picoclaw** | Go | `inspire/picoclaw/pkg/agent/loop.go`, `pkg/bus/bus.go`, `pkg/session/manager.go`, `pkg/routing/route.go` |
