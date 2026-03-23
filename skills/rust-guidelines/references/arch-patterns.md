# Rust Architecture Patterns

Architecture guardrails, NEVER/ALWAYS lists, workspace decisions, and domain-specific adaptations for production Rust systems.

---

## NEVER DO: Critical Prohibitions

### 1. Never Use f64/f32 for Money
```rust
// ❌ NEVER
struct Account { balance: f64 }  // Float precision errors!

// ✅ ALWAYS
use rust_decimal::Decimal;
struct Account { balance: Decimal }  // Or i64 for cents
```

### 2. Never Unwrap in Library Code
```rust
// ❌ NEVER
let value = result.unwrap();

// ✅ ALWAYS — return Result<T, E> and let the caller decide
```

### 3. Never Clone Without Justification
```rust
// ❌ NEVER — arbitrary .clone() everywhere
// ✅ ALWAYS — use &T when possible, document why clone is needed
```

### 4. Never Ignore Errors with `let _ =`
```rust
// ❌ NEVER
let _ = fs::write("config.json", data);  // Silent failure!

// ✅ ALWAYS
fs::write("config.json", data)
    .context("Failed to write config file")?;
```

### 5. Never Block Async Runtime
```rust
// ❌ NEVER
async fn bad() { std::thread::sleep(Duration::from_secs(1)); }  // Blocks executor!

// ✅ ALWAYS
async fn good() { tokio::time::sleep(Duration::from_secs(1)).await; }
```

### 6. Never Default to Arc<Mutex<T>>
```rust
// ❌ DON'T default to this
struct App { counter: Arc<Mutex<i32>> }

// ✅ Prefer simpler alternatives first
use std::sync::atomic::{AtomicI32, Ordering};
struct App { counter: AtomicI32 }  // Lock-free, faster

// ✅ Only use RwLock/Mutex when truly needed
struct App { cache: Arc<RwLock<HashMap<String, Data>>> }  // Justified
```

### 7. Never Accept String When &str Suffices
```rust
// ❌ NEVER — unnecessary allocation
fn validate(input: String) -> bool { !input.is_empty() }

// ✅ ALWAYS
fn validate(input: &str) -> bool { !input.is_empty() }
```

### 8. Never Write unsafe Without SAFETY Comments
```rust
// ❌ NEVER
unsafe { *ptr = value; }

// ✅ ALWAYS
// SAFETY: ptr is valid, aligned, initialized, exclusive access guaranteed.
unsafe { *ptr = value; }
```

### 9. Never Use Stringly-Typed APIs
```rust
// ❌ NEVER
fn set_status(status: &str) { /* accepts any string */ }

// ✅ ALWAYS
#[derive(Debug, Clone, Copy)]
pub enum Status { Active, Inactive, Pending }
fn set_status(status: Status) { /* compile-time safety */ }
```

### 10. Never Collect When Iteration Suffices
```rust
// ❌ NEVER — intermediate allocation
let doubled: Vec<_> = nums.iter().map(|x| x * 2).collect();
for n in doubled { println!("{n}"); }

// ✅ ALWAYS
for n in nums.iter().map(|x| x * 2) { println!("{n}"); }
```

### 11. Never Add Errors Without Context
```rust
// ❌ NEVER — what file? where? why?
File::open(path)?

// ✅ ALWAYS
File::open(path)
    .with_context(|| format!("Failed to open config: {}", path.display()))?
```

### 12. Never Return References to Local Data
```rust
// ❌ NEVER — dangling reference
fn get_string() -> &str { let s = String::from("hello"); &s }

// ✅ ALWAYS
fn get_string() -> String { String::from("hello") }
fn get_static() -> &'static str { "hello" }
```

### 13. Never Use transmute Without repr(C)
```rust
// ❌ NEVER — undefined layout, UB
struct Foo { x: u32, y: u64 }
let bytes: [u8; 12] = unsafe { std::mem::transmute(foo) };

// ✅ ALWAYS
#[repr(C)] struct Foo { x: u32, y: u64 }
// Or use safe alternatives: to_ne_bytes(), bytemuck, etc.
```

### 14. Never Interpolate User Input in SQL
```rust
// ❌ NEVER — SQL injection
let query = format!("SELECT * FROM users WHERE id = {}", user_id);

// ✅ ALWAYS
sqlx::query!("SELECT * FROM users WHERE id = $1", user_id)
    .fetch_one(&pool).await?;
```

### 15. Never Hold std::sync::Mutex Across .await
```rust
// ❌ NEVER — deadlocks the executor thread
let guard = mutex.lock().unwrap();
some_async_op().await;  // Guard still held!

// ✅ ALWAYS — drop before awaiting, or use tokio::sync::Mutex
let value = { mutex.lock().unwrap().clone() };
some_async_op().await;
```

---

## ALWAYS DO: Mandatory Best Practices

### Memory Safety
- **Borrow over clone** — `&T` when reading, own when transforming, document clone necessity
- **Use smart pointers appropriately** — `Box` for heap, `Rc` for single-thread sharing, `Arc` for multi-thread
- **Checked arithmetic** — `a.checked_add(b).ok_or(Error::Overflow)?` for critical calculations
- **with_capacity** — pre-allocate when size is known to avoid reallocations
- **Document all unsafe blocks** with SAFETY comments explaining preconditions

### Error Handling
- **Use `thiserror` for library errors** — derive `Error` with `#[from]` conversions and descriptive messages
- **Use `anyhow` for application errors** — not for libs, never mix both in one crate
- **Always propagate with `?`** — keep error chains intact
- **Always add context** — `.with_context(|| ...)` at every call site boundary
- **Test error paths** — `assert!(matches!(result, Err(MyError::NotFound)))`

