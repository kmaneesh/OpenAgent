# OpenAgent

A **deterministic**, open-claw-style open agent with a minimal core and pluggable extensions. Built to run well with **offline 14B-parameter models** by moving most functionality into extensions and tools.

## What is OpenAgent?

OpenAgent is a small orchestrator that discovers and loads **extensions** (e.g. WhatsApp, Discord). Rich behavior lives in extensions and **tools** that the model can call, so the core stays minimal and a smaller local model can still deliver capable behavior by delegating to well-defined interfaces.

- **Deterministic** — Predictable, reproducible execution paths.
- **Extension-first** — Integrations and features live in installable extensions, not in the core.
- **Async-first** — All extensions use `async`/`await`; no blocking in extension code.
- **Tool-oriented** — Capabilities are exposed as tools for the 14B model to invoke.
- **Offline-friendly** — Designed for local inference with a 14B model.

## Requirements

- **Python 3.11+**

## Installation

Clone the repository and install the core and any extensions you need:

```bash
git clone https://github.com/kmaneesh/OpenAgent.git
cd OpenAgent

# Core (required)
pip install -e .

# Extensions (optional; install as needed)
pip install -e extensions/whatsapp
pip install -e extensions/discord
```

## Quick Start

Run the agent (loads all installed extensions):

```bash
python -m openagent.main
# or
openagent
```

Verify which extensions are registered:

```bash
python -c "import importlib.metadata as m; print(m.entry_points(group='openagent.extensions'))"
```

## Project Structure

```
OpenAgent/
├── src/openagent/          # Core: entry point, extension manager, interfaces
├── extensions/
│   ├── whatsapp/           # WhatsApp extension
│   └── discord/            # Discord extension
├── tests/                  # Tests for core and extensions
├── pyproject.toml          # Core package config
└── README.md
```

The core only discovers and initializes extensions; it does not contain domain logic. Each extension has its own `pyproject.toml` and registers via the `openagent.extensions` entry point group.

## Extensions

| Extension   | Description                    | Install from        |
|------------|---------------------------------|---------------------|
| **whatsapp** | WhatsApp integration (neonize) | `extensions/whatsapp` |
| **discord**  | Discord bot integration        | `extensions/discord`  |

Install with `pip install -e extensions/<name>`. Extensions are discovered at runtime; no need to register them in the core.

## Development

- **Run tests**

  From the repo root:

  ```bash
  pytest
  ```

- **Add a new extension**

  1. Create a package under `extensions/<name>/` with its own `pyproject.toml`.
  2. Declare an entry point in the `openagent.extensions` group pointing to a class that implements the `Extension` protocol (`initialize()` async).
  3. Depend on `openagent-core` and install with `pip install -e extensions/<name>`.

Extensions must be **first-class async**: use `async def` for lifecycle and handlers, and avoid blocking the event loop.

## License

See [LICENSE](LICENSE) in this repository.
