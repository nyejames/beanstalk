# Canvas Helper Import / Runtime Reachability Refactor Plan

## Purpose

This plan fixes two related but separate compiler issues triggered by adding canvas helpers to the built-in HTML source library:

1. **Grouped external package imports can be misclassified as facade imports.**
   `import @web/canvas { get_canvas, context_2d, Canvas2d }` expands to `web/canvas/<symbol>` paths, and current grouped import resolution can route those paths through source/module facade checks before virtual external package resolution.

2. **Unused JS-backed wrappers can emit runtime assets and trigger backend validation.**
   Builder-runtime packages such as `@web/canvas` are registered eagerly, and current scans treat external calls inside any compiled HIR block as active. An unused `@html` wrapper that calls canvas can therefore emit `@beanstalk/runtime`, JS assets, glue, import maps, or Wasm unsupported-function diagnostics.

The plan separates those issues by compiler owner:

- **Header/import layer:** external package grouped import resolution before facade enforcement, without weakening real source-library/module facade privacy.
- **HIR/backend layer:** syntactic reachability from `HirModule::start_function` for runtime metadata, backend support validation, and HTML JS glue/emission planning.

## Current repo anchors

Use these paths as the starting map for the coding agent:

- `src/compiler_frontend/headers/import_environment/mod.rs`
  - `ImportEnvironmentBuilder::resolve_and_register_grouped_import`
  - current order: provider-backed grouped import → facade resolution → normal target resolution.
- `src/compiler_frontend/headers/import_environment/target_resolution.rs`
  - `resolve_import_target`
  - private `resolve_virtual_package_import`
  - `resolve_namespace_target`
- `src/compiler_frontend/headers/import_environment/external_imports.rs`
  - `register_external_import`
  - external receiver-method import/auto-import behavior.
- `src/compiler_frontend/headers/import_environment/provider_imports.rs`
  - provider-backed grouped/bare `.js` import resolution.
- `src/compiler_frontend/headers/import_environment/facade_resolution.rs`
  - source-library and module-root facade checks.
- `src/compiler_frontend/hir/`
  - add reachability helper here.
  - `module.rs` has `HirModule::start_function`.
  - `functions.rs` has function entry blocks.
  - `blocks.rs`, `statements.rs`, and `terminators.rs` define CFG shape.
- `src/build_system/create_project_modules/frontend_orchestration.rs`
  - `collect_referenced_builder_runtime_package_ids`
  - `module_external_imports` assembly.
- `src/backends/external_package_validation.rs`
  - validates all external calls today; should validate reachable calls only.
- `src/backends/js/emitter.rs`, `src/backends/js/js_statement.rs`, `src/backends/js/mod.rs`
  - JS function emission and `referenced_external_functions` tracking.
- `src/projects/html_project/external_js/runtime_glue.rs`
  - generated glue and import-map planning.
- `src/projects/html_project/external_js/runtime_emission_plan.rs`
  - build-level runtime asset/runtime-module collection.
- `src/projects/html_project/external_libraries/web/canvas/mod.rs`
  - registers `@web/canvas` as builder-runtime metadata.
- `src/projects/html_project/external_libraries/web/canvas/canvas.js`
  - built-in JS-backed canvas package.
- `libraries/html/#mod.bst`
  - target for authored `@html` wrapper helpers.
- `docs/compiler-design-overview.md`
- `docs/language-overview.md`
- `docs/roadmap/roadmap.md`
- `docs/src/docs/progress/#page.bst`
- `docs/codebase-style-guide.md`

## Non-goals and deliberately deferred features

These are not part of the fix. Keep them explicitly deferred in docs and matrix updates.

- [ ] Do **not** add direct facade re-export syntax.
  - `import @web/canvas { get_canvas }` inside `libraries/html/#mod.bst` remains local to the facade file unless `@html` authors a real declaration wrapping it.
- [ ] Do **not** add wildcard imports.
- [ ] Do **not** make imports inside `#mod.bst` automatically exported.
- [ ] Do **not** automatically re-export external receiver methods through a facade type alias.
- [ ] Do **not** weaken source-library or module-root facade privacy for real source imports.
- [ ] Do **not** make `@web/canvas` prelude-imported.
- [ ] Do **not** add Wasm lowering for `@web/canvas`; reachable canvas calls in HTML-Wasm should still fail with structured unsupported-backend diagnostics.
- [ ] Do **not** implement general JS dependency graphs, JS default exports, JS re-exports, CommonJS, JS constants, callbacks, classes, async functions, or user-authored external binding files.
- [ ] Do **not** implement source-library HIR caching.
- [ ] Do **not** implement full tree-shaking/minification. Reachability here is correctness and artifact-planning reachability from `start`, not a release optimizer.
- [ ] Do **not** do constant-condition dead-code elimination. Reachability is syntactic CFG/function reachability from explicit roots.
- [ ] Do **not** change memory-model semantics. Borrow validation and GC fallback remain unchanged.

## Phase 0 — Baseline reproduction, branch hygiene, and plan placement

### Context and reasoning

Start by pinning the current failure modes before refactoring. This prevents import-layer failures from being hidden by runtime-emission failures and gives later phases targeted checks.

This phase also records the plan in the repo if the implementation work is going into a branch. The downloaded plan can be committed as:

```text
docs/roadmap/plans/canvas-helper-import-runtime-reachability-plan.md
```

### Implementation checklist

- [ ] Create or confirm a dedicated working branch.
- [ ] Save this plan into `docs/roadmap/plans/canvas-helper-import-runtime-reachability-plan.md` if the repo keeps accepted implementation plans in-tree.
- [ ] Run current targeted reproduction tests before changes:

```bash
cargo test build_html_project_fallible_js_without_runtime_import_does_not_emit_runtime_module
cargo test build_html_project_web_canvas_emits_builtin_js_asset_and_glue
cargo run -- tests --backend html
cargo run -- tests --backend html_wasm
```

- [ ] Reproduce the grouped import failure with a minimal fixture or temporary local project:

```beanstalk
import @web/canvas { get_canvas, context_2d, Canvas2d }
```

- [ ] Confirm the current diagnostic is `BST-IMPORT-0011` / `Not exported by facade` when a root/module facade shape is present.
- [ ] Confirm the runtime-module failure is caused by unused `@html`/source-library wrapper declarations, not by the project-local JS test itself.
- [ ] Inspect any local diff that adds canvas helpers to `libraries/html/#mod.bst`.
- [ ] Decide the public authored wrapper names before implementation. Suggested minimal helper:

```beanstalk
get_canvas_context |id String| -> Canvas2d, Error!:
    canvas = get_canvas(id)!
    context = context_2d(canvas)!
    return context
;
```

- [ ] Record the current status in PR notes:
  - grouped virtual package import fails in the import/header stage;
  - runtime emission leak comes from broad HIR/backend scans.

### Documentation checklist

- [ ] Add a roadmap note only if the plan is committed now; otherwise defer to Phase 7.
- [ ] Do not update the implementation matrix yet; it should reflect completed behavior, not intended behavior.

### End-of-phase audit / style / validation

- [ ] Audit that no code behavior changed in this phase except optional plan-file addition.
- [ ] Check that the plan file path, if added, is linked from `docs/roadmap/roadmap.md` or ready to be linked in Phase 7.
- [ ] Run formatting only if a docs formatter is used locally; no Rust validation is required for a docs-only commit.
- [ ] Confirm all reproduction results are written down for later comparison.

---

## Phase 1 — Header-layer fix for grouped external package imports

### Context and reasoning

`@web/canvas` is a virtual external package, not a source-library module. Grouped imports expand into individual symbol paths such as `web/canvas/get_canvas`. Current grouped import resolution checks provider-backed imports first, then source/module facade resolution, then normal target resolution. That can misclassify virtual external package symbols as source/module facade imports.

The fix belongs in header import preparation, not HIR. HIR should only see stable external IDs after imports are resolved.

The key invariant: **only virtual external package resolution may move before facade enforcement. Source symbol resolution must not move before facade enforcement.**

### Implementation checklist

- [ ] In `src/compiler_frontend/headers/import_environment/target_resolution.rs`, split the virtual-package symbol lookup out of `resolve_import_target`.

  Recommended shape:

```rust
pub(crate) enum ExternalPackageSymbolLookup {
    Found { symbol_id: ExternalSymbolId },
    PackageFoundSymbolMissing { package_path: StringId, symbol_name: StringId },
    NoMatch,
}

pub(crate) struct ExternalPackageSymbolResolutionInput<'a> {
    pub(crate) import_path: &'a InternedPath,
    pub(crate) external_package_registry: &'a ExternalPackageRegistry,
    pub(crate) string_table: &'a mut StringTable,
}

pub(crate) fn resolve_external_package_symbol(
    input: ExternalPackageSymbolResolutionInput<'_>,
) -> ExternalPackageSymbolLookup
```

- [ ] Keep `resolve_import_target` behavior intact by calling the new helper from the existing fallback path.
- [ ] Preserve the current missing-symbol diagnostic behavior for virtual packages.
- [ ] Do not expose a helper that resolves source symbols before facade checks.
- [ ] In `src/compiler_frontend/headers/import_environment/mod.rs`, update `resolve_and_register_grouped_import` order:
  - provider-backed grouped import first;
  - new external-package-only grouped import second;
  - facade resolution third;
  - normal target resolution last.
- [ ] Add a helper method on `ImportEnvironmentBuilder`:

```rust
fn resolve_and_register_external_package_grouped_import(
    &mut self,
    file_visibility: &mut FileVisibility,
    registry: &mut VisibleNameRegistry,
    import: &FileImport,
) -> Result<Option<()>, CompilerDiagnostic>
```

- [ ] Guard the new helper so it only participates for `import.from_grouped == true`.
  - This preserves direct symbol-path rejection for `import @web/canvas/get_canvas` bare imports.
- [ ] On `Found`, call existing `register_external_import` so receiver-method import validation and type auto-import behavior remains centralized.
- [ ] On `PackageFoundSymbolMissing`, return the existing package-symbol missing diagnostic, not a facade diagnostic.
- [ ] On `NoMatch`, return `Ok(None)` and continue to facade/source resolution.
- [ ] Ensure source imports still hit facade checks before source-symbol target resolution.

