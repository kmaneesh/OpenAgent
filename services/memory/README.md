# Agentic Memory MCP-lite Service

MCP-lite service for "Agentic Memory" using **LanceDB** and **FastEmbed-rs**. Two vector stores:

- **LTS** — Long-term store for summaries
- **STS** — Short-term store for full conversation chains

## Tech Stack

| Component | Choice |
|-----------|--------|
| Protocol | MCP-lite (sdk-rust, Unix socket) |
| Vector engine | `lancedb` (native Rust, serverless file-based) |
| Embeddings | `fastembed` (pure Rust, local ONNX) |
| Runtime | `tokio` |
| Tracing | OpenTelemetry (file-based, same as sandbox/browser) |

## Prerequisites

- **Rust** 1.70+
- **protoc** (Protocol Buffers compiler) — required by LanceDB:
  ```bash
  brew install protobuf   # macOS
  apt install protobuf-compiler  # Linux
  ```

## Build

```bash
# From project root
make memory        # cross-compile
make local         # build for current host only
```

Or directly:

```bash
cd services/memory
cargo build --release
```

Binaries land in `bin/memory-<platform>` (e.g. `bin/memory-darwin-arm64`).

## Run

The service is managed by OpenAgent's **ServiceManager**. It reads `service.json`, spawns the binary, and connects via Unix socket at `data/sockets/memory.sock`.

To run standalone (e.g. for testing):

```bash
OPENAGENT_SOCKET_PATH=data/sockets/memory.sock \
OPENAGENT_MEMORY_PATH=./data/memory \
./bin/memory-darwin-arm64
```

### Environment

| Variable | Default | Description |
|----------|---------|-------------|
| `OPENAGENT_SOCKET_PATH` | `data/sockets/memory.sock` | Unix socket path |
| `OPENAGENT_MEMORY_PATH` | `./data/memory` | LanceDB storage directory |
| `OPENAGENT_LOGS_DIR` | `logs` | OTEL trace files |
| `RUST_LOG` | `info` | Log level |

## Deterministic API

Same pattern as sandbox and browser: sync handlers, JSON params, string result.

### `memory.index_trace`

Index a trace into LTS or STS.

| Param | Type | Required | Description |
|-------|------|----------|-------------|
| `content` | string | yes | Content to embed and store |
| `metadata` | object | no | Optional metadata (session_id, source, etc.) |
| `store` | string | yes | `"lts"` or `"sts"` |

Returns: `{"id":"<uuid>","store":"lts"}` or `{"error":"..."}` on failure.

### `memory.search_memory`

Semantic search over LTS and/or STS. Top 5 results as JSON.

| Param | Type | Required | Description |
|-------|------|----------|-------------|
| `query` | string | yes | Query string for semantic search |
| `store` | string | no | `"lts"`, `"sts"`, or `"all"` (default) |

Returns: JSON array of `{id, content, metadata, created_at, store}` or `{"error":"..."}` on failure.

## Failure as a Result

On database errors, embedding failures, or invalid params, the service returns a JSON error string instead of panicking:

```json
{"error": "description of what went wrong"}
```

## Embedding Model

Uses **BAAI/bge-small-en-v1.5** (384 dimensions) via FastEmbed. Model is downloaded on first run and cached locally. No external API calls.
