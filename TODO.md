# OpenAgent — Project TODO

Top-level cross-service backlog. Service-specific phased roadmaps live in their own
`TODO.md` files (e.g. `services/agent/TODO.md`).

---

## Inspire Gap Analysis (2026-04-13)

Reference: `inspire/` — 6 projects: agno, ironclaw, nanobot, openclaw, picoclaw, zeroclaw.
Scope: functionality only. No new channels; existing channels (WhatsApp, Slack, Telegram, Discord) are complete.

### What OpenAgent Has Today (confirmed by code audit)

- **ReAct loop** — max 100 iterations, session-isolated, diary after each turn
- **Hybrid memory** — STM (40-message window) + LTM (LanceDB: memory/diary/knowledge tables, hybrid BM25+ANN)
- **Tool discovery** — `agent.discover` BM25 catalog; ~25 service tools + 4 cron + 4 skills
- **Pinned tools** — memory.search, sandbox.execute, sandbox.shell, web.search, web.fetch, agent.discover
- **Sandbox** — Python/Node code execution + shell via microsandbox OCI isolation
- **STT/TTS** — Whisper transcription + Kokoro voice synthesis; dispatch modality mirroring
- **Browser automation** — agent-browser skill with 15 tools (click, fill, snapshot, screenshot, etc.)
- **Cron/scheduling** — cron/at/every job types; shell and agent jobs; persistent SQLite
- **Skills system** — SKILL.md with frontmatter; semantic skill injection per step; `skill.read` built-in
- **Provider routing** — fast/strong classifier; fallback chain configured
- **Tower middleware** — GuardLayer, SttLayer, AgentLayer, TtsLayer, RateLimit, ConcurrencyLimit
- **OTEL** — traces/metrics/logs/baggage, JSONL per service per day
- **Guard / scrub** — credential redaction + injection detection on all inbound messages
- **Validator** — JSON repair for malformed LLM output
- **Doctor/diagnostics** — `/api/diagnose` structured health report

---

### 🔴 High Priority Gaps

#### 1. Diary Compaction (memory service)

**Gap:** `diary` table rows are stubs — zero vectors, no keywords, no real embeddings.
Back-fill is the only path to making diary search useful. All reference projects (zeroclaw,
picoclaw, nanobot) have working memory consolidation.

**What to build:**
- `services/memory` compaction job: scan diary rows where `vector` is zero, embed `content`,
  run keyword extraction, update row in-place
- Trigger: `cron.add` with `every_ms: 3600000` (hourly) injecting `memory.prune` + compaction
- Single-pass: process at most N rows per run (default 50) to stay within Pi memory
- CLI flag `--compact-once` for manual trigger

**Owner:** `services/memory/src/`

---

#### 2. STM Summarization on Eviction

**Gap:** When STM window (40 messages) is full, oldest messages are dumped to a local
Markdown file and discarded. No summarization — context is permanently lost. All reference
projects (nanobot, picoclaw, zeroclaw, agno) summarize evicted history into LTM.

**What to build:**
- On eviction: batch-summarize the N oldest messages via the agent's LLM provider
- Write summary as a `memory.index` entry (store=memory) instead of raw dump
- Keep dump as fallback if LLM call fails
- Configure batch size and eviction threshold in `openagent.toml`

**Owner:** `openagent/src/agent/` (session handling)

---

#### 3. Thinking Mode / Reasoning Depth Control

**Gap:** ZeroClaw, OpenClaw, and Nanobot all support user-controllable reasoning depth
(`/think:low|medium|high|max` or `think` keyword prefix). OpenAgent always uses a single
temperature/max_tokens. Deep-think queries cost the same as trivial ones.

**What to build:**
- Parse `/think` directive prefix in `dispatch.rs` and agent handlers (strip before sending to LLM)
- Map level → temperature + max_tokens adjustment in provider config
- Default: standard. `/think` or `/think:high` → lower temperature, 2x max_tokens
- Config: `[agent.thinking]` block with per-level overrides in `openagent.toml`
- Works with any provider (just parameter adjustment, no o1/reasoning model required)

