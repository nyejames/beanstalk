# Beanstalk Pre-Alpha Checklist

This is a working execution plan for getting the compiler to a credible first alpha.

Use the language surface integration matrix as a reference for what is currently implemented: `docs/language-surface-integration-matrix.md`

## Release gates

These are the non-negotiable conditions for starting Alpha.

- All claimed Alpha features compile, type check, and run through the full supported pipeline.
- Unsupported syntax or incomplete features fail with structured compiler diagnostics, not panics.
- The integration suite covers the supported language surface, not just recent feature areas.
- The JS backend and HTML builder are stable enough for real small projects and docs-style sites.
- Compiler diagnostics are useful, accurate, consistently formatted, and visually moving toward the Nushell-style goal.
- Cross-platform output is stable enough that Windows and macOS do not produce avoidable golden drift.

### REVERT MISTAKEN AST DRIFT

The regression began when AST was changed from:
- a consumer of header-owned top-level declaration knowledge

into:
- a pass-driven stage that rebuilds module-wide declaration/index state from sorted headers before lowering bodies.

The root misunderstanding was introduced in commit `9786f4f41cbec596fe9d2f5b3f7cd7a9594654fc`.
Later commits `8cd0572a8d01747975438a9c7b76757f5beb15fe` and `87df41a78d37a60bec59a740d1603bc9703e815a` expanded and entrenched AST-side normalization/finalization responsibilities.
`bdf78deb5ca8f0cb1bb673add470a3e366502cd9` is surrounding orchestration churn, not the root cause.

## Correct architecture to restore

Pipeline:

1. Tokenize files
2. Parse file headers
3. Dependency sort headers
4. Lower sorted headers directly into AST
   - top-level declarations are already known from headers
   - function/local declarations are added in order as encountered during body parsing

Principles:

- Header parsing owns top-level declaration discovery
- Dependency sorting owns inter-header order
- AST owns:
  - type resolution
  - constant folding
  - body lowering
  - template lowering only where genuinely required by the AST→HIR boundary
- AST does **not** rebuild the module declaration universe from headers
- Function/local declarations remain incremental and ordered


# Part 1 — finish the entry-fragment contract

## Overview

The goal is to finish the cleanup around top-level page fragments and make the builder/backend contract match the current design docs. Header parsing and AST-side top-level template lowering are already in the new shape. The remaining drift is now in HIR and the HTML Wasm export plan.

Desired design:

* entry `start()` is the only runtime fragment producer
* AST exposes folded const fragments with `runtime_insertion_index`
* builders merge AST const fragments into the runtime list returned by entry `start()`
* builders do not depend on HIR-level ordered fragment streams or scan entry-body internals for export decisions

Done so far:

* `parse_file_headers.rs` already emits `top_level_const_fragments`, tracks `runtime_insertion_index`, restricts implicit starts to the entry file, and rejects non-entry top-level executable code 
* `body_dispatch.rs` already lowers top-level runtime templates to `NodeKind::PushStartRuntimeFragment(...)` 
* `top_level_templates.rs` is already reduced to const-fragment collection and doc-fragment extraction 
* `pass_emit_nodes.rs` already treats entry `start()` as the runtime fragment producer and folds const templates separately 

## Work to do

### 1. Remove remaining HIR fragment-stream leftovers

Files:

* `src/compiler_frontend/hir/hir_nodes.rs`
* `src/compiler_frontend/hir/hir_builder.rs`

`StartFragment` and the old const-string pool are already gone. The remaining issue is `HirModule.entry_runtime_fragment_functions` and any lowering code that still treats HIR as carrying a builder-facing ordered runtime fragment list 

Keep `PushRuntimeFragment` only if it is the clearest lowering for `NodeKind::PushStartRuntimeFragment`. Remove `entry_runtime_fragment_functions` and any code that populates or consumes it.

### 2. Simplify the HTML Wasm export plan

Files:

* `src/projects/html_project/wasm/export_plan.rs`
* any HTML Wasm wrapper code that depends on the current export selection logic

`export_plan.rs` still walks reachable blocks from the entry function and exports direct user-function calls found inside `start()` 

Replace that with a direct contract:

* export entry `start()`
* export string/memory helper functions
* JS wrapper calls `start()`
* JS wrapper decodes the returned runtime fragment list
* builder merges `const_top_level_fragments` using `runtime_insertion_index`

Remove entry-body call scanning as an export-selection policy.

### 3. Resolve the runtime fragment return-type contract

Files:

* `src/compiler_frontend/ast/module_ast/pass_emit_nodes.rs`
* `src/compiler_frontend/ast/module_ast/pass_declarations.rs`
* relevant HIR type-lowering code
* `src/compiler_frontend/mod.rs`
* `docs/compiler-design-overview.md`

The docs say entry `start()` returns `Vec<String>` 

The implementation currently uses `DataType::Collection(Box::new(DataType::StringSlice), Ownership::MutableOwned)` for the implicit start signature in both emit and declaration collection  

Pick one canonical contract and align all layers. If `StringSlice` is the real semantic carrier, document it precisely. If the semantic contract is `Vec<String>`, reflect that clearly in the frontend type story and backend expectations.

## Done when

* `HirModule` does not carry a builder-facing ordered runtime fragment list
* HTML Wasm exports entry `start()` directly
* builder-side merging is driven by AST const fragment metadata plus entry `start()` output
* the runtime fragment return type is named consistently in code and docs

# Part 2 — remove AST-side module declaration recollection

## Overview

