# Cortex TODO

Phased implementation plan for Cortex as the future Rust orchestrator service.

---

## Phase 0: Capture the Boundary ✅ DONE

- Finalize Cortex as a separate service, not an embedded OpenAgent module.
- Keep current Python loop as a temporary shell.
- Treat Python middleware such as STT and whitelist as pre-Cortex middleware for now.
- Lock Cortex transport to MCP-lite over JSON + UDS.
- Define Cortex as the only component allowed to call the LLM in the target architecture.

---

## Phase 1: Cortex Skeleton MVP ✅ DONE

Goal: replace the current agent loop with a minimal Cortex step.

- Create `src/main.rs`
- Add MCP-lite server bootstrap using `sdk-rust`
- Expose a single step-style tool or request path for session execution
- Add request/response schemas for:
  - session id
  - user input
  - response text
  - optional tool activity summary
- Implement basic prompt builder
- Implement LLM client boundary
- Return plain answer without tools or planning first
- Add OTEL spans, metrics, and structured logs

Exit criteria:
- Python shell can send one message to Cortex and get one response back ✅

---

## Phase 1B: AutoAgents Core Integration ✅ DONE (with deviations — see below)

Goal: replace Cortex's manual `reqwest` LLM calls and ad-hoc tool handling with AutoAgents as the execution framework.

### Cargo.toml additions

- Add `autoagents-llm` — unified `LLMProvider` trait ✅
- Add `autoagents-core` — `AgentDeriveT`, `AgentExecutor`, `AgentHooks`, `ToolT` ✅
- `autoagents-derive` — NOT added; raw `Value` used for tool args instead (see deviations)
- Do NOT add: `autoagents-protocol`, `autoagents-telemetry`, any `autoagents-core::memory` feature ✅

### CortexAgent ✅ (fully updated to framework runtime)

- `CortexAgent` struct: `agent_name`, `system_prompt`, `action_context`, `provider_config`, `tools`, `router: Arc<ToolRouter>`
- Implements `AgentDeriveT` — `Output = ReActOutput`, `output_schema()` returns `ReActOutput::structured_output_format()`
- Implements `AgentExecutor` — `execute()` IS the full multi-turn ReAct loop; `max_turns = MAX_REACT_ITERATIONS (10)`
- Implements `AgentHooks` — all default no-ops; Phase 3 overrides `on_run_complete` (diary write) and `on_tool_call` (whitelist check)
- `ReActOutput` implements `AgentOutputT` — `output_schema()` returns JSON schema string; `structured_output_format()` returns the structured output JSON
- `CortexAgentError` newtype — bridges to `RunnableAgentError::ExecutorError` via `From<CortexAgentError>`
- `CortexAgent::new()` per-request (stateless by design) — see deviation #6
- `StepRequest` holds `BaseAgent<CortexAgent, DirectAgent>` — `base_agent.run(Task::new(user_input))` is the Tower service entry point

### Tool stubs ✅ (present; bypassed at runtime — see deviation #2)

- `MemorySearchTool`, `SandboxExecuteTool`, `BrowserNavigateTool`, `ActionDispatcherTool`
- Satisfy `AgentDeriveT` interface; return `{"status":"stub"}` — real routing is in `ToolRouter`

### LLM provider swap ✅

- `autoagents-llm::LLMBuilder` replaces manual `reqwest` calls
- Anthropic and OpenAI-compat backends selected from config
- `llm.rs` retained (not deleted) — wraps `autoagents-llm` with OpenAgent prompt types and OTEL

### Items NOT done from original plan (deferred — see deviations)

- `ActorAgent` / ractor multi-agent — deferred; no startup-time agent construction
- `autoagents-derive` proc macros — not needed with `Value`-based tool args

Exit criteria:
- `CortexAgent` implements full AutoAgents trait set ✅
- Stub tools callable without live services ✅
- 28/28 tests pass ✅
- Manual `reqwest` LLM code deleted ✅

---

## Phase 2: Tool Routing Baseline ✅ DONE

Goal: let Cortex execute tools directly.

- Add `tool_router` module ✅ — prefix dispatch: `browser.*` → `browser.sock`, `sandbox.*` → `sandbox.sock`
- Define structured LLM tool-call output contract ✅ — `StructuredStepOutput` + `parse_step_model_output` in `response.rs`
- Validate tool names and arguments before execution ✅ — type check + empty-guard in parser
- Full ReAct loop ✅ — `CortexAgent::run()`: LLM → validate → parse → tool dispatch → inject result → repeat
- Append tool result back into the reasoning loop ✅ — appended as user message between iterations
- Record tool call telemetry ✅ — span fields, `react_summary` in response JSON, structured logs per iteration
- Validator wired into loop ✅ — `maybe_validate_response` called before each `parse_step_model_output`
- `cortex.discover` disabled ✅ — deterministic tool set only; discover type rejected in parser

