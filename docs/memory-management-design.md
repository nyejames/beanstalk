# Beanstalk Memory Management Strategy

> Legacy consolidated reference
>
> Memory design is now maintained under
> `docs/src/docs/codebase/memory-management/`.
>
> This file is retained temporarily and should not be used as the primary
> design reference.

The goal of Beanstalk's memory system is to guarantee memory safety under all
circumstances, while allowing performance to scale with the strength of static
analysis.

Programs that satisfy stronger static rules run faster with no difference in
language semantics. Beanstalk treats ownership as an optimization target. If
static guarantees are missing or incomplete, the value falls back to GC.

## Design authority and implementation status

This document records Beanstalk's accepted memory model and intended backend
architecture. It includes ownership, ABI, destruction, and lowering decisions
that may not yet be fully implemented, particularly in the Wasm backend.

Treat these rules as the design the compiler and backends should converge on.
This document is not an implementation-status report. Use
`docs/src/docs/progress/#page.bst` for current support, partial coverage,
experimental paths, and major gaps.

Focused memory references are maintained under
`docs/src/docs/codebase/memory-management/`. New work should read and update
those pages.

Use the split pages for task-focused reading:

- `docs/src/docs/codebase/memory-management/overview.bd`
- `docs/src/docs/codebase/memory-management/access-and-aliasing/overview.bd`
- `docs/src/docs/codebase/memory-management/borrow-validation/overview.bd`
- `docs/src/docs/codebase/memory-management/ownership-and-drops/overview.bd`
- `docs/src/docs/codebase/memory-management/runtime-and-backend-lowering/overview.bd`

## Memory model at a glance

- GC is the semantic baseline. Correctness never depends on deterministic
  destruction.
- Existing values use shared, read-only access by default.
- Copies are explicit via `copy`.
- Mutation requires explicit exclusive access through `~place`.
- Moves are compiler-inferred at legal consumption points.
- Borrow validation is mandatory and backend-independent.
- Ownership-aware lowering is an optimisation that must preserve GC-equivalent
  behaviour.

## Related references

- `docs/language-overview.md` for user-facing syntax such as `~`, explicit
  copies, and no-shadowing.
- `docs/compiler-design-overview.md` for compiler stage ownership and where borrow validation fits in the pipeline.
- `docs/build-system-design.md` for project and backend orchestration where memory lowering crosses build boundaries.
- `docs/src/docs/progress/#page.bst` for current implementation status.

Implementation map:

- [`src/compiler_frontend/hir/`](../src/compiler_frontend/hir/) defines the
  semantic IR that borrow validation reads without mutating.
- [`src/compiler_frontend/analysis/borrow_checker/`](../src/compiler_frontend/analysis/borrow_checker/)
  owns exclusivity validation and side-table borrow facts.
- [`src/backends/js/`](../src/backends/js/) is the current GC-baseline
  lowering path.
- [`src/backends/wasm/hir_to_lir/ownership.rs`](../src/backends/wasm/hir_to_lir/ownership.rs)
  is the experimental ownership-lowering hook point for Wasm.

## Access and aliasing

### Vocabulary

- **Value:** a Beanstalk language value; not necessarily heap allocated.
- **Place:** existing storage that can be read or mutated.
- **Shared access:** read-only access to one or more underlying roots.
- **Alias binding:** a binding that observes existing storage rather than
  creating an independent copy.
- **Copy:** a new independent value created through explicit copy semantics.
- **Exclusive access:** mutation-capable access to one existing place.
- **Move:** compiler-inferred ownership transfer at a legal consumption point.
- **Fresh value:** a newly produced literal, template, constructor result or
  computed aggregate.

"Reference" is useful explanatory vocabulary but not a first-class source type.

### Shared access

Reading or binding an existing value uses shared, read-only access by default.
Ordinary assignment and argument passing do not implicitly create an
independent copy. This is a source aliasing contract, not a requirement that
every scalar be heap allocated or passed through a runtime pointer.

For `x = y`:

- the source semantics is a shared alias by default;
- it is not an implicit copy;
- it may be optimised to storage transfer at a safe final use;
- later mutation must respect active aliases;
- `copy y` creates independence.

Any number of shared reads may exist simultaneously. Shared access is
read-only. Shared access never implies ownership. Hashmap `get` returns shared
access into the map, so the map cannot be mutated while that result is live.