The goal is to restore the original ownership split: headers and dependency sorting own top-level declaration discovery, and AST lowers sorted headers without rebuilding the module symbol manifest.

Desired design:

* top-level declaration discovery is header-owned
* AST does not recollect module-wide declarations
* AST does not rebuild per-file declared-path/name tables
* AST consumes a shared top-level symbol manifest prepared before body lowering

## Things to preserve

Do not remove or redesign the current local declaration path inside function bodies.
Keep:
- `new_declaration(...)`
- `context.add_var(...)`
- local declaration insertion in source order as statements are parsed

That part still matches the original architecture.

Done so far:

* header parsing already owns top-level declaration discovery and start-function classification in the way described by the compiler design overview  
* bare file imports and imported file starts are already gone on the language-rule side, which removes one reason AST previously had extra import/start bookkeeping 

## Work to do

### 1. Delete the declaration recollection pass

Files:

* `src/compiler_frontend/ast/module_ast/pass_declarations.rs`
* `src/compiler_frontend/ast/module_ast/orchestrate.rs`

`pass_declarations.rs` still rebuilds a module-wide declaration and visibility database inside AST, including start declaration stubs and builtin absorption 

Delete this pass. Remove `collect_declarations(...)` from `orchestrate.rs` and remove the stale pass-order comments that still present AST construction as declaration collection first 

### 2. Move symbol-manifest ownership out of AST

Files:

* `src/compiler_frontend/headers/parse_file_headers.rs`
* `src/compiler_frontend/mod.rs`
* `src/compiler_frontend/module_dependencies.rs`
* new manifest module if needed

Introduce a frontend-owned manifest that is prepared before AST construction.

Recommended shape:

* top-level declaration stubs
* export visibility
* per-file visible symbol sets
* canonical symbol-to-source mapping
* file import metadata
* builtin manifest merged once here

AST should receive this manifest and consume it. It should not reconstruct it.

### 3. Update AST construction API to consume the manifest

Files:

* `src/compiler_frontend/mod.rs`
* `src/compiler_frontend/ast/module_ast/orchestrate.rs`

`mod.rs` and `orchestrate.rs` still reflect the old ownership model in both comments and inputs  

After the manifest exists, update the AST entrypoint so it takes:

* sorted headers
* top-level const fragment metadata
* shared symbol manifest
* existing AST build context

AST construction should then begin from manifest-driven visibility/type resolution and ordered header lowering, not from symbol recollection.

## Done when

* `pass_declarations.rs` is deleted
* `collect_declarations(...)` no longer exists in AST orchestration
* the top-level symbol manifest is created before AST and passed in
* AST no longer owns module declaration discovery

# Part 3 — shrink AST state and replace cloned declaration scopes

## Overview

The goal is to finish the real AST simplification. Once recollection is removed, `AstBuildState` and `ScopeContext` can be reduced to true AST-stage responsibilities and layered scope growth.

Desired design:

* `AstBuildState` only carries AST-stage state
* `ScopeContext` does not clone a full module declaration vec into every child scope
* top-level declarations come from the shared manifest
* parameters and locals grow incrementally in source order

Done so far:

* the AST body-lowering path already has the right semantic direction for start/runtime template handling
* import semantics are already simpler because start aliasing is gone
* the remaining complexity is now concentrated in `AstBuildState`, `ScopeContext`, and the orchestration around them

## Work to do

### 1. Shrink `AstBuildState`

File:

* `src/compiler_frontend/ast/module_ast/build_state.rs`

`AstBuildState` still stores a second symbol database:

* `importable_symbol_exported`
* `file_imports_by_source`
* `declared_paths_by_file`
* `declared_names_by_file`
* `module_file_paths`
* `canonical_source_by_symbol_path`
* `register_declared_symbol(...)` 

Move that state to the shared manifest layer. Keep only true AST-stage state:

* emitted AST nodes
* warnings
* module constants
* folded const template values
* resolved type/signature tables
* rendered path usage sink
* builtin AST payloads only if still needed after manifest construction

### 2. Rewrite `ScopeContext` around layered scope data

Files:

* `src/compiler_frontend/ast/module_ast/scope_context.rs`
* `src/compiler_frontend/ast/module_ast/pass_emit_nodes.rs`
* any body-lowering helpers that assume cloned full declaration vectors

`ScopeContext` still stores `declarations: Vec<Declaration>` and clones it into child contexts. `new_child_function`, `new_template_parsing_context`, `new_constant`, and `add_var(...)` still operate in that model 

Replace it with:

* immutable shared top-level declaration view from the manifest
* small local declaration layer for parameters and locals
* `add_var(...)` only extends the local layer
* visibility gating still applied per file as needed

This is the core fix for source-ordered local growth without carrying a cloned module declaration vec everywhere.

### 3. Rework AST emission and import visibility to use the manifest + layered scopes

Files:

* `src/compiler_frontend/ast/module_ast/pass_emit_nodes.rs`
* `src/compiler_frontend/ast/import_bindings.rs`
* constant-header/type-resolution call sites

`import_bindings.rs` already enforces the correct import rules, but it still resolves visibility against AST-owned symbol tables populated by recollection 

Rewire it so:

* visibility gates come from the shared manifest
* constant-header resolution consumes manifest data plus per-file visible symbols
* `pass_emit_nodes.rs` builds function/start contexts from the manifest + layered local scope model, not from large prebuilt declaration vecs

## Done when

* `AstBuildState` no longer stores a second symbol registration database
* `ScopeContext` no longer clones the full module declaration vec into child contexts
* file visibility is resolved from the shared manifest
* function and start bodies grow only local/parameter scope incrementally