Exit criteria:
- Cortex can complete one LLM → tool → LLM round-trip ✅
- 38/38 tests pass ✅

### Outstanding from Phase 2 plan

- **Tower Phase 1** — `TraceLayer` + `TimeoutLayer` wired in `step_service.rs`. ✅ DONE
- **`memory.search` in default tool set** — added in Phase 3. `ToolRouter` resolves `memory.*` by prefix convention; `DEFAULT_TOOL_NAMES` now includes `memory.search`. ✅

---

## Deviations from AutoAgents Pattern

Intentional pragmatic decisions. The AutoAgents framework is used as both **trait contract** and **runtime executor** — with one deliberate bypass of the framework's built-in `TurnEngine`/`ReActAgent` (see Deviation #2).

### 1. ~~No `BaseAgent`~~ — RESOLVED ✅ (fully wired)

`BaseAgent::<CortexAgent, DirectAgent>::new(cortex_agent, llm_provider, Some(Box::new(memory_adapter)), tx, false)` is constructed in `handle_step`. `StepRequest` now holds the full `BaseAgent<CortexAgent, DirectAgent>` — `base_agent.run(Task::new(user_input))` is the runtime entry point.

**Runtime path:** `base_agent.run(task)` → `on_run_start` → `AgentExecutor::execute(task, context)` → `on_run_complete`. The full AutoAgents hook lifecycle fires. `AgentExecutor::execute()` IS the multi-turn ReAct loop — it uses `context.llm()` (provider built once at `BaseAgent::new()`) and `context.memory()` (HybridMemoryAdapter) throughout. Tool dispatch goes through `self.router` (stored in `CortexAgent`) over UDS — not through the framework's `ToolProcessor`.

### 2. Framework's `TurnEngine`/`ReActAgent` bypassed — own execute() implements ReAct

**Why not `TurnEngine`:** AutoAgents' built-in `ReActAgent` executor uses `TurnEngineConfig::react()` with `ToolMode::Enabled` — it dispatches tools through `context.tools()` via the LLM's native `function_call`/`tool_use` API response format. This requires models that reliably emit structured tool-call responses. Local sub-30B models (Qwen, Llama, Mistral) do not. Our JSON text output format (`{"type":"tool_call","tool":"...","arguments":{...}}`) is the correct tradeoff for the target hardware.

**What we do instead:** `AgentExecutor::execute()` in `agent.rs` IS the full multi-turn ReAct loop. It:
- Uses `context.llm().chat_stream()` (reuses the pre-built provider from `BaseAgent::new()`)
- Uses `context.memory()` for recall and remember
- Dispatches tools via `self.router.call()` over UDS (not `ToolProcessor::process_tool_calls`)
- Fires all `AgentHooks` lifecycle methods manually from inside the loop
- `AgentDeriveT::tools()` returns `vec![]` — tool dispatch is string-keyed via `ToolRouter`, not trait-dispatch via `ToolT::execute()`

**Future:** When Phase 5 wires typed tool stubs as `ToolT` implementations, they can co-exist with `ToolRouter` dispatch without changing `execute()`.

### 3. ~~No `CortexMemoryAdapter`~~ — RESOLVED ✅

`HybridMemoryAdapter` (`src/memory_adapter.rs`) implements the full `MemoryProvider` trait:
- **STM:** AutoAgents `SlidingWindowMemory` (`TrimStrategy::Drop`, `DEFAULT_STM_WINDOW = 40` messages). Eviction intercepted by checking `stm.size() >= window_size` before `remember()` — oldest message dumped to `data/stm/{session_id}/{unix_ms}_eviction.md`. `clear()` dumps full window to `{unix_ms}_clear.md`.
- **LTM:** `memory.search` via `ToolRouter` on `memory.sock`. Query is `user_input` (semantic signal); gracefully no-ops when memory service is down.
- **Recall:** `[ltm_hits…, stm_window…]` — LTM prepended as background context, STM appended as recent window.
- **Memory wired into ReAct loop:** History recalled at loop start; user + assistant messages persisted after each turn.

`SlidingWindowMemory` (40-message window) is the permanent STM implementation — no replacement planned.

### 4. No `ActorAgent` / ractor — no multi-agent runtime

