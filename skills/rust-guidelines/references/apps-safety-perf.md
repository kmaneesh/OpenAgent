# Applications, FFI, Safety, Performance

## Applications

### M-APP-ERROR
Apps may use anyhow/eyre. Don't mix multiple app-level error crates. Libraries use canonical error structs.

### M-MIMALLOC-APPS
```rust
use mimalloc::MiMalloc;
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;
```

## FFI

### M-ISOLATE-DLL-STATE
Between DLLs: only share portable data. Portable = #[repr(C)], no TypeId, no static, no thread-local. String, Vec, Box, etc. are not portable. Each DLL has its own type IDs and statics.

## Safety

### M-UNSAFE
Valid reasons: FFI, performance (after benchmark), novel abstractions. Not for transmute, Send hacks, "simplifying" casts. Document safety; use Miri for unsafe code.

### M-UNSOUND
Unsound = safe code that can cause UB. Never acceptable. No exceptions.

### M-UNSAFE-IMPLIES-UB
unsafe only for UB risk. delete_database() is dangerous but not unsafe.

## Performance

### M-YIELD-POINTS
Long CPU-bound async: `yield_now().await` every 10–100μs. Prevents starving other tasks.

### M-HOTPATH
Identify hot paths early. Profile (VTune, Superluminal). Benchmark with criterion/divan. Enable debug=1 for bench profile.

### M-THROUGHPUT
Optimize for throughput. Batch, exploit locality, avoid empty cycles. Don't hot-spin for single items.
