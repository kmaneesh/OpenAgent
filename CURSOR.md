# OpenAgent — Cursor Project Context

## What We're Building

**OpenAgent** is a **deterministic**, **open-claw-style** open agent: a small core orchestrator that offloads most behavior into **extensions** and **tools**. The design targets compatibility with **offline 14B-parameter models** by keeping the core logic minimal and moving heavy or domain-specific work into pluggable, discoverable components.

## Design Principles

1. **Deterministic behavior** — The agent’s decisions and execution paths are predictable and reproducible where possible, which helps debugging and aligns with smaller local models.
2. **Extension-first** — First-class features (Discord, WhatsApp, integrations, toolkits) live in **extensions**, not in the core. The core only discovers, loads, and coordinates them.
3. **First-class async** — All extensions are async-first. Extension lifecycle, handlers, and tool invocations use `async`/`await`; no synchronous blocking in extension code. The core supports async discovery and initialization.
4. **Tool-oriented** — Rich functionality is exposed as **tools** that the 14B model can call. This reduces the need for large in-context reasoning and keeps the model’s role focused on planning and tool use.
5. **Offline-friendly** — The system is built to run with a local 14B model. Extensions and tools provide structure and capabilities so the smaller model can behave like a larger one by delegating to well-defined interfaces.

## Reference: OpenClaw

OpenClaw reference code lives at **`/Users/maneesh/Downloads/openclaw`**. When implementing agent logic, orchestration, or tool/extension patterns, refer to that TypeScript codebase for logic and guidance. Use it as the source of truth for behavior and structure; reimplement patterns in Python as needed for this project.

## Repository Layout

- **`src/openagent/`** — Core package: entry point, extension manager, and shared interfaces. No domain logic; only discovery and lifecycle.
- **`extensions/*`** — Independently installable extensions (e.g. `whatsapp`, `discord`). Each has its own `pyproject.toml` and registers via the `openagent.extensions` entry point group.
- **`tests/`** — Tests for core and for each extension.

## Files to change: extensions/discord

When editing the Discord extension, change only files under **`extensions/discord/`**:

- **`extensions/discord/pyproject.toml`** — Package metadata, dependencies, entry point (`discord_plugin:DiscordExtension`), and py-modules.
- **`extensions/discord/src/discord_plugin.py`** — Extension entry point; implements `Extension` (e.g. `DiscordExtension`).
- **`extensions/discord/src/discord_connector.py`** — Discord connection / client logic.
- **`extensions/discord/src/discord_bridge.py`** — Bridge between OpenAgent and Discord (events, messages).
- **`extensions/discord/src/discord_schema.py`** — Data structures / schemas for Discord payloads.
- **`extensions/discord/tests/conftest.py`** — Pytest fixtures for Discord tests.
- **`extensions/discord/tests/test_plugin.py`** — Tests for the plugin/extension.
- **`extensions/discord/tests/test_connector.py`** — Tests for the connector.
- **`extensions/discord/tests/test_bridge.py`** — Tests for the bridge.

Do not change core (`src/openagent/`) or other extensions when working on Discord.

## Extension Contract

Extensions implement the `Extension` protocol from `openagent.interfaces` and are **first-class async**:

- `initialize()` is async (e.g. `async def initialize(self) -> None`) — Run startup logic when the extension is loaded. No blocking calls; use `await` for I/O or other async work.
- Handlers, tool implementations, and any extension entry points are async. The core awaits them; extensions must not block the event loop.

Discovery is done via Python entry points (`openagent.extensions`). The core does not hard-code extension names; it discovers whatever is installed.

## Development Conventions

- **Python ≥ 3.11**
- Core: `pip install -e .` then run with `python -m openagent.main` or `openagent`
- Extensions: install with `pip install -e extensions/<name>` (e.g. `extensions/whatsapp`, `extensions/discord`)
- Verify registration:  
  `python -c "import importlib.metadata as m; print(m.entry_points(group='openagent.extensions'))"`

## When Editing This Project

- **Core** — Keep it minimal. Add orchestration, discovery, and interfaces; avoid domain-specific logic and heavy dependencies.
- **New features** — Prefer a new extension under `extensions/` and new tools within that extension (or a dedicated tools extension) rather than expanding the core.
- **Async only** — Extensions must be first-class async: use `async def` for lifecycle and handlers, and avoid synchronous blocking (e.g. no blocking HTTP or sleep in extension code).
- **Determinism** — When adding behavior, prefer explicit, reproducible flows and tool calls over non-deterministic or opaque steps.
- **14B target** — Design tools and prompts so a 14B offline model can reliably choose and invoke tools; keep tool schemas and docstrings clear and stable.
