# Compiler Error → Design Question Mapping

When hitting a compiler error, resist the mechanical fix. Use these tables to ask the right design question first.

---

## Ownership & Borrowing Errors

| Error | Mechanical Fix (avoid) | Design Question (ask instead) |
|---|---|---|
| E0382 "use of moved value" | "Clone it" | Who should own this data? |
| E0597 "does not live long enough" | "Extend lifetime" | Is the scope boundary correct? |
| E0506 "cannot assign to borrowed" | "End borrow first" | Should mutation happen elsewhere? |
| E0507 "cannot move out of reference" | "Clone before move" | Why are we moving from a reference? |
| E0515 "cannot return reference to local" | "Return owned" | Should caller own the data? |
| E0716 "temporary dropped while borrowed" | "Bind to variable" | Why is this temporary? |
| E0106 "missing lifetime specifier" | "Add 'a" | What is the actual lifetime relationship? |

**Thinking prompt before fixing:** What is this data's domain role?
- Entity (unique identity) → owned
- Value Object (interchangeable) → clone/copy OK
- Temporary (computation result) → maybe restructure

---

## Smart Pointer / Resource Errors

| Pattern | Mechanical Fix (avoid) | Design Question (ask instead) |
|---|---|---|
| "Need heap allocation" | "Use Box" | Why can't this be on stack? |
| `Rc` memory leak | "Use Weak" | Is the cycle necessary in the design? |
| `RefCell` panic | "Use try_borrow" | Is runtime check the right approach? |
| `Arc` overhead complaint | "Accept it" | Is multi-thread access actually needed? |

**Pointer selection:**
- Single owner → owned value or `Box<T>`
- Shared single-thread → `Rc<T>`; mutable interior → `Rc<RefCell<T>>`
- Shared multi-thread → `Arc<T>`; mutable interior → `Arc<Mutex<T>>` or `Arc<RwLock<T>>`

---

## Mutability Errors

| Error | Mechanical Fix (avoid) | Design Question (ask instead) |
|---|---|---|
| E0596 "cannot borrow as mutable" | "Add mut" | Should this really be mutable? |
| E0499 "cannot borrow mutably more than once" | "Split borrows" | Is the data structure right? |
| E0502 "cannot borrow as mutable, also borrowed immutably" | "Separate scopes" | Why do we need both borrows simultaneously? |
| `RefCell` borrow panic | "Use try_borrow" | Is runtime borrow checking appropriate here? |

**Before adding mutability:** Is transformation a better model? Can a builder pattern avoid mutation?

---

## Trait / Generic Errors

| Error | Mechanical Fix (avoid) | Design Question (ask instead) |
|---|---|---|
| E0277 "trait bound not satisfied" | "Add trait bound" | Is this abstraction at the right level? |
| E0308 "mismatched types" | "Fix the type" | Should types be unified or remain distinct? |
| E0599 "method not found" | "Import the trait" | Is the trait the right abstraction? |
| E0038 "trait cannot be made into an object" | "Make object-safe" | Do we really need dynamic dispatch? |

**When type is known:**
- Compile time → generics, `impl Trait` (static dispatch)
- Runtime → `dyn Trait` (dynamic dispatch) — use sparingly

---

## Type-Driven Design Issues

| Pattern | Mechanical Fix (avoid) | Design Question (ask instead) |
|---|---|---|
| Primitive obsession | "It's just a string" | What does this value represent? |
| Boolean flags proliferating | "Add an is_valid flag" | Can states be encoded as types? |
| `Option` used everywhere | "Check for None" | Is absence actually valid in the domain? |
| Runtime validation | "Return Err if invalid" | Can validity be encoded at construction? |

**Parse, don't validate:** construct valid types at system boundaries.
- Numeric range → bounded newtypes
- Valid states → typestate pattern
- Semantic meaning → newtype wrapper

---

## Error Handling Issues

| Pattern | Mechanical Fix (avoid) | Design Question (ask instead) |
|---|---|---|
| `unwrap` panicking | "Use ?" | Is `None`/`Err` actually possible here? |
| Type mismatch on `?` | "Use anyhow" | Are error types designed correctly? |
| Lost error context | "Add .context()" | What does the caller need to know? |
| Too many error variants | "Use `Box<dyn Error>`" | Is error granularity at the right level? |

**Failure taxonomy:**
- Expected recoverable failure → `Result<T, E>`
- Absent value (normal) → `Option<T>`
- Programming bug / invariant → `panic!`
- Unrecoverable system error → `panic!` or abort

---

## Concurrency / Send Errors

| Error | Mechanical Fix (avoid) | Design Question (ask instead) |
|---|---|---|
| E0277 "`Send` not implemented" | "Add `Send` bound" | Should this type actually cross threads? |
| E0277 "`Sync` not implemented" | "Wrap in `Mutex`" | Is shared mutable access really needed? |
| `Future` is not `Send` | "Use `spawn_local`" | Is async the right model here? |
| Deadlock | "Reorder locks" | Is the locking design fundamentally correct? |

**Workload model:**
- CPU-bound → threads (`std::thread`, `rayon`)
- I/O-bound → async (`tokio`)
- Mixed → hybrid: async tasks + `spawn_blocking`

---

## Persistent Error Escalation

If you've tried the design question twice and the error persists, escalate:

| Persistent Error | Escalate To | Question |
|---|---|---|
| E0382 (moved value) | Ownership strategy | What design choice led to this ownership pattern? |
| E0277 (trait bound) | Trait design | Is the trait abstraction right for this domain? |
| E0596 (cannot borrow) | Mutability model | Where should mutation be authorized? |
| Lifetime errors | Data flow design | What is the actual lifetime relationship in the domain? |
| Any error × 3 | Architecture review | Is the fundamental design correct? |

---

## Quick Error Lookup

```
E0382  moved value             → ownership
E0597  lifetime too short      → scope / lifetime
E0506  cannot assign borrowed  → mutability design
E0507  cannot move from ref    → ownership
E0515  return local ref        → owned return
E0596  cannot borrow mut       → mutability
E0499  mut borrow twice        → data structure
E0502  mut + immut borrow      → borrow scope
E0277  trait bound missing     → abstraction level
E0308  type mismatch           → type design
E0599  method not found        → trait import / design
E0038  not object-safe         → dyn vs impl Trait
E0106  missing lifetime        → lifetime relationship
E0716  temporary dropped       → ownership / binding
```