# Part 4 — finalize, audit, document, and clean tests

## Overview

The goal is to finish the refactor cleanly. This phase makes the new AST pipeline readable, aligned with the docs, and free of leftover lint/style drift.

Desired design:

* the AST pipeline is easy to follow in `orchestrate.rs`
* comments explain what each stage is doing and why it exists in the overall compiler pipeline
* touched code follows the style guide and compiler design docs
* tests reflect the new ownership model, not the old recollection model

Done so far:

* the top-level template rewrite already removed a large amount of fragment-specific complexity
* the remaining work is now mostly structural cleanup, documentation alignment, and test/lint follow-through

## Work to do

### 1. Simplify `orchestrate.rs` to match the final architecture

Files:

* `src/compiler_frontend/ast/module_ast/orchestrate.rs`
* `src/compiler_frontend/mod.rs`

After Parts 1–3, rewrite `orchestrate.rs` so the pass sequence reflects the real pipeline:

1. consume shared top-level symbol manifest
2. resolve import visibility and type/signature tables
3. lower sorted headers directly
4. finalize const fragments, doc fragments, template normalization, and module constants

Remove all stale wording that still describes AST as reconstructing declarations first  

### 2. Review the touched areas against the style guide and compiler design docs

Files:

* all files touched by this plan
* `docs/compiler-design-overview.md`
* `docs/codebase-style-guide.md`

Do a deliberate alignment pass:

* stage ownership matches the design overview
* module boundaries remain clear
* no transitional wrappers or compatibility shims remain
* comments explain behavior and rationale, not syntax
* error paths use structured diagnostics and avoid user-input panics
* naming stays explicit and full
* files and functions still have one clear responsibility  

### 3. Add strong comments to the new AST pipeline key parts

Files:

* `src/compiler_frontend/ast/module_ast/orchestrate.rs`
* `src/compiler_frontend/ast/module_ast/build_state.rs`
* `src/compiler_frontend/ast/module_ast/scope_context.rs`
* `src/compiler_frontend/ast/import_bindings.rs`
* `src/compiler_frontend/ast/module_ast/pass_emit_nodes.rs`

The newly refactored AST pipeline key parts should be clearly commented.

Comments should explain:

* what this stage owns
* what it no longer owns
* why header parsing now owns top-level declaration discovery
* how AST uses the shared manifest
* how local scope growth works during body lowering
* how entry `start()` and const fragment finalization relate to the big picture

Use concise WHAT/WHY comments and file-level docs. The style guide requires this level of explanation for complex stage logic 

### 4. Rewrite tests for the final architecture

Focus areas:

* manifest-driven import visibility
* layered local scope growth
* no AST-side declaration recollection assumptions
* entry `start()` as the runtime fragment producer
* builder merge behavior for const fragments + runtime fragments
* HTML Wasm export plan behavior after direct entry `start()` export

Delete or rewrite tests that are only validating the old recollection model.

### 5. Clean remaining lints and dead code

This final phase must explicitly include:

* cleaning up any remaining `clippy` lints
* reviewing dead code, stale `#[allow(dead_code)]`, and leftover unused paths
* removing stale comments and docs from the old architecture
* running the required checks from the style guide:

  * `cargo clippy`
  * `cargo test`
  * `cargo run tests` 

## Done when

* `orchestrate.rs` reflects the final AST pipeline clearly
* touched code follows the style guide and compiler design docs
* key AST pipeline files are well commented with clear WHAT/WHY and stage ownership
* tests validate the new architecture
* remaining clippy lints in touched areas are cleaned up
* no stale architectural comments from the old model remain








# Implementation plan: make `/` real division and `//` integer division

### Design decision

* `/` is always real division.
* `Int / Int` naturally evaluates to `Float`.
* `//` becomes integer division.
* `Int // Int` naturally evaluates to `Int`.
* In explicitly `Int` contexts, using `/` should produce a targeted type error suggesting `//` or `Int(...)`.
* `//=` should exist if `//` exists.
* The old `//` root operator should be removed and replaced later with an explicit builtin/function/method design.

### Current repo anchors

The current compiler shape already gives you clean ownership boundaries for this change:

* Contextual numeric coercion is still intentionally narrow and only handles `Int -> Float` at declaration/return sites in `src/compiler_frontend/type_coercion/numeric.rs` and `compatibility.rs`  
* The tokenizer currently treats `//` as `Root` and `//=` as `RootAssign` in `src/compiler_frontend/tokenizer/lexer.rs` and `tokens.rs`  
* Arithmetic typing still resolves `Int / Int` as `Int` in `src/compiler_frontend/ast/expressions/eval_expression/operator_policy/arithmetic.rs` 
* Constant folding still performs integer division for `Int / Int` in `src/compiler_frontend/optimizers/constant_folding.rs` 
* HIR and JS backend still carry/lower `Root` as a real operator in `src/compiler_frontend/hir/hir_nodes.rs`, `hir_expression/operators.rs`, and `src/backends/js/js_expr.rs`   

## Phase 1: reclaim `//` in the tokenizer and AST

### Files

* `src/compiler_frontend/tokenizer/tokens.rs`
* `src/compiler_frontend/tokenizer/lexer.rs`
* `src/compiler_frontend/ast/expressions/expression.rs`
* `src/compiler_frontend/ast/expressions/parse_expression_dispatch.rs`

### Changes

* Rename token/operator concepts:

  * `TokenKind::Root` -> `TokenKind::IntDivide`
  * `TokenKind::RootAssign` -> `TokenKind::IntDivideAssign`
  * `Operator::Root` -> `Operator::IntDivide`
