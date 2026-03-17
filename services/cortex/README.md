# Cortex Service

Rust service — OpenAgent's cognitive control plane and **multi-agent supervisor**. Cortex owns the full ReAct loop, action search, tool routing, and memory orchestration. In the multi-agent setup, Cortex acts as the supervisor: it holds (via the Research service) a persistent task DAG, picks the next runnable task, and dispatches worker agents by name.

Cortex coordinates:
- LLM provider (via `autoagents-llm`, with fallback chain)
- Memory service (STM sliding window + LTM via `memory.sock`)
- Research service (`research.sock`) — reads task graph, updates task state
- Tool services (browser, sandbox, and others via `ToolRouter`)

All coordination is MCP-lite JSON over Unix Domain Sockets — permanent internal protocol, never migrating to HTTP/Axum between services.

## Current Status (Phases 0–6 complete)

Cortex is a production Rust MCP-lite service. The following are all shipped:

- **Transport:** MCP-lite JSON over Unix Domain Sockets (`data/sockets/cortex.sock`) — permanent protocol, never migrating to HTTP/Axum internally
- **Full ReAct loop** (`src/agent.rs`): LLM → parse → tool dispatch → inject result → repeat, up to `MAX_REACT_ITERATIONS = 10`
- **Tool routing** (`src/tool_router.rs`): prefix-based dispatch over UDS — `browser.*`, `sandbox.*`, `memory.*`, `research.*`, `cortex.*` (self-call for worker dispatch)
- **Memory system** (`src/memory_adapter.rs`): `HybridMemoryAdapter` — sliding-window STM (40 messages, permanent) + LTM via `memory.search`; diary writes fire-and-forget after each cycle
- **Prompt system** (`src/prompt.rs`): MiniJinja embedded templates (`prompts/*.j2`) — no recompile to change prompts
- **Action search** (`src/action/`): `ActionCatalog` keyword-scores top-k tools from `service.json` manifests + `SKILL.md` files per step; `memory.search`, `research.status`, and `cortex.step` always pinned
- **Provider fallback chain** (`src/llm.rs`): `dispatch_with_fallback()` tries primary provider then each fallback in order; per-attempt structured logs
- **Research context injection** (`src/handlers.rs`): at each generation turn, fetches active research via `research.status`, formats runnable tasks into the system prompt so the supervisor picks the next task without an extra tool call
- **Worker agent dispatch** (`src/handlers.rs` + `src/tool_router.rs`): supervisor calls `cortex.step` with `agent_name` to dispatch tasks to named worker agents; ToolRouter self-routes `cortex.*` back to `cortex.sock`; worker resolves its config from `openagent.yaml` by name and runs a full independent ReAct loop
- **OTEL** (`sdk-rust`): traces, logs, metrics to `logs/cortex-*.jsonl` (daily rotation)

What does not exist yet:
- Reflection scheduler (Phase 8)
- Curiosity queue (Phase 9)

See [`TODO.md`](./TODO.md) for the full phase breakdown.

## Role

Cortex owns:
- session step execution
- prompt/context assembly
- tool discovery and tool routing
- plan-of-action execution
- reflection scheduling
- memory orchestration
- diary emission orchestration

Cortex does not own:
- vector storage internals
- markdown KB storage internals
- browser automation internals
- sandbox execution internals
- direct user interface concerns

Those remain in dedicated services.

## Final Topology

```text
OpenAgent Shell (Python, temporary)
        |
        v
   Cortex Service (Rust)
        |
  +-----+----------+------------------+
  |                |                  |
  v                v                  v
LLM Service    Memory Service     Tool Services
               (STM/LTM/KB)       (browser, sandbox, ...)
```

All service communication should use MCP-lite over JSON + UDS.

## Design Principles

- Cortex is a service boundary, not a library inside OpenAgent.
- Cortex is the single source of cognition. The shell must not call the LLM directly.
- Tools are called by Cortex, not by the outer Python loop.
- Memory remains a separate service. Cortex decides when to read and write memory, but does not own LanceDB or the KB vault directly.
- STM is working control state and should remain local to Cortex or Cortex-managed runtime state.
- LTM and KB remain persistent memory concerns behind the memory service boundary.

## Cognitive Stages

Cortex is organized as a subsystem with three major stages around the LLM.

### 1. Pre-LLM Cognition

Responsibilities:
- load active session
- load active plan
- decide current goal and current runnable task
- retrieve memory context
- load STM segments
- assemble final prompt package

Inputs:
- user input
- session state
- plan state
- STM state
- memory bundle

