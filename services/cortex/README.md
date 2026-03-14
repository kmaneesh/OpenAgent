# Cortex Service

Rust service target for OpenAgent's cognitive control plane. Cortex is the future agent loop, planner, retrieval orchestrator, and tool router. It does not store all knowledge itself. Instead, it coordinates:

- LLM service
- memory service
- tool services

OpenAgent's current Python outer loop is treated as a temporary shell that will call Cortex. The final architecture keeps Cortex as a separate service, not embedded in the Python app.

## Phase 1 Status

Phase 1 is implemented as a minimal single-step reasoning service.

What exists now:
- Cortex is a standalone Rust MCP-lite service.
- Transport is locked to JSON over Unix Domain Sockets.
- Service identity is declared in `service.json`.
- `cortex.describe_boundary` reports the current service boundary.
- `cortex.step` performs one LLM-backed response step.
- The system prompt is loaded from `config/openagent.yaml` or `config/openagent.yml`.
- Traces, metrics, and structured logs are emitted for each Cortex step.

What does not exist yet:
- tool routing
- memory retrieval
- planner / DAG store
- STM segmentation

This keeps Phase 1 aligned with [`TODO.md`](./TODO.md): one message in, one LLM response out, no tools or planning yet.

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

Segmented STM design currently intended:
- system core
- active objective
- active plan snapshot
- conversation context
- tool interaction log
- reasoning scratchpad
- observation buffer
- curiosity queue

Only selected segments should be compacted.

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