* Update lexer behavior:

  * `//` -> `IntDivide`
  * `//=` -> `IntDivideAssign`
* Update token helpers:

  * `is_assignment_operator()`
  * `continues_expression()`
* Update expression dispatch so `TokenKind::IntDivide` lowers to `Operator::IntDivide`
* Remove root-operator parsing entirely

### Notes

This should be a hard replacement, not a compatibility layer. Pre-alpha is the right time to delete the old syntax cleanly.

## Phase 2: change operator typing rules

### Files

* `src/compiler_frontend/ast/expressions/eval_expression/operator_policy/arithmetic.rs`
* `src/compiler_frontend/ast/expressions/eval_expression/operator_policy/diagnostics.rs`

### New typing rules

* `Int + Int -> Int`
* `Int - Int -> Int`
* `Int * Int -> Int`
* `Int % Int -> Int`
* `Int / Int -> Float`
* `Int // Int -> Int`
* Mixed `Int`/`Float` arithmetic remains `Float`
* `//` should be `Int`-only for now

### Recommended restrictions

Reject:

* `Float // Float`
* `Int // Float`
* `Float // Int`

That keeps `//` simple and predictable.

### Diagnostics to add

When `/` appears in an explicitly `Int` context, emit a targeted type error like:

* “Regular division returns `Float`.”
* “Use `//` for integer division.”
* “Use `Int(...)` for an explicit conversion.”

That is much better than a generic expected/found message.

## Phase 3: keep contextual coercion narrow

### Files

* `src/compiler_frontend/type_coercion/compatibility.rs`
* `src/compiler_frontend/type_coercion/numeric.rs`

### Changes

Do not expand coercion policy.

Keep:

* implicit `Int -> Float` only
* only at contextual boundaries such as declarations and returns

Do not add:

* implicit `Float -> Int`
* general “expression-level float defaulting”
* special hidden coercion paths for `//`

### Why

The current frontend separation is good:

* operator typing decides the natural type of an expression
* contextual coercion applies afterwards only where the language explicitly allows it  

This change should preserve that architecture.

## Phase 4: fix constant folding to match runtime semantics

### File

* `src/compiler_frontend/optimizers/constant_folding.rs`

### Changes

Update constant folding so:

* `5 / 2` folds to `2.5` as `Float`
* `5 // 2` folds to `2` as `Int`

Add explicit support for `Operator::IntDivide`.

Keep zero-division checks for both operators.

### Recommended integer division rule

Use truncation toward zero.

Examples:

* `5 // 2 -> 2`
* `-5 // 2 -> -2`
* `5 // -2 -> -2`

That is the easiest rule to mirror consistently in Rust-style logic and in the JS backend.

### Important

Constant folding and runtime lowering must match exactly. This is not optional.

## Phase 5: update compound assignment

### File

* `src/compiler_frontend/ast/expressions/mutation.rs`

### Changes

Add support for:

* `//=`

Keep support for:

* `/=`

But change semantics:

* `x /= y` should only be valid when the target type can accept the division result
* `Int /= Int` should now fail because `/` produces `Float`
* `Int //= Int` should succeed
* `Float /= Int` should succeed

### Recommended behavior

```beanstalk
x Int ~= 10
x /= 4      -- error
x //= 4     -- ok, x becomes 2

y Float ~= 10
y /= 4      -- ok, y becomes 2.5
```

## Phase 6: thread the new operator through HIR

### Files

* `src/compiler_frontend/hir/hir_nodes.rs`
* `src/compiler_frontend/hir/hir_expression/operators.rs`
* `src/compiler_frontend/hir/hir_display.rs` if needed

### Changes

* Replace `HirBinOp::Root` with `HirBinOp::IntDiv`
* Map AST `Operator::IntDivide` to HIR `IntDiv`
* Update result-type inference:

  * `Div` may now produce `Float` even for two `Int` operands
  * `IntDiv` produces `Int`

### Important

The current HIR inference logic is still shaped around the old operator split, so this must be updated alongside AST typing, not later  

## Phase 7: update backend lowering

### Files

* `src/backends/js/js_expr.rs`
* any future Wasm/LIR operator-lowering sites

### Changes

* Keep `Div` lowered as `/`
* Add `IntDiv` lowering
* Remove root-operator lowering

### Recommended JS lowering

Lower integer division as truncation toward zero:

```text
Math.trunc(left / right)
```

That matches the recommended constant-folding rule.

### Why

The JS backend currently lowers `Div` as raw `/` and `Root` as `Math.pow(...)` . This is the exact place where backend semantics will drift if you do not update it.

## Test plan

Follow the existing repo preference for strong integration tests using real Beanstalk snippets and artifact/golden assertions, not just narrow unit tests 

### Unit tests

Add or update tests for:

* tokenization of `//`
* tokenization of `//=`
* `Int / Int -> Float`
* `Int // Int -> Int`
* invalid mixed `//` cases
* constant folding of `/`
* constant folding of `//`
* divide-by-zero for both
* `/=` on `Int` target failing
* `//=` on `Int` target succeeding

### Integration tests

Add cases for:

* top-level real division output
* top-level integer division output
* `Int` declaration rejecting `/`
* `Float` declaration accepting `/`
* `Int` return rejecting `/`
* `Int` return accepting `//`
* `Float /= Int`
* `Int /= Int` failure
* `Int //= Int` success
* mixed `Int`/`Float` real division
* invalid `//` mixed numeric usage