**Plan:** Supervisor ractor actor + per-YAML-agent worker actors registered at startup.
**What exists:** Single `CortexAgent` constructed inside `handle_step` per request. Agent selection is `resolve_step_config(agent_name)` — picks config block only.
**Why:** ractor adds operational surface (mailboxes, supervisor restart policy, actor lifecycle). Not justified until memory and tool layers are stable. Architecture is ready — adding actor dispatch is an `AppContext` field plus `tokio::spawn` in `main.rs`.

### 5. No `autoagents-derive` proc macros

**Plan:** `#[derive(ToolInput)]` for all tool input structs.
**What exists:** Tool inputs use raw `serde_json::Value` in `execute(args: Value)`.
**Why:** Tool inputs are arbitrary LLM JSON dispatched over a UDS socket as `Value` anyway. Strong typing via proc macros adds boilerplate with no safety gain at the service boundary.

### 6. `CortexAgent` constructed per-request, not at startup

**Plan:** `CortexAgent::from_config()` at startup, registered with ractor supervisor.
**What exists:** `CortexAgent::new()` inside `handle_step()` on every request. Config re-loaded from disk per step via `CortexConfig::load()`.
**Why:** Stateless by design for Phase 1B. Disk read cost per step is acceptable. Moves to startup construction when actors are added.

---

## Phase 3: Memory System ✅ DONE

Goal: make Cortex memory-aware and extend the memory service to serve three searchable stores.

### Memory hierarchy (4 levels)

```
Level 0: In-process sliding window    (SlidingWindowMemory, 40 messages; lives for one cortex.step call)
Level 1: STM overflow                 (markdown files: data/stm/{session_id}/{unix_ms}_{reason}.md)
Level 2: Diary                        (markdown: data/diary/{session_id}/{turn_index}-{ts}.md
                                       + LanceDB stub index row — no embedding at write time)
Level 3: memory                       (LanceDB `memory` table — compacted summaries, embedded)
Level 4: knowledge                    (markdown + LanceDB `knowledge` index — curated KB)
```

### LanceDB tables (final names)

| Table | Role | Status |
|---|---|---|
| `memory` | Compacted episode summaries — direct vector storage | Rename from `ltm` in memory service |
| `diary` | Index rows → diary markdown files (stub at write, filled at compaction) | New |
| `knowledge` | Index rows → KB markdown files | New (empty until compaction) |
| `stm` | **Eliminated** — STM is now markdown files | Remove from memory service |

### `memory.search` stores

`memory | diary | knowledge | all` — STM is internal only, never searchable.

### Retrieval flow

```
loop start (iteration 0, generation turns only):
  → memory.search(query=user_input, store=memory) — seeds memory segment

during loop:
  → buffer eviction → write to data/stm/{session_id}/{turn_index}.md
  → no duplicate tool loads

loop end (ReActOutput returned):
  → write diary markdown to data/diary/{session_id}/{turn_index}-{ts}.md
  → write stub LanceDB diary row (no summary/keywords/embedding)
  → fire-and-forget (non-blocking)
  → STM markdown files for this session pruned
```

### Offline compaction (idle-triggered — NOT Phase 3)

1. Find diary rows with blank summary
2. LLM call → generate summary + keywords per entry
3. Embed summary → update diary LanceDB row
4. Sufficient entries from session/topic → synthesise `memory` entry
5. Dense `memory` cluster → synthesise `knowledge` article (markdown + knowledge index row)

### YAML additions

```yaml
memory:
  diary_path: data/diary
  stm_path: data/stm
  socket: data/sockets/memory.sock
```

### Step 1 — Cortex (build first)

- [x] `src/memory_adapter.rs` — `HybridMemoryAdapter` implementing `MemoryProvider` (STM via `SlidingWindowMemory` + LTM via `memory.sock`). Eviction/clear hooks dump to `data/stm/{session_id}/` markdown files. ✅
- [x] Wire memory retrieval at loop start — `recall(user_input)` merges LTM + STM; history injected before current turn ✅
- [x] Wire STM eviction → markdown file writes (`{unix_ms}_eviction.md`, `{unix_ms}_clear.md`) ✅
- [x] Wire diary write at end of `execute()` — markdown + stub LanceDB row via `memory.diary_write` (fire-and-forget via `tokio::spawn`) ✅
- [x] Add `memory.search` to `DEFAULT_TOOL_NAMES` ✅
- [x] YAML: parse `memory` block (`diary_path`, `stm_path`) into `CortexConfig` ✅

### Step 2 — Memory service (build after Cortex)

