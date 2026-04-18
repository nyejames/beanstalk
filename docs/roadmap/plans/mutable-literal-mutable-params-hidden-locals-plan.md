# Mutable Literal Arguments via Synthesized Hidden Locals

## Goal

Allow fresh rvalues to satisfy mutable function parameters directly without requiring an explicit user variable first.

This specifically enables calls such as:

```beanstalk
mutate([: content])
mutate({1, 2, 3})
mutate(MyStruct("x"))
```

for parameters declared as `~T`, while keeping `~literal` invalid.

The implementation direction is:

- fresh rvalues may satisfy `~T` parameters directly
- `~` remains place-only syntax at the call site
- the compiler lowers eligible fresh rvalue arguments through **synthesized hidden locals** in HIR
- borrow checking and later ownership analysis continue to operate on normal locals rather than a new HIR node kind

## Why this direction

The current frontend rule requires a mutable parameter to be passed with `~place`, and explicitly rejects `~` on literals, temporaries, or computed expressions. This is enforced in the call validation layer, not just documented behavior. `CallAccessMode` is currently only `Shared` or `Mutable`, and mutable validation requires the argument expression to be a mutable place in `src/compiler_frontend/ast/expressions/call_validation.rs`.

The current HIR already has the right primitives for hidden-local lowering:

- explicit block locals and explicit statements in `src/compiler_frontend/hir/hir_nodes.rs`
- compiler temp allocation in `src/compiler_frontend/hir/hir_expression.rs`
- HIR side tables for names and source mappings in `src/compiler_frontend/hir/hir_side_table.rs`
- call lowering that already materializes result temps in `src/compiler_frontend/hir/hir_expression/calls.rs`

The compiler design docs also already state that compiler-introduced locals are treated exactly like user locals, which makes this direction consistent with the documented architecture rather than a special-case deviation.

## Intended language rule

### Supported

A parameter declared as `~T` may be satisfied by either:

1. an explicit mutable place argument:

```beanstalk
value ~= [:
  content
]
mutate(~value)
```

2. a fresh compatible rvalue:

```beanstalk
mutate([: content])
mutate({1, 2, 3})
mutate(MyStruct("x"))
```

### Still rejected

These stay invalid:

```beanstalk
mutate(~[: content])
mutate(~"text")
mutate(~MyStruct("x"))
```

Reason: `~` remains a request for explicit mutable access to an existing place. Fresh rvalues do not need `~` because they are lowered as fresh owned values behind the scenes.

## Semantic model

The call-site rule becomes:

- `~place` means explicit mutable/exclusive access to an existing place
- plain fresh rvalues are allowed in `~T` slots because the compiler can materialize them as fresh hidden locals
- hidden locals are compiler-introduced implementation machinery only; they are not language-level temporaries

This preserves the existing meaning of `~` while removing the current ergonomic blocker.

## Non-goals

This plan does **not** introduce:

- a dedicated HIR node for fresh owned argument values
- new ownership syntax
- implicit mutability for ordinary named locals at call sites
- support for `~` on literals or computed expressions
- a broader rewrite of borrow checking or ownership lowering

## Current implementation seams that will change

### 1. AST call argument metadata and validation

Relevant files:

- `src/compiler_frontend/ast/expressions/call_argument.rs`
- `src/compiler_frontend/ast/expressions/call_validation.rs`
- `src/compiler_frontend/ast/expressions/function_calls.rs`
- likely constructor / method call surfaces that share the same resolver path

Current state:

- `CallArgument` stores only `value`, `target_param`, `access_mode`, and locations
- `CallAccessMode` is only `Shared` or `Mutable`
- `resolve_call_arguments()` validates type compatibility first, then validates access mode
- `validate_call_access_mode()` rejects mutable arguments unless the argument expression is a mutable place

Required design change:

The AST layer must distinguish three cases instead of two:

1. shared argument
2. mutable-place argument (`~place`)
3. fresh-rvalue argument accepted for a mutable parameter

The parser in `function_calls.rs` can stay mostly the same. The real change is in normalization and validation.

### 2. HIR lowering of call arguments

Relevant files:

- `src/compiler_frontend/hir/hir_expression.rs`
- `src/compiler_frontend/hir/hir_expression/calls.rs`
- `src/compiler_frontend/hir/hir_builder.rs`
- `src/compiler_frontend/hir/hir_side_table.rs`
- potentially `src/compiler_frontend/hir/hir_nodes.rs` if explicit provenance metadata is added there rather than only in side tables

Current state:

- `lower_call_expression()` lowers all args with ordinary `lower_expression()` and passes plain `Vec<HirExpression>` into `HirStatementKind::Call`
- temp locals already exist for expression lowering and call results via `allocate_temp_local()` in `hir_expression.rs`
- compiler temps are currently named `__hir_tmp_N` for diagnostics/debug rendering via the side table

Required design change:

Call lowering needs a new normalization step before the final `HirStatementKind::Call` is emitted:

- when a mutable parameter is satisfied by a fresh rvalue, lower that expression first
- synthesize a hidden local in the current block/region
- assign the lowered value into that local
- pass `Load(Local(temp))` as the call argument value

This should reuse the existing temp-local allocation and explicit assignment machinery rather than inventing a new HIR expression kind.

### 3. HIR side-table provenance for compiler-generated fresh-arg temps

Relevant file:

- `src/compiler_frontend/hir/hir_side_table.rs`

Current state:

- the side table stores source mappings and canonical human-readable names
- compiler temps already receive generated local names for rendering
- there is currently no explicit provenance/category metadata for locals

Required design change:

Add explicit provenance for hidden locals created for fresh mutable call arguments.

This should record enough information to support:

- clear HIR/debug rendering
- avoiding confusing diagnostics
- future ownership optimizations or audits

Recommended minimal metadata:

- local origin enum
  - `User`
  - `CompilerTemp`
  - `CompilerFreshMutableArg`
- optional originating call statement/value location
- optional argument index metadata if useful for debugging

This can live in the side table rather than the core HIR node structs unless a stronger reason appears during implementation.

### 4. Borrow checker integration

Relevant files:

- `src/compiler_frontend/analysis/borrow_checker/mod.rs`
- `src/compiler_frontend/analysis/borrow_checker/types.rs`
- `src/compiler_frontend/analysis/borrow_checker/transfer.rs`
- `src/compiler_frontend/analysis/borrow_checker/transfer/access.rs`
- `src/compiler_frontend/analysis/borrow_checker/transfer/call_semantics.rs`

Current state:

- mutable user parameters lower to `ArgEffect::MayConsume`
- mutable argument analysis in `transfer/access.rs` already treats `Load(place)` specially and otherwise falls back to collecting roots from the expression tree
- move-vs-borrow decisions for mutable user parameters are chosen from last-use facts in `classify_move_decision(...)`

Required design change:

The borrow checker should not need a new semantic category for fresh mutable arguments if HIR lowering always materializes them into locals first.

What must be checked carefully:

- synthesized fresh-arg locals must be tracked like ordinary locals in function layout/state
- move classification must treat those locals as fresh owned candidates, not as aliases
- advisory drop-site generation must remain correct when the call does not consume the local on all paths
- diagnostics should not surface raw hidden-local names unless intentionally debug-facing

Expected outcome:

The borrow checker continues to reason about ordinary locals and ordinary `Load(Local(...))` call arguments. That is the main benefit of this design.

### 5. Tests and matrix/docs updates

Relevant files:

- `tests/cases/manifest.toml`
- new or updated cases under `tests/cases/...`
- `docs/roadmap/language-surface-integration-matrix.md`
- `docs/language-overview.md`
- `docs/compiler-design-overview.md`
- `docs/memory-management-design.md`

Current state:

The language-surface matrix currently lists named args and call-site `~` behavior as implemented, and includes canonical rejection cases such as mutable-param-requires-explicit-tilde and tilde-on-non-place-expression. Those matrix rows will need to be updated so the supported surface reflects the new rule precisely rather than implying that mutable parameters always require `~place`.

## Design decisions

### Decision 1: do not add a new HIR node kind

Use synthesized hidden locals only.

Reason:

- HIR already has explicit locals, assignments, blocks, and side tables
- borrow checking already reasons over locals and places
- the compiler docs describe HIR as a stable semantic IR with ownership layered later
- a dedicated fresh-owned-arg HIR node would push ownership-specific detail too early into HIR

### Decision 2: keep `~` place-only

Fresh rvalues satisfying mutable parameters should **not** use `~`.

Reason:

- `~` already means explicit mutable/exclusive access to a place
- overloading it for literals would muddle its meaning
- keeping `~literal` invalid preserves syntax consistency

### Decision 3: lower only when the callee slot requires mutable access

Do not synthesize hidden locals for all rvalue call arguments.

Reason:

- shared rvalue arguments do not need this machinery
- eager temp synthesis would bloat HIR and analysis unnecessarily
- this feature should stay tightly scoped to the mutable-parameter gap

### Decision 4: keep the normalization boundary in the AST call layer

The AST call-resolution layer should decide whether an argument is:

- shared
- mutable place
- fresh mutable rvalue

HIR should consume already-normalized call metadata rather than rediscovering the rule from raw expression shapes.

Reason:

- call-shape policy already lives in `call_argument.rs` and `call_validation.rs`
- function calls, constructors, receiver methods, and shared call validation already centralize argument policy there

### Decision 5: preserve diagnostics that explain *why* `~literal` is invalid

Diagnostics should clearly distinguish:

- “this mutable parameter accepts a fresh rvalue without `~`”
- “`~` is only valid on mutable places”

That avoids a confusing user experience where the same literal is accepted without `~` but rejected with it.

## Proposed AST metadata shape

The exact names can change during implementation, but the normalized call metadata should gain an explicit argument-passing classification.

Recommended shape:

```rust
pub enum CallPassingMode {
    Shared,
    MutablePlace,
    FreshMutableValue,
}
```

This can either replace or refine `CallAccessMode` in `src/compiler_frontend/ast/expressions/call_argument.rs`.

Why not keep only `CallAccessMode`?

Because `Mutable` is currently overloaded. The compiler needs to know whether the caller provided:

- an existing mutable place
- or a fresh compatible rvalue that must be materialized later

That distinction should be explicit in the normalized AST call metadata.

## Proposed HIR lowering flow

For a call like:

```beanstalk
mutate([: content])
```

where `mutate` expects `|value ~String|`, HIR lowering should conceptually do:

1. lower `[: content]` to an HIR expression and any expression prelude
2. allocate a hidden temp local in the current block
3. emit `Assign(Local(temp), lowered_value)`
4. pass `Load(Local(temp))` into `HirStatementKind::Call`
5. tag the temp in the side table as `CompilerFreshMutableArg`

Conceptually:

```text
$tmp_fresh_arg_0 = <lowered template string>
call mutate(load $tmp_fresh_arg_0)
```

The concrete visible temp name can remain a generated debug name and does not need to become part of the language contract.

## Ownership/borrow expectations

The hidden local created for a fresh mutable argument should behave as:

- a fresh non-aliased local
- initialized immediately before the call
- eligible for ordinary mutable borrow vs move classification through existing `ArgEffect::MayConsume` handling

This means the existing last-use/may-consume machinery in `transfer/access.rs` and `transfer/call_semantics.rs` should remain the authority for move choice.

The important implementation constraint is:

- the hidden local must lower as a **slot local**, not as an alias local

That preserves the intended ownership behavior and avoids misclassifying the value as borrowed from elsewhere.

## Diagnostics requirements

The implementation should preserve or add the following diagnostics:

### Keep rejecting

```beanstalk
mutate(~[: content])
```

with a message equivalent to:

- `~` is only valid on mutable places
- fresh values for mutable parameters should be passed without `~`

### Accept

```beanstalk
mutate([: content])
```

when the parameter is `~String`.

### Keep rejecting

```beanstalk
mutate(value)
```

when `value` is a place and the parameter is `~T`.

Reason:

The explicit mutable-place rule still matters for existing named values. This change only removes the fresh-rvalue blocker.

## File-by-file implementation checklist

### AST

#### `src/compiler_frontend/ast/expressions/call_argument.rs`

- extend normalized call metadata to distinguish fresh mutable rvalues from mutable-place arguments
- keep helper constructors readable and explicit
- document the new meaning clearly at the type definition

#### `src/compiler_frontend/ast/expressions/call_validation.rs`

- update `validate_call_access_mode()` so mutable parameters accept:
  - mutable places with explicit `~`
  - fresh compatible rvalues without `~`
- keep shared-parameter rejection of `~arg`
- keep rejection of `~` on literals/computed expressions
- ensure the new rule is centralized here rather than scattered

#### `src/compiler_frontend/ast/expressions/function_calls.rs`

- keep parsing of `~` as syntax
- pass through enough metadata so the resolver can classify fresh mutable rvalues correctly
- update diagnostics around `~` misuse where necessary

#### other AST call surfaces sharing the same resolver

- audit struct constructor and receiver-method call paths that rely on shared call normalization
- confirm the new mutable-rvalue rule applies only where mutable parameters actually exist

### HIR

#### `src/compiler_frontend/hir/hir_expression/calls.rs`

- add a call-argument lowering step that materializes hidden locals for fresh mutable arguments before building the final HIR call statement
- keep ordinary shared args and mutable-place args on the existing lowering path
- reuse existing prelude sequencing and explicit statement emission

#### `src/compiler_frontend/hir/hir_expression.rs`

- reuse `allocate_temp_local()` and `emit_assign_local_statement()`
- add a dedicated helper for fresh mutable argument materialization if needed, rather than inlining that logic repeatedly

#### `src/compiler_frontend/hir/hir_builder.rs`

