# Beanstalk Memory Management Strategy
The goal of Beanstalk’s memory system is to guarantee memory safety under all circumstances, while allowing performance to scale with the strength of static analysis.

Beanstalk adopts a GC-first, statically-eliding strategy. Garbage collection defines the baseline semantics. Static analysis progressively removes GC participation where it can prove safety.

## High-Level Goals
- Memory safety is always ensured.
- No explicit lifetime annotations.
- No explicit move syntax.
- Early implementations rely on GC. Later implementations reduce or eliminate it.
- The language semantics do not change as memory management improves.

## Core Design Philosophy

Beanstalk’s memory model is intentionally layered:

Garbage collection guarantees correctness for all heap-managed values. Static analysis enforces exclusivity rules and identifies where ownership might matter.

Runtime ownership mechanisms (when enabled) exploit these guarantees to reduce GC work.

Ownership is therefore not a semantic requirement, but a performance contract. If the compiler cannot prove that a value obeys the rules required for deterministic destruction, that value simply remains GC-managed.

## GC as the Semantic Baseline
- In the baseline execution model:
- All heap values are managed by a garbage collector.
- No deterministic drops are required for correctness.
- possible_drop sites compile to no-ops.
- Borrowing rules still apply to prevent races and logical misuse.

This model is used by:
- The JavaScript backend
- Early Wasm backends using Wasm GC
- Debug and development builds

## Ownership as an Optional Runtime State
When enabled by the backend, ownership is represented as runtime metadata, not a static type distinction.

Values eligible for non-GC management are passed around as pointers with an **embedded ownership bit**:
- `borrowed` → the callee must not drop the value
- `owned` → the callee is responsible for dropping the value

The compiler guarantees that:
- Ownership may be transferred at most once along any control-flow path.
- Borrowed values are never used after a potential ownership transfer.
- All paths that might own a value reach a drop point.

The runtime flag merely selects which *already-safe* behaviour to execute. If these guarantees cannot be proven, ownership metadata is simply not generated and GC applies.

## Borrowing Rules
Beanstalk enforces a small, strict set of rules that apply uniformly across the language.
These are similar to Rust.

### Shared Access (Default)

* All variable usage creates a shared reference by default.
* Any number of shared references may exist simultaneously.
* Shared access is read-only.
* Shared references never imply ownership.

This allows aggressive reuse of values without copying.

### Mutable Access (`~`)

Mutable access must always be explicit.

* At most one mutable access to a value may exist at any time.
* Mutable access excludes all other access (shared or mutable).
* Mutable access may be either:

  * a mutable borrow, or
  * an ownership transfer

Which of these occurs is determined by static last-use analysis and finalised at runtime.

### Ownership Transfer (Moves)
A move transfers full responsibility for a value. 
Moves are inferred automatically by the compiler,
the programmer does not annotate ownership explicitly.

Moves are identified statically as *possible ownership consumption points* and resolved dynamically using the ownership flag.

## Last-Use Analysis
Beanstalk uses last-use analysis as a sufficient condition for ownership transfer, not a necessary one.

This analysis runs backwards through control flow and answers a simpler question:

> “Is this the final use of this value on this path?”

If a use is determined to be the last possible use:
- The compiler may allow ownership transfer at that point.
- No drop is inserted immediately.
- Responsibility is deferred to the consumer.

If a path exits a scope without a last use:
* A `possible_drop` is inserted.
* The drop executes only if the value is owned at runtime.

In GC-only backends, these sites are ignored. In hybrid backends, they become conditional destruction points.

## Control Flow and Drops
Control flow constructs (`if`, `loop`, `break`, `return`) interact with ownership explicitly.

* Every control-flow exit that leaves a scope capable of owning a value has an associated `possible_drop`.
* Branches are analysed independently.
* Merges are conservative: if *any* path may own a value, a drop point must exist.

This guarantees:
- Deterministic destruction where enabled
- Correct GC fallback where not

Infinite loops require no destruction unless they can exit.

## Unified ABI (Deferred Responsibility)
Beanstalk is designed to support a unified ABI when ownership lowering is active, but does not require it initially. It deliberately avoids generating separate functions for borrowed vs owned arguments.

Instead, all functions use a **single ABI**:

* Arguments are passed as tagged pointers.
* The callee masks out the tag to access the value.
* If the ownership flag is set:

  * the callee must drop the value before returning.
* If the flag is clear:

  * the callee must not drop the value.

Static analysis guarantees that the caller will not use a value again if ownership may have been transferred.

This design:

* avoids monomorphization,
* keeps Wasm binaries small,
* and preserves predictable performance.

## Compiler Responsibilities

The compiler enforces memory safety through the following steps:

1. **AST lowering**, where:
   * types are checked, 
   * name resolution happens, 
   * eager folding of expressions takes place.
2. **HIR lowering**, where:
   * Advisory possible_drop insertion
   * control flow is linearized,
   * ownership boundaries are identified.
3. **Borrow validation**, which:
   * performs last-use analysis,
   * enforces exclusivity rules,
   * prevents illegal overlapping access,
   * Enables ownership eligibility.
4. **Lowering to LIR**, where:
   * ownership flags are generated,
   * possible drops become conditional frees,
   * runtime checks are emitted.

At no point does the compiler rely on undefined behaviour or unchecked aliasing.

## Design Tradeoffs
Beanstalk intentionally trades maximal static precision for predictable semantics, implementation tractability and backend flexibility.

Compared to a fully static borrow checker:

* Some ownership decisions are deferred to runtime.
* Some errors are detected later than theoretically possible.
* Small runtime cost from possible drop checks.

In exchange:

* The language remains approachable.
* The compiler remains tractable.
* The model integrates cleanly with Wasm.
* Future static analysis can be layered on incrementally.

## Future Extensions

This memory model is designed to evolve.

Possible future enhancements include:

* Region-based memory management
* Stronger static lifetime inference
* Place-based alias tracking
* Drop elision via region scopes
* Compile-time ownership specialisation

All of these can be added **without breaking the existing ABI or semantics**, because the current design already treats ownership as a constrained runtime property.

### No Explicit Lifetimes
There is no syntax for lifetimes and no lifetime parameters.