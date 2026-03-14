# Cortex TODO

Phased implementation plan for Cortex as the future Rust orchestrator service.

## Phase 0: Capture the Boundary

- Finalize Cortex as a separate service, not an embedded OpenAgent module.
- Keep current Python loop as a temporary shell.
- Treat Python middleware such as STT and whitelist as pre-Cortex middleware for now.
- Lock Cortex transport to MCP-lite over JSON + UDS.
- Define Cortex as the only component allowed to call the LLM in the target architecture.

## Phase 1: Cortex Skeleton MVP

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
- Python shell can send one message to Cortex and get one response back

## Phase 1B: AutoAgents Core Integration

Goal: replace Cortex's manual `reqwest` LLM calls and ad-hoc tool handling with AutoAgents as the execution framework. This is a foundational refactor that must land before Phase 2 (tool routing) to avoid rebuilding twice.

### Cargo.toml additions

- Add `autoagents-llm` — unified `LLMProvider` trait
- Add `autoagents-core` with features: `agent`, `tool`, `actor` (pulls in `ractor`)
- Add `autoagents-derive` — `#[tool]`, `#[derive(ToolInput)]` proc macros
- Do NOT add: `autoagents-protocol`, `autoagents-telemetry`, any `autoagents-core::memory` feature

### CortexAgent

- Define `CortexAgent` struct with fields: `config: AgentConfig`, `tools: Vec<Box<dyn ToolT>>`
- Implement `AgentDeriveT` manually (do not use `#[agent]` macro) — 4 methods, reads from `AgentConfig`
- Add `CortexAgent::from_config(cfg, clients)` constructor — validate eagerly at startup, panic with clear message on invalid config
- Wire `BaseAgent::new(cortex_agent, llm, Some(memory_adapter), tx, false)` in handler

### CortexMemoryAdapter

- Define `SegmentedStm` struct with 8 named segments and per-segment `max_tokens` budgets (system_core, active_objective, plan_snapshot, conversation, tool_log, scratchpad, observations, curiosity_queue)
- Implement `MemoryProvider` for `CortexMemoryAdapter`:
  - `get_messages()` — assemble segments → fetch LTM bundle from memory service → inject top-k action candidates as system message → return flat `Vec<ChatMessage>`
  - `add_message()` — route to correct STM segment by `ChatMessage` role; keep this path fast (no network calls)
- Implement `AgentHooks` for `CortexMemoryAdapter`:
  - `on_turn_complete()` — fire episode write to memory service (async, non-blocking); fire diary write
- Stub `McpLiteClient` calls for memory service — they will be fully wired in Phase 3

### Tool layer

- Add `MemorySearchTool` — thin `ToolT` wrapper, stubs MCP-lite call to memory service (Phase 3 wires it fully)
- Add `SandboxExecuteTool` — thin `ToolT` wrapper, stubs MCP-lite call to sandbox service
- Add `BrowserNavigateTool` — thin `ToolT` wrapper, stubs MCP-lite call to browser service
- Add `ActionDispatcherTool` — dynamic meta-tool: `name="action.call"`, `args={name: string, args: object}`; internally looks up `ActionCatalog` and routes over MCP-lite
- Use `#[derive(ToolInput)]` for all tool input structs
- Wire static tools + dispatcher into `CortexAgent::from_config()`

### LLM provider swap

- Replace manual `reqwest` HTTP block in `llm.rs` with `autoagents-llm::LLMBuilder`
- Support OpenAI-compat (Ollama) and Anthropic backends — select from `config/openagent.yaml` provider key
- Remove `llm.rs` once `LLMProvider` is wired into `BaseAgent`

### Multi-agent bootstrap

- Wrap `BaseAgent` in `ActorAgent` for each named agent in `config.agents`
- Register actors with `ractor` supervisor on startup
- Supervisor actor reads agent routing rules from config — dispatches `cortex.step` requests to correct worker actor by `agent_name` field
- Keep supervisor logic simple at this phase — routing by name match only