Outputs:
- prompt package for LLM
- candidate tools
- execution state snapshot

### 2. LLM Reasoning

The LLM is treated as a reasoning engine, not the system brain.

Expected outputs:
- answer
- tool call
- plan update suggestion
- reflection output

The LLM must produce structured output. It must not mutate state directly.

### 3. Post-LLM Cognition

Responsibilities:
- validate LLM output
- execute tool calls
- update plan DAG
- write episodic memory
- emit deterministic diary entry
- schedule reflection
- emit final response to caller

Outputs:
- user-visible response
- plan update
- tool execution log
- memory write events
- diary write event

## Planned Subsystems

### Planner

Persistent task graph stored in SQLite. Plans are not memory objects; they are control state.

Expected tables:
- `plans`
- `tasks`
- `task_dependencies`
- `tool_calls`
- `turns`
- `sessions`

Each session owns an active plan. Each task can depend on previous tasks.

### Retrieval

Unified search interface in Cortex, backed by the memory service.

Initial strategy:
- one unified memory search
- memory bundle return shape
- KB graph expansion handled by memory service or requested via memory API

Longer term:
- richer routing by task type and uncertainty
- tighter coupling between plan state and retrieval query construction

### Action Registry and Action Search

Because the number of tools and skills will grow, Cortex should not expose the full action set to the LLM every cycle.

Instead:
- maintain an action registry
- index service tools and local skills together
- search top-k candidate actions for the current task
- pass only a small candidate set to the LLM

Action discovery is the main abstraction, not service names. Browser and sandbox are important because they expose many tools each, while local skills provide guidance about how to use those tools. Cortex should discover and rank available actions across tool services and local skills rather than hardcode one-off integrations.

Current design:
- discover service tools from `services/*/service.json` at boot
- discover local skills from `skills/*/SKILL.md` at boot
- keep the catalog only in transient Cortex memory for now
- inject top candidate action summaries only on generation turns
- inject nothing on deterministic tool-call turns

Each action record should include:
- name
- kind
- summary
- owner
- input schema summary
- tags
- embedding later

Skills are guidance-first in the current phase. Later they should move to a hybrid model where some skills become executable workflows.

### Tool Router

Routes tool calls to owning services over MCP-lite.

Initial services:
- `memory`
- tool services such as `browser`
- tool services such as `sandbox`

Later:
- additional platform and compute services

### STM Manager

`SlidingWindowMemory` (40-message window, `TrimStrategy::Drop`) is the permanent STM implementation. Segmented STM is cancelled — the flat sliding window is sufficient for the target hardware.

Evicted messages dump to `data/stm/{session_id}/{unix_ms}_eviction.md` for offline review. STM is internal only and never searchable.

### Reflection

Future reflection responsibilities:
- cross-thread synthesis
- well-supported hypothesis generation
- contradiction detection handoff
- research digests
- curiosity queue generation

## Memory Relationship

Cortex should use the memory service, not absorb it.

Memory service logical layers:
- STM support data if needed
- LTM in LanceDB
- KB in markdown vault with graph links
- Diary as markdown on disk plus metadata/index support

Cortex responsibilities toward memory:
- request retrieval
- write episodes
- trigger promotion/compaction workflows
- consume memory bundles during reasoning
- hand off deterministic diary writes after each completed tool cycle

Memory service responsibilities:
- storage
- indexing
- graph parsing
- clustering support
- knowledge persistence
- diary persistence and diary indexing

## AutoAgents Integration Architecture

