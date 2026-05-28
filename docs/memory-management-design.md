# Beanstalk Memory Management Strategy
The goal of Beanstalk’s memory system is to guarantee memory safety under all circumstances, while allowing performance to scale with the strength of static analysis.

Programs that satisfy stronger static rules run faster with no difference in language semantics. Beanstalk treats ownership as an optimization target.
If static guarantees are missing or incomplete, the value falls back to GC.

## Related references

This document describes Beanstalk's memory model, GC fallback, ownership optimisation, and borrow-analysis strategy.

Use:
- `docs/language-overview.md` for user-facing syntax such as `~`, explicit copies, and no-shadowing
- `docs/compiler-design-overview.md` for compiler stage ownership and where borrow validation fits in the pipeline
- `docs/src/docs/progress/#page.bst` for current implementation status

## Core Design Philosophy
- No explicit lifetime annotations
- No explicit move syntax
- No language-level temporary references

Beanstalk does not expose temporary reference categories in the language. Fresh literals, templates, constructor calls, and computed expressions are values. When a fresh value must satisfy a mutable/exclusive parameter, the compiler may materialize it into a hidden local before borrow validation. That hidden local is a compiler lowering detail, not user-visible lifetime syntax.

Beanstalk’s memory model is intentionally layered:
- Garbage collection guarantees correctness for all heap-managed values by default
- Static analysis enforces exclusivity rules and identifies where ownership might matter
- Runtime ownership mechanisms (when enabled) exploit these guarantees to reduce GC work
- Ownership is purely for optimisation. If the compiler cannot prove that a value obeys the rules required for deterministic destruction, that value simply remains GC-managed

## GC as the Semantic Baseline
In the baseline execution model:
- All heap values are managed by a garbage collector
- No deterministic drops are required for correctness
- drop_if_owned sites compile to no-ops
- Borrowing rules still apply to prevent races and logical misuse

This model is used by:
- The JavaScript backend
- Early Wasm backends using Wasm GC
- Debug and development builds

## Ownership as an Optional Runtime State
When enabled by the backend, ownership is represented as runtime metadata, not a static type distinction. The split between compile-time specialisation and runtime ownership metadata is still deferred until benchmarking and backend work make the tradeoff concrete.

Values eligible for non-GC management are passed around as pointers with an **embedded ownership bit**:
- `borrowed` → the callee must not drop the value
- `owned` → the callee is responsible for dropping the value

The compiler guarantees that:
- Ownership may be transferred at most once along any control-flow path
- Borrowed values are never used after a potential ownership transfer
- All paths that might own a value reach a drop point

The runtime flag merely selects which *already-safe* behaviour to execute. If these guarantees cannot be proven, ownership metadata is simply not generated and GC applies.

## Borrowing Rules
Beanstalk enforces a small, strict set of rules that apply uniformly across the language. These are similar to Rust.

### Shared Access (Default)

- All variable usages create a shared reference by default
- Created by default assignment: `x = y`
- Any number of shared references may exist simultaneously
- Shared access is read-only
- Shared references never imply ownership
- **No explicit `&` or `&mut` operators** - these don't exist in Beanstalk

This allows aggressive reuse of values without copying.

### Mutable Access (`~`)

Mutable access must always be explicit.

- At most one mutable access to a value may exist at any time.
- `~` at a call site requests mutable/exclusive access for that specific argument
- `~` stays place-only: use it for existing mutable places (`~place`), not fresh literals/temporaries/computed values
- Mutable/exclusive parameters can be satisfied by either explicit `~place` or a plain fresh value lowered through a compiler-introduced hidden local
- Collections and mutable receiver/member calls follow the same explicit rule
- Mutable access excludes all other access (shared or mutable)
- The user never writes `~` for fresh values. `~` requests mutable/exclusive access to an existing place.

Mutable access may be either a mutable borrow, or an ownership transfer.
Which of these occurs is determined by static last-use analysis and finalised at runtime.

Beanstalk's no-shadowing rule is specified in `docs/language-overview.md`. The memory model benefits from it because each visible name maps to one binding, which simplifies access and last-use analysis.

### Ownership Transfer (Moves)
A move transfers full responsibility for a value. 
Moves are inferred automatically by the compiler,
the programmer does not annotate ownership explicitly.

- Moves are identified via last-use analysis but finalized at runtime using ownership flags
- The compiler determines when the last use of a variable happens statically for any given scope
- If the variable is passed into a function call or assigned to a new variable, and it's determined to be a move at runtime, then the new owner is responsible for dropping the value
- Otherwise, the last time an owner uses a value without moving it, a drop_if_owned() insertion will drop the value

Moves are identified statically as *possible ownership consumption points* and resolved dynamically using the ownership flag.

There may be some monomorphization, but the extent to which the compiler will statically generate each function won't be fully decided until benchmarks are in place.