### Exit criteria

- `cortex.step` request flows through AutoAgents `BaseAgent` → `LLMProvider` → response
- All tests pass (update `test_cortex_provider.py` to reflect new step contract if needed)
- Manual `reqwest` LLM code deleted
- `CortexMemoryAdapter` passes unit tests for segment routing and `get_messages()` assembly
- Stub tools callable without live services (return fixed JSON)

---

## Phase 2: Tool Routing Baseline

Goal: let Cortex execute tools directly.

- Add `tool_router` module
- Add static tool registry first
- Add service client wrappers for:
  - memory
  - tool services such as `browser`
  - tool services such as `sandbox`
- Define structured LLM tool-call output contract
- Validate tool names and arguments before execution
- Append tool result back into the reasoning loop
- Record tool call telemetry

Exit criteria:
- Cortex can complete one LLM -> tool -> LLM round-trip

## Phase 3: Memory Retrieval and Episode Writes

Goal: make Cortex memory-aware.

- Add `memory_client` module
- Add unified memory search request contract
- Retrieve memory bundle before LLM reasoning
- Inject memory bundle into prompt assembly
- Add episodic memory write after significant results
- Capture LLM output after each completed cycle and run validator before downstream memory feedback
- Add deterministic diary write event after each completed tool cycle
- Add session-linked memory references in logs/telemetry

Exit criteria:
- Cortex can read from memory before reasoning, validate output, write an episode, and emit a diary event after execution

## Phase 4: Prompt System

Goal: externalize prompts and stop hardcoding cognitive instructions.

- Add prompt loader
- Use YAML prompt files
- Support runtime template rendering
- Add prompt version metadata
- Create initial prompt families:
  - step reasoning
  - tool selection
  - memory compaction handoff
  - plan update

Exit criteria:
- Cortex loads prompts from files without recompilation

## Phase 4A: Diary Store and Index

Goal: capture human-readable request/response history without polluting normal memory retrieval.

- Define diary markdown path convention
- Define deterministic diary template
- Persist request and response in markdown
- Add LanceDB diary index storing only:
  - entry id
  - session id
  - timestamp
  - short summary
  - keywords
  - file path
  - validator status
  - flags
- Ensure diary indexing is asynchronous and can be deferred when the system is under load
- Ensure diary search is only exposed to HITL/audit workflows

Exit criteria:
- Every completed cycle produces a deterministic markdown diary entry plus a LanceDB reference index row, and diary entries can be semantically scanned by HITL without being used in normal context injection

## Phase 5: Action Search

Goal: avoid exposing every tool and skill to the LLM at every step.

- Add `action_registry` module
- Treat action discovery as the main abstraction rather than direct service naming
- Define action metadata schema:
  - name
  - kind
  - summary
  - tags
  - owner
  - schema summary
  - embedding
- Add local skill loading from `skills/*/SKILL.md`
- Keep skills guidance-only first, then move to hybrid/executable skills later
- Add examples to skills later when vector rerank is introduced
- Add action embedding/index build process
- Implement top-k action search
- Ensure browser and sandbox register many tools through the same discovery path
- Pass only candidate action summaries into the LLM context on generation turns
- Keep deterministic tool-call turns free of reinjected action context

Exit criteria:
- Cortex can search actions semantically and expose only a limited candidate set

## Phase 6: Plan Store and DAG

Goal: give Cortex persistent control state.

- Add SQLite-backed plan store
- Add tables:
  - plans
  - tasks
  - task_dependencies
  - tool_calls
  - turns
  - sessions
- Add runnable-task selection
- Add plan snapshot injection into prompt
- Update plan after each tool call or step
- Keep a compact active plan summary in STM or step state

Exit criteria:
- Cortex can resume a multi-step task across turns

## Phase 7: Segmented STM

Goal: preserve working cognition shape instead of a flat buffer.

- Introduce segmented STM state:
  - system core
  - active objective
  - active plan snapshot
  - conversation context
  - tool interaction log
  - reasoning scratchpad
  - observation buffer
  - curiosity queue