- [x] `db.rs`: rename `LTS_TABLE` from `"ltm"` to `MEMORY_TABLE = "memory"` ✅
- [x] `db.rs`: remove `STS_TABLE` (`"stm"`) — STM is now markdown ✅
- [x] `db.rs`: add `DIARY_TABLE = "diary"` (same Arrow schema as memory) ✅
- [x] `db.rs`: add `KNOWLEDGE_TABLE = "knowledge"` (same Arrow schema) ✅
- [x] `handlers.rs`: add `handle_diary_write` — write stub diary LanceDB row (zero vector placeholder) ✅
- [x] `handlers.rs`: extend `handle_search` — `store=memory|diary|knowledge|all` (fan out, RRF merge) ✅
- [x] `handlers.rs`: update `handle_index` — `store=memory` only (removed `stm` option) ✅
- [x] `handlers.rs`: update `handle_prune` — prune old diary entries by age (replaced STS prune) ✅
- [x] `handlers.rs`: update `handle_delete` — `store=memory|diary|knowledge` ✅
- [x] `tools.rs`: add `memory.diary_write` tool definition ✅
- [x] `tools.rs`: update `memory.search` params — `store` enum: `memory | diary | knowledge | all` ✅
- [x] `tools.rs`: update `memory.index` params — `store` enum: `memory` only ✅
- [x] `tools.rs`: update `memory.delete` params — `store` enum: `memory | diary | knowledge` ✅
- [x] `main.rs`: ensure `memory`, `diary`, `knowledge` tables at startup ✅

### Exit criteria

- Cortex retrieves from `memory` store at loop start via `HybridMemoryAdapter` LTM recall ✅
- STM overflow written to markdown files at `data/stm/{session_id}/{unix_ms}_{reason}.md` ✅
- Every completed loop produces diary markdown + stub LanceDB diary row (fire-and-forget) ✅
- `memory.search` covers `memory | diary | knowledge | all` ✅
- `memory.search` wired into `DEFAULT_TOOL_NAMES` — model can call it during reasoning ✅
- 46/46 tests pass ✅

---

## Phase 4: Prompt System ✅ DONE

Goal: externalize prompts and stop hardcoding cognitive instructions.

- [x] Add `src/prompt.rs` — MiniJinja `OnceLock<Environment<'static>>` with compile-time `include_str!` embedding ✅
- [x] `prompts/step_system.j2` — base system prompt + JSON output format section ✅
- [x] `prompts/tool_context.j2` — appends available-tools block to the system prompt ✅
- [x] `prompts/correction.j2` — injected as a user turn when model returns non-JSON ✅
- [x] Three render functions: `render_step_system`, `render_tool_context`, `render_correction` ✅
- [x] `handlers.rs`: `build_structured_system_prompt` deleted; replaced with `crate::prompt::render_step_system` ✅
- [x] `llm.rs`: `append_action_context` delegated to `crate::prompt::render_tool_context` ✅
- [x] 7 unit tests covering all render paths ✅

Exit criteria:
- Cortex loads prompts from embedded MiniJinja templates without recompilation ✅

---

## Phase 4A: Diary Store and Index ✅ DONE

Goal: capture human-readable request/response history without polluting normal memory retrieval.

- [x] `prompts/diary_entry.j2` — deterministic MiniJinja template for diary markdown ✅
- [x] `src/prompt.rs`: add `DiaryEntryContext` struct + `render_diary_entry()` function ✅
- [x] `src/diary.rs`: replace `format!` string with `render_diary_entry()` call ✅
- [x] `src/diary.rs`: enrich `memory.diary_write` params — `keywords`, `validator_status`, `flags` ✅
- [x] `memory/src/handlers.rs`: `handle_diary_write` accepts and stores enriched metadata ✅
- [x] `memory/src/tools.rs`: `memory.diary_write` schema extended with optional fields ✅
- [x] Diary search (`store=diary`) accessible only when explicitly requested — not included in normal `HybridMemoryAdapter` LTM recall (which uses `store=memory`) ✅
- [x] 3 unit tests for `render_diary_entry` (sections, tool list, whitespace trim) ✅

Exit criteria:
- Every completed cycle produces a deterministic markdown diary entry plus a LanceDB reference index row ✅
- Diary entries can be semantically scanned by HITL via `store=diary` without being used in normal context injection ✅

---

## Phase 5: Action Search ✅ DONE

Goal: avoid exposing every tool and skill to the LLM at every step.

