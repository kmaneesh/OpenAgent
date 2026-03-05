# Sandbox Service

MCP-lite wrapper for [microsandbox](https://microsandbox.dev) — executes code in VM-level isolated sandboxes.

## Prerequisites

1. **Microsandbox server** — must be running (e.g. `msb server start --dev`)
2. **API key** — `MSB_API_KEY` env var (run `msb server keygen`)

## Build

```bash
cd services/sandbox
cargo build --release
```

Binary: `target/release/sandbox`

For ServiceManager (binaries at repo root):

```bash
mkdir -p bin
cp target/release/sandbox ../../bin/sandbox-darwin-arm64   # macOS
# Or: GOOS=linux GOARCH=arm64 cargo build --release -o ../../bin/sandbox-linux-arm64
```

## Run

```bash
export MSB_API_KEY=your_key
export OPENAGENT_SOCKET_PATH=data/sockets/sandbox.sock
./target/release/sandbox
```

## Tools

- **sandbox.execute** — Execute Python code. Params: `language` (e.g. "python"), `code`.

## Env vars

| Var | Default | Description |
|-----|---------|-------------|
| `MSB_SERVER_URL` | http://127.0.0.1:5555 | Microsandbox server URL |
| `MSB_API_KEY` | (required) | API key from `msb server keygen` |
| `OPENAGENT_SOCKET_PATH` | data/sockets/sandbox.sock | MCP-lite Unix socket |