### Testing
- **Write tests before implementation** (TDD) — failing test → minimum impl → refactor
- **Test edge cases** — zero, empty, negative, overflow, concurrency boundaries
- **Property-based testing** with `proptest` for complex logic
- **Integration tests** in `tests/` directory for public API surface
- **Use `#[tokio::test]`** for async tests; `#[should_panic(expected = "...")]` for panics
- **Target >80% coverage** — measure with `cargo-tarpaulin`

### Code Quality
- **Run `cargo clippy -- -D warnings`** before every commit
- **Run `cargo fmt --all`** before every commit
- **Document all public items** with `///`, `# Examples`, `# Errors`, `# Panics`
- **Descriptive names** — `user_count`, `max_retry_attempts` not `n`, `x`
- **Small, focused functions** — single responsibility, testable in isolation
- **`Debug` for all types** — always derive or implement

---

## Critical Patterns with Code Examples

### Ownership
```rust
// ✅ Prefer borrowing over cloning
fn count_words(text: &str) -> usize { text.split_whitespace().count() }

// ✅ Take ownership when transforming
fn to_uppercase(mut s: String) -> String { s.make_ascii_uppercase(); s }

// ✅ Clone only when necessary — document why
fn store_in_cache(key: String, value: Data) {
    CACHE.insert(key.clone(), value);  // Clone needed: cache takes ownership
    log::info!("Stored {}", key);     // Original still available
}
```

### Error Handling
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TaskError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("task not found: {0}")]
    NotFound(Uuid),
    #[error("invalid status transition from {from:?} to {to:?}")]
    InvalidTransition { from: TaskStatus, to: TaskStatus },
}

// Application layer
async fn process_request(id: Uuid) -> anyhow::Result<Response> {
    let task = repo.find_by_id(id)
        .await
        .context("Failed to query database")?
        .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;
    Ok(Response::success(task))
}
```

### Async
```rust
// ✅ spawn_blocking for CPU-intensive work
async fn process_heavy(data: Vec<u8>) -> anyhow::Result<Output> {
    task::spawn_blocking(move || expensive_computation(&data))
        .await
        .context("Background task panicked")?
        .context("Computation failed")
}

// ✅ Parallel operations
let (users, orders) = tokio::try_join!(
    fetch_users(&pool),
    fetch_orders(&pool),
)?;

// ✅ Racing with timeout
tokio::select! {
    result = do_work() => result?,
    _ = tokio::time::sleep(Duration::from_secs(30)) => {
        return Err(anyhow::anyhow!("operation timed out"));
    }
}
```

### State Sharing
```rust
// Prefer in order: Atomic → channel → RwLock → Mutex
use std::sync::atomic::{AtomicI32, Ordering};
struct App { counter: AtomicI32 }            // Lock-free counter

use tokio::sync::mpsc;
let (tx, rx) = mpsc::channel(100);           // Message passing

use std::sync::{Arc, RwLock};
let data = Arc::new(RwLock::new(HashMap::new()));  // Read-heavy map
```

---

## Workspace Decision Matrix

### When to Choose Each Structure

| Scenario | Structure |
|---|---|
| <5K lines, 1-2 devs, simple domain | Single crate |
| Library usable independently | Binary + library (`lib.rs` + `main.rs`) |
| Multiple services sharing code | Multi-crate workspace |
| >20K lines / 5+ devs | Multi-crate workspace (always) |

### Decision Tree
```
Project size?
├─ Small (<5K lines, simple) → Single crate
├─ Medium (5K-20K lines)
│  ├─ Library reusable externally? Yes → Binary + Library
│  ├─ Multiple executables?       Yes → Workspace
│  └─ Otherwise                      → Single crate with modules
└─ Large (>20K lines, 5+ devs)   → Multi-crate Workspace (always)
```

### Workspace Setup
```toml
# Cargo.toml (workspace root)
[workspace]
members = ["core", "api", "db", "worker", "cli"]
resolver = "2"

[workspace.dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "net", "sync", "macros"] }
serde = { version = "1", features = ["derive"] }
anyhow = "1"
thiserror = "2"
uuid = { version = "1", features = ["v4"] }
```

```toml
# Individual crate Cargo.toml
[dependencies]
tokio.workspace = true    # Inherit version + features from workspace
anyhow.workspace = true
```

### Red Flags: Don't Use Workspace When
- Single-developer hobby project
- Code isn't actually shared between binaries
- Adding complexity before it's needed
- All crates always deploy together (monolith)

---

## Domain-Specific Adaptations

### Web Services (axum)
```rust
// State injection
#[derive(Clone)]
struct AppState { db: PgPool, cache: Arc<Cache> }

async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<User>, AppError> {
    let user = state.db.find_user(id).await?;
    Ok(Json(user))
}

// Error type for axum
#[derive(Debug)]
struct AppError(anyhow::Error);
impl IntoResponse for AppError { /* map to status codes */ }
```

### CLI Tools (clap)
```rust
#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    #[arg(short, long)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}
```

### Background Services
- Owned `JoinSet` for dynamic task groups
- `CancellationToken` for coordinated shutdown
- `broadcast` channel for fan-out events
- `watch` channel for latest-value state

---

## Architecture Quality Gates

Before considering architecture complete:
- [ ] Every "NEVER DO" has a corresponding "ALWAYS DO"
- [ ] All code examples use valid Rust syntax
- [ ] Error types implement `Display` + `Error` + `Debug`
- [ ] Unsafe blocks have SAFETY comments
- [ ] Performance targets are specific and measurable (`<10ms p50`, `>10K RPS`)
- [ ] Testing strategy covers unit / integration / property tests
- [ ] Public APIs documented with `# Examples`, `# Errors`, `# Panics`