Implementation (keyword-based, no embedding needed for Phase 5):
- `ActionCatalog` — discovers tools from `services/*/service.json` + skills from `skills/*/SKILL.md` ✅
- `search_catalog()` — keyword-scored top-k ranking (name, summary, params field scoring) ✅
- `ACTION_SEARCH_LIMIT = 8` — max candidates per step ✅
- `ALWAYS_INCLUDE = ["memory.search"]` — pinned regardless of search results ✅
- `cortex.discover` always appended — agent can search for more tools mid-task ✅
- Generation turns use `search_tools_for_step(&catalog, &user_input)` ✅
- Tool-call turns receive empty candidate list (agent is mid-ReAct, no re-injection) ✅

Exit criteria:
- Cortex exposes only top-k relevant tools per turn rather than all tools ✅
- `cortex.discover` available for mid-task tool expansion ✅

---

## Phase 6: Plan Store and DAG

Goal: give Cortex persistent control state.

- Add SQLite-backed plan store
- Add tables: plans, tasks, task_dependencies, tool_calls, turns, sessions
- Add runnable-task selection
- Add plan snapshot injection into prompt
- Update plan after each tool call or step
- Keep a compact active plan summary in STM or step state

Exit criteria:
- Cortex can resume a multi-step task across turns

---

## Phase 7: Segmented STM — ❌ CANCELLED

**Decision (2026-03-17):** Sliding window STM (`SlidingWindowMemory`, 40 messages) is the permanent STM implementation. Segmented STM is cancelled.

**Why:** The sliding window is simple, deterministic, and sufficient for the target hardware (Raspberry Pi). Segmented STM adds per-segment budget management and compaction policy complexity without a proven benefit at the current scale. If the window proves too coarse, add structured metadata fields to individual messages rather than splitting into named segments.

**Current state (permanent):**
- `HybridMemoryAdapter` uses `SlidingWindowMemory` (40-message window, `TrimStrategy::Drop`)
- Evicted messages dump to `data/stm/{session_id}/{unix_ms}_eviction.md` for offline review
- `clear()` dumps full window to `{unix_ms}_clear.md`
- STM is never searchable (internal only); overflow files are pruned per session

---

## Phase 8: Reflection

Goal: add background cognition after the main loop is stable.

- Add reflection scheduler
- Add cross-thread synthesis requests
- Add well-supported hypothesis generation
- Add research digest generation
- Add contradiction candidate generation for HITL

Exit criteria:
- Cortex can periodically synthesize research state without disrupting core task execution

---

## Phase 9: Curiosity and Investigation Queue

Goal: enable research collaborator behavior.

- Add curiosity queue generation
- Add confidence-gated autonomous exploration levels
- Keep suggestion output non-intrusive
- Present optional research leads rather than forcing direction changes

Exit criteria:
- Cortex can surface research leads as suggestions instead of direct interruptions

---

## Phase 10: Harden the Service Boundary

- Add retries/timeouts per dependent service
- Add degraded-mode behavior when memory or tool services are unavailable
- Add replay-friendly step logs
- Add trace correlation across LLM, tools, and memory
- Add protocol versioning notes

Exit criteria:
- Cortex survives partial subsystem failures without corrupting control state

---

## Tower Middleware Migration — ✅ COMPLETE (2026-03)

**Cortex is a pure MCP-lite service. It does not own Tower middleware.**

Tower was removed from Cortex entirely:
- `step_service.rs` deleted
- `tower` dep removed from `Cargo.toml`
- `handlers.rs` calls `base_agent.run()` directly — no Tower wrapping

**The Tower middleware stack lives in the `openagent` Rust binary (the control plane):**

```
openagent (Axum TCP :8080)
  TimeoutLayer(130s)          ✅
  HandleErrorLayer(→ HTTP 408) ✅
  TraceLayer                   ✅
  GuardLayer → guard.check     ✅
  SttLayer   → stt.transcribe  ✅
  TtsLayer   → tts.synthesize  ✅ (disabled by default; toggle in openagent.toml)
  Router → /step /tool/:name /tools /health
```

Python middleware deleted:
- `whitelist.py` → GuardLayer ✅
- `stt.py`       → SttLayer   ✅
- `tts.py`       → TtsLayer   ✅

Dispatch loop added to openagent (`src/dispatch.rs`):
- Subscribes to ServiceManager event bus
- Routes `message.received` → `cortex.step` → `channel.send`
- Semaphore-bounded (max 4 concurrent) for Pi-friendly backpressure ✅

---

## Deferred by Design

Not for early MVP:
- full contradiction arbitration
- concept canonicalization
- knowledge decay management inside Cortex
- splitting memory into multiple services
- dynamic distributed scheduling
