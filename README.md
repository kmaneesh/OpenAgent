# OpenAgent

A deterministic, extension-first agent platform on a **progressive Rust migration path** for low-power/offline deployments (Raspberry Pi and beyond).

The control plane is the Rust binary (`openagent`). Python is retired as a control plane and exists only as an optional web UI container. All new code is Rust. WhatsApp remains in Go (whatsmeow); every other service is Rust.

---

## Architecture Overview

```
                       ┌────────────────────────────────────────────────┐
  Platform channels    │         openagent  (Rust binary, :8080)        │
  ───────────────      │                                                 │
  Telegram             │  Channels (in-process)                         │
  Discord      ──────► │    ↓ message.received events                   │
  Slack                │  Dispatch loop                                  │
  WhatsApp (Go) ──────►│    ↓ Guard check → Agent.step (in-process)     │
  CLI                  │        ↓ ReAct loop  →  Tool Router             │
  IRC / MQTT / ...     │               ↓                                 │
                       │        ActionCatalog (tool + skill lookup)      │
                       │               ↓                                 │
                       │        TCP →  Rust services (browser, memory…) │
                       │               ↓ built-in (cron.*, skill.read)  │
                       │    ↓ Response                                    │
                       │  Channel.send → platform                        │
                       └────────────────────────────────────────────────┘
```

**Wire protocol:** MCP-lite — newline-delimited JSON frames over TCP. Axum (:8080) is external-facing only. Services speak MCP-lite; Axum speaks JSON to external callers. The protocol never changes regardless of which Rust code is added above it.

---

## Module Reference

### `openagent` — Rust Control Plane Binary

Entry point: [openagent/src/main.rs](openagent/src/main.rs)

Startup sequence:
1. Load `config/openagent.toml` + env-var overrides
2. Init OTEL (traces, logs, metrics → `logs/` as JSONL)
3. Open guard DB (`data/guard.db`)
4. Discover service manifests (`services/*/service.json`) and connect to running daemons
5. Init in-process channels (Telegram, Discord, Slack, WhatsApp, CLI, …)
6. Build `ActionCatalog` (tools from service.json + skills from `skills/`) + extend with built-in cron entries
7. Build `ToolRouter` (tool-name → TCP address map, cron in-process)
8. Build `AgentContext` (catalog + router + telemetry)
9. Spawn dispatch loop, cron scheduler (if enabled), Axum server
10. Wait for SIGTERM / Ctrl-C / console `quit`

---

### `agent` — ReAct Loop and Tool Orchestration

Source: [openagent/src/agent/](openagent/src/agent/)

The agent owns the full reasoning loop. It does **not** call tools directly — it goes through `ToolRouter` which routes to TCP services or built-in handlers.

**Key submodules:**

| File | Role |
|---|---|
| [handlers.rs](openagent/src/agent/handlers.rs) | `handle_step` — entry point for one full ReAct turn; builds context, calls LLM, dispatches tool calls, writes diary |
| [core.rs](openagent/src/agent/core.rs) | `AgentCore` — iterates the ReAct loop up to `max_iterations` (default 40) |
| [llm.rs](openagent/src/agent/llm.rs) | LLM provider dispatch (`build_llm_provider`, provider fallback chain) |
| [prompt.rs](openagent/src/agent/prompt.rs) | MiniJinja template rendering — system prompt, tool schemas, skill context |
| [memory_adapter.rs](openagent/src/agent/memory_adapter.rs) | `HybridMemoryAdapter` — sliding STM window (40 messages) + LTM via `memory.search` |
| [diary.rs](openagent/src/agent/diary.rs) | Fire-and-forget diary write after each final answer (markdown file + LanceDB row) |
| [classifier.rs](openagent/src/agent/classifier.rs) | Lightweight message classifier (research intent, routing hints) |
| [metrics.rs](openagent/src/agent/metrics.rs) | `AgentTelemetry` — per-step latency, token counts, error rates |

**Pinned tools** (always in every LLM turn — no discovery needed):
- `memory.search` — semantic recall from long-term memory
- `sandbox.execute` — run Python/Node in an isolated VM
- `sandbox.shell` — run shell commands in an isolated VM
- `web.search` — SearXNG web search
- `web.fetch` — fetch a URL as clean Markdown
- `agent.discover` — built-in: search the action catalog for tools and skills