### Tests

Add or update tests before moving to HIR reachability.

- [ ] Add an integration fixture proving grouped `@web/canvas` import works when a root/module facade exists.
  - Suggested fixture name: `html_web_canvas_grouped_import_root_facade`.
  - Include a `#mod.bst` in the module shape that previously caused `module ''` facade misclassification.
- [ ] Add a negative fixture proving a missing `@web/canvas` symbol reports package missing-symbol behavior, not `Not exported by facade`.
- [ ] Add or preserve a negative fixture proving `import @web/canvas/get_canvas` remains rejected as a direct symbol-path import.
- [ ] Add or preserve source facade tests proving private source imports still cannot bypass `#mod.bst`.
- [ ] Add a grouped receiver-method fixture:

```beanstalk
import @web/canvas { Canvas2d, fill_rect }
```

  and keep the invalid form rejected:

```beanstalk
import @web/canvas { fill_rect }
```

  unless `Canvas2d` is visible through another accepted route.

### Documentation checklist

- [ ] Do not update `docs/language-overview.md` unless current text incorrectly implies grouped external package imports are unsupported.
- [ ] Add a Phase 7 note to update `docs/src/docs/progress/#page.bst` under **Paths and imports**:
  - grouped external package imports are resolved as virtual packages before facade enforcement;
  - direct symbol-path imports remain rejected;
  - source/module facade rules are unchanged.

### End-of-phase audit / style / validation

- [ ] Style guide review:
  - new helper names are explicit;
  - no dense iterator chains for multi-stage validation;
  - no user-facing `CompilerError` for import mistakes;
  - no inline long imports;
  - no compatibility wrappers.
- [ ] Stage-boundary audit:
  - header import environment owns import resolution;
  - AST/HIR are not involved in grouped import parsing/resolution;
  - source facade privacy is still enforced before source target resolution.
- [ ] Run targeted tests:

```bash
cargo test import_environment
cargo test build_html_project_web_canvas_emits_builtin_js_asset_and_glue
cargo run -- tests --backend html
```

- [ ] Record any changed diagnostic codes or expected fixture outputs.

---

## Phase 2 — Add shared HIR reachability analysis

### Context and reasoning

Runtime asset decisions and backend support checks need to know which HIR calls can execute from the module entry `start` function. This is an HIR concern because import syntax has already resolved into stable `CallTarget::ExternalFunction` IDs.

This helper must be simple, syntactic, and backend-neutral. It should not perform constant folding, dead-code elimination, borrow analysis, ownership analysis, or backend lowering.

### Implementation checklist

- [ ] Add a new file:

```text
src/compiler_frontend/hir/reachability.rs
```

- [ ] Update `src/compiler_frontend/hir/mod.rs` to expose the helper at the appropriate crate visibility.
- [ ] Define a result struct:

```rust
#[derive(Clone, Debug, Default)]
pub(crate) struct HirReachability {
    pub(crate) reachable_functions: FxHashSet<FunctionId>,
    pub(crate) reachable_blocks: FxHashSet<BlockId>,
    pub(crate) reachable_external_functions: FxHashSet<ExternalFunctionId>,
}
```

- [ ] Define an input/context struct instead of a long parameter list:

```rust
pub(crate) struct HirReachabilityInput<'a> {
    pub(crate) hir: &'a HirModule,
    pub(crate) root_functions: Vec<FunctionId>,
}
```

- [ ] Provide convenience functions:

```rust
pub(crate) fn collect_reachability_from_start(
    hir: &HirModule,
) -> Result<HirReachability, CompilerError>

pub(crate) fn collect_hir_reachability(
    input: HirReachabilityInput<'_>,
) -> Result<HirReachability, CompilerError>
```

- [ ] Build local maps at the start:
  - `FunctionId -> &HirFunction`
  - `BlockId -> &HirBlock`
- [ ] Traverse a function worklist and a block worklist.
- [ ] When visiting a reachable function, enqueue its entry block.
- [ ] When visiting a reachable block:
  - [ ] scan `HirStatementKind::Call` statements;
  - [ ] enqueue `CallTarget::UserFunction` targets;
  - [ ] record `CallTarget::ExternalFunction` targets;
  - [ ] enqueue CFG successor blocks from the terminator.
- [ ] Cover terminator successors:
  - `Jump { target, .. }`
  - `If { then_block, else_block, .. }`
  - `FallibleBranch { success_block, error_block, .. }`
  - `Match { arms, .. }` → each `arm.body`
  - `Break { target }`
  - `Continue { target }`
  - terminal: `Return`, `ReturnSuccess`, `ReturnError`, `RuntimeFailure`, `AssertFailure`
  - `Uninitialized` → `CompilerError` internal invariant failure.
- [ ] Do not inspect nested `HirExpression` for calls unless current HIR semantics prove calls can live there. Current call lowering is statement-based; keep this documented in a comment.
- [ ] Keep the helper deterministic where possible. Sets do not need output ordering, but tests should compare sorted vectors when needed.

### Tests

Add tests under a dedicated HIR test file, not inside production code.