- Add per-segment size budgets
- Define which segments compact and which never compact
- Keep STM local to Cortex-managed runtime state

Exit criteria:
- Cortex prompt assembly reads from segmented STM rather than one flat transcript

## Phase 8: Reflection

Goal: add background cognition after the main loop is stable.

- Add reflection scheduler
- Add cross-thread synthesis requests
- Add well-supported hypothesis generation
- Add research digest generation
- Add contradiction candidate generation for HITL

Exit criteria:
- Cortex can periodically synthesize research state without disrupting core task execution

## Phase 9: Curiosity and Investigation Queue

Goal: enable research collaborator behavior.

- Add curiosity queue generation
- Add confidence-gated autonomous exploration levels
- Keep suggestion output non-intrusive
- Present optional research leads rather than forcing direction changes

Exit criteria:
- Cortex can surface research leads as suggestions instead of direct interruptions

## Phase 10: Harden the Service Boundary

- Add retries/timeouts per dependent service
- Add degraded-mode behavior when memory or tool services are unavailable
- Add replay-friendly step logs
- Add trace correlation across LLM, tools, and memory
- Add protocol versioning notes

Exit criteria:
- Cortex survives partial subsystem failures without corrupting control state

## Tower Middleware Migration

Tower layers replace Python middleware progressively. Cortex is the only service that uses `tower`. Other services remain plain `tokio` daemons.

### Tower Phase 1 — introduce the stack (alongside Cortex Phase 2)

- Add `tower` and `tower-http` to `Cargo.toml`
- Wrap inner `ReActService` (or equivalent step handler) in a `tower::ServiceBuilder`
- Add `TraceLayer` (from `tower_http`) — one span per step request, correlates with existing OTEL traces
- Add `TimeoutLayer` — configurable per-step deadline (default 90s), replaces Python-side timeout
- Python STT/whitelist middleware stays on Python side at this stage — do not remove yet

### Tower Phase 2 — port Python middleware (alongside Cortex Phase 3)

- Implement `WhitelistLayer` — checks sender against whitelist before passing to inner service
- Implement `SttLayer` — transcribes audio payload if `content_type == audio/*`; passes text downstream
- Implement `TtsLayer` (post-processing) — converts text response to audio if session config requires it
- Wire all three into `ServiceBuilder` in correct order: Whitelist → STT → inner → TTS
- Remove corresponding Python middleware once each layer is tested end-to-end
- Add Rust integration tests for each layer in isolation (`tower_test` or `tokio::test`)

Layer composition pattern:
```rust
let svc = ServiceBuilder::new()
    .layer(TraceLayer::new_for_grpc())   // or custom UDS trace layer
    .layer(TimeoutLayer::new(Duration::from_secs(90)))
    .layer(WhitelistLayer::new(whitelist.clone()))
    .layer(SttLayer::new(stt_client.clone()))
    .service(react_service);
```

### Tower Phase 3 — Axum control plane (Phase 4 endgame)

- Add `axum` to `Cargo.toml`
- Replace raw UDS accept loop with `axum::serve` on `UnixListener`
- Map `POST /tool/:name` routes to existing Tower service stack
- Keep Tower middleware stack unchanged — Axum is the transport layer in front
- Update `McpLiteClient` in Python/sdk-go to use HTTP over UDS (one-line transport swap)
- Platform connectors (Discord, Telegram, Slack) wire directly to Cortex Axum endpoint
- Python process retired — only needed as a launch wrapper if `systemd` unit isn't used directly

## Deferred by Design

Not for early MVP:
- full contradiction arbitration
- concept canonicalization
- knowledge decay management inside Cortex
- splitting memory into multiple services
- dynamic distributed scheduling

## Immediate Next Steps

1. Create `src/main.rs` and service bootstrap
2. Define session step request/response contract
3. Implement LLM client boundary
4. Add static tool router for memory and tool services such as browser/sandbox
5. Wire Python shell to call Cortex instead of the old loop