### Alias bindings

```beanstalk
alias = source
```

An alias binding normally observes the same source-semantic storage or root.
It does not create an independent value. It can participate in use-after-move
and mutation-conflict analysis. An ownership-aware backend may optimise an
alias into storage transfer when no later source use exists, but it must still
behave as though source aliasing rules were respected.

### Explicit copies

```beanstalk
snapshot = copy source
```

`copy` creates independent value semantics. All source types use explicit copy
when an independent duplicate is required. Expressions can construct new
values from shared inputs; constructing a result does not implicitly copy the
inputs. Template interpolation reads inputs and produces a new output string.
Map insertion and aggregate storage follow ordinary move and copy rules.

### Mutable bindings

```beanstalk
count ~= 0
count = 1
```

`~` on the declaration marks the binding as mutation-capable. Reassignment
still uses `=`. Mutable binding syntax is different from call-site exclusive
access. Mutability is an access property, not separate semantic type identity.

### Exclusive access

Mutable access must always be explicit.

- At most one mutable access to a value may exist at any time.
- `~` at a call site requests mutable/exclusive access for that specific
  argument.
- `~` stays place-only: use it for existing mutable places (`~place`), not
  fresh literals, temporaries or computed values.
- Mutable/exclusive parameters can be satisfied by either explicit `~place` or
  a plain fresh value lowered through a compiler-introduced hidden local.
- Collections and mutable receiver/member calls follow the same explicit rule.
- Hashmap `set`, `remove`, and `clear` follow the same explicit mutable
  receiver rule.
- Mutable access excludes all other access (shared or mutable).

`~place` requests exclusive access. Static analysis may realise an eligible
final-use call or assignment as ownership transfer, but the source marker
expresses access rather than ownership.

### Fresh values and hidden locals

Fresh literals, templates, constructor calls, and computed expressions are
values. When a fresh value must satisfy a mutable/exclusive parameter, the
compiler may materialise it into a hidden local before borrow validation. That
hidden local is a compiler lowering detail, not user-visible lifetime syntax.

The user never writes `~` for fresh values. `~` requests mutable/exclusive
access to an existing place.

### Collections, maps and aggregate storage

Aggregates own stored values under the ownership model. Insertion may borrow,
copy or consume according to the accepted access and ownership contract.
Independent duplication requires `copy`. Mutating collection and map methods
require exclusive receiver access.

Map `get` returns shared access into stored data. The same map cannot be
mutated while that shared result remains live. Map `remove` returns the
removed owned value. Map `set` does not silently duplicate keys or values
outside ordinary copy and move rules.

Hashmaps own stored keys and values. Inserting into a map follows the same
move and copy rules as storing values in other aggregate data.

### No explicit references, moves or lifetimes

Beanstalk has no source-level:

- `&` or `&mut` reference type constructors;
- lifetime annotations or lifetime parameters;
- temporary-reference syntax;
- explicit move keyword or operator;
- separate borrowed and owned function signatures.

These are permanent design boundaries, not merely unimplemented features.

### No shadowing

Beanstalk's no-shadowing rule is specified in `docs/language-overview.md`. The
memory model benefits from it because each visible name maps to one binding,
which simplifies access and last-use analysis.

## Borrow validation

### Stage boundary

Borrow validation runs after HIR generation. It reads coherent, validated HIR,
enforces source-language access and move safety, and writes analysis facts
beside HIR. It does not rewrite HIR, choose the final allocation strategy, or
perform backend ownership lowering.

### Inputs

- Validated `HirModule`.
- Explicit functions, blocks, regions, locals, places and terminators.
- External package registry.
- External call parameter access rules.
- Return-alias metadata.
- Source locations and string table for diagnostics.

Borrow validation does not reparse source syntax, solve generics or traits, or
see AST-only generic or trait state.

### Access roots and abstract state

Accepted abstract states:

- **uninitialized:** local has no valid value;
- **slot:** local may own independent storage;
- **alias:** local observes roots owned elsewhere;
- **slot + alias:** conservative control-flow result where either shape may
  arrive.

These are analysis lattice states. They are not Beanstalk source types and do
not directly prescribe one backend runtime representation.

