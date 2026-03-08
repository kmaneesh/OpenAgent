# Documentation and AI

## Documentation

### M-FIRST-DOC-SENTENCE
First sentence ≈15 words, one line. Becomes summary in module index.

### M-MODULE-DOCS
Public modules: `//!` docs. Cover: implementation details, side effects, examples, when to use.

### M-CANONICAL-DOCS
Sections: Summary, # Examples, # Errors, # Panics, # Safety (if unsafe), # Abort (if fn may abort the process). No parameter tables; explain in prose.

### M-DOC-INLINE
`#[doc(inline)]` on `pub use` for re-exports. Don't inline std/3rd-party.

## AI

### M-DESIGN-FOR-AI
- Idiomatic Rust APIs (follow API Guidelines + Library UX)
- Thorough docs (modules, public items)
- Thorough examples (in docs + repo)
- Strong types (avoid primitive obsession)
- Testable APIs (mocks, fakes, features)
- Good test coverage (enables refactoring)

Rust's type system helps agents; compiler catches many mistakes.