Everything else is discovered via `agent.discover` on demand.

#### Action Catalog

Source: [openagent/src/agent/action/catalog.rs](openagent/src/agent/action/catalog.rs)

Loaded at startup from two sources:
- **Service tools** — `services/*/service.json` → each `tools[]` entry becomes an `ActionEntry` with a TCP address
- **Skills** — `skills/*/SKILL.md` → frontmatter parsed into `ActionEntry` with `kind = SkillGuidance`

Built-in tools (cron) are injected via `extend_with_builtins()` — these have an empty `address` and are never dialed over TCP.

`tool_address_map()` produces the `tool-name → TCP address` map consumed by `ToolRouter`.

#### Action Search

Source: [openagent/src/agent/action/search.rs](openagent/src/agent/action/search.rs)

Keyword ranking (BM25-style) over `search_blob` strings. Returns top-k entries per step. Used by `agent.discover` to surface matching tools and skills to the LLM.

#### Tool Router

Source: [openagent/src/agent/tool_router.rs](openagent/src/agent/tool_router.rs)

Dispatches tool calls:

```
tool call arrives
    ├── "skill.read"  → handle_skill_read() in-process (no TCP)
    ├── "cron.*"      → cron::tools::handle() in-process (no TCP)
    └── everything else → TCP connect → MCP-lite frame → read response
```

Timeout per call: 30 seconds. TCP_NODELAY is set on every connection.

---

### `cron` — Scheduled Jobs

Source: [openagent/src/cron/](openagent/src/cron/)

Persistent scheduled jobs backed by SQLite (`data/openagent.db`). Enable in config:

```toml
[cron]
enabled   = true
poll_secs = 30
```

**Schedule types:**

| Kind | Example | Behaviour |
|---|---|---|
| `cron` | `{"kind":"cron","expr":"0 9 * * 1-5"}` | Recurring — 5-field crontab |
| `at` | `{"kind":"at","at":"2026-12-31T09:00:00Z"}` | One-shot UTC timestamp; auto-deleted after run |
| `every` | `{"kind":"every","every_ms":3600000}` | Fixed interval |

**Job types:**
- `shell` — runs `sh -c <command>` with a 120s timeout; stdout+stderr captured
- `agent` — injects a synthetic `message.received` event into the dispatch loop; the agent handles it like a real user message; `channel = cron://<job_id>` gives each job its own isolated session history

**Tools (discoverable via `agent.discover`):**

| Tool | Description |
|---|---|
| `cron.add` | Create a shell or agent job |
| `cron.list` | List all jobs with next_run times |
| `cron.get` | Fetch one job by id |
| `cron.remove` | Delete a job |
| `cron.update` | Patch schedule / command / prompt / name / enabled |
| `cron.run` | Trigger a shell job immediately (outside schedule) |
| `cron.runs` | Show execution history for a job |

**Storage — two tables in `data/openagent.db`:**
- `cron_jobs` — one row per job (id, schedule JSON, job_type, command, prompt, next_run, …)
- `cron_runs` — execution history (started_at, finished_at, status, output, duration_ms)

Submodules: [types.rs](openagent/src/cron/types.rs) · [schedule.rs](openagent/src/cron/schedule.rs) · [store.rs](openagent/src/cron/store.rs) · [scheduler.rs](openagent/src/cron/scheduler.rs) · [tools.rs](openagent/src/cron/tools.rs)

---

### `channels` — In-Process Platform Connectors

Source: [openagent/src/channels/](openagent/src/channels/)

All platform integrations run in-process inside `openagent` (no separate channels daemon). Each channel listens for inbound messages and pushes `message.received` events onto the shared broadcast bus. The dispatch loop picks them up.

**Supported channels:**

| Channel | Notes |
|---|---|
| Telegram | Bot API — polling or webhook |
| Discord | Gateway (slash commands + DMs) |
| Slack | Socket Mode (app-level token) |
| WhatsApp | Cloud API (webhook inbound; Go daemon still active for personal-number inbound) |
| WhatsApp Web | Browser-based via whatsapp_web |
| CLI | Interactive terminal session |
| iMessage | AppleScript bridge (macOS) |
| IRC | Plain TCP |
| Mattermost | WebSocket |
| Signal | signal-cli bridge |
| MQTT | IoT pub/sub |
| Reddit | Polling |
| Twitter/X | API v2 |