Cortex uses [AutoAgents](https://github.com/liquidos-ai/AutoAgents) (v0.3.6) as its agent execution framework. The integration uses the framework as the **runtime** — `BaseAgent::run()` is the entry point, `AgentExecutor::execute()` contains the ReAct loop, and all `AgentHooks` lifecycle methods fire. One part of the framework is intentionally bypassed: the built-in `TurnEngine`/`ReActAgent` executor (see below).

### Crates adopted

| Crate | Role in Cortex |
|---|---|
| `autoagents-llm` | Unified `LLMProvider` trait — replaces manual `reqwest` LLM calls. Provider built once at `BaseAgent::new()`, reused for all loop iterations via `context.llm().chat_stream()`. |
| `autoagents-core` | `BaseAgent`, `AgentDeriveT`, `AgentExecutor`, `AgentHooks`, `MemoryProvider`, `Context`. Full framework runtime — `base_agent.run(task)` is the step entry point. |
| `autoagents-protocol` | `Event` type used for `BaseAgent::new()` channel construction only. |

### Crates deliberately excluded

| Crate | Why excluded |
|---|---|
| `autoagents-core::prebuilt::ReActAgent` + `TurnEngine` | Requires native LLM tool-calling format (`function_call`/`tool_use`). Local sub-30B models don't reliably produce this. Cortex uses JSON text output format instead. |
| `autoagents-derive` | Tool inputs are `serde_json::Value` dispatched over UDS — proc macros add no safety at this boundary. |
| `autoagents-telemetry` | OpenAgent has its own OTEL pipeline via `sdk-rust` (file-based OTLP/JSON, daily rotation). |

---

### CortexAgent — framework runtime, custom ReAct loop

`BaseAgent::run(Task)` is the entry point. The framework fires hooks in order:

```
base_agent.run(task)
  → on_run_start(context)
  → AgentExecutor::execute(task, context)   ← our ReAct loop lives here
      for iteration in 0..MAX_REACT_ITERATIONS:
          on_turn_start(iteration, context)
          context.llm().chat_stream(messages, ...)  ← reuses pre-built provider
          parse JSON output → "final" | "tool_call"
          if tool_call:
              on_tool_call(llm_tool_call, context)  → HookOutcome::Continue | Abort
              on_tool_start(...)
              self.router.call(tool_name, args)      ← UDS dispatch, not ToolProcessor
              on_tool_result(...) | on_tool_error(...)
          on_turn_complete(iteration, context)
      return ReActOutput
  → on_run_complete(output, context)
```

Why NOT the framework's `TurnEngine`/`ReActAgent`: AutoAgents' built-in ReAct executor dispatches tools via `context.tools()` using native LLM function-calling API. Local sub-30B models (Qwen, Llama, Mistral) don't reliably emit `tool_use` responses. Cortex instructs the model to output exactly one JSON object per turn (`{"type":"tool_call",...}` or `{"type":"final",...}`) and dispatches via `ToolRouter` over UDS. Everything else — `BaseAgent`, `MemoryProvider`, `AgentHooks` — is used as designed.

```rust
pub struct CortexAgent {
    agent_name: String,
    system_prompt: String,          // pre-built with JSON format instructions
    action_context: Option<String>, // candidate tool summaries for generation turns
    provider_config: ProviderConfig, // for telemetry labels
    tools: Vec<Box<dyn ToolT>>,     // AgentDeriveT compliance; empty at runtime
    router: Arc<ToolRouter>,        // UDS dispatch: "browser.open" → browser.sock
}

impl AgentDeriveT for CortexAgent {
    type Output = ReActOutput;      // Serialize + DeserializeOwned + AgentOutputT
    fn output_schema(&self) -> Option<Value> { Some(ReActOutput::structured_output_format()) }
    fn tools(&self) -> Vec<Box<dyn ToolT>> { vec![] }  // ToolRouter handles dispatch
}

impl AgentExecutor for CortexAgent {
    type Output = ReActOutput;
    type Error = CortexAgentError;  // → RunnableAgentError::ExecutorError via From<>
    fn config(&self) -> ExecutorConfig { ExecutorConfig { max_turns: 10 } }
    async fn execute(&self, task: &Task, context: Arc<Context>) -> Result<ReActOutput, CortexAgentError> {
        // Full ReAct loop — see src/agent.rs
    }
}

impl AgentHooks for CortexAgent {}  // all no-ops in Phase 2; Phase 3 overrides on_run_complete
```

---

### HybridMemoryAdapter — MemoryProvider implementation

`HybridMemoryAdapter` (`src/memory_adapter.rs`) implements AutoAgents' `MemoryProvider` trait:

- **STM:** AutoAgents `SlidingWindowMemory` (`TrimStrategy::Drop`, `DEFAULT_STM_WINDOW = 40` messages). Eviction intercepted: when window full, oldest message is dumped to `data/stm/{session_id}/{unix_ms}_eviction.md` before `SlidingWindowMemory` pops it. `clear()` dumps full window to `{unix_ms}_clear.md`.
- **LTM:** `memory.search` via `ToolRouter` on `memory.sock`. Query is `user_input` (semantic signal). Gracefully no-ops when memory service is down.
- **Recall:** `[ltm_hits…, stm_window…]` — LTM prepended as background context, STM as recent window.

`SlidingWindowMemory` (40-message window) is the permanent STM implementation.

---

### Tool dispatch — string-keyed via ToolRouter, not ToolProcessor

The LLM sees a fixed candidate set injected as text in the system prompt (not as native tool schemas). When the model outputs `{"type":"tool_call","tool":"browser.open","arguments":{...}}`:

1. `parse_step_model_output()` extracts `tool` name and `arguments`
2. `on_tool_call()` hook fires — can abort
3. `self.router.call(tool_name, &arguments)` dispatches over UDS to the owning service
4. `on_tool_result()` or `on_tool_error()` fires
5. Result injected back as the next user message

`ToolRouter` uses prefix-based routing: `browser.*` → `browser.sock`, `sandbox.*` → `sandbox.sock`, `memory.*` → `memory.sock`. AutoAgents' `ToolProcessor::process_tool_calls()` is not used — tool names are strings at runtime, not compile-time types.

---

### Multi-agent — Supervisor/Worker dispatch (Phase 6, complete)

Cortex implements the Anthropic Supervisor/Worker pattern. No new service binary is needed per worker — agent identity is config, not code.

**Supervisor flow (every generation turn):**
1. `fetch_research_context()` calls `research.status` via ToolRouter — result formatted as a `## Active Research` block injected into the system prompt
2. Supervisor sees runnable tasks, assigned agents, and the `cortex.step` tool schema (always pinned)
3. For simple tasks: supervisor handles directly in its own ReAct loop
4. For specialised tasks: supervisor calls `cortex.step` with `agent_name="search-agent"` (or any named agent from config)

**Worker flow (when `cortex.step` is called with `agent_name`):**
1. `cortex.sock` receives the call — same `handle_step` handler, same process
2. `resolve_step_config(agent_name)` picks the worker's `system_prompt` and `model` from `config/openagent.yaml`
3. A fresh `CortexAgent` + `HybridMemoryAdapter` is constructed with the worker identity
4. Worker runs a full independent ReAct loop (up to `MAX_REACT_ITERATIONS = 10`)
5. Worker can call `research.task_done`, `research.task_add`, or any service tool directly
6. Worker returns a result string; supervisor uses it, then calls `research.task_done` to advance the DAG

**Self-call routing:** `ToolRouter` routes `cortex.*` → `cortex.sock` via the same prefix mechanism as all other services — no special-casing. Concurrent worker invocations are handled by Tokio's async runtime.

**Config example** (`config/openagent.yaml`):
```yaml
agents:
  - name: supervisor
    system_prompt: "You are a research supervisor. Decompose tasks, delegate to workers, synthesise results."
  - name: search-agent
    system_prompt: "You are a focused search agent. Execute one search task and return structured findings."
  - name: analysis-agent
    system_prompt: "You are an analysis agent. Identify contradictions and synthesise findings across sources."
```

Workers are **stateless per invocation** — they receive full context in the step request. No session state is shared between the supervisor's turn and a worker invocation.

---

## Current Library Set

Libraries in use:
- `sdk-rust` — MCP-lite server and shared OTEL setup
- `tokio` — async service runtime
- `serde`, `serde_json`, `serde_yaml` — protocol payloads and config loading
- `anyhow` — process-level error handling
- `autoagents-llm` — unified LLM provider trait; streaming via `chat_stream()`
- `autoagents-core` — `BaseAgent`, `AgentDeriveT`, `AgentExecutor`, `AgentHooks`, `MemoryProvider`
- `autoagents-protocol` — `Event` type for `BaseAgent` channel construction
- `async-trait` — async trait support for AutoAgents impls
- `futures` — stream accumulation for LLM streaming
- `tower` — `CortexTraceLayer` + `TimeoutLayer` middleware stack
- `tracing`, `opentelemetry`, `tracing-opentelemetry` — observability

Libraries intentionally avoided:
- `autoagents-core::prebuilt::ReActAgent` / `TurnEngine` — requires native LLM tool-calling; local models don't support this reliably
- `autoagents-derive` — tool inputs are `Value` over UDS; proc macros add no benefit
- `autoagents-telemetry` — own OTEL via `sdk-rust`
- embedded vector storage inside Cortex
- direct browser/memory/sandbox implementation inside Cortex
- `axum` or `tower` in any service other than Cortex

## Tools

`cortex.describe_boundary`

Returns a JSON document describing service ownership, transport contract, and implementation scope.

`cortex.step`

Request params:
| Param | Required | Description |
|-------|----------|-------------|
| `session_id` | ✅ | Stable session identifier |
| `user_input` | ✅ | Task description or user message |
| `agent_name` | optional | Named agent from `openagent.yaml`. Omit for default. Set to dispatch a worker. |
| `user_key` | optional | User key for research context lookup. Defaults to `session_id`. |
| `turn_kind` | optional | `"generation"` (default) or `"tool_call"` (skips tool injection). |

Behavior:
1. Loads `config/openagent.yaml` (or `OPENAGENT_CONFIG_PATH`)
2. Resolves agent by `agent_name` (falls back to first agent)
3. On generation turns: fetches active research via `research.status`, injects runnable tasks into system prompt; searches ActionCatalog for top-8 relevant tools; always pins `memory.search`, `research.status`, `cortex.step`
4. Runs full ReAct loop (up to 10 iterations): LLM → parse JSON output → tool dispatch over UDS → inject result → repeat
5. Returns `response_text` + `react_summary` (iterations, tool calls, candidates)

Response shape:
```json
{
  "session_id": "...",
  "agent_name": "supervisor",
  "provider_kind": "openai_compat",
  "model": "...",
  "response_type": "final",
  "response_text": "...",
  "react_summary": {
    "iterations": 3,
    "tool_calls_made": ["research.status", "cortex.step"],
    "default_tool_count": 11,
    "candidates": ["memory.search", "research.status", "cortex.step", "..."]
  }
}
```

Observability:
- traces: `logs/cortex-traces-YYYY-MM-DD.jsonl`
- metrics: `logs/cortex-metrics-YYYY-MM-DD.jsonl`
- logs: `logs/cortex-logs-YYYY-MM-DD.jsonl`

## Diary Layer

In addition to KB and episodic memory, the architecture includes a diary layer.

Purpose:
- human-readable audit trail
- request and response captured in English prose-like markdown
- searchable by HITL only
- never injected into normal agent reasoning context

Design:
- diary entry content stored as markdown on disk
- diary semantic index stored in LanceDB
- LanceDB diary index stores only reference-oriented summary fields, not full diary content

Recommended diary markdown contents:
- request
- response
- tool activity summary
- validator status
- optional flags

Recommended storage split:
- `md` is the human-readable source of truth
- `LanceDB` stores summary/index rows and file references for HITL semantic scan

Important rule:
- diary is excluded from normal Cortex retrieval and prompt hydration
- diary is only referenced by HITL scan/review workflows

Generation strategy:
- deterministic template only
- no extra LLM call for diary generation
- diary is dumped directly from request/response/tool state
- diary indexing can run asynchronously when the system is not under load

Suggested path shape:
- `data/diary/<session_id>/<timestamp>-<turn_id>.md`

## Prompt Management

Prompts should be runtime-loaded configuration, not compiled into Rust binaries.

Recommended pattern:
- YAML prompt files
- per-subsystem prompt folders
- template rendering at runtime
- versioned prompt metadata

Likely prompt families:
- planning
- memory compaction
- tool selection
- reflection
- contradiction review preparation

## Protocol

MCP-lite over JSON + UDS remains the preferred protocol.

Why:
- local machine
- low latency
- simple debugging
- language independence
- matches existing service direction in the repo

Recommended message shape:

```json
{
  "id": "uuid",
  "type": "tool.call",
  "tool": "browser.search",
  "params": {
    "query": "POLG mutation DNA repair"
  }
}
```

## Migration Strategy

This is an evolution, not a rewrite. Each phase is independently shippable.

**Phase 1 (now):**
- Keep Python outer loop
- Route each turn to Cortex via `cortex.step` MCP-lite call
- Python middleware (STT, whitelist) stays outside Cortex

**Phase 2 — Tower middleware begins:**
- Cortex owns tool routing (memory, browser, sandbox)
- Introduce `tower::ServiceBuilder` inside Cortex
- Begin porting Python middleware to `tower::Layer` (whitelist first, then STT/TTS)
- Python middleware removed as each Tower layer ships and passes integration tests

**Phase 3 — Cortex owns the loop:**
- Cortex owns the full ReAct loop (LLM → tool → LLM iterations)
- Python becomes a thin launcher: config load, service spawn, platform adapter glue
- Full Tower middleware stack active inside Cortex

**Stability guarantee:** The MCP-lite JSON over UDS socket contract is permanent. Downstream services never change protocol. Axum in `openagent` is external-facing only — it never replaces the UDS protocol between `openagent` and services.

## Scope for MVP

Do not build the full cognition stack at once.

The first useful Cortex should only do:
- receive session step request
- retrieve memory context
- call LLM
- execute tool call
- return result

Planning, reflection, contradiction handling, curiosity, and advanced memory lifecycle should come later.
