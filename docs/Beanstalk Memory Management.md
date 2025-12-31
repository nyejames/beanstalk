# Beanstalk Memory Management Strategy

The goal of Beanstalk’s memory and borrow system is to **ensure safety and avoid common bugs** without requiring a garbage collector or significantly impacting performance.

This is achieved through borrow checking rules, static analysis and runtime ownership checks.

Beanstalk aims to be:
- Safer than manual memory management
- More predictable than a GC
- Faster than pure RC or GC
- Lighter than full Rust-style borrow checking
- More flexible than pure region systems

## High-Level Goals
- Memory safety is ensured.
- No explicit lifetime annotations. All lifetimes are inferred.
- Possible moves are determined by the compiler.

## Core Design Philosophy

Beanstalk’s memory model is built around a **hybrid strategy**:

* **Static analysis** is used to enforce exclusivity, prevent obvious misuse, and identify *potential* ownership transfer points.
* **Runtime ownership checks** are used to resolve whether a value is borrowed or owned at specific boundaries.

Ownership in Beanstalk is therefore **not a purely static property**, but a runtime state constrained by compile-time guarantees.
This allows Beanstalk to remain memory-safe while avoiding the complexity, rigidity, and binary growth associated with fully static borrow checking.

The compiler’s job is to ensure that *any possible runtime ownership outcome is safe*.

Beanstalk simply chooses to let the runtime flip the final cleanup switch *after* the compiler has made sure it’s impossible to blow anything up.

## What “Memory Safety” Means in Beanstalk

A Beanstalk program is memory safe if all of the following are true:

* No value is accessed after it has been dropped.
* No value is dropped more than once.
* No mutable access overlaps with any other access.
* References never outlive the data they point to.
* All memory that *might* be owned is eventually dropped.

These guarantees hold regardless of which runtime ownership paths are taken.

## Ownership as a Runtime State

In Beanstalk, ownership is represented as a **runtime flag**, not a distinct static type.

Values that live in linear memory are passed around as pointers with an **embedded ownership bit**:

* `borrowed` → the value must not be dropped by the callee
* `owned` → the callee is responsible for dropping the value

The compiler statically guarantees that:

* Ownership is transferred at most once.
* Borrowed values are not used after a potential ownership transfer.
* All control-flow paths that might own a value lead to a drop point.

The runtime flag merely selects which *already-safe* behavior to execute.

## Borrowing Rules

Beanstalk enforces a small, strict set of rules that apply uniformly across the language.

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

Which of these occurs is determined by static last-use analysis and finalized at runtime.

### Ownership Transfer (Moves)
A move transfers full responsibility for a value. 
Moves are inferred automatically by the compiler,
the programmer does not annotate ownership explicitly.

Moves are identified statically as *possible ownership consumption points* and resolved dynamically using the ownership flag.

## Last-Use Analysis

Instead of tracking precise lifetimes, Beanstalk uses **last-use analysis**.

This analysis runs backwards through control flow and answers a simpler question:

> “Is this the final use of this value on this path?”

If a use is determined to be the last possible use:

* The compiler may allow ownership transfer at that point.
* No drop is inserted immediately.
* Responsibility is deferred to the consumer.

If a path exits a scope without a last use:

* A `possible_drop` is inserted.
* The drop executes only if the value is owned at runtime.

This approach is:

* conservative (never unsound),
* path-aware,
* and significantly simpler than full lifetime inference.

## Control Flow and Drops

Control flow constructs (`if`, `loop`, `break`, `return`) interact with ownership explicitly.

* Every control-flow exit that leaves a scope capable of owning a value has an associated `possible_drop`.
* Branches are analyzed independently.
* Merges are conservative: if *any* path may own a value, a drop point must exist.

This ensures that:

* Owned values are always dropped exactly once.
* Borrowed values are never dropped.
* Infinite loops do not require drops unless they can exit.

## Unified ABI for Borrowed and Owned Values

Beanstalk deliberately avoids generating separate functions for borrowed vs owned arguments.

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

1. **Type checking and eager folding** in the AST.
2. **HIR lowering**, where:
   * control flow is linearized,
   * possible drop points are inserted,
   * ownership boundaries are identified.
3. **Borrow validation**, which:
   * enforces exclusivity rules,
   * prevents illegal overlapping access,
   * validates move soundness.
4. **Lowering to LIR**, where:
   * ownership flags are generated,
   * possible drops become conditional frees,
   * runtime checks are emitted.

At no point does the compiler rely on undefined behavior or unchecked aliasing.

## Design Tradeoffs

This model intentionally trades:

* maximal static precision
  for
* simplicity, predictability, and extensibility

Compared to a fully static borrow checker:

* Some ownership decisions are deferred to runtime.
* Some errors are detected later than theoretically possible.

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
* Compile-time ownership specialization

All of these can be added **without breaking the existing ABI or semantics**, because the current design already treats ownership as a constrained runtime property.

Excellent instinct. This section does important social work: it sets expectations, defuses “why didn’t you just…” conversations, and clarifies that Beanstalk is making *intentional* tradeoffs rather than falling short of Rust.

Below is a clean drop-in section, followed by a comparison table you can include near the end of the document.

## Why This Is Not Rust
Rust’s borrow checker is designed to prove *exact ownership and lifetime behavior at compile time*.
Beanstalk’s memory model is designed to **guarantee safety while remaining flexible, predictable, and lightweight**, especially in a Wasm environment.

This design helps to reduce the binary code size, compile speeds and the language complexity.

The key philosophical difference is this:
- Rust statically proves exactly what happens.
- Beanstalk statically proves that whatever happens will be safe.

### No Explicit Lifetimes
Rust exposes lifetimes as part of the language.
Beanstalk does not. There is no syntax for lifetimes and no lifetime parameters.
Control-flow boundaries determine where ownership *may* end, not where it *must* end.

### Ownership Is a Runtime State, Not a Static Type
In Rust:
- Ownership is a compile-time property.
- Functions must be monomorphized or specialized for owned vs borrowed arguments.
- The compiler must know *exactly* when moves occur.

In Beanstalk:
- Ownership is a runtime flag constrained by static rules.
- Functions do not distinguish between borrowed and owned parameters.
- The compiler only needs to know **where ownership could legally occur**.