Configure via `[channels.<name>]` in `config/openagent.toml`. Channels not configured or with `enabled = false` are skipped.

**Outbound TTS** — `channels/tts.rs` wraps outbound text through the TTS service before sending if `[middleware.tts] enabled = true`.

---

### `guard` — Contact Whitelist

Source: [openagent/src/guard/](openagent/src/guard/)

Inline contact whitelist backed by `data/guard.db` (SQLite, WAL mode). No external service.

Access policy per inbound message:
- Guard disabled → always allowed
- Platform `web` or `whatsapp` → bypass (platform handles auth)
- Sender in table as `"allowed"` → allowed
- Sender in table as `"blocked"` → blocked, visit recorded
- Sender not in table → blocked, visit recorded (operator must call `allow()`)

`scrub.rs` strips PII from log lines before they reach OTEL.

---

### `dispatch` — Event Router

Source: [openagent/src/dispatch.rs](openagent/src/dispatch.rs)

Single loop that bridges the event broadcast bus to the agent and back:

```
event_rx.recv()
  → extract content / channel URI / sender
  → derive session_id = "{channel}:{sender}"
  → guard.check (drop if blocked)
  → channel.typing_start (best-effort, no await)
  → handle_step (in-process, semaphore-limited to 4 concurrent)
  → channel.send(response)
```

A semaphore caps concurrent `handle_step` calls at 4 to prevent thrashing on Pi-class hardware.

---

### `server` — Axum Control Plane API

Source: [openagent/src/server/](openagent/src/server/)

Axum HTTP server on `:8080`. External-facing only — platform connectors and the web UI call it. **Not used for inter-service communication** (that is MCP-lite/TCP).

**Tower middleware stack** (outermost → innermost):
```
ConcurrencyLimitLayer (max 50)
  → HandleErrorLayer
  → TimeoutLayer
  → TraceLayer
  → CorsLayer
  → GuardMiddleware
  → SttMiddleware
  → AgentMiddleware
  → TtsMiddleware
  → Router
```

**Routes:**

| Method | Path | Description |
|---|---|---|
| GET | `/health` | Liveness check + tool count |
| GET | `/tools` | All registered tools from ActionCatalog |
| POST | `/step` | Run one in-process agent reasoning step |
| POST | `/tool/:name` | Raw tool call (internal / debug) |

STT middleware transcribes audio payloads before they reach the agent. TTS middleware converts text responses to audio if the client requests it.

---

### `service` — External Rust/Go Daemon Manager

Source: [openagent/src/service/](openagent/src/service/)

Manages connections to external service daemons (binaries started by `services.sh` or systemd). `openagent` does **not** spawn or restart services — the supervisor (systemd / services.sh) owns that.

**Responsibilities:**
1. Read `service.json` manifests from `services/*/service.json`
2. TCP-connect to each service's `address` from the manifest
3. Send `tools.list` → register tools into the ActionCatalog
4. Health-check loop (ping/pong every 5s); reconnect automatically
5. Subscribe to `event` frames → push to broadcast bus

**MCP-lite wire protocol (TCP, newline-delimited JSON):**

Agent → Service:
```json
{"id":"…","type":"tools.list"}
{"id":"…","type":"tool.call","tool":"browser.open","params":{…}}
{"id":"…","type":"ping"}
```

Service → Agent:
```json
{"id":"…","type":"tools.list.ok","tools":[…]}
{"id":"…","type":"tool.result","result":"…","error":null}
{"id":"…","type":"pong","status":"ready"}
```

---

### `observability` — OTEL Telemetry

Source: [openagent/src/observability/](openagent/src/observability/)

Three-pillar observability written to `logs/` (or `$OPENAGENT_LOGS_DIR`):
- **Traces** — OTEL spans per agent step, tool call, channel send
- **Logs** — structured JSON lines (`openagent.log`)
- **Metrics** — counters and histograms (`metrics.jsonl`)