## Documentation updates

### `docs/language-overview.md`

Add or update a numeric semantics section.

Suggested content:

* Whole-number literals are `Int`
* Decimal literals are `Float`
* `+`, `-`, `*`, `%` preserve `Int` when both operands are `Int`
* `/` is real division and returns `Float`
* `//` is integer division and requires `Int` operands
* There is no implicit `Float -> Int`
* Use `//` for integer division
* Use `Int(...)` for explicit conversion when you really want one

Also update any operator tables/examples that still imply integer `/`.

### `docs/compiler-design-overview.md`

Update the “Type checking and coercion” section to reflect the new split:

* generic expression evaluation stays strict
* contextual coercion is still only `Int -> Float`
* `/` is an operator-owned typing rule, not contextual coercion
* `Int / Int` naturally evaluates to `Float`
* `//` is a separate integer-division operator

That keeps the docs aligned with the current compiler architecture rather than muddying the boundary between operator typing and contextual coercion 

### `docs/language-overview.md` and any syntax references mentioning roots

Remove operator-based root syntax.

Do not document the replacement root API in the same PR unless you are actually implementing it.

For this change, the cleaner move is:

* remove `//` as root syntax
* leave roots for a later explicit builtin/function/method design pass

## Cleanup checklist

* remove `Root` and `RootAssign` token names
* remove `Operator::Root`
* remove `HirBinOp::Root`
* remove JS lowering for root operator
* remove stale comments mentioning `//` as roots
* update any failing snapshots/goldens
* update diagnostics text that still describes `/` as integer-preserving

## Recommended PR breakdown

### PR 1

* reclaim `//`
* rename tokens/operators
* change AST arithmetic typing
* update constant folding
* add diagnostics
* add unit tests

### PR 2

* thread `IntDiv` through HIR
* update JS/backend lowering
* add `//=`
* add integration coverage

### PR 3

* docs pass
* remove leftover root references
* cleanup stale comments/tests

## Final recommendation

This is the right change.

It fixes the surprising part of integer arithmetic without making the whole numeric system fuzzy. The important discipline is to keep the rule narrow:

* `/` is real division
* `//` is integer division
* no implicit `Float -> Int`
* explicit `Int` contexts reject `/` with a helpful error

That gives Beanstalk a clean numeric story instead of a permissive one.








 
# Refactor collection builtins into explicit compiler-owned operations and remove compatibility-shaped dispatch

Collection builtins should lower through an explicit compiler-owned representation instead of leaning on method-call-shaped compatibility scaffolding. This removes fake dispatch surface, simplifies backend contracts, and makes collection semantics easier to audit for Alpha.

**Why this PR exists**

The language rules are already clear: collection operations are compiler-owned builtins, not ordinary user-defined receiver methods. The current implementation still carries method-call-shaped indirection, including synthetic builtin paths and compatibility behavior that blurs the semantic boundary. That is workable in pre-alpha, but it is exactly the kind of representation drift that makes backend audits noisy and future maintenance harder.

**Goals**

* Represent collection builtin operations explicitly as compiler-owned operations.
* Remove synthetic “pretend method” compatibility paths where they no longer carry semantic value.
* Keep call-site mutability rules strict and explicit.
* Make collection lowering easier to audit in JS and HTML/Wasm runtime-heavy tests.

**Non-goals**

* No change to user-facing collection syntax in this PR.
* No redesign of collection semantics or error-return behavior.
* No broad container-type redesign.

**Implementation guidance**

#### 1. Replace method-shaped collection builtin representation

Audit how collection builtins currently move through AST/HIR/backend lowering.

The target shape should make it obvious that these are not normal receiver methods. Choose one current representation and thread it through:

**Preferred direction**

* add a dedicated compiler-owned builtin operation representation for collection operations

Possible shapes:

* dedicated AST node variants such as:

  * `CollectionGet`
  * `CollectionSet`
  * `CollectionPush`
  * `CollectionRemove`
  * `CollectionLength`
* or a smaller shared builtin-op enum if that keeps lowering cleaner

Avoid keeping synthetic method paths just to preserve the old AST shape.

#### 2. Remove compatibility-only dispatch artifacts

Clean up compatibility-shaped pieces such as:

* synthetic builtin method path for `set`
* collection-op lowering that depends on pretending there is a normal method symbol behind the syntax
* any compatibility branch retained only because older AST/HIR/backend shapes expected methods everywhere

Keep only what is still semantically justified.

#### 3. Re-audit mutability and place validation at the builtin boundary

Use this PR to make collection builtin validation visibly consistent with the language guide:

* mutating collection operations require explicit mutable/exclusive access at the receiver site
* non-mutating operations reject unnecessary `~`
* mutating operations require a mutable place receiver
* indexed-write / `get(index) = value` behavior remains explicit and compiler-owned

The parser/frontend diagnostics for these cases should stay clear and specific.

#### 4. Simplify HIR/backend lowering contracts

Once AST stops pretending these are methods, lower them through a smaller explicit contract.

Target result:

* HIR and JS lowering do not need to infer “is this really a collection builtin disguised as a method call?”
* lowering logic can switch on a dedicated builtin-op kind
* collection get/set/remove/push/length semantics become easier to test directly

#### 5. Re-check JS runtime helper usage against frontend semantics

Audit the emitted JS/runtime behavior for:

* `get`
* `set`
* `push`
* `remove`
* `length`

Specifically check for “working by accident” behavior and for any mismatch between current frontend validation and runtime helper semantics.

#### 6. Strengthen backend-facing coverage