- no structural rewrite expected
- may need a dedicated counter/name convention for fresh mutable arg temps if you want them distinct from generic temp locals

#### `src/compiler_frontend/hir/hir_side_table.rs`

- add provenance metadata for compiler-generated locals
- make fresh mutable arg temps distinguishable from generic temps and user locals
- expose helper accessors needed by HIR display or diagnostics

#### `src/compiler_frontend/hir/hir_nodes.rs`

- only change this file if provenance must be promoted into core HIR data rather than staying in the side table
- prefer keeping core HIR unchanged if possible

### Borrow checker

#### `src/compiler_frontend/analysis/borrow_checker/transfer/access.rs`

- verify fresh-arg hidden locals flow through call transfer exactly like normal locals
- add focused tests for move-vs-borrow behavior on synthesized fresh locals
- ensure any diagnostics involving those locals remain user-friendly

#### `src/compiler_frontend/analysis/borrow_checker/transfer/call_semantics.rs`

- likely no semantic change needed if mutable user parameters remain `ArgEffect::MayConsume`
- confirm no alias metadata assumption breaks for fresh hidden locals

#### `src/compiler_frontend/analysis/borrow_checker/types.rs`

- no structural change required unless fresh-temp-specific facts or diagnostics need dedicated metadata

### Docs and roadmap

#### `docs/language-overview.md`

Update the function-call mutability rules to say:

- `~place` is required for existing mutable places
- fresh rvalues may satisfy `~T` parameters directly without `~`
- `~` on literals/temporaries/computed expressions remains invalid

#### `docs/compiler-design-overview.md`

Update the HIR and borrow-checking sections to note that:

- compiler-introduced hidden locals are used to materialize fresh mutable call arguments
- this is a HIR lowering strategy rather than a new language-level temporary concept

#### `docs/memory-management-design.md`

Clarify that fresh rvalues passed to mutable parameters are lowered to compiler-owned locals before borrow validation / last-use analysis.

#### `docs/roadmap/language-surface-integration-matrix.md`

Update the named-arguments / call-site mutability row so it reflects the new supported rule and new canonical cases.

#### `docs/roadmap/roadmap.md`

Add this plan to the Next Plans list.

## Test plan

### Parser / AST tests

Add or update focused tests around:

- fresh literal accepted for mutable parameter
- fresh template accepted for mutable parameter
- fresh struct constructor accepted for mutable parameter
- fresh collection accepted for mutable parameter
- `~literal` rejected with targeted diagnostic
- existing place still requires explicit `~`
- immutable/shared parameter still rejects `~arg`

Likely touch area:

- `src/compiler_frontend/ast/expressions/tests/function_call_tests.rs`

### HIR tests

Add focused lowering tests that assert:

- a fresh mutable argument produces a hidden local
- the hidden local is assigned before the call
- the call receives `Load(Local(temp))`
- side-table provenance is present if implemented

Likely touch areas:

- `src/compiler_frontend/hir/tests/...`
- especially tests near call lowering and statement lowering

### Borrow-checker tests

Add tests that assert:

- fresh mutable args are treated as fresh locals, not aliases
- move-vs-borrow behavior still works for mutable user parameters
- no false borrow conflict appears from the synthesized temp itself
- advisory drop sites remain sane where relevant

Likely touch area:

- `src/compiler_frontend/analysis/borrow_checker/tests/...`

### Integration tests

Add canonical end-to-end cases under `tests/cases/` for:

- mutable parameter with fresh template arg
- mutable parameter with fresh struct arg
- mutable parameter with fresh collection arg
- rejection of `~literal`
- rejection of missing `~` for existing mutable place

Also update:

- `tests/cases/manifest.toml`
- any goldens or expected diagnostic fragments needed by the new cases

## Done when

This plan is complete when all of the following are true:

- mutable user parameters accept fresh rvalues directly without requiring a user-declared temp
- `~literal` and `~computed_expression` still fail with clear diagnostics
- HIR lowers fresh mutable args through synthesized hidden locals rather than a bespoke node kind
- hidden locals have explicit compiler provenance suitable for debugging/diagnostics
- borrow checking and last-use analysis continue to work on normal locals without a semantic special case
- the language docs, compiler design docs, memory docs, roadmap, and language-surface matrix all reflect the new rule
- canonical parser, HIR, borrow-checker, and end-to-end tests exist

## Suggested roadmap blurb

`docs/roadmap/plans/mutable-literal-mutable-params-hidden-locals.md`

Support fresh rvalues directly in mutable (`~T`) function-parameter slots by lowering them through synthesized hidden locals in HIR. Keeps `~` place-only, keeps `~literal` invalid, avoids adding a new HIR node kind, and extends tests/docs for the new call-site rule.