`setup_otel(service_name, logs_dir)` initialises the OTEL pipeline. All Rust services call it via `sdk_rust::setup_otel`.

In production (systemd), set `Environment=OPENAGENT_LOGS_DIR=/var/log/openagent` in the unit file. In dev, logs go to `logs/` relative to the project root.

---

### Skills — Progressive Knowledge Disclosure

Source: [skills/](skills/)

Skills are domain knowledge files (not tools). They teach the LLM _what_ to do and _how_ to think when using one or more tools together.

**Three-level disclosure:**

1. **Semantic search** — every `agent.step`, the top-k skill summaries appear as one-line entries in the action catalog result. The LLM sees the description only.
2. **Full body on demand** — LLM calls `skill.read(name="…")` → receives the full `SKILL.md` body + table of available references.
3. **Reference on demand** — LLM calls `skill.read(name="…", reference="auth")` → receives that reference file.

`skill.read` is a **pinned built-in capability** — always available, no discovery needed. It is handled in-process by `ToolRouter` without a TCP hop.

**Skill file layout:**
```
skills/<name>/
  SKILL.md          ← required; frontmatter (name, description, hint, allowed-tools, enforce) + body
  references/       ← deep-dive docs (optional)
  templates/        ← ready-to-run scripts (optional)
  drafts/           ← agent-generated candidates pending human review (gitignored)
```

**SKILL.md frontmatter:**
```markdown
---
name: agent-browser
description: Browser automation for AI agents.
hint: Call skill.read(name="agent-browser") for commands, patterns, and auth workflows.
allowed-tools: browser.open, browser.navigate, browser.snapshot
enforce: false
enabled: true
---
```

---

## External Services (Rust, started separately)

Services are long-lived daemon processes started by `services.sh` or systemd. `openagent` connects to them over TCP and does not restart them.

| Service | Port | Language | Description |
|---|---|---|---|
| **memory** | 9000 | Rust | Vector memory — LanceDB + FastEmbed; diary writes, semantic search |
| **browser** | 9001 | Rust | Headless browser automation via agent-browser CLI |
| **sandbox** | 9002 | Rust | VM-isolated code/shell execution (microsandbox) |
| **stt** | 9003 | Rust | Speech-to-text (faster-whisper int8) |
| **tts** | 9004 | Rust | Text-to-speech (Kokoro ONNX) |
| **validator** | 9005 | Rust | Tool output validator |
| **whatsapp** | 9010 | Go | WhatsApp inbound via whatsmeow |

Each service has a `service.json` manifest declaring its name, address, and tool schemas. `openagent` reads only the manifest — it never depends on service internals.

**Starting services (dev):**
```bash
./services.sh start          # all services
./services.sh start browser  # one service
./services.sh status
```

**Building:**
```bash
make local    # current host (fast dev loop)
make all      # cross-compile for all targets (Pi, etc.)
```

**Sandbox service** requires a running microsandbox server:
```bash
msb server start --dev   # dev mode — no API key
```

**TTS service** requires kokoro model files:
```bash
make download-tts-models   # downloads ~400 MB from HuggingFace
```

---

## Configuration

`config/openagent.toml` — all `${VAR}` tokens resolved from environment at load time.

```toml
[provider]
kind     = "openai_compat"
base_url = "http://localhost:1234/v1"
api_key  = ""
model    = "qwen/qwen3.5-9b"
timeout  = 300.0

[guard]
enabled = true
db_path = "data/guard.db"

[middleware.stt]
enabled = false

[middleware.tts]
enabled = false
voice   = "af_sarah"

[cron]
enabled   = false     # set true to activate scheduled jobs
poll_secs = 30

[services]
disabled = []         # service names to skip even if binary exists

[channels.telegram]
enabled = false
token   = "${TELEGRAM_BOT_TOKEN}"

[channels.discord]
enabled = true
token   = "${DISCORD_BOT_TOKEN}"

[channels.slack]
enabled   = true
bot_token = "${SLACK_BOT_TOKEN}"
app_token = "${SLACK_APP_TOKEN}"
```

Environment variable overrides (prefix `OPENAGENT_`):