- [ ] Add `src/compiler_frontend/hir/tests/reachability_tests.rs` or equivalent module-specific test path.
- [ ] Test start-only reachability:
  - start function reaches block A;
  - an unrelated function/block calls an external function;
  - unreachable external function is not reported.
- [ ] Test user-function call reachability:
  - start calls function B;
  - function B calls an external function;
  - external function is reported.
- [ ] Test branch and fallible branch successors.
- [ ] Test match arm successor traversal.
- [ ] Test loop-ish `Break` / `Continue` / `Jump` handling.
- [ ] Test missing function/block or `Uninitialized` terminator returns internal `CompilerError`.

### Documentation checklist

- [ ] Add a Phase 7 note to update `docs/compiler-design-overview.md`:
  - HIR owns backend-neutral reachability over functions/CFG blocks;
  - reachability starts from entry `start` for HTML builds;
  - this reachability is not DCE, constant folding, or ownership analysis.

### End-of-phase audit / style / validation

- [ ] Style guide review:
  - file-level docs explain what reachability owns and what it must not own;
  - helper uses context/input structs;
  - no clever iterator-heavy control flow;
  - internal invariant failures use `CompilerError`, not user diagnostics.
- [ ] Stage-boundary audit:
  - reachability reads HIR only;
  - no AST visibility/import logic is introduced;
  - no borrow-analysis mutation or ownership semantics are added.
- [ ] Run targeted tests:

```bash
cargo test hir::tests::reachability
cargo test reachability
```

- [ ] Run a broader compile check:

```bash
cargo test
```

---

## Phase 3 — Make module external-import metadata reachable-call-driven

### Context and reasoning

`Module::module_external_imports` drives runtime JS asset emission, runtime module emission, generated glue paths, and import-map entries. It must represent external packages whose functions are reachable from `start`, not packages that merely appear in unused source-library functions or provider import records.

This phase should fix the original failing unit test without changing JS emission yet. Later phases align backend validation and JS lowering with the same reachability model.

### Implementation checklist

- [ ] In `src/build_system/create_project_modules/frontend_orchestration.rs`, after HIR lowering and borrow checking, call:

```rust
let reachability = collect_reachability_from_start(&hir_module)?;
```

  Convert internal `CompilerError` to `CompilerMessages` at the existing build boundary.

- [ ] Replace `collect_referenced_builder_runtime_package_ids` with a helper based on reachable external function IDs:

```rust
fn collect_reachable_external_package_ids(
    reachable_external_functions: &FxHashSet<ExternalFunctionId>,
    registry: &ExternalPackageRegistry,
) -> FxHashSet<ExternalPackageId>
```

- [ ] Filter provider-created `module_external_imports` by reachable package ID.
  - Current provider table collection is by source file; keep that as the source of available resolved imports.
  - Only retain a provider-created package when one of its external function IDs is reachable.
- [ ] Filter builder-runtime package metadata by reachable package ID.
  - `@web/canvas` should be added to `module_external_imports` only when a reachable call uses one of its functions.
- [ ] Preserve deterministic sort/dedup by `ExternalPackageId`.
- [ ] Confirm packages with only compile-time constants or type-only use do not emit runtime assets.
- [ ] Confirm the external package registry remains fully populated for type checking and diagnostics; only `module_external_imports` is filtered.
- [ ] Remove or rename stale comments that say “scans all HIR blocks.”

### Tests

- [ ] Update or add a build-system test proving unused builder-runtime packages do not emit assets:
  - unused `@html` wrapper calls `@web/canvas` internally;
  - page does not call wrapper;
  - no `_beanstalk/js/canvas-*` asset;
  - no `_beanstalk/js/glue/*` module;
  - no `_beanstalk/js/runtime/beanstalk-runtime.js`;
  - no import map.
- [ ] Update the existing failing test:

```bash
cargo test build_html_project_fallible_js_without_runtime_import_does_not_emit_runtime_module
```

- [ ] Add a provider-created JS regression:
  - a source function imports a local `.js` module that imports `@beanstalk/runtime`;
  - that source function is never reachable from `start`;
  - runtime module and import map are not emitted.
- [ ] Preserve the positive case:
  - reachable canvas or provider JS call emits runtime asset, generated glue, runtime module, and import map as needed.

### Documentation checklist

- [ ] Add a Phase 7 note for `docs/compiler-design-overview.md` Stage 7:
  - `module_external_imports` are runtime metadata for reachable external calls;
  - runtime modules are still driven by accepted JS runtime imports, not inferred from fallibility.
- [ ] Add a Phase 7 note for `docs/src/docs/progress/#page.bst` **External platform packages**:
  - runtime assets, glue, runtime modules, and import maps are emitted only for reachable external calls from entry `start`.

### End-of-phase audit / style / validation

- [ ] Style guide review:
  - no broad scans remain in `frontend_orchestration.rs` for builder-runtime package attachment;
  - comments describe reachable metadata, not import presence;
  - helper names distinguish package IDs from function IDs.
- [ ] Stage-boundary audit:
  - build orchestration only uses HIR reachability after HIR exists;
  - header/import layer still records import visibility independent of runtime emission;
  - backend emission metadata does not leak into AST/HIR construction.
