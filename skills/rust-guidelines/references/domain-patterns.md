# Domain-Specific Rust Patterns

Constraints, crate choices, and design implications for common Rust application domains.

---

## Web Services (axum / actix-web)

### Domain Constraints

| Domain Rule | Design Constraint | Rust Implication |
|---|---|---|
| Stateless HTTP | No request-local globals | State in extractors |
| Concurrency | Many connections simultaneously | Async, `Send + Sync` |
| Latency SLA | Fast response | Efficient ownership, avoid clone |
| Security | Input validation | Type-safe extractors |
| Observability | Request tracing | `tracing` + Tower layers |

### Critical Rules
- **Handlers must not block** — `spawn_blocking` for any CPU-intensive work
- **Shared state must be thread-safe** — `Arc<T>` (read-only), `Arc<RwLock<T>>` (mutable)
- **Resources live for request duration** — use extractors, not globals

### Framework Comparison

| Framework | Style | Best For |
|---|---|---|
| axum | Functional, tower-native | Modern APIs, composable middleware |
| actix-web | Actor-based | High-performance, battle-tested |
| warp | Filter composition | Composable, functional style |
| rocket | Macro-driven | Rapid development |

### Key Crates
- `axum` / `actix-web` — web framework
- `tower` / `tower-http` — middleware (CORS, tracing, rate limit)
- `sqlx` — async SQL with compile-time query checking
- `jsonwebtoken` — JWT auth
- `validator` — struct validation

---

## Embedded / no_std

### Domain Constraints

| Domain Rule | Design Constraint | Rust Implication |
|---|---|---|
| No heap | Stack allocation only | `heapless`, no `Box`/`Vec` |
| No std | Core library only | `#![no_std]` |
| Real-time | Predictable timing | No dynamic allocation |
| Resource limited | Minimal memory | Static buffers, `arrayvec` |
| Hardware safety | Safe peripheral access | HAL + ownership |
| Interrupt safe | No blocking in ISR | Atomics, critical sections |

### Critical Rules
- **No dynamic allocation** — use `heapless::Vec<T, N>`, fixed-size arrays
- **Interrupt-safe shared state** — `Mutex<RefCell<T>>` + critical section
- **Peripheral ownership** — HAL takes ownership at init; singletons prevent conflicts

### Key Crates
- `embassy` — async embedded framework
- `rtic` — real-time interrupt-driven concurrency
- `heapless` — stack-allocated collections
- `embedded-hal` — hardware abstraction traits
- `cortex-m` / `esp-idf-hal` — MCU-specific HALs

---

## CLI Tools

### Domain Constraints

| Domain Rule | Design Constraint | Rust Implication |
|---|---|---|
| User ergonomics | Clear help, useful errors | `clap` derive macros |
| Config precedence | CLI > env > file > defaults | Layered config loading |
| Exit codes | Non-zero on error | `main() -> Result<()>` |
| Stdout/stderr | Data vs error output | `eprintln!` for errors |
| Interruptible | Handle Ctrl+C | Signal handling |

### Critical Rules
- **Errors to stderr, data to stdout** — enables piping and scripting
- **Config layering**: CLI args → env vars → config file → defaults
- **Non-zero exit on any error** — script and automation compatibility

### Key Crates
- `clap` — argument parsing (derive API)
- `dialoguer` — interactive prompts
- `indicatif` — progress bars
- `colored` / `owo-colors` — terminal color
- `figment` / `config` — layered configuration
- `ratatui` — terminal UI

---

## Cloud-Native / Kubernetes

### Domain Constraints

| Domain Rule | Design Constraint | Rust Implication |
|---|---|---|
| 12-Factor | Config from environment | Env-based config, no hardcodes |
| Observability | Metrics + traces + logs | `tracing` + `opentelemetry` |
| Health checks | Liveness/readiness endpoints | Dedicated routes |
| Graceful shutdown | Clean SIGTERM handling | Signal handler + drain |
| Horizontal scale | Stateless design | No node-local state |
| Container-friendly | Small binaries | `musl`, `strip = true`, LTO |

### Critical Rules
- **Stateless** — no `static mut`, all state external (DB, Redis)
- **Handle SIGTERM** — `tokio::signal` + drain in-flight requests before exit
- **Structured logs** — JSON output, `tracing-subscriber` with `json` feature

### Key Crates
- `tonic` — gRPC server/client
- `opentelemetry` + `tracing-opentelemetry` — distributed tracing
- `kube` — Kubernetes client
- `axum` — HTTP health + metrics endpoints