Expand tests so collection behavior is not only parser/frontend-covered but also backend-contract-covered.

Add or improve cases for:

* successful `get/set/push/remove/length`
* out-of-bounds `get`
* explicit mutable receiver requirement for mutating ops
* indexed write forms
* result propagation/fallback after `get`
* HTML-Wasm runtime-sensitive collection paths where emitted runtime behavior matters

**Primary files to audit**

* `src/compiler_frontend/ast/field_access/collection_builtin.rs`
* `src/compiler_frontend/ast/field_access/mod.rs`
* relevant AST/HIR lowering files for method/builtin calls
* JS runtime helper emission and expression/statement lowering
* integration fixtures covering collection operations

**Checklist**

* Introduce one explicit representation for collection builtins.
* Remove synthetic method-path compatibility scaffolding where it is no longer needed.
* Keep parser/frontend mutability/place validation aligned with the language rules.
* Thread the new builtin-op shape through HIR/backend lowering.
* Re-audit JS runtime semantics for all collection builtins.
* Add backend-facing and HTML-Wasm-sensitive regression coverage.
* Remove stale compatibility branches and comments once the new shape lands.

**Done when**

* Collection builtins no longer depend on fake method-dispatch representation.
* AST/HIR/backend code treats collection ops as compiler-owned operations explicitly.
* Mutability/place diagnostics remain clear and correct.
* JS/backend tests prove collection behavior directly rather than indirectly through compatibility shape.

**Implementation notes for the later execution plan**

* Keep the representation change central and mechanical: choose one shape and thread it through.
* Avoid adding a second abstraction layer just to preserve old code.
* Land this before or alongside the JS backend semantic audit so the audit sees the final builtin representation.


# PR - Split the JS runtime prelude by concern and harden backend helper contracts

The JS backend runtime prelude currently centralizes too many unrelated helper groups in one file. Split it into focused modules, keep one small orchestration layer, and add stronger tests around the helper contracts that define Alpha runtime semantics.

**Why this PR exists**

The JS backend is the near-term stable backend and one of the main Alpha product surfaces. The runtime prelude is readable and well commented, but it is still too broad in one file: bindings, aliasing, computed places, cloning, errors, results, collections, strings, and casts all live together. That makes semantic auditing, targeted refactors, and regression testing harder than they need to be.

**Goals**

* Split the JS runtime helper emission into small focused modules.
* Preserve the current runtime semantics exactly unless a bug is being intentionally fixed.
* Make helper-group ownership obvious.
* Strengthen targeted tests for each helper surface.

**Non-goals**

* No wholesale JS backend redesign.
* No formatting/style churn unrelated to helper extraction.
* No user-facing language changes.

**Implementation guidance**

#### 1. Split `prelude.rs` into focused runtime helper modules

Refactor the current prelude into a small orchestration module plus focused helper emitters.

**Suggested structure**

* `src/backends/js/runtime/mod.rs`
* `src/backends/js/runtime/bindings.rs`
* `src/backends/js/runtime/aliasing.rs`
* `src/backends/js/runtime/places.rs`
* `src/backends/js/runtime/cloning.rs`
* `src/backends/js/runtime/errors.rs`
* `src/backends/js/runtime/results.rs`
* `src/backends/js/runtime/collections.rs`
* `src/backends/js/runtime/strings.rs`
* `src/backends/js/runtime/casts.rs`

The top-level emitter should only own:

* helper emission order
* high-level comments about why these groups exist
* any tiny shared glue that genuinely belongs at orchestration level

#### 2. Keep helper boundaries semantically intentional

Use the split to make helper responsibilities clearer:

* binding helpers: reference record construction, parameter normalization, read/write resolution
* alias helpers: borrow/value assignment semantics
* computed-place helpers: field/index place access
* clone helpers: explicit `copy` semantics
* error helpers: canonical runtime `Error` construction and context helpers
* result helpers: propagation and fallback behavior
* collection helpers: runtime contracts for ordered collections
* string helpers: string coercion and IO
* cast helpers: numeric/string cast behavior and result-carrier error paths

Avoid “misc” modules. Keep each file narrow.

#### 3. Re-check helper APIs for accidental overlap or leakage

During extraction, audit whether helper groups expose duplicated or cross-cutting behavior that should be simplified.

Examples to watch for:

* collection helpers depending on unrelated error-helper details without a clean boundary
* result helpers assuming too much about caller lowering shape
* alias/binding helpers carrying responsibilities that belong in computed-place helpers

Do not redesign aggressively; just remove obvious leakage.

#### 4. Strengthen JS backend tests around runtime contracts

Add targeted tests for helper-backed semantics, not just broad output snapshots.

Focus on:

* aliasing and assignment semantics
* explicit copy behavior
* result propagation/fallback helpers
* builtin error helper lowering
* collection runtime helpers
* cast success/failure behavior
* mutable receiver / place validation paths where JS runtime behavior depends on correct lowering

Prefer targeted artifact assertions or rendered-output assertions where full JS snapshots are noisy.

#### 5. Keep comments strong while reducing file breadth

The current prelude comments are useful. Preserve that quality after the split:

* each runtime helper file gets a short module doc comment
* each emitter function explains WHAT/WHY at the group level
* avoid repeating a giant duplicated overview in every file

**Primary files to touch**

* `src/backends/js/prelude.rs`
* `src/backends/js/mod.rs`
* JS backend tests and integration fixtures with runtime-heavy behavior

**Checklist**