- [ ] Run targeted tests:

```bash
cargo test build_html_project_fallible_js_without_runtime_import_does_not_emit_runtime_module
cargo test build_html_project_web_canvas_emits_builtin_js_asset_and_glue
cargo run -- tests --backend html
```

- [ ] Record output path deltas for generated runtime files.

---

## Phase 4 — Validate external backend support only for reachable calls

### Context and reasoning

HTML-Wasm should not reject a module merely because an unused source-library wrapper contains a JS-only external call. It should reject only reachable external calls that lack Wasm lowering. The validation owner remains backend pre-lowering validation, but it should consume the same HIR reachability helper.

### Implementation checklist

- [ ] In `src/backends/external_package_validation.rs`, replace the full block scan with `collect_reachability_from_start`.
- [ ] Validate only `reachability.reachable_external_functions`.
- [ ] Preserve `BackendTarget::Js` and `BackendTarget::Wasm` behavior.
- [ ] Preserve special handling for `ExternalFunctionId::Io` in Wasm.
- [ ] Choose the error location carefully:
  - current validation reports at the call statement;
  - if only IDs are available from reachability, extend reachability to store first reachable call statement metadata, or build a local lookup of reachable external call IDs to first statement location while traversing.
- [ ] Preferred: extend `HirReachability` with structured call-site records:

```rust
pub(crate) struct ReachableExternalCall {
    pub(crate) function_id: ExternalFunctionId,
    pub(crate) statement_id: HirNodeId,
    pub(crate) location: SourceLocation,
}
```

  and keep `reachable_external_functions` as a convenience set.
- [ ] Make backend validation use the first reachable call site for diagnostics, preserving user-facing source labels.
- [ ] Do not downgrade unsupported reachable calls to warnings.

### Tests

- [ ] Add or update HTML-Wasm fixture:
  - `@html` has an unused wrapper that calls `@web/canvas`;
  - page does not call wrapper;
  - HTML-Wasm build succeeds.
- [ ] Add or preserve HTML-Wasm fixture:
  - page reaches `@web/canvas` directly or through `@html` wrapper;
  - build fails with `BST-RULE-0058`.
- [ ] Add a source-location regression if practical:
  - unsupported reachable external call diagnostic points to the call site, not an import line or wrapper declaration unless that wrapper call is the reachable unsupported call.

### Documentation checklist

- [ ] Add a Phase 7 note for `docs/src/docs/progress/#page.bst` **External platform packages**:
  - unsupported external package backend validation is reachable-call-based;
  - Wasm support for JS-backed packages remains experimental/deferred.
- [ ] Add a Phase 7 note for `docs/roadmap/roadmap.md`:
  - Wasm implementations for non-math/JS-backed packages remain deferred; this refactor only prevents unused code from failing Wasm builds.

### End-of-phase audit / style / validation

- [ ] Style guide review:
  - unsupported backend calls still use `CompilerDiagnostic`;
  - internal reachability failures use `CompilerError` and are converted at build/backend boundaries;
  - no locationless user diagnostics.
- [ ] Stage-boundary audit:
  - backend validation consumes HIR and registry metadata only;
  - no import syntax or header visibility is rechecked here.
- [ ] Run targeted tests:

```bash
cargo run -- tests --backend html_wasm
cargo test validate_hir_external_package_support
```

- [ ] Run existing canvas asset test to ensure JS support did not regress:

```bash
cargo test build_html_project_web_canvas_emits_builtin_js_asset_and_glue
```

---

## Phase 5 — Align HTML JS lowering, generated glue, and import maps with reachability

### Context and reasoning

Even if `module_external_imports` is filtered, the JS emitter currently lowers every HIR function and tracks external calls during lowering. If it emits unused functions that call external module exports, `referenced_external_functions` may still include unreachable calls and generate glue/import maps unnecessarily.

The cleanest HTML behavior is to emit only functions reachable from `start`. This is not a general optimizer; it is the HTML page execution model. Non-entry files contribute declarations, but only declarations reachable from entry `start` need JS output in a page bundle.

### Implementation checklist

- [ ] Add an emission policy enum to `src/backends/js/mod.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsFunctionEmissionPolicy {
    AllFunctions,
    ReachableFromStart,
}
```

- [ ] Add the field to `JsLoweringConfig`:

```rust
pub function_emission_policy: JsFunctionEmissionPolicy,
```

- [ ] Keep default/legacy JS behavior as `AllFunctions` unless all callers are intentionally HTML entry builds.
- [ ] In `compile_html_module_js`, set:

```rust
js_lowering_config.function_emission_policy = JsFunctionEmissionPolicy::ReachableFromStart;
```

- [ ] In `JsEmitter::lower_module`:
  - compute reachability when policy is `ReachableFromStart`;
  - sort and emit only reachable functions;
  - still build symbol maps deterministically so user-function calls lower to stable names;
  - preserve `auto_invoke_start` behavior.