**Owner:** `openagent/src/dispatch.rs`, `openagent/src/agent/`

---

#### 4. SOP System (Standard Operating Procedures)

**Gap:** `openagent/src/sop/` directory exists but is completely empty. ZeroClaw and
OpenClaw both have SOP systems that give the agent structured multi-step plans for known
task types (e.g., "research a topic", "debug code", "write a report").

**What to build:**
- `SopStore`: load SOPs from `skills/*/sop.md` or a dedicated `sops/` directory
- SOP format: YAML frontmatter (name, triggers: [keyword list]) + Markdown body steps
- Injection: if user input matches trigger keywords, inject the SOP as an additional
  context section in the system prompt ("When doing X, follow these steps:")
- SOPs are read-only guidance — not enforced, just strongly suggestive
- Seed with 3-4 SOPs: `research`, `debug`, `write`, `summarize`

**Owner:** `openagent/src/sop/`, `openagent/src/agent/`

---

### 🟡 Medium Priority Gaps

#### 5. Reflection Step (Agent Self-Review)

**Gap:** After convergence, no agent checks whether the answer actually satisfies the
original request. Agno, IronClaw, and ZeroClaw all have a response quality gate that
can trigger retry with a higher-tier model.

**What to build:**
- Post-convergence `reflection_step`: build a 1-shot prompt ("Did this response fully
  answer: <original_request>? Reply YES or NO with reason.")
- If NO: retry with strong provider (if different from current), bump max_iterations
- Maximum 1 retry to avoid spirals. Log reflection outcome to diary
- Gated by `[agent.reflection] enabled = false` in config (off by default)

**Owner:** `openagent/src/agent/handlers.rs`

---

#### 6. User Memory / Learned Facts Store

**Gap:** All reference projects (agno, ironclaw, zeroclaw) maintain a per-user facts
layer — preferences, name, habits — separate from session history and semantic memory.
OpenAgent has no concept of a persistent user profile.

**What to build:**
- `user_memory` LanceDB table (or a JSON sidecar `data/users/<id>/profile.json`)
- Write path: LLM calls `memory.index(store=user, ...)` with user-specific facts
- Read path: auto-injected as a "What I know about you:" block into system prompt
- Update: overwrite by id or append; `memory.delete(store=user, id=<id>)` to forget
- Only injected when `session_id` maps to a known user (non-cron sessions)

**Owner:** `services/memory/src/`, `openagent/src/agent/`

---

#### 7. Complexity Classifier → Smart Tool Pinning

**Gap:** The existing fast/strong classifier routes to a different model. But tool pinning
is static (always the same 6 tools). ZeroClaw and IronClaw pin different tool subsets
based on message complexity and context.

**What to build:**
- Extend classifier output: `complexity: low | medium | high`
- `low` (≤8 words, no tools needed): pin only memory.search (no sandbox, no browser)
- `medium`: current pinned set
- `high` (long, research signals): pin web.search + web.fetch + memory.search + sandbox + agent.discover
- Reduces token overhead for simple queries on Pi

**Owner:** `openagent/src/agent/classify.rs` (new file)

---

#### 8. Subagent Spawning

**Gap:** All 6 reference projects support spawning nested agents for subtasks. OpenAgent
has a single agent turn per step. Complex tasks (research + write + validate) would
benefit from isolated sub-runs that each have their own session and tool access.

**What to build:**
- `agent.spawn(prompt, [agent_name], [max_iterations])` built-in tool
- Spawns a fresh `handle_step` in a new session_id (`<parent_session>:sub:<n>`)
- Returns sub-agent's `response_text` to the parent LLM as a tool result
- Nesting limit: 2 (prevent infinite loops)
- Subagent gets a stripped system prompt (no meta-skills injected, just task focus)

**Owner:** `openagent/src/agent/`, built-in tool in `ToolRouter`

---

#### 9. `/metrics` Prometheus Endpoint

**Gap:** OTEL exports JSONL only. ZeroClaw and IronClaw expose Prometheus scrape
endpoints. On Pi with Grafana, scraping is far more usable than parsing JSONL.

**What to build:**
- Add `opentelemetry-prometheus` exporter alongside the file exporter
- Expose `GET /metrics` on Axum server (same `:8080` port, prometheus text format)
- Key metrics: `agent_step_duration_ms`, `tool_call_count`, `memory_search_duration_ms`,
  `lancedb_table_size`, `sandbox_execute_count`, `tts_synthesize_count`

**Owner:** `openagent/src/observability/`, `openagent/src/server/routes.rs`

---

### 🟢 Lower Priority

#### 10. Structured Tool Result Validation

**Gap:** The `validator.repair_json` tool exists but is only usable by the LLM manually.
All tool results pass through as raw strings with no schema validation. If a tool returns
malformed JSON, the LLM silently gets garbage.

**What to build:**
- In `ToolRouter`, after each tool call: if the service declares `result_schema` in
  service.json, validate the result with `validator.repair_json` automatically
- Add optional `result_schema` field to service.json tool entries
- Silent repair on validation failure (log WARN, return repaired or empty object)

**Owner:** `openagent/src/agent/tool_router.rs`

---

#### 11. Skill `enforce` Flag Enforcement

**Gap:** `enforce: true` in skill frontmatter is parsed but never acted on. The intent is
to restrict the LLM to only `allowed_tools` when the matched skill is active.

**What to build:**
- In `ToolRouter`: when active skill has `enforce=true`, reject tool calls not in
  `allowed_tools` (return a structured error: "Tool X not permitted in this mode")
- Inject a clear constraint into the system prompt: "You may only use: [tool list]"
- Tested by the agent-browser skill (already has `allowed_tools` specified)

**Owner:** `openagent/src/agent/tool_router.rs`, skill injection in `openagent/src/agent/`

---

#### 12. Knowledge Ingestion Tool (`knowledge.add`)

**Gap:** The `knowledge` LanceDB table exists but there is no dedicated write tool for it.
The only path is `memory.index(store=memory, ...)` which does not write to the knowledge
table. Curated reference material (API docs, personal notes) cannot be added.

**What to build:**
- `memory.index` handler: accept `store=knowledge` (already supported in Rust, just not
  exposed in service.json schema — quick fix: add "knowledge" to the enum)
- OR: dedicated `knowledge.add` + `knowledge.search` tools with tagging/category support
- Start simple: just expose `knowledge` as a valid store in `memory.index` service.json

**Owner:** `services/memory/service.json` (immediate 1-line fix) → `services/memory/src/handlers.rs`

---

## Cross-Cutting Items

| Item | Owner | Status |
|---|---|---|
| Agent phased plan (Phases 0–5) | `openagent/src/agent/` | ✅ Complete |
| Tower middleware (full stack) | `openagent/src/server/` | ✅ Complete |
| Provider fallback chain | `openagent/src/` | ✅ Complete |
| Rate limiting middleware | `openagent/src/server/` | ✅ Complete |
| Credential scrubbing + injection detection | `openagent/src/guard/` | ✅ Complete |
| Web UI diary page | `app/routes/diary.py` | ✅ Complete |
| STT/TTS middleware + dispatch modality mirror | `openagent/src/server/`, `dispatch.rs` | ✅ Complete |
| WhatsApp TTS voice replies ("speak" trigger) | `openagent/src/dispatch.rs` | ✅ Complete |
| memory service.json store names (ltm→memory) | `services/memory/service.json` | ✅ Complete |
| Diary compaction | `services/memory/src/` | ❌ Not started |
| STM summarization on eviction | `openagent/src/agent/` | ❌ Not started |
| Thinking mode (`/think` directive) | `openagent/src/dispatch.rs` | ❌ Not started |
| SOP system | `openagent/src/sop/` | ❌ Not started (dir exists, empty) |
| Reflection step (post-convergence) | `openagent/src/agent/handlers.rs` | ❌ Not started |
| User memory / learned facts | `services/memory/`, `openagent/src/agent/` | ❌ Not started |
| Complexity classifier → smart tool pinning | `openagent/src/agent/` | ❌ Not started |
| Subagent spawning | `openagent/src/agent/` | ❌ Not started |
| Prometheus `/metrics` endpoint | `openagent/src/` | ❌ Not started |
| Tool result schema validation | `openagent/src/agent/tool_router.rs` | ❌ Not started |
| Skill `enforce` flag enforcement | `openagent/src/agent/` | ❌ Not started |
| `knowledge` store exposed in memory.index | `services/memory/service.json` | ❌ Not started (trivial) |
| Web UI research page | `app/routes/research.py` | 🔄 Building |
| Channels omnibus | `services/channels/` | WIP |


Reference implementation: `inspire/zeroclaw/`. Full comparison recorded in conversation
history. Actionable gaps below, ordered by impact.

### 🔴 High Priority

#### 1. Credential Scrubbing on Inbound Messages ✅ DONE

**Implementation:** `openagent/src/scrub.rs` — `scrub::process(input, context)`.
Hand-rolled byte scanner, no new deps.

Applied in two places (Guard layer):
- `openagent/src/middleware.rs` — scrubs `user_input` in buffered HTTP request body before STT/Agent
- `openagent/src/dispatch.rs` — scrubs channel message `content` before `agent.step`

Patterns: `token`, `api_key`, `password`, `secret`, `bearer`, `credential`, `auth` + variants.
Redaction: keeps first 4 chars, replaces remainder with `[REDACTED]`. Values < 8 chars not redacted.
Logs `WARN guard.scrub.credential_detected` with context label; secret value never logged.

---

#### 2. Channels Omnibus Service (`services/channels/`)

**Gap / Status:** Individual per-platform binaries (discord, slack, telegram) are
operationally wasteful on Pi targets. ZeroClaw uses a single unified daemon with a
`Channel` trait. Work already started — see `services/channels/README.md`.

**What's already planned:**
- Unified Rust daemon, single MCP-lite socket (`data/sockets/channels.sock`)
- `Channel` trait ported from zeroclaw (`src/traits.rs` — already exists)
- URL-based routing: `telegram://bot/chat_id`, `discord://guild/channel`, `slack://workspace/channel?thread=ts`
- Draft streaming support (`update_draft`, `finalize_draft`, `cancel_draft`)
- Typing indicators (`start_typing`, `stop_typing`)
- Reactions + threaded replies

**Platforms in scope (WIP):** Discord, Slack, Telegram, iMessage, IRC, Mattermost, Signal

**What this unlocks:**
- Draft streaming: model streams text → channel edits message in place (Telegram/Discord)
  instead of sending a full reply at end. Significant UX improvement.
- Typing indicators: "Bot is typing…" while Agent step runs.
- Single binary to cross-compile and deploy on Pi vs. 4–6 individual binaries.

**Dependency:** Agent needs a streaming path through `agent.step` to feed partial
responses to the channel service for draft updates. Currently `agent.step` is
request-response only.

---

### 🟡 Medium Priority

#### 3. Draft Streaming Path in Agent

**Gap:** ZeroClaw streams partial LLM output to channels mid-response. Our `agent.step`
is fully synchronous request-response — the caller gets the final answer only.

**What to build:**
- Add a streaming variant to the MCP-lite protocol: `agent.step_stream` returning
  delta frames as the LLM generates them
- The channels omnibus service subscribes to these deltas and calls `update_draft` on
  the platform
- Final frame carries the full `ReActOutput` including tool call summary

**Scope:** Requires protocol extension + new Tower service variant. Phase 3 of channels
integration.

---

#### 4. Action Budget / Rate Limiting

**Gap:** ZeroClaw has `SecurityPolicy` with `max_actions_per_hour` and sliding window
rate limiting per user. We have a Python whitelist (allow/deny by user ID only) and no
action budgeting.

**What to build:**
- `RateLimitLayer` as a Tower middleware in Agent (alongside the planned `WhitelistLayer`)
- Configurable budget: `max_steps_per_hour` per session_id from `openagent.yaml`
- Returns a structured error when budget exceeded (not a panic)
- Fits naturally in the Tower Phase 2 middleware stack

---

#### 5. Provider Fallback Chain

**Gap:** ZeroClaw has `ReliableProvider` wrapping any provider with a fallback chain.
If the primary model times out or errors, the next provider in the list is tried.

**What to build:**
- `FallbackProvider` wrapper in `autoagents-llm` or in `agent/src/llm.rs`
- Config: `providers.fallback_chain = ["primary", "secondary"]` in `openagent.yaml`
- Try each provider in order; return first success; log fallback activations

---

### 🟢 Lower Priority / Nice to Have

#### 6. Prometheus `/metrics` Endpoint

**Gap:** ZeroClaw exposes a Prometheus scrape endpoint. We write JSONL only (OTLP file
export). On Pi with Grafana, scraping is more convenient than parsing JSONL.

**What to build:** Expose `/metrics` from the Agent gateway (Phase 4 Axum endpoint).
Implement via `opentelemetry-prometheus` exporter alongside the file exporter.

---

#### 7. Prompt Injection Detection ✅ DONE

**Implementation:** Bundled with credential scrubbing in `openagent/src/scrub.rs`.
`detect_injection()` runs as the second pass inside `scrub::process()`.

Phrases detected: "ignore previous instructions", "you are now", "disregard", "jailbreak",
"pretend you are", "roleplay as", "dan mode", and others.
Logs `WARN guard.scrub.injection_detected` with the matched phrase; text is NOT modified
(detection only — preserves model's ability to reason about the flagged content if needed).

---

#### 8. Query Classifier (Fast vs. Strong Model)

**Gap:** ZeroClaw has `classifier.rs` — routes simple queries to a cheap/fast model and
complex ones to a strong model. We always use the agent's single configured model.

**What to build:**
- Add optional `fast_model` config block alongside each agent's `model`
- Classify on heuristics: short query + no tool history → fast model; otherwise strong
- Aligns with existing multi-agent YAML pattern (fast agent for routing already planned)

---

## Channels Service — Build Order

Independent of the Agent phased plan. Can proceed in parallel.

```
Phase 1: Trait scaffold + Telegram
  - Channel trait + ChannelAddress URL type
  - Telegram implementation (send, listen, typing, draft via message edit)
  - MCP-lite registration: tools.list returns channel tools
  - Replace services/telegram/

Phase 2: Discord + Slack
  - Discord implementation (send, listen, threads, reactions)
  - Slack implementation (send, listen, thread_ts, socket mode)
  - Replace services/discord/ and services/slack/

Phase 3: Draft streaming wire-up
  - agent.step_stream protocol extension
  - Channel service subscribes to Agent delta frames
  - Draft update loop per active step

Phase 4: Remaining platforms
  - iMessage (macOS only, SQLite + AppleScript)
  - IRC, Mattermost, Signal
```

WhatsApp stays in `services/whatsapp/` (Go/whatsmeow) — not part of this omnibus.

---

## Cross-Cutting Items

| Item | Owner | Status |
|---|---|---|
| Agent phased plan (Phases 0–5) | `services/agent/TODO.md` | ✅ Complete |
| Agent Phase 6: Plan/Research DAG | `services/research/` | 🔄 Building |
| Research service (`services/research/`) | New Rust service | 🔄 Building |
| Multi-agent: Supervisor/Worker | Agent + Research service | Follows Research service |
| Tower middleware (full stack) | `openagent/src/` | ✅ Complete |
| Provider fallback chain | `services/agent/src/llm.rs` | ✅ Complete |
| Rate limiting middleware | `openagent/src/server.rs` | ✅ Complete |
| Web UI diary page | `app/routes/diary.py` | ✅ Complete |
| Web UI research page | `app/routes/research.py` | 🔄 Building |
| Channels omnibus | `services/channels/` | WIP |
| Memory offline compaction | `services/memory/` | Deferred |
| Agent Phase 7: Segmented STM | — | ❌ CANCELLED (sliding window is permanent) |
| Axum over UDS between openagent and services | — | ❌ CANCELLED (MCP-lite JSON is permanent) |
| Agent Phase 8: Reflection | `services/agent/` | After Research stable |
| Agent Phase 9: Curiosity queue | `services/agent/` | After Phase 8 |