---

## Fintech / Financial Systems

### Domain Constraints

| Domain Rule | Design Constraint | Rust Implication |
|---|---|---|
| Audit trail | Immutable records | `Arc<T>`, event sourcing |
| Precision | No floating point for money | `rust_decimal::Decimal` |
| Consistency | Transaction boundaries | Clear ownership, DB transactions |
| Compliance | Complete structured logging | `tracing` with all fields |
| Reproducibility | Deterministic execution | No race conditions, seeded RNG |

### Critical Rules
- **Never `f64` for money** — `0.1 + 0.2 != 0.3`; use `Decimal` or `i64` (cents)
- **All transactions immutable and traceable** — event sourcing, audit log
- **Deterministic** — avoid non-deterministic ordering or concurrency bugs

### Key Crates
- `rust_decimal` — decimal arithmetic
- `chrono` / `time` — date/time
- `sqlx` — type-checked SQL queries
- `uuid` — transaction IDs

---

## IoT / Connected Devices

### Domain Constraints

| Domain Rule | Design Constraint | Rust Implication |
|---|---|---|
| Unreliable network | Offline-first | Local buffering, retry with backoff |
| Power constraints | Efficient code | Sleep modes, minimal allocation |
| Resource limits | Small footprint | `no_std` where needed |
| Security | Encrypted comms | TLS, signed firmware |
| Reliability | Self-recovery | Watchdog timer, error handling |
| OTA updates | Safe upgrades | Rollback capability |

### Critical Rules
- **Assume network failure** — local queue + retry with exponential backoff
- **Minimize power** — avoid busy-wait; use sleep modes and interrupts
- **Secure by default** — all comms TLS, verify firmware signatures

### Key Crates
- `rumqttc` — MQTT (std); `mqtt-sn` for constrained devices
- `embassy` — async embedded
- `reqwest` — HTTP with TLS
- `tokio-tungstenite` — WebSocket

---

## Machine Learning / Inference

### Domain Constraints

| Domain Rule | Design Constraint | Rust Implication |
|---|---|---|
| Large data | Efficient memory | Zero-copy, streaming |
| GPU acceleration | CUDA/Metal support | `candle`, `tch-rs` |
| Model portability | Standard formats | ONNX |
| Batch processing | Throughput over latency | Batched inference |
| Numerical precision | Float handling | `ndarray`, careful `f32`/`f64` |
| Reproducibility | Deterministic | Seeded RNG, versioning |

### Critical Rules
- **Avoid copying large tensors** — use references, views, in-place ops
- **Batch for GPU** — minimize kernel launch overhead
- **Separate IO from compute** — async data loading, sync/CPU compute

### Key Crates
- `candle` — Hugging Face Rust ML framework
- `tch-rs` — LibTorch bindings
- `ort` — ONNX Runtime
- `ndarray` — N-dimensional arrays
- `rayon` — data-parallel iteration

---

## Coding Style Quick Reference (50 rules condensed)

| Category | Rule |
|---|---|
| **Naming** | No `get_` prefix — `fn name()` not `fn get_name()` |
| **Naming** | Iterator convention: `iter()` / `iter_mut()` / `into_iter()` |
| **Naming** | Conversion: `as_` (cheap borrow), `to_` (expensive), `into_` (ownership transfer) |
| **Naming** | Treat acronyms as words: `Uuid` not `UUID`, `HttpClient` not `HTTPClient` |
| **Data types** | Newtypes for domain semantics: `struct Email(String)` |
| **Data types** | Pre-allocate: `Vec::with_capacity()`, `String::with_capacity()` |
| **Strings** | Prefer `s.bytes()` over `s.chars()` when ASCII-only |
| **Strings** | Use `Cow<str>` when data might be borrowed or owned conditionally |
| **Error handling** | Use `?` propagation, not `try!()` |
| **Error handling** | `.expect()` over `.unwrap()` — message describes the invariant |
| **Memory** | Meaningful lifetime names: `'src`, `'ctx` not just `'a` |
| **Memory** | `try_borrow()` for `RefCell` to avoid panic |
| **Concurrency** | Lock-free first: `AtomicT` for counters and flags |
| **Concurrency** | Prefer channels over shared mutable state |
| **API** | `#[must_use]` on `Result`-returning functions |
| **API** | `#[non_exhaustive]` on public enums to allow future variants |
| **Docs** | Every public item gets `///` doc comment |
| **Docs** | `# Examples`, `# Errors`, `# Panics`, `# Safety` sections as applicable |