* Split the JS runtime prelude into focused helper-group modules.
* Keep one small orchestration layer responsible for emission order.
* Preserve current helper semantics unless fixing an identified bug.
* Audit for duplicated or leaked helper responsibilities during extraction.
* Add or expand targeted tests for helper-backed runtime semantics.
* Prefer targeted assertions over brittle full-file snapshots where code shape is not the contract.

**Done when**

* No single JS runtime helper file owns most of the backend runtime surface.
* Helper-group ownership is obvious from file layout.
* Existing JS semantics remain stable.
* Runtime-heavy test coverage is stronger and lower-noise than before.

**Implementation notes for the later execution plan**

* Keep the first pass mostly structural.
* Only fix helper semantics in the same PR when the bug is obvious and covered.
* This PR should make the later “JS backend semantic audit for Alpha surface” materially easier.

# PR - Migrate remaining brittle fixtures, prune redundant coverage, and close the Alpha test matrix gaps

Now that the integration runner supports strict goldens, normalized goldens, rendered-output assertions, and targeted artifact assertions, finish migrating brittle fixtures to the right assertion surface, remove redundant cases that no longer add value, and fill the most visible Alpha-surface coverage gaps.

**Why this PR exists**

The integration runner is already capable of lower-noise assertion modes. The remaining work is fixture migration and coverage curation: some fixtures are still too brittle for what they actually test, some gaps remain visible in the language surface matrix, and some older coverage is now redundant or weaker than newer canonical cases.

**Goals**

* Migrate remaining brittle fixtures to normalized, rendered-output, or targeted artifact assertions where appropriate.
* Keep strict byte-for-byte goldens only where exact output shape is actually the contract.
* Fill the clearest remaining Alpha-surface gaps.
* Remove or rewrite redundant tests that duplicate stronger canonical coverage.
* Keep the matrix and manifest aligned with the real supported surface.

**Non-goals**

* No broad feature expansion.
* No weakening of semantic checks just to reduce failures.
* No mass deletion of tests without replacing lost confidence.

**Implementation guidance**

#### 1. Audit all remaining brittle fixtures by assertion intent

For each currently noisy fixture, decide what it is really testing:

* **Strict golden** when exact HTML/JS/Wasm shape is the contract
* **Normalized golden** when emitted code structure matters but counter-name drift is noise
* **Rendered output** when runtime behavior is the contract
* **Artifact assertions** when only a few targeted output properties matter

Document the migration reason in the PR notes so future fixture authors can follow the pattern.

#### 2. Migrate the remaining runtime-fragment-heavy brittle cases

Prioritize fixtures where full generated-output snapshots are still too noisy compared with the semantic intent.

Common candidates:

* runtime fragment ordering / interleave behavior
* result propagation/fallback through generated output
* runtime collection read/write flows
* call/lowering paths where helper/counter drift is noisy
* short-circuit/runtime behavior cases where rendered output is the real contract

#### 3. Fill the explicit matrix gaps

Add or strengthen canonical cases for the most visible remaining gaps:

* choice / match backend-runtime coverage
* char failure diagnostics
* HTML-Wasm collection runtime coverage
* cross-platform newline / rendering drift-sensitive surfaces
* any remaining receiver-method runtime-sensitive cases outside plain JS coverage

Where possible, prefer one strong canonical fixture over several narrow redundant fixtures.

#### 4. Prune or rewrite redundant coverage

Audit tests that are now redundant because newer canonical cases cover the same behavior more clearly.

Candidates to prune or rewrite:

* older fixtures that assert emitted-shape noise rather than semantics
* overlapping frontend-only tests that add little beyond stronger integration cases
* repeated narrow cases that can be merged into one clearer canonical scenario

Do not delete coverage blindly. Replace weak/redundant tests with stronger intent-aligned tests.

#### 5. Harden the integration harness itself where needed

Use this PR to remove remaining obvious harness rough edges that affect trust in the suite.

In particular:

* remove any remaining `todo!`/panic-shaped paths in integration assertion code that can still be exercised during normal test workflows
* add small runner-level tests around normalization / rendered-output behavior where confidence is still thin
* keep harness failures clearly distinct from semantic mismatches

#### 6. Keep matrix and manifest ownership disciplined

For every test migration or new canonical fixture:

* update `docs/language-surface-integration-matrix.md`
* update `tests/cases/manifest.toml`
* remove vague “temporary” coverage where the new canonical case supersedes it

The goal is that the matrix describes the real supported Alpha surface and the canonical fixtures that prove it.

**Suggested migration heuristic**

Use this decision rule consistently:

* exact emitted shape matters → strict golden
* emitted structure matters but generated counters do not → normalized golden
* runtime behavior matters → rendered output
* only a few output facts matter → artifact assertions

**Checklist**

* Audit remaining brittle fixtures by semantic intent.
* Migrate noisy full-file goldens to normalized/rendered/artifact modes where appropriate.
* Add missing canonical cases for the visible Alpha matrix gaps.
* Rewrite or remove redundant weaker tests that no longer add confidence.
* Remove remaining avoidable `todo!`/panic-shaped harness paths in active test code.
* Update the language surface matrix and test manifest alongside fixture changes.
* Add small runner-level regression tests where the assertion infrastructure itself needs confidence.

**Done when**

* Remaining broad golden failures mostly indicate real semantic regressions, not generator noise.
* The visible Alpha matrix gaps are materially reduced.
* The suite has fewer redundant fixtures and stronger canonical cases.
* Harness failures are clearly infrastructure failures, not mixed with semantic mismatches.
* The matrix and manifest accurately reflect the current supported surface.