Every analysed place resolves to one or more storage roots. Fields and indexes
currently resolve conservatively through their base roots. Alias bindings carry
root sets. Merged control flow can union root sets. Shared and mutable
conflicts are checked against overlapping roots.

### Exclusivity

- Many shared accesses can coexist.
- Exclusive access conflicts with overlapping active shared access.
- Exclusive access conflicts with another overlapping exclusive access.
- Mutation of an alias writes through to the referent where source semantics
  permit it.
- Invalid immutable mutation is rejected before backend lowering.
- GC does not relax exclusivity rules.
- Map alias rules use the same conflict machinery.

Borrow rules prevent conflicting mutation, invalid aliasing and logical use
after ownership transfer.

### Future-use analysis

Borrow validation uses control-flow future-use information to determine
whether an operation must borrow, may consume, or is path-dependent.

- **no future use:** an operation may consume ownership;
- **future use on all relevant paths:** operation remains a borrow;
- **path-dependent future use:** move-sensitive handling must remain
  conservative or reject inconsistency.

Future-use analysis answers whether a root is required again after the current
program point. It is a sufficient basis for safe consumption, not a
source-visible lifetime system.

The current checker precomputes may/must future-use facts and runs a forward
fixed-point transfer over reachable HIR blocks.

### Control flow and joins

- Branches are analysed independently.
- Loops participate in fixed-point analysis.
- Joins conservatively combine possible states and roots.
- Branch-local aliases are removed when they leave visibility.
- Inconsistent move behaviour across paths is rejected where necessary.
- Returns, breaks, and region exits inform advisory destruction planning.
- Infinite paths without exits do not require an exit drop merely because they
  loop.

### Function and external-call summaries

Borrow validation consumes and produces:

- function parameter mutability;
- function return-alias summaries;
- fresh result;
- alias of one or more parameters;
- unknown or imprecise alias result;
- external function access rules;
- external return-alias metadata.

Call transfer uses resolved metadata, not source imports or runtime names.

### Side-table facts

- function summaries;
- block-entry states;
- block-exit states;
- statement-entry states;
- statement access facts;
- terminator access facts;
- value access facts;
- return-alias summaries;
- reactive invalidation facts;
- advisory drop sites;
- aggregate statistics where useful for tooling.

These facts are read-only outputs keyed by HIR identity. They do not become a
second semantic IR and must not mutate HIR.

### Reactive invalidation

Reactivity V1 uses stable reactive sources observed by template subscriptions.
Subscriptions are not mutable borrows and do not hold exclusive access to the
source.

- A subscription is a read-only dependency.
- A subscription is not an active borrow.
- A subscription does not grant mutation.
- A subscription does not extend a source-language lifetime category.
- Writes, place mutations, map mutations, and mutable arguments can create
  invalidation facts.
- Ordinary alias and exclusivity checks still apply.

Under the GC baseline, reactive cells and mounted template instances stay
alive through ordinary reachability. Ownership-aware lowerings must preserve
the same semantics by ensuring a reactive cell or template-instance state is
not deterministically freed while a live subscription or mounted fragment can
still observe it.

Reactive invalidation is source-level in V1. Field, item, and path-level
invalidation are later optimizations and must not change the ownership
semantics of the underlying value.

### Conservative precision

Borrow validation may be conservative. Stronger field-, projection-, region-
or path-level analysis may accept more programs or unlock more optimisation,
but it must not change the semantics of already-valid programs.

Current design limitations:

- Projections may share their base root conservatively.
- Map lookup aliases the map conservatively.
- Joined control flow may retain mixed slot/alias state.
- Advisory drop sites are candidates, not exact source lifetimes.
- Exact region destruction may be refined later.

## Ownership and drops

### Ownership as an optimisation state

Beanstalk ownership is an optimisation state layered over GC-safe source
semantics. It is not a separate source type system.

- Ownership metadata is separate from semantic `TypeId`.
- Shared/mutable parameter syntax does not create owned/borrowed source types.
- Backend ownership eligibility is not required for correctness.
- GC fallback is always legal.
- Ownership-aware and GC paths must preserve identical observable behaviour.

The compiler may transfer destruction responsibility only where static
analysis proves that doing so cannot invalidate a later source use. If that
proof is unavailable, the value remains GC-managed.