| Var | Effect |
|---|---|
| `OPENAGENT_ROOT` | Project root (default: current directory) |
| `OPENAGENT_LOGS_DIR` | Log output directory (default: `logs/`) |
| `OPENAGENT_LLM_BASE_URL` | Provider base URL |
| `OPENAGENT_MODEL` | Model name |
| `OPENAGENT_API_KEY` | Provider API key |
| `RUST_LOG` | Log filter (e.g. `debug`, `openagent=trace`) |

---

## Storage Layout

| Path | Contents |
|---|---|
| `data/openagent.db` | SQLite — sessions, cron_jobs, cron_runs |
| `data/guard.db` | SQLite — contact whitelist |
| `data/memory/` | LanceDB vector store (long-term memory, diary index) |
| `data/artifacts/` | Media, downloads, binary outputs |
| `logs/` | OTEL traces, structured logs, metrics JSONL |
| `skills/` | Skill knowledge files (human-maintained) |
| `config/openagent.toml` | Runtime config |

---

## Port Allocation

| Port | Service |
|---|---|
| 8080 | openagent Axum control plane |
| 9000 | memory |
| 9001 | browser |
| 9002 | sandbox |
| 9003 | stt |
| 9004 | tts |
| 9005 | validator |
| 9010 | whatsapp |

New services: assign the next available port above 9010. Never reuse a port.

---

## Requirements

- **Rust** — `cargo` (stable) + `cross` for cross-compilation
- **Go 1.21+** — WhatsApp service only
- A local or remote LLM via an OpenAI-compatible endpoint (e.g. [LM Studio](https://lmstudio.ai), [Ollama](https://ollama.com))
- **microsandbox** (`msb`) — required if sandbox service is enabled
- **agent-browser** (`npm install -g agent-browser && agent-browser install`) — required if browser service is enabled

---

## Building

```bash
# Build all Rust services for current host
make local

# Cross-compile all targets (Pi arm64, Linux amd64)
make all

# Build one service
make browser
make sandbox

# Download TTS model files (kokoro, ~400 MB)
make download-tts-models

# WhatsApp (Go)
make whatsapp
```

Compiled binaries land in `bin/<name>-<os>-<arch>` (gitignored).

---

## Running

```bash
# Start external services first
./services.sh start

# Start the control plane
openagent
# or
cargo run --release -p openagent
```

Interactive console commands (stdin when TTY is attached):

| Command | Effect |
|---|---|
| `status` | Service health summary |
| `tools` | List registered tools |
| `logs` | Tail the log file |
| `quit` / `shutdown` | Graceful shutdown |

---

## Testing

```bash
# Rust unit tests (openagent)
cargo test -p openagent

# Rust tests (a specific service)
cd services/browser && cargo test

# Go tests (WhatsApp only)
cd services/whatsapp && go test ./...

# Python web UI tests
cd app && pytest
```

---

## Adding a New Rust Service

1. Create `services/<name>/` with `Cargo.toml` and `src/main.rs`
2. Use `sdk-rust` (`McpLiteServer`, `setup_otel`) for MCP-lite boilerplate
3. Write `service.json` with `address`, `tools[]`, and `binary` paths
4. Assign the next free port (above 9010)
5. Add build targets to `Makefile`
6. `ServiceManager` discovers it automatically via `services/*/service.json`

**Rust service rules:**
- `tokio` features: `["rt-multi-thread", "macros", "sync", "net"]` — never `"full"`
- `mimalloc` as `#[global_allocator]`
- Release profile: `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = true`
- No `unwrap()` / `expect()` outside tests — return `Result<_, E>`
- No `unbounded_channel` — always set a capacity
- TCP address via env var `OPENAGENT_TCP_ADDRESS`; call `serve_auto(default_addr)`

---

## Deployment (Raspberry Pi)

```bash
# Cross-compile on dev machine
make all

# Copy binaries to Pi
rsync -av bin/ pi@raspberrypi.local:~/openagent/bin/

# On Pi — systemd manages services and openagent
sudo systemctl start openagent
sudo systemctl start openagent-memory openagent-browser ...
```

Set `OPENAGENT_LOGS_DIR=/var/log/openagent` in the systemd unit's `Environment=` line.

---

## License

See [LICENSE](LICENSE).
