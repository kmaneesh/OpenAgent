# OpenAgent — Project TODO

Top-level cross-service backlog. Service-specific phased roadmaps live in their own
`TODO.md` files (e.g. `services/cortex/TODO.md`).

---

## ZeroClaw Gap Analysis (2026-03-15)

Reference implementation: `inspire/zeroclaw/`. Full comparison recorded in conversation
history. Actionable gaps below, ordered by impact.

### 🔴 High Priority

#### 1. Credential Scrubbing on Inbound Messages ✅ DONE

**Implementation:** `openagent/src/scrub.rs` — `scrub::process(input, context)`.
Hand-rolled byte scanner, no new deps.

Applied in two places (Guard layer):
- `openagent/src/middleware.rs` — scrubs `user_input` in buffered HTTP request body before STT/Cortex
- `openagent/src/dispatch.rs` — scrubs channel message `content` before `cortex.step`

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
- Typing indicators: "Bot is typing…" while Cortex step runs.
- Single binary to cross-compile and deploy on Pi vs. 4–6 individual binaries.

**Dependency:** Cortex needs a streaming path through `cortex.step` to feed partial
responses to the channel service for draft updates. Currently `cortex.step` is
request-response only.

---

### 🟡 Medium Priority

#### 3. Draft Streaming Path in Cortex

**Gap:** ZeroClaw streams partial LLM output to channels mid-response. Our `cortex.step`
is fully synchronous request-response — the caller gets the final answer only.

**What to build:**
- Add a streaming variant to the MCP-lite protocol: `cortex.step_stream` returning
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
- `RateLimitLayer` as a Tower middleware in Cortex (alongside the planned `WhitelistLayer`)
- Configurable budget: `max_steps_per_hour` per session_id from `openagent.yaml`
- Returns a structured error when budget exceeded (not a panic)
- Fits naturally in the Tower Phase 2 middleware stack

---

#### 5. Provider Fallback Chain

**Gap:** ZeroClaw has `ReliableProvider` wrapping any provider with a fallback chain.
If the primary model times out or errors, the next provider in the list is tried.

**What to build:**
- `FallbackProvider` wrapper in `autoagents-llm` or in `cortex/src/llm.rs`
- Config: `providers.fallback_chain = ["primary", "secondary"]` in `openagent.yaml`
- Try each provider in order; return first success; log fallback activations

---

### 🟢 Lower Priority / Nice to Have

#### 6. Prometheus `/metrics` Endpoint

**Gap:** ZeroClaw exposes a Prometheus scrape endpoint. We write JSONL only (OTLP file
export). On Pi with Grafana, scraping is more convenient than parsing JSONL.

**What to build:** Expose `/metrics` from the Cortex gateway (Phase 4 Axum endpoint).
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

Independent of the Cortex phased plan. Can proceed in parallel.

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
  - cortex.step_stream protocol extension
  - Channel service subscribes to Cortex delta frames
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
| Cortex phased plan (Phases 0–5) | `services/cortex/TODO.md` | ✅ Complete |
| Cortex Phase 6: Plan/Research DAG | `services/research/` | 🔄 Building |
| Research service (`services/research/`) | New Rust service | 🔄 Building |
| Multi-agent: Supervisor/Worker | Cortex + Research service | Follows Research service |
| Tower middleware (full stack) | `openagent/src/` | ✅ Complete |
| Provider fallback chain | `services/cortex/src/llm.rs` | ✅ Complete |
| Rate limiting middleware | `openagent/src/server.rs` | ✅ Complete |
| Web UI diary page | `app/routes/diary.py` | ✅ Complete |
| Web UI research page | `app/routes/research.py` | 🔄 Building |
| Channels omnibus | `services/channels/` | WIP |
| Memory offline compaction | `services/memory/` | Deferred |
| Cortex Phase 7: Segmented STM | — | ❌ CANCELLED (sliding window is permanent) |
| Axum over UDS between openagent and services | — | ❌ CANCELLED (MCP-lite JSON is permanent) |
| Cortex Phase 8: Reflection | `services/cortex/` | After Research stable |
| Cortex Phase 9: Curiosity queue | `services/cortex/` | After Phase 8 |