- [ ] Ensure `referenced_external_functions` is populated only while lowering emitted reachable functions.
- [ ] Ensure generated glue uses `js_module.referenced_external_functions` as before, but now that set is reachable-only.
- [ ] Ensure `generate_module_glue` still finds package asset paths from filtered `module.module_external_imports`.
- [ ] If a reachable external function cannot find a runtime asset package, keep the existing internal compiler error; that indicates metadata/filter mismatch.
- [ ] Update comments in JS emitter and `JsModule` to mention “referenced during emitted JS lowering,” not “any HIR reference.”

### Tests

- [ ] Add a JS emission test proving an unreachable source function is not emitted in HTML JS output.
- [ ] Add a runtime glue regression:
  - unreachable function calls local provider JS import with `ExternalModuleExport`;
  - no glue module and no module script preamble are emitted.
- [ ] Add a positive reachable wrapper test:
  - reachable call produces glue preamble and wrapper import.
- [ ] Preserve existing JS backend tests that expect direct lowering of all functions, if any, by leaving `AllFunctions` as default for non-HTML paths.

### Documentation checklist

- [ ] Add a Phase 7 note for `docs/compiler-design-overview.md` Stage 7:
  - HTML JS lowering emits the entry-reachable function set for page bundles.
  - This is not a language-level reachability guarantee for future library/object export modes.
- [ ] Add a Phase 7 note for `docs/roadmap/roadmap.md`:
  - broad JS minification/tree-shaking remains deferred; current reachability is page-bundle correctness and runtime metadata gating.

### End-of-phase audit / style / validation

- [ ] Style guide review:
  - enum is used instead of boolean policy;
  - config field name is clear;
  - no compatibility shims beyond one current config shape;
  - no broad scan remains for JS `referenced_external_functions` in HTML mode.
- [ ] Stage-boundary audit:
  - JS backend consumes HIR reachability;
  - HIR helper remains backend-neutral;
  - build system does not inspect JS output to infer reachability.
- [ ] Run targeted tests:

```bash
cargo test lower_hir_to_js
cargo test runtime_glue
cargo test build_html_project_fallible_js_without_runtime_import_does_not_emit_runtime_module
cargo run -- tests --backend html
```

---

## Phase 6 — Add the authored `@html` canvas helper surface

### Context and reasoning

`@html` is a builder-provided source library behind a `#mod.bst` facade. Imports inside `libraries/html/#mod.bst` are local implementation details. Public `@html` names must be authored declarations, not re-exported imports.

A minimal helper can wrap `@web/canvas` canvas/context lookup while keeping raw drawing APIs on `@web/canvas` unless `@html` authors additional wrappers.

### Implementation checklist

- [ ] Edit `libraries/html/#mod.bst`.
- [ ] Add a local grouped import near other utility helpers:

```beanstalk
import @web/canvas { get_canvas, context_2d, Canvas2d }
```

- [ ] Add an authored wrapper declaration. Suggested initial shape:

```beanstalk
get_canvas_context |id String| -> Canvas2d, Error!:
    canvas = get_canvas(id)!
    context = context_2d(canvas)!
    return context
;
```

- [ ] Decide whether to add more authored wrapper functions now.
  - If adding drawing wrappers, prefer explicit `@html` names such as `fill_canvas_rect` rather than pretending receiver methods were re-exported.
  - Keep raw `get_canvas`, `context_2d`, `Canvas2d`, and receiver methods non-importable from `@html` unless each has a real authored declaration/alias design.
- [ ] Ensure the wrapper compiles in both JS and HTML-Wasm when unused.
- [ ] Ensure a reachable wrapper call emits `@web/canvas` assets and fails HTML-Wasm if the reachable external call lacks Wasm lowering.

### Tests

- [ ] Add integration fixture: unused `@html` canvas helper does not emit runtime artifacts.
  - Suggested name: `html_canvas_helper_unused_no_runtime_assets`.
- [ ] Add integration fixture: reachable `@html { get_canvas_context }` emits canvas asset, glue, runtime module, and import map in HTML JS.
- [ ] Add integration fixture: HTML-Wasm ignores unused `@html` canvas helper.
- [ ] Add integration fixture: HTML-Wasm rejects reachable `@html` canvas helper call with `BST-RULE-0058`.
- [ ] Add negative fixture: `import @html { get_canvas }` remains rejected unless `@html` authors that exact declaration.
- [ ] If drawing wrappers are added, add one reachable smoke test that calls the wrapper, not raw receiver syntax.

### Documentation checklist

- [ ] Update `docs/language-overview.md` only if adding a public `@html` helper surface that should be user-facing.
  - Mention `@html` exposes authored canvas convenience helpers.
  - Keep raw `@web/canvas` APIs documented as explicit external package imports.
  - Keep direct facade re-export syntax deferred.
- [ ] Add a Phase 7 progress matrix update under **Builder-provided source libraries**:
  - `@html` includes authored canvas helper wrappers over `@web/canvas`.
  - Raw imports inside `#mod.bst` remain local.
- [ ] Add a Phase 7 progress matrix update under **External platform packages** if wrapper tests broaden coverage.

### End-of-phase audit / style / validation

- [ ] Source-library API audit:
  - no raw re-export behavior was introduced;
  - helper names are clear and stable enough for Alpha;
  - wrapper signatures use external opaque types only where needed.
- [ ] Import/facade audit:
  - `@html` facade exports only authored declarations;
  - direct `@html { get_canvas }` remains rejected unless intentionally authored.