**Implementation notes for the later execution plan**

* Treat this as a curation PR, not a random grab-bag.
* Migrate fixtures in small themed batches so failures stay interpretable.
* Prefer behavior-first assertions for runtime semantics.
* Keep strict goldens only where exact emitted shape is intentionally contractual.


## Phase 5 - cross-platform consistency and test stability

### PR - Finish CRLF normalization in strings and templates

Remove avoidable Windows/macOS golden drift from source normalization and emitted outputs.

**Checklist**
- Audit remaining CRLF behavior in strings, templates, and emitted output.
- Make sure normalized newline handling is consistent through the frontend and builder outputs.
- Add regression tests specifically for Windows-shaped input.

**Done when**
- Golden outputs are stable across normal Windows/macOS workflows.

**Done when**
- Non-semantic generator-shape churn no longer causes broad golden failures.
- Semantic changes still fail with clear, targeted integration diffs.

### PR - Add rendered-output assertions for runtime-fragment semantics

Some integration behaviors are fundamentally about rendered output, not emitted JS text layout.
For runtime-fragment-heavy cases, asserting rendered slot output provides stronger semantic confidence
than snapshotting compiler-generated temporary symbols.

**Fits with other PRs**
- Builds on the normalized-assertion work above.
- Supports the Phase 6 JS backend semantic audit with behavior-first checks.

**Checklist**
- Add an optional integration assertion mode that executes generated HTML+JS in a deterministic test harness and compares rendered runtime-slot output.
- Keep this mode focused on semantic surfaces (runtime fragments, call/lowering paths, collection/read flows) where emitted-text snapshots are noisy.
- Ensure harness failures distinguish:
  - test harness limitations/infrastructure errors
  - actual rendered-output mismatches
- Add targeted cases that currently rely on brittle full-file goldens but are really asserting rendered text behavior.
- Document expectation-writing guidance so new cases choose rendered assertions when appropriate.

**Done when**
- Runtime-fragment semantics are asserted directly at rendered-output level where needed.
- Integration failures are lower-noise and more actionable during backend/lowering changes.

### PR - Fix remaining Windows test-runner stability issues

Remove test-runner and lock-poisoning rough edges that still make Windows less reliable.

**Checklist**
- Audit known lock poisoning paths and test-runner failure behavior.
- Ensure failed tests/builds do not leave the runner in a poisoned or misleading state.
- Add targeted tests where possible.

**Done when**
- Windows failures look like normal compiler/test failures, not infrastructure weirdness.

---

## Phase 6 - JS backend and HTML builder hardening pass

### PR - JS backend semantic audit for Alpha surface

Verify that the JS backend behavior matches the intended Alpha language rules for the supported feature set.

**Checklist**
- Audit runtime helpers involved in aliasing, copying, arrays, result propagation, casts, and builtin helpers.
- Add or expand integration tests where behavior depends on emitted JS runtime logic.
- Fix any semantics that are currently “working by accident”.
- Re-check collection builtin lowering in `src/compiler_frontend/ast/field_access/collection_builtin.rs` and remove any compatibility-only branches that drift from current frontend semantics.
- Confirm builtins using synthetic/fake parameter declarations are either removed or intentionally retained with clear justification
- Add backend-facing tests for:
  - collection get/set/push/remove/length
  - error helper builtin methods
  - mutable receiver method place validation

**Done when**
- The JS backend is trustworthy enough for real Alpha examples.

### PR - HTML builder final stabilization pass

Treat the HTML project builder as a real Alpha product surface.

**Checklist**
- Re-audit route derivation, homepage rules, duplicate path diagnostics, tracked assets, cleanup, and output layout.
- Add any remaining config and artifact assertions needed for confidence.
- Ensure docs site and small static-site projects remain a valid proving ground.

**Done when**
- The HTML project builder can be presented as a stable Alpha capability.

---

## Final pre-alpha sweep

### PR - Alpha checklist audit

Verify that the Alpha gates are genuinely met.

**Checklist**
- Re-run the feature matrix and mark all supported areas as covered.
- Re-check that unsupported/deferred features fail cleanly.
- Re-check that docs and examples match actual support.
- Re-check diagnostics quality on a representative set of failures.
- Re-check cross-platform golden stability.

**Done when**
- There is a credible yes/no answer to “is Alpha ready?”

### PR - Alpha cleanup PR

Land final small consistency and hygiene fixes before the release branch/tag.

**Checklist**
- Remove obsolete rejection fixtures for features that are now supported.
- Tighten comments, TODOs, and dead-code justifications.
- Prune stale scaffolding where the current design has clearly replaced it.
- Update release-facing docs and contribution notes if needed.

**Done when**
- The repo feels intentional at the point Alpha begins.

---

## Deferred until after Alpha
These are intentionally not Alpha blockers unless they become necessary for one of the supported slices.

This is a collection of notes and findings for future roadmaps once the roadmap above is complete.

- builtin `Error` enrichment beyond what is already required for the current compiler/runtime surface
- full tagged unions
- full pattern-matching design
- full interfaces implementation
- richer numeric redesign work not required by Alpha

**Wasm**

Broader Wasm maturity beyond the current experimental path.

## Notes and limitations from previous investigations
- The WASM backend can't handle Choice/Union types yet (maps to Handle but produces i32/i64 mismatches). 
- rt_string_from_i64 Wasm helper: Explicitly noted in the 1ac2613 commit message as an "incremental bridge implementation". It produces valid output but is not a complete runtime implementation. This is scoped for a dedicated follow-up and does not cause panics.
