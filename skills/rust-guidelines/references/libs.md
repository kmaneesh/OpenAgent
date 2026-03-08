# Library Guidelines

## Interoperability

### M-TYPES-SEND
Public types Send for Tokio. Futures must be Send. Rc across .await → !Send. Atomics for thread-per-core: usually negligible cost.

### M-ESCAPE-HATCHES
Types wrapping native handles: `unsafe fn from_native()`, `into_native()`, `to_native()`. Document safety requirements.

### M-DONT-LEAK-TYPES
Prefer std in public APIs. Leak external types: only behind features, or when substantial benefit (e.g. serde). Umbrella crates may leak siblings.

## UX

### M-ESSENTIAL-FN-INHERENT
Core functionality in inherent impls. Traits forward to them. Users discover via inherent methods.

### M-IMPL-IO (sans-IO)
Accept `impl Read`/`impl Write` for one-shot I/O. Lets callers pass File, TcpStream, &[u8], etc.

### M-IMPL-RANGEBOUNDS
Accept `impl RangeBounds<T>` for range params. Callers use `1..3`, `1..`, `..`.

### M-IMPL-ASREF
Accept `impl AsRef<Path>`, `impl AsRef<str>`, `impl AsRef<[u8]>` where you don't need ownership.

### M-SERVICES-CLONE
Heavy services: Clone with Arc inner. `impl Clone for ServiceCommon { inner: Arc<ServiceCommonInner> }`.

### M-INIT-CASCADED
4+ params → group semantically. `Deposit::new(account, amount)` not `new(bank, customer, currency, amount)`.

### M-INIT-BUILDER
4+ optional params → FooBuilder. Chainable `.x()`, `.build()`. Required params in `builder(deps)`.

### M-ERRORS-CANONICAL-STRUCTS
Error struct: Backtrace, cause, `is_xxx()` helpers. Implement Display, Error. ErrorKind internal; expose is_* methods.

### M-DI-HIERARCHY
Prefer concrete > generics > dyn Trait. Use enum for mock vs real; generics for user-provided impls. Avoid dyn unless nesting forces it.

### M-AVOID-WRAPPERS
Don't expose Rc, Arc, Box, RefCell in APIs. Hide behind &T, &mut T, T.

### M-SIMPLE-ABSTRACTIONS
Avoid Service<Backend<Store>>. Service types ≤1 level nesting.

## Resilience

### M-MOCKABLE-SYSCALLS
I/O/syscalls mockable. No default() that does real I/O. Accept I/O core or `new_mocked() -> (Self, MockCtrl)`.

### M-TEST-UTIL
Test utilities behind `#[cfg(feature = "test-util")]`.
### M-STRONG-TYPES
Use PathBuf for paths; proper type family. Avoid String for path-like data.

### M-NO-GLOB-REEXPORTS
No `pub use foo::*`. Re-export items individually. Exception: platform-specific HAL modules.

### M-AVOID-STATICS
Statics can duplicate across crate versions. Avoid for correctness-critical state. Only for performance.

## Building

### M-OOBE
cargo build just works. No extra tools, no env vars by default. Tier 1 platforms.

### M-SYS-CRATES
-sys: static + dynamic linking, embed sources, no external build scripts. Use cc for native build.

### M-FEATURES-ADDITIVE
Features additive; any combination works. Adding foo must not disable/modify other items.