- [ ] Run targeted tests:

```bash
cargo run -- tests --backend html
cargo run -- tests --backend html_wasm
cargo test build_html_project_fallible_js_without_runtime_import_does_not_emit_runtime_module
```

---

## Phase 7 — Documentation, roadmap, progress matrix, and final validation

### Context and reasoning

This phase makes the intended surface explicit: grouped external imports are supported, `@html` wrappers are authored declarations, runtime artifacts are reachable-call-driven, and several tempting library-system features remain deferred.

Do documentation last so it describes the implemented behavior, not the plan.

### Roadmap updates

Edit `docs/roadmap/roadmap.md`.

- [ ] Add a note under `# Notes`:

```markdown
- The canvas helper import/runtime reachability refactor is complete. Grouped virtual external
  package imports resolve before source/module facade enforcement without weakening real source
  facade privacy. HTML JS runtime assets, generated glue, runtime modules, import maps, and
  unsupported-backend validation are driven by HIR calls reachable from the entry `start` function.
```

- [ ] Add or update deferred follow-up text:

```markdown
- Deliberately deferred library-system follow-ups after the canvas reachability refactor:
  direct facade re-export syntax, wildcard imports, automatic re-export of receiver methods through
  facade type aliases, source-library HIR caching, user-authored external binding files, broader
  JS-backed external package APIs, and Wasm implementations for JS-backed packages such as
  `@web/canvas`. Current reachability is artifact-planning correctness, not general JS
  tree-shaking/minification.
```

- [ ] If the plan file is committed, link it from the roadmap notes:

```markdown
  Implementation record: `docs/roadmap/plans/canvas-helper-import-runtime-reachability-plan.md`.
```

### Progress matrix updates

Edit `docs/src/docs/progress/#page.bst`.

- [ ] Update **Paths and imports** watch points to include:

```text
Grouped imports for virtual external packages resolve through external package metadata before
source/module facade enforcement. This prevents root/module facades from misclassifying paths such
as @web/canvas { get_canvas }, while direct symbol-path imports and all source facade privacy rules
remain strict.
```

- [ ] Update **External platform packages** coverage/watch points to include:

```text
Runtime JS assets, generated glue, registered runtime modules, import-map entries, and unsupported
backend checks are driven by external function calls reachable from the entry start function, not by
fallibility or by unused source-library/provider declarations. Unused @web/canvas/@html wrappers do
not emit canvas assets or @beanstalk/runtime. Reachable @web/canvas calls remain JS-backed and are
rejected for HTML-Wasm with structured unsupported-backend diagnostics.
```

- [ ] Update **Builder-provided source libraries** if an `@html` helper is added:

```text
The HTML builder's @html source library may expose authored canvas convenience helpers backed by
@web/canvas. Imports inside libraries/html/#mod.bst remain local implementation details; raw
@web/canvas symbols are not re-exported through @html unless @html authors real wrapper declarations.
```

- [ ] Keep deferred features visible in matrix text:
  - direct facade re-export syntax remains deferred;
  - wildcard imports remain deferred;
  - automatic method re-export through facade type aliases remains deferred;
  - Wasm support for JS-backed packages remains experimental/deferred;
  - broader external-method APIs beyond the narrow `@web/canvas` surface remain future design work.

### Compiler design documentation updates

Edit `docs/compiler-design-overview.md`.

- [ ] In the import/library/external package contract, clarify:
  - external package imports resolve to stable IDs in header/AST stages;
  - runtime metadata is attached later based on reachable HIR calls.
- [ ] In Stage 5 HIR, add:

```text
HIR also exposes backend-neutral reachability over functions, blocks, and external call IDs. The
HTML builder uses reachability from the entry start function for runtime artifact planning and
backend support validation. This is syntactic CFG reachability, not constant-condition DCE,
optimization, or ownership analysis.
```

- [ ] In Stage 7 backend lowering, update HTML JS path text:
  - HTML JS page bundles emit the start-reachable function set;
  - runtime module emission and import maps are driven by reachable accepted runtime imports;
  - fallibility alone does not imply runtime module emission.

### Language overview updates

Edit `docs/language-overview.md` only for user-visible surface changes.

- [ ] If `@html { get_canvas_context }` or similar is added, document it in the builder library / external package section.
- [ ] Preserve current language statements:
  - source libraries are normal modules behind `#mod.bst` facades;
  - `#mod.bst` imports are local;
  - direct facade re-export syntax is deferred;
  - users should import raw drawing APIs from `@web/canvas` unless `@html` authors wrappers.
- [ ] Do not add a memory-model note; this change should not affect memory semantics.

### Test manifest and fixtures

- [ ] Update `tests/cases/manifest.toml` for any new integration fixtures.
- [ ] Prefer stable `diagnostic_codes` for failure cases.
- [ ] Prefer output/artifact assertions for runtime asset behavior:
  - assert absent canvas asset path;
  - assert absent/present `_beanstalk/js/runtime/beanstalk-runtime.js`;
  - assert absent/present import map;
  - assert absent/present glue module.
- [ ] Avoid brittle full-output goldens unless exact output is contractual.

