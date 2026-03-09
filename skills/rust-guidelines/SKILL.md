---
name: rust-guidelines
description: Pragmatic Rust design guidelines from Microsoft. Use when writing, reviewing, or refactoring Rust code. Covers universal patterns, libraries (interop, UX, resilience, building), applications, FFI, safety, performance, documentation, and AI-friendly design. Triggers include "Rust best practices", "Rust code review", "idiomatic Rust", "Rust API design", "Rust guidelines", or any Rust development task.
---

# Pragmatic Rust Guidelines

Source: [Microsoft Rust Guidelines](https://microsoft.github.io/rust-guidelines/guidelines/index.html). Apply the spirit, not the letter. `must` = always hold; `should` = flexibility allowed.

## Golden Rule

Each guideline exists for a reason. Before working around one, understand why it exists. Don't blindly follow if it would violate its motivation.

## Quick Checklist

### Universal
- **M-LOG-STRUCTURED** — Use structured logging with message templates (tracing, named events, OTel conventions). Redact sensitive data.
- **M-DOCUMENTED-MAGIC** — Document magic values; prefer named constants.
- **M-PANIC-ON-BUG** — Programming bugs → panic, not `Result`. Contract violations → panic.
- **M-PANIC-IS-STOP** — Panic means stop. Don't use panics for flow control or error communication.
- **M-REGULAR-FN** — Prefer regular functions over associated functions for non-instance logic.
- **M-CONCISE-NAMES** — Avoid weasel words: `Service`, `Manager`, `Factory`. Use `Bookings`, `BookingDispatcher`, `Builder`.
- **M-SMALLER-CRATES** — Err on more crates. Split if a submodule can be used independently.
- **M-PUBLIC-DISPLAY** — Types meant to be read implement `Display`.
- **M-PUBLIC-DEBUG** — All public types implement `Debug`. Sensitive data → custom impl that redacts.
- **M-LINT-OVERRIDE-EXPECT** — Use `#[expect]` not `#[allow]` for lint overrides; add `reason`.
- **M-STATIC-VERIFICATION** — Use miri, clippy, rustfmt, cargo-audit, cargo-udeps, cargo-hack.
- **M-UPSTREAM-GUIDELINES** — Follow Rust API Guidelines, Style Guide, Design Patterns.

### Libraries / Interoperability
- **M-TYPES-SEND** — Public types should be `Send` for Tokio/async compatibility.
- **M-ESCAPE-HATCHES** — Types wrapping native handles: provide `unsafe fn from_native()`, `into_native()`, `to_native()`.
- **M-DONT-LEAK-TYPES** — Prefer `std` types in public APIs. Leak external types only behind features or when essential.

### Libraries / UX
- **M-ESSENTIAL-FN-INHERENT** — Core functionality in inherent impls; traits forward to them.
- **M-IMPL-IO** — Accept `impl Read`/`impl Write` for sans-IO flexibility.
- **M-IMPL-RANGEBOUNDS** — Accept `impl RangeBounds<T>` for range params.
- **M-IMPL-ASREF** — Accept `impl AsRef<Path>`, `impl AsRef<str>`, `impl AsRef<[u8]>` where feasible.
- **M-SERVICES-CLONE** — Heavy services implement `Clone` (Arc-style, not fat copy).
- **M-INIT-CASCADED** — 4+ params → group semantically (e.g. `Account`, `Currency`).
- **M-INIT-BUILDER** — 4+ optional params → `FooBuilder` with chainable `.x()` and `.build()`.
- **M-ERRORS-CANONICAL-STRUCTS** — Errors: struct with Backtrace, cause, `is_xxx()` helpers. Implement `Display` + `Error`.
- **M-DI-HIERARCHY** — Prefer concrete types > generics > `dyn Trait`. Avoid `dyn` unless nesting forces it.
- **M-AVOID-WRAPPERS** — Don't expose `Rc`, `Arc`, `Box`, `RefCell` in public APIs.
- **M-SIMPLE-ABSTRACTIONS** — Avoid `Service<Backend<Store>>`-style nesting. Keep service types ≤1 level.

### Libraries / Resilience
- **M-AVOID-STATICS** — Statics can duplicate across crate versions. Avoid for correctness-critical state.
- **M-NO-GLOB-REEXPORTS** — No `pub use foo::*`. Re-export items individually.
- **M-STRONG-TYPES** — Use `PathBuf` not `String` for paths; proper type family.
- **M-TEST-UTIL** — Test utilities, mocks, fake data behind `#[cfg(feature = "test-util")]`.
- **M-MOCKABLE-SYSCALLS** — I/O and syscalls mockable. No `MyIoLibrary::default()`; accept I/O core or provide `new_mocked()`.

### Libraries / Building
- **M-FEATURES-ADDITIVE** — Features are additive; any combination must work.
- **M-SYS-CRATES** — `-sys` crates: static + dynamic linking, embed sources, no external build scripts.
- **M-OOBE** — `cargo build` just works on Tier 1 platforms. No extra tools or env vars by default.

### Applications
- **M-APP-ERROR** — Apps may use anyhow/eyre. Don't mix multiple app-level error crates.
- **M-MIMALLOC-APPS** — Use mimalloc as `#[global_allocator]` for apps.

### FFI
- **M-ISOLATE-DLL-STATE** — Only share portable (`#[repr(C)]`, no TypeId/static/tls) data between DLLs.

### Safety
- **M-UNSOUND** — Unsound code is never acceptable. Safe code must not cause UB.
- **M-UNSAFE-IMPLIES-UB** — `unsafe` only for UB risk, not for "dangerous but defined" behavior.
- **M-UNSAFE** — Valid reasons: FFI, performance (after benchmark), novel abstractions. Not for transmute/ Send hacks.

### Performance
- **M-YIELD-POINTS** — Long CPU-bound async tasks: `yield_now().await` every 10–100μs.
- **M-HOTPATH** — Identify hot paths early; profile; benchmark with criterion/divan.
- **M-THROUGHPUT** — Optimize for throughput. Batch, avoid empty cycles, exploit locality.

### Documentation
- **M-DOC-INLINE** — `#[doc(inline)]` on `pub use` for inlined re-exports.
- **M-CANONICAL-DOCS** — Summary, # Examples, # Errors, # Panics, # Safety, # Abort when applicable.
- **M-MODULE-DOCS** — Public modules have `//!` docs. First sentence <15 words.
- **M-FIRST-DOC-SENTENCE** — First sentence ≈15 words, one line.

### AI
- **M-DESIGN-FOR-AI** — Idiomatic APIs, thorough docs, strong types, testable APIs, good test coverage. Helps both humans and agents.

## Compiler & Clippy Lints

```toml
[lints.rust]
ambiguous_negative_literals = "warn"
missing_debug_implementations = "warn"
redundant_imports = "warn"
redundant_lifetimes = "warn"
trivial_numeric_casts = "warn"
unsafe_op_in_unsafe_fn = "warn"
unused_lifetimes = "warn"

[lints.clippy]
cargo = { level = "warn", priority = -1 }
complexity = { level = "warn", priority = -1 }
correctness = { level = "warn", priority = -1 }
pedantic = { level = "warn", priority = -1 }
perf = { level = "warn", priority = -1 }
style = { level = "warn", priority = -1 }
suspicious = { level = "warn", priority = -1 }
# nursery = { level = "warn", priority = -1 }  # optional

# Restriction group — drives consistency and quality
allow_attributes_without_reason = "warn"
as_pointer_underscore = "warn"
assertions_on_result_states = "warn"
clone_on_ref_ptr = "warn"
deref_by_slicing = "warn"
disallowed_script_idents = "warn"
empty_drop = "warn"
empty_enum_variants_with_brackets = "warn"
empty_structs_with_brackets = "warn"
fn_to_numeric_cast_any = "warn"
if_then_some_else_none = "warn"
map_err_ignore = "warn"
redundant_type_annotations = "warn"
renamed_function_params = "warn"
semicolon_outside_block = "warn"
string_to_string = "warn"
undocumented_unsafe_blocks = "warn"
unnecessary_safety_comment = "warn"
unnecessary_safety_doc = "warn"
unneeded_field_pattern = "warn"
unused_result_ok = "warn"

# Allow: structured logging uses literal strings with template syntax
literal_string_with_formatting_args = "allow"
```

## OpenAgent Service Patterns

Conventions established across OpenAgent Rust services (`services/discord`, `services/sandbox`, etc.).

### Mutex choice
- `std::sync::Mutex` — preferred when the lock is **not** held across `.await`
- `tokio::sync::Mutex` — only when the lock must cross an `.await` point
- Never hold a `std::sync::Mutex` guard across `.await`; it deadlocks the executor thread

### Atomic ordering for connection flags
- `Ordering::Acquire` on load / `Ordering::Release` on store for `connected`/`authorized` booleans
- Establishes the happens-before relationship so tool handlers see the latest gateway state

### Sync tool handlers calling async (block_in_place pattern)
SDK tool handlers are `Fn(Value) -> anyhow::Result<String>` (sync). To call an async Serenity HTTP method from inside one, use:
```rust
// Handle::current().block_on() bridges sync handler → async Serenity HTTP.
use tokio::runtime::Handle;
tokio::task::block_in_place(|| Handle::current().block_on(some_async_fn()))
```
Requires `rt-multi-thread` in tokio features.

### Tokio features — minimal set for service binaries
```toml
tokio = { version = "1", features = ["rt-multi-thread", "net", "sync", "macros"] }
```
Never use `features = ["full"]` — it pulls in test-util, io-std, and other unneeded weight.

### Graceful Serenity shutdown
Capture `shard_manager` before spawning the client task; call `shutdown_all()` on exit so Discord sees a clean gateway close:
```rust
let shard_manager = Arc::clone(&client.shard_manager); // field, not method in serenity 0.12
let handle = tokio::spawn(async move { client.start().await });
server.serve(&socket_path).await;
shard_manager.shutdown_all().await;
handle.abort();
```

### Bot message filtering
Always filter `msg.author.bot` before emitting `message.received` events. Forwarding a bot's own messages causes the agent to process its own output and loop.

### Param extraction in tool handlers
Prefer `filter` + `ok_or_else` over `unwrap_or("").is_empty()`:
```rust
let channel_id = params["channel_id"]
    .as_str()
    .filter(|v| !v.is_empty())
    .ok_or_else(|| anyhow::anyhow!("channel_id is required"))?
    .to_string();
```

### `running` flag in status responses
Don't store a `started: AtomicBool`. If the tool handler is executing, the service is running. Hardcode `"running": true` in `status_json()`.

## References

- [universal.md](references/universal.md) — Universal guidelines (logging, panic, names, static verification)
- [libs.md](references/libs.md) — Library interop, UX, resilience, building
- [apps-safety-perf.md](references/apps-safety-perf.md) — Applications, FFI, safety, performance
- [docs-ai.md](references/docs-ai.md) — Documentation and AI-friendly design

Full source: https://microsoft.github.io/rust-guidelines/