### Inferred transfer

Assignments and calls can be consumption points. The compiler chooses borrow
or consumption. There is no move keyword. Consumption invalidates source
ownership state. Aliases cannot remain usable after a possible transfer. A
callee that receives ownership assumes eventual release responsibility.
Responsibility may be discharged by release or a further proven transfer.

Static analysis identifies legal consumption points and guarantees that no
later source use remains. Ownership-aware lowering may then use runtime
ownership metadata to choose whether a conditional transfer or destruction
path performs ownership work.

### Last-use consumption

The conceptual question is: is this value required again on any relevant
control-flow path after this operation?

- No later use → transfer may occur.
- Later use → operation borrows.
- Path-dependent outcome → analysis must conservatively preserve safety.
- Last use is sufficient for transfer, not necessarily the only future
  optimisation basis.
- The model does not expose exact lifetimes.

Ownership transfers at most once along a control-flow path. A borrowed value
is never released by the borrower. An owned value must reach a valid release
or transfer point. A later source use after possible transfer is invalid.
Joins conservatively account for paths that may own.

### Unified ownership ABI

Ownership-aware lowering uses one calling convention rather than separate
source-visible borrowed and owned function variants.

Heap-managed or handle-based values that participate in ownership lowering
carry ownership metadata with the runtime value. Scalar values may continue to
use target-native representations.

ABI behaviour:

- Borrowed metadata → callee must not destroy the value.
- Owned metadata → callee receives release responsibility.
- Callee must release or safely transfer that responsibility before it ends.
- Caller must not use a value again after a path that may have transferred
  ownership.
- Source function signatures do not distinguish borrowed and owned variants.
- Backend specialisation may remove redundant checks without changing the
  semantic ABI.

When ownership metadata is set, the callee receives release responsibility. It
must release the value or safely transfer that responsibility before its
ownership obligation ends.

### Tagged ownership state

The runtime ownership bit chooses between already-safe behaviours. It never
turns an invalid source program into a valid one.

```text
borrowed
    The callee may read or mutate only according to the source access contract,
    but it must not destroy the value.

owned
    The callee has destruction responsibility and must release or safely
    transfer it before that responsibility ends.
```

### Conditional destruction

`drop_if_owned` is inserted at possible destruction points. It checks ownership
metadata and releases only when responsibility is owned. It is a no-op for
borrowed state. Under pure GC it can lower to no-op. It can be eliminated when
ownership effect is statically known. It does not belong in source syntax.
Different backends may represent it differently while preserving behaviour.

### Control-flow exits

Control flow constructs (`if`, `loop`, `break`, `return`) interact with
ownership explicitly.

- Every control-flow exit that leaves a scope capable of owning a value has an
  associated `drop_if_owned`.
- Branches are analysed independently.
- Merges are conservative: if any path may own a value, a drop point must
  exist.

This guarantees deterministic destruction where enabled and correct GC fallback
where not. Infinite loops require no destruction unless they can exit.

Possible destruction points include normal region/scope exit, function return,
error return, loop break, branch exits, merged paths, non-exiting loops, and
return values that transfer responsibility out.

### Static specialisation

Ownership-aware specialisation can classify functions or call paths using
categories equivalent to:

```text
MayConsume
NeverConsumes
AlwaysConsumes
```

These are design categories, not required exact Rust enum names.

- `MayConsume` retains conditional ownership handling.
- `NeverConsumes` can omit drop responsibility.
- `AlwaysConsumes` may remove ownership-condition branches.
- Specialisation must not create source-visible overloads.
- Exact monomorphisation extent remains a backend and benchmarking decision.
- The unified semantic contract remains fixed even if implementation encoding
  evolves.

## Runtime and backend lowering

### Backend-neutral contract

Backends may choose different allocation, GC, handle and ownership
representations. They may not choose different language semantics.

- Borrow validation precedes target lowering.
- Invalid source access is rejected before backend codegen.
- Backend does not infer access from source syntax.
- Backend consumes HIR plus borrow facts.
- Ownership optimisation is optional.
- Backend representation cannot affect `copy`, alias, or `~` semantics.
- Fallback always preserves valid program behaviour.

### GC semantic baseline

In the baseline execution model:

- All heap values are managed by a garbage collector.
- No deterministic drops are required for correctness.
- `drop_if_owned` sites compile to no-ops.
- Borrowing rules still apply to prevent conflicting mutation, invalid
  aliasing and logical use after ownership transfer.

The GC semantic baseline may be realised through JavaScript host GC, Wasm GC,
a backend runtime collector or another target-specific collector. The progress
matrix records which target paths are available today.

### JavaScript

JavaScript normally realises the GC baseline through host reachability. Borrow
validation still determines valid source behaviour. Ownership-specific
advisory facts may be ignored. Explicit copy and alias semantics must still be
preserved. JS host behaviour must not silently mutate shared aliases or
duplicate values contrary to Beanstalk semantics. Reactive cells and mounted
fragments stay alive through runtime reachability.

### Wasm

Wasm can use GC-backed, linear-memory, handle, or hybrid lowering.
Ownership-aware heap values use handles or pointers carrying ownership state.
Borrow facts identify legal transfer and destruction points. Conditional drops
release owned handles. Missing ownership proof falls back to GC-managed
behaviour. Runtime helpers own allocation, release, string, collection, and
handle contracts. The unified ownership ABI applies across ownership-aware
calls. Source semantics remain backend independent.

`src/backends/wasm/hir_to_lir/ownership.rs` is the experimental
ownership-lowering hook point for Wasm.

### Runtime liveness

Live reactive subscriptions and mounted fragments can keep state observable.
Ownership-aware lowering must not free a source or template instance while it
remains observable. GC reachability is the semantic reference behaviour.
Finer invalidation granularity may improve performance but not lifetime
semantics.

### Borrow-fact consumption

Backends may consume:

- value access classifications;
- call effects;
- return-alias summaries;
- move decisions;
- advisory drop sites;
- region/exit information;
- reactive invalidation and liveness metadata.

Backends must not mutate those facts, reinterpret source syntax, invent new
move points that invalidate borrow validation, or treat missing facts as proof
of ownership.

### Fallback requirements

When a backend cannot prove or implement an ownership optimisation safely, it
must retain GC-managed behaviour for that value or feature.

A missing optimisation is valid. A semantic divergence is not.

### Compiler responsibilities

The compiler enforces memory safety through the following steps:

1. **AST lowering**, where:
   * types are checked;
   * name resolution happens;
   * eager folding of expressions takes place;
   * source access modes are validated.
2. **HIR lowering**, where:
   * control flow is linearized;
   * ownership boundaries are identified;
   * fresh mutable call arguments are materialised into compiler-owned locals
     before borrow validation;
   * recoverable checked-numeric failures are lowered into ordinary HIR
     branches and locals before borrow validation.
3. **Borrow validation**, which:
   * enforces exclusivity rules;
   * prevents illegal overlapping access;
   * detects invalid use after possible ownership transfer;
   * performs or consumes last-use analysis;
   * produces side-table facts for ownership-aware lowering;
   * identifies advisory `drop_if_owned` sites.
4. **Final lowering**, where:
   * ownership flags are generated;
   * possible drops become conditional frees;
   * runtime checks are emitted.

At no point does the compiler rely on undefined behaviour or unchecked
aliasing.

Borrow validation does not mutate HIR. It produces side-table facts keyed by
HIR/value identity. HIR remains the semantic representation under GC;
ownership-aware lowerings consult the side tables later.

## Design tradeoffs and extension points

Beanstalk intentionally trades maximal static precision for predictable
semantics, implementation tractability and backend flexibility.

Compared to a fully static borrow checker:

- Some ownership decisions are deferred to runtime.
- Some optimisation decisions are deferred to runtime instead of being fully
  statically resolved.
- Small runtime cost from `drop_if_owned` checks.

In exchange:

- The language remains approachable.
- The compiler remains tractable.
- The model integrates cleanly with Wasm.
- Future static analysis can be layered on incrementally.

Future enhancements might include:

- region-based memory management;
- stronger static lifetime inference;
- place-based alias tracking;
- drop elision via region scopes;
- compile-time ownership specialisation;
- retain elimination;
- escape analysis;
- GC elision for proven regions.

These extensions must preserve source-language semantics, GC fallback and the
unified ownership contract.

Internal compiler data structures, runtime encodings and target ABI details
may continue to evolve while backend implementation matures.
