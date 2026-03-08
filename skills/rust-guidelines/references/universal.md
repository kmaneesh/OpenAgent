# Universal Guidelines

## M-UPSTREAM-GUIDELINES
Follow Rust API Guidelines, Style Guide, Design Patterns, Undefined Behavior reference. Key: C-COMMON-TRAITS (Display, Copy, Clone, Eq, etc.), C-CTOR (Foo::new()).

## M-STATIC-VERIFICATION
Use: miri, cargo-udeps, cargo-hack, cargo-audit, rustfmt, clippy. Enable compiler lints (ambiguous_negative_literals, missing_debug_implementations, unsafe_op_in_unsafe_fn, etc.) and clippy categories.

## M-LINT-OVERRIDE-EXPECT
Use `#[expect(clippy::unused_async, reason = "API fixed")]` not `#[allow]`. Prevents stale overrides.

## M-PUBLIC-DEBUG
All public types implement Debug. Sensitive types: custom impl that redacts; unit test that it doesn't leak.

## M-PUBLIC-DISPLAY
Types meant to be read implement Display. Error types must (std::error::Error).

## M-SMALLER-CRATES
Err on more crates. Split if submodule can be used independently. Crates for standalone items; features for extra functionality.

## M-CONCISE-NAMES
Avoid: Service, Manager, Factory. Use: Bookings, BookingDispatcher, Builder. Builder for repeated construction; accept `impl Fn() -> Foo` for factory params.

## M-REGULAR-FN
Associated functions for instance creation. General computation → regular functions.

## M-PANIC-IS-STOP
Panic = stop. Don't catch, don't use for errors. Valid: poison, user-requested unwrap, const, programming error.

## M-PANIC-ON-BUG
Programming errors → panic. Contract violations → panic. No Error type for unrecoverable bugs. Use type system for "correct by construction" when possible.

## M-DOCUMENTED-MAGIC
Magic values need comments: external systems, side effects if changed, why chosen. Prefer named constants.

## M-LOG-STRUCTURED
Use message templates (tracing event!), named properties, hierarchical names (file.open.success). Avoid format! at log time. Redact sensitive data. Follow OTel semantic conventions.