### Copies are Explicit
- No implicit copying for any types unless they are part of an expression creating a new value out of multiple references, or when used inside a template head
- All types require explicit copy semantics when copying is needed
- Most operations use borrowing instead of copying

## Last-Use Analysis
Beanstalk uses last-use analysis as a sufficient condition for ownership transfer, not a necessary one.

This analysis runs backwards through control flow and answers a simpler question:

> “Is this the final use of this value on this path?”

If a use is determined to be the last possible use:
- The compiler may allow ownership transfer at that point
- No drop is inserted immediately
- Responsibility is deferred to the consumer

If a path exits a scope without a last use:
- A `drop_if_owned` is inserted
- The drop executes only if the value is owned at runtime

In GC-only backends, these sites are ignored. In hybrid backends, they become conditional destruction points.

## Control Flow and Drops
Control flow constructs (`if`, `loop`, `break`, `return`) interact with ownership explicitly.

- Every control-flow exit that leaves a scope capable of owning a value has an associated `drop_if_owned`
- Branches are analysed independently
- Merges are conservative: if *any* path may own a value, a drop point must exist

This guarantees:
- Deterministic destruction where enabled
- Correct GC fallback where not

Infinite loops require no destruction unless they can exit.

## Unified ABI (Deferred Responsibility)
Beanstalk is designed to support a unified ABI when ownership lowering is active. It deliberately avoids generating separate functions for borrowed vs owned arguments.

The amount that this runtime ABI vs a purely static approach will be used is unclear until the language is tested with both.

Function signatures make no distinction between a mutable reference or a move (owned value). Instead, all function calls use a single ABI:

- Arguments are passed as tagged pointers
- The callee masks out the tag to access the value
- If the ownership flag is set:
  * the callee must drop the value before returning

- If the flag is clear:
  * the callee must not drop the value

Static analysis guarantees that the caller will not use a value again if ownership may have been transferred.

This design keeps dispatch static, avoids excessive monomorphization and prevents binary-size growth on Wasm while still allowing the compiler to freely choose between moves and mutable references based on last-use analysis.

Future release optimisations can also remove the 'if owned' part if all calls either consume or borrow their arguments the same way across the program.
```Rust
    enum OwnershipEffect {
        MayConsume,     // Default, drop_if_owned will be used in this function
        NeverConsumes,  // No drop inserted at all
        AlwaysConsumes, // Drop is always inserted
    }
```

But all of this optimisation can be skipped for GC backends such as JS.

## Compiler Responsibilities

The compiler enforces memory safety through the following steps:

1. **AST lowering**, where:
   * types are checked, 
   * name resolution happens, 
   * eager folding of expressions takes place.
2. **HIR lowering**, where:
   * control flow is linearized,
   * ownership boundaries are identified,
   * fresh mutable call arguments are materialized into compiler-owned locals before borrow validation.
3. **Borrow validation**, which:
   * enforces exclusivity rules,
   * prevents illegal overlapping access,
   * detects invalid use after possible ownership transfer,
   * performs or consumes last-use analysis,
   * produces side-table facts for ownership-aware lowering,
   * identifies advisory `drop_if_owned` sites.
4. **Final Lowering**, where:
   * ownership flags are generated,
   * possible drops become conditional frees,
   * runtime checks are emitted.

At no point does the compiler rely on undefined behaviour or unchecked aliasing.

Borrow validation does not mutate HIR. It produces side-table facts keyed by HIR/value identity. HIR remains the semantic representation under GC; ownership-aware lowerings consult the side tables later.

## Design Tradeoffs
Beanstalk intentionally trades maximal static precision for predictable semantics, implementation tractability and backend flexibility.

Compared to a fully static borrow checker:

* Some ownership decisions are deferred to runtime.
* Some optimisation decisions are deferred to runtime instead of being fully statically resolved.
* Small runtime cost from drop_if_owned checks.

Comparing the other languages:
* Swift SIL - ownership is explicit in IR, verifier checks validity, optimizer exploits it
* Rust MIR - borrow checker annotates regions and rejects programs, MIR shape is mostly stable
* Beanstalk - borrow checker validates and annotates eligibility, but semantics remain GC-backed

In exchange:
* The language remains approachable.
* The compiler remains tractable.
* The model integrates cleanly with Wasm.
* Future static analysis can be layered on incrementally.

## Future Extensions

This memory model is designed to evolve.

Future enhancements might include:

* Region-based memory management
* Stronger static lifetime inference
* Place-based alias tracking
* Drop elision via region scopes
* Compile-time ownership specialisation

All of these can be added **without breaking the existing ABI or semantics**, because the current design already treats ownership as a constrained runtime property.

### No Explicit Lifetimes
There is no syntax for lifetimes and no lifetime parameters.