Cortex uses [AutoAgents](https://github.com/liquidos-ai/AutoAgents) as its agent execution framework. The integration is selective — only the parts that strengthen Cortex without polluting its memory, protocol, or observability boundaries.

### Crates adopted

| Crate | Role in Cortex |
|---|---|
| `autoagents-llm` | Unified `LLMProvider` trait — replaces manual `reqwest` LLM calls. One trait covers OpenAI-compat, Anthropic, Ollama, etc. Built-in streaming and tool-calling. |
| `autoagents-core` | `BaseAgent`, `AgentExecutor`, `ToolT` trait, `ActorAgent` for multi-agent via `ractor`. The execution engine. |
| `autoagents-derive` | Proc macros where useful (`#[tool]`, `#[derive(ToolInput)]`). Not used for agent struct itself — see CortexAgent below. |

### Crates deliberately excluded

| Crate | Why excluded |
|---|---|
| `autoagents-core::memory` | Cortex has a segmented STM architecture (8 segments, per-segment budgets, LTM retrieval over MCP-lite). AutoAgents' `SlidingWindowMemory` is a flat buffer and would pollute this design. |
| `autoagents-protocol` | OpenAgent uses MCP-lite (tagged JSON over UDS). AutoAgents' protocol types (`ActorID`, `Event`, `SubmissionId`) are incompatible and unnecessary. |
| `autoagents-telemetry` | OpenAgent has its own OTEL pipeline via `sdk-rust` (file-based OTLP/JSON, daily rotation). Mixing two OTEL setups in one process causes provider conflicts. |

---

### CortexAgent — single generic struct, config-driven

Cortex does not use the `#[agent]` proc macro. A single `CortexAgent` struct implements `AgentDeriveT` manually, reading everything from `config/openagent.yaml` at construction time. Adding a new agent is an edit to YAML — no Rust recompile.

```rust
pub struct CortexAgent {
    config: AgentConfig,       // from openagent.yaml: name, description, system_prompt
    tools: Vec<Box<dyn ToolT>>, // built from config at construction
}

impl AgentDeriveT for CortexAgent {
    type Output = CortexOutput;
    fn name(&self) -> &str { &self.config.name }
    fn description(&self) -> &str { &self.config.description }
    fn tools(&self) -> Vec<Box<dyn ToolT>> { self.tools.clone() }
    fn output_schema(&self) -> Option<Value> { None }
}

impl CortexAgent {
    pub fn from_config(cfg: &AgentConfig, clients: &ServiceClients) -> Self {
        // Validates eagerly at startup — panics with clear message on bad config
        // Never fails silently mid-run
    }
}
```

---

### Tool Architecture — static set + one dynamic dispatcher

The LLM always sees a small, fixed tool set. Exposing the full service catalog per turn is expensive in tokens and unreliable on sub-30B models.

**Static tools** (always in LLM context, always needed):

```rust
// Each wraps a MCP-lite call over UDS — AutoAgents never sees the socket
Box::new(MemorySearchTool::new(clients.memory.clone()))
Box::new(SandboxExecuteTool::new(clients.sandbox.clone()))
Box::new(BrowserNavigateTool::new(clients.browser.clone()))
```

**One dynamic dispatcher** (for everything else):

```rust
pub struct ActionDispatcherTool {
    catalog: Arc<ActionCatalog>,  // loaded from services/*/service.json at boot
}

// LLM calls: action.call(name="browser.search", args={...})
// Dispatcher looks up catalog, routes over MCP-lite
impl ToolT for ActionDispatcherTool {
    fn name(&self) -> &str { "action.call" }
    fn description(&self) -> &str {
        "Call any available action by name. Available actions are listed in your context."
    }
    fn args_schema(&self) -> Value { /* { name: string, args: object } */ }
}
```

**No two-step search.** Candidate action summaries are injected into the system prompt by `CortexMemoryAdapter::get_messages()` at turn start — the LLM reads them in context and calls `action.call` directly. This avoids the `search → call` two-step that fails on smaller models.

---

### CortexMemoryAdapter — own MemoryProvider implementation

Cortex implements AutoAgents' `MemoryProvider` trait directly. AutoAgents calls the two methods; the implementation hides all STM/LTM complexity behind them.

```rust
pub struct CortexMemoryAdapter {
    stm: SegmentedStm,              // 8 segments, per-segment budgets (local, in-process)
    memory_client: McpLiteClient,   // → memory service over UDS
    action_catalog: Arc<ActionCatalog>,
    session_id: String,
}

impl MemoryProvider for CortexMemoryAdapter {
    async fn get_messages(&self) -> Vec<ChatMessage> {
        // 1. Assemble STM segments (system core, objective, plan snapshot,
        //    conversation context, tool log, scratchpad, observations, curiosity)
        // 2. Fetch LTM bundle from memory service over MCP-lite
        // 3. Inject top-k action candidate summaries as a system message
        // 4. Respect per-segment size budgets — compact if over limit
        // 5. Return flat Vec<ChatMessage> — AutoAgents sees nothing unusual
    }

    async fn add_message(&mut self, message: ChatMessage) {
        // 1. Route to correct STM segment by role and content type
        //    (assistant → conversation context; tool → tool interaction log; etc.)
        // 2. Keep heavy async writes (episode, diary) out of this hot path
    }
}

// Diary and LTM writes happen at turn boundary, not per-message
impl AgentHooks for CortexMemoryAdapter {
    async fn on_turn_complete(&self, result: &TurnResult) {
        // Fire episode write to memory service (MCP-lite)
        // Fire deterministic diary write (markdown + LanceDB index row)
        // Both are async and non-blocking to the main loop
    }
}
```

---

### Multi-agent — ractor actor model

Named agents from `config/openagent.yaml` (supervisor, worker-search, worker-code, etc.) each run as `ractor` actors inside the Cortex process. The supervisor actor dispatches tasks to worker actors via typed `ractor` messages. AutoAgents' `ActorAgent` wrapper is used directly.

```
ractor supervisor actor
    ├── worker-search actor  (CortexAgent, search-tuned prompt)
    ├── worker-code actor    (CortexAgent, code-tuned prompt)
    └── worker-memory actor  (CortexAgent, memory-tuned prompt)
```

Each actor is a `CortexAgent` constructed from its YAML block. Adding a worker agent = one new entry in `agents:` YAML.

---

## Phase 1 Library Set

Libraries used now:
- `sdk-rust` for the MCP-lite server and shared OTEL setup
- `tokio` for the async service runtime
- `serde` and `serde_json` for protocol payloads
- `serde_yaml` for loading the OpenAgent config file
- `anyhow` for process-level error handling
- `reqwest` with `rustls-tls` for async LLM HTTP calls (transitional — replaced by `autoagents-llm`)
- `tracing`, `opentelemetry`, and `tracing-opentelemetry` for observability

Libraries being added (AutoAgents integration):
- `autoagents-llm` — replaces manual `reqwest` LLM calls
- `autoagents-core` — agent execution, tool trait, actor model
- `autoagents-derive` — proc macros for tool input/output types
- `ractor` — actor runtime for multi-agent (pulled in via `autoagents-core`)

Libraries planned for later phases:
- `uuid` for request/session correlation where the service generates identifiers
- `tower` + `tower-http` — middleware stack (Phase 2); replaces Python middleware chain layer-by-layer
- `axum` — HTTP/UDS control plane transport (Phase 4 endgame only); do not add before Phase 4

Libraries intentionally avoided:
- `autoagents-core::memory` implementations — own `MemoryProvider` impl
- `autoagents-protocol` — own MCP-lite protocol
- `autoagents-telemetry` — own OTEL via `sdk-rust`
- agent frameworks other than AutoAgents
- embedded vector storage inside Cortex
- direct browser/memory/sandbox implementation inside Cortex
- `tower` or `axum` in any service other than Cortex

## Phase 1 Tools

`cortex.describe_boundary`

Returns a JSON document describing:
- service ownership
- non-goals for Phase 0
- the transport contract
- the Phase 1 dependency set

`cortex.step`

Request:
- `session_id`
- `user_input`
- `agent_name` (optional)

Behavior:
- loads OpenAgent config from `OPENAGENT_CONFIG_PATH`, `config/openagent.yml`, or `config/openagent.yaml`
- selects the requested agent or falls back to the first configured agent
- reads `system_prompt` from config
- sends `system_prompt` + `user_input` to the configured provider
- returns plain response text plus provider metadata

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

**Phase 4 — Axum control plane (endgame):**
- `axum` over UDS replaces the Python process
- Platform connectors (Discord, Telegram, Slack) wire directly to Cortex/Axum
- `service.json` manifest and MCP-lite protocol unchanged for all other services
- Python retired

**Stability guarantee:** The MCP-lite UDS socket contract is stable across all phases. Downstream services never change protocol because the control plane above them is being replaced.

## Scope for MVP

Do not build the full cognition stack at once.

The first useful Cortex should only do:
- receive session step request
- retrieve memory context
- call LLM
- execute tool call
- return result

Planning, reflection, contradiction handling, curiosity, and advanced memory lifecycle should come later.