### Final validation

Run targeted validation first:

```bash
cargo test build_html_project_fallible_js_without_runtime_import_does_not_emit_runtime_module
cargo test build_html_project_web_canvas_emits_builtin_js_asset_and_glue
cargo test runtime_glue
cargo test validate_hir_external_package_support
cargo run -- tests --backend html
cargo run -- tests --backend html_wasm
```

Then run full validation:

```bash
just validate
```

### End-of-phase audit / style / validation

- [ ] Style guide final review:
  - new files have file-level WHAT/WHY docs;
  - no tests live in production files;
  - no stale comments mention broad HIR scans;
  - no compatibility shims preserve obsolete paths;
  - no clippy suppressions were added without justification;
  - comments explain stage ownership and subtle ordering requirements.
- [ ] Stage-boundary final review:
  - header import preparation owns grouped external package import resolution;
  - AST consumes visibility and does not rebuild imports;
  - HIR reachability is backend-neutral;
  - build/backend layers consume reachability for artifact planning and validation;
  - borrow checking and memory semantics are unchanged.
- [ ] Diagnostics review:
  - import failures use `CompilerDiagnostic` and stable import codes;
  - unsupported reachable backend calls use `BST-RULE-0058`;
  - internal HIR invariant problems use `CompilerError`;
  - source locations remain useful.
- [ ] Runtime artifact review:
  - fallible JS without runtime import does not emit runtime module;
  - accepted JS runtime imports emit runtime module only when the package call is reachable;
  - unused `@html` canvas wrapper does not emit canvas asset/glue/runtime/import map;
  - reachable canvas wrapper emits the expected artifacts in HTML JS;
  - reachable canvas wrapper is rejected in HTML-Wasm.
- [ ] Documentation review:
  - roadmap names completed work and deferred follow-ups;
  - matrix reflects implemented behavior and coverage;
  - language overview only claims user-visible supported surface;
  - compiler design overview describes ownership of reachability and runtime metadata.

---

## Cross-phase regression checklist

Use this checklist after each substantial code phase and before merging.

### Import behavior

- [ ] `import @web/canvas` namespace import still works.
- [ ] `import @web/canvas { get_canvas, context_2d, Canvas2d }` grouped import works under root/module facade shapes.
- [ ] `import @web/canvas/get_canvas` remains rejected as a direct symbol-path import.
- [ ] Missing package symbols report package-symbol diagnostics, not facade diagnostics.
- [ ] Source-library and module-root private source imports remain gated by `#mod.bst`.
- [ ] Grouped receiver-method imports still require receiver type visibility.

### Runtime artifact behavior

- [ ] Unused builder-runtime package calls do not add `ModuleExternalImport` entries.
- [ ] Unused provider-created JS packages do not emit runtime assets.
- [ ] Reachable provider-created JS packages emit assets/glue/import maps as before.
- [ ] Runtime module emission is tied to accepted JS runtime imports, not fallibility.
- [ ] Generated glue imports are relative to the glue module path.

### Backend behavior

- [ ] HTML JS emits/runs reachable page code.
- [ ] HTML JS does not emit unreachable wrapper functions in page bundles once reachable-only policy is enabled.
- [ ] HTML-Wasm ignores unused JS-only external calls.
- [ ] HTML-Wasm rejects reachable unsupported external calls with structured diagnostics.

### Documentation behavior

- [ ] Roadmap deferrals are explicit.
- [ ] Progress matrix status/coverage text does not overstate support.
- [ ] Language overview does not imply raw `@html` re-export exists.
- [ ] Compiler design overview records HIR reachability without describing it as optimization/DCE.

## Suggested implementation ownership split for coding agents

Each item below should fit in a single large-context coding pass.

1. **Agent chunk A:** Phase 1 only.
   - Header external grouped import resolution and tests.
2. **Agent chunk B:** Phase 2 only.
   - HIR reachability helper and unit tests.
3. **Agent chunk C:** Phase 3 only.
   - Reachability-driven `module_external_imports` and build artifact tests.
4. **Agent chunk D:** Phase 4 only.
   - Backend support validation reachability and HTML-Wasm tests.
5. **Agent chunk E:** Phase 5 only.
   - HTML JS reachable-only emission and glue/import-map tests.
6. **Agent chunk F:** Phase 6 only.
   - `@html` helper surface and integration tests.
7. **Agent chunk G:** Phase 7 only.
   - Documentation, roadmap, progress matrix, final validation.

## Definition of done

- [ ] Grouped `@web/canvas` imports work even with a root/module facade present.
- [ ] Source facades remain strict for real source imports.
- [ ] `@html` canvas helpers are authored wrappers, not raw re-exports.
- [ ] Unused canvas wrappers do not emit canvas JS, glue, `@beanstalk/runtime`, or import maps.
- [ ] Reachable canvas wrappers emit expected HTML JS artifacts.
- [ ] HTML-Wasm accepts unused JS-only wrappers and rejects reachable JS-only canvas calls.
- [ ] The failing unit test passes.
- [ ] New tests cover both positive and negative behavior.
- [ ] Roadmap and progress matrix explicitly state completed behavior and deferred features.
- [ ] `just validate` passes.
