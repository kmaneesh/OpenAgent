# AGENTS.md

This file defines how coding agents should work in this repository.

## Mission

Build OpenAgent as a deterministic, extension-first Python agent platform that works well with offline 14B-class models.

## Source Of Truth

- Project context and intent: [`CURSOR.md`](./CURSOR.md)
- Behavioral reference implementation: `/Users/maneesh/Downloads/openclaw`

When in doubt, follow `CURSOR.md` first, then align implementation patterns with OpenClaw where applicable.

## Architecture Rules

1. Keep core minimal.
- Core lives in `src/openagent/`.
- Core is responsible for orchestration, extension discovery, lifecycle, and shared interfaces only.
- Do not add domain-specific logic or heavy third-party dependencies to core.

2. Prefer extensions for features.
- New capabilities should be implemented under `extensions/<name>/`.
- Each extension must be independently installable and versioned.
- Register extensions via entry points in the `openagent.extensions` group.
- Extensions must be first-class async and event-driven by default.
- Design extension runtimes around async event handlers/callbacks/queues, not polling-only synchronous flows.

3. Tool-oriented design.
- Expose complex functionality as explicit tools/functions in extensions.
- Keep tool contracts stable, clear, and deterministic.

4. Deterministic behavior by default.
- Prefer explicit control flow over hidden side effects.
- Keep initialization and execution paths reproducible and testable.

## Python And Packaging

- Minimum supported Python: `>=3.11`
- Core package name: `openagent-core`
- Use editable installs for local development:
  - `pip install -e .`
  - `pip install -e extensions/<name>`

## Repository Layout Expectations

- `src/openagent/`: core orchestrator, manager, interfaces
- `extensions/*`: independent extension packages
- Extension source layout must be flat at `extensions/<name>/src/` (for example `plugin.py`, `handlers.py`, `builders.py` directly in `src/`)
- Do not create nested package folders like `extensions/<name>/src/<extension_name>/`
- `tests/`: mirrors application and extension structure
- `data/`: shared runtime storage (sqlite/session/artifacts)

## Naming Rules

- Use **extension** as the canonical term in architecture, APIs, docs, and tests.
- Keep `plugin.py` only as the per-extension entrypoint filename convention.
- In core runtime code, prefer `load_extensions` and extension-oriented naming. `load_plugins` is compatibility-only.

## Agent File Rule

- `AGENTS.md` must remain present and non-empty.
- Do not replace this file with placeholders or empty sections.

## Coding Standards

- Favor small, composable modules and clear interfaces.
- Add type hints for public APIs.
- Avoid introducing global mutable state unless necessary.
- Keep logging/output concise and useful for debugging.
- Use ASCII by default unless file/content already requires Unicode.
- Every I/O or network operation must be non-blocking.
- Use `aiohttp` for external HTTP/API calls.
- Use `asyncio.to_thread(...)` when integrating legacy synchronous SDK/file/network code inside async flows.

## Testing Standards

- Every new core behavior should include tests under `tests/openagent/`.
- Every extension should include tests under `tests/extensions/<name>/`.
- Tests should cover:
  - extension discovery/loading behavior
  - extension initialization behavior
  - deterministic execution of key paths

## Change Discipline

- Do not break entry-point based discovery.
- Do not hard-code extension names in core.
- Prefer backward-compatible interface evolution.
- If behavior differs from OpenClaw-inspired patterns, document why in code comments or PR notes.

## Agent Workflow

1. Read `CURSOR.md` before substantial changes.
2. Implement minimal viable changes in core; push feature logic to extensions.
3. Update/add tests in the mirrored `tests/` tree.
4. Keep docs in sync (`README.md`, extension metadata, and usage commands).
