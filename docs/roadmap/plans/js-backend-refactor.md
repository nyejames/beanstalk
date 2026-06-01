# JS Backend Refactor Implementation Plan

## Purpose

This plan defines a low-risk cleanup pass for the Beanstalk JavaScript backend and HTML external-JS bridge.

The goal is to improve readability, reduce duplicated lowering logic, remove obsolete scaffolding, and make existing module boundaries clearer without changing Beanstalk language behavior or expanding the JS external-library surface.

This is not a trait implementation plan. Do not touch ongoing trait work unless a mechanical compile fix is required by a moved import or renamed API.

## Refactor principles

- Preserve the current Alpha JS / HTML behavior.
- Prefer deletion and consolidation over new abstractions.
- Do not add a broad helper-planning layer unless the existing tests prove helper emission cannot be clarified through smaller changes.
- Do not split files merely to make them smaller. Split only when a file owns separable concepts or duplicated behavior becomes shared.
- Keep the JavaScript backend GC-based. Do not add ownership-aware drop behavior or deterministic destruction lowering.
- Keep browser-specific import maps, generated ES module glue, emitted JS assets, and runtime-module output inside `src/projects/html_project`.
- Keep project-local `.js` imports restricted to the documented annotated single-file surface.
- Keep user-facing diagnostics on `CompilerDiagnostic` / `CompilerMessages`; keep backend invariants and infrastructure failures on `CompilerError`.
- Prefer integration tests for observable behavior and focused unit tests for backend contracts that are hard to assert through Beanstalk fixtures.

## Non-goals

Do not implement or refactor toward any of these in this pass:

- trait constraints, trait dispatch, trait objects, or dynamic-safe trait handling;
- arbitrary JS dependency graphs;
- default exports, re-exports, classes, callbacks, async functions, JS constants, property accessors, generic external types, collections/options in JS signatures, or multi-success JS returns;
- a general JavaScript parser;
- a new structured diagnostic-payload hierarchy for JS parser errors;
- broad expression-lowering directory splits unless a phase explicitly identifies a smaller, necessary extraction.

## Current repo anchors

Primary JS backend files:

- `src/backends/js/mod.rs`
- `src/backends/js/emitter.rs`
- `src/backends/js/js_expr.rs`
- `src/backends/js/js_statement.rs`
- `src/backends/js/js_function.rs`
- `src/backends/js/symbols.rs`
- `src/backends/js/utils.rs`
- `src/backends/js/runtime/**`
- `src/backends/js/libraries/core/**`
- `src/backends/js/tests/**`

Primary HTML external-JS files:

- `src/projects/html_project/html_project_builder.rs`
- `src/projects/html_project/js_path.rs`
- `src/projects/html_project/external_js/mod.rs`
- `src/projects/html_project/external_js/js_import_provider.rs`
- `src/projects/html_project/external_js/package_registration.rs`
- `src/projects/html_project/external_js/parser/**`
- `src/projects/html_project/external_js/runtime_assets.rs`
- `src/projects/html_project/external_js/runtime_emission_plan.rs`
- `src/projects/html_project/external_js/runtime_glue.rs`
- `src/projects/html_project/external_js/runtime_module_registry.rs`

Documentation and test anchors:

- `docs/codebase-style-guide.md`
- `docs/compiler-design-overview.md`
- `docs/language-overview.md`
- `docs/src/docs/progress/#page.bst`
- `tests/cases/manifest.toml`

## Global validation commands

Run narrow commands at the end of each phase, then run the full gate at the end.

Targeted commands:

```bash
cargo test js
cargo test runtime_glue
cargo test external_js::parser
cargo test emission_policy
cargo test lower_hir_to_js
cargo test build_import_tests
cargo run -- tests --backend html
```

Full gate:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features
cargo test
cargo run -- tests --backend html
cargo run -- tests --backend html_wasm
cargo run -- tests
just validate
git diff --check
```

If a test filter no longer matches after module moves, run the closest equivalent filter and record the updated command in implementation notes.

---

# Phase 0 — Baseline and guardrails

## Summary

Establish a clean baseline before moving code. This phase should not change production behavior.

## Reasoning

The planned changes are mostly mechanical, but they touch backend output and HTML glue. A green baseline prevents behavior changes from being hidden by file moves.

## Tasks

- [ ] Record the current commit SHA.
- [ ] Run `git status --short` and confirm the worktree state.
- [ ] Run targeted baseline tests:
  - [ ] `cargo test js`
  - [ ] `cargo test runtime_glue`
  - [ ] `cargo test external_js::parser`
  - [ ] `cargo test emission_policy`
  - [ ] `cargo test lower_hir_to_js`
  - [ ] `cargo test build_import_tests`
  - [ ] `cargo run -- tests --backend html`
- [ ] Inspect any failure before editing code. Do not begin refactoring on an unexplained red baseline.
- [ ] Search for stale or risky JS-backend markers:

```bash
rg "TODO|FIXME|allow\(dead_code\)|unwrap\(|todo!\(|panic!|standard_html|placeholder|retained" \
  src/backends/js \
  src/projects/html_project/external_js \
  src/projects/html_project/js_path.rs
```

- [ ] Record findings in implementation notes.

## Audit / style guide review

- [ ] Confirm no production files changed in this phase.
- [ ] Confirm the refactor scope excludes trait implementation work.
- [ ] Confirm no planned phase expands the external JS language surface.

## Validation

- [ ] `git diff --check`
- [ ] Targeted baseline commands above.

## Documentation

- [ ] No documentation changes.

---

# Phase 1 — Remove obsolete scaffolding and clarify JS lowering config constructors

## Summary

Delete no-op JS core math helper scaffolding and replace misleading HTML JS config setup with constructors that encode the actual caller contracts.

## Reasoning

The `@core/math` JS helper module is a placeholder because current math functions lower through inline expressions. Keeping it makes helper emission look broader than it is. `JsLoweringConfig::standard_html` is also misleading if it defaults to all-function emission while the HTML path immediately overrides it to reachable-from-start emission.

## Tasks

### 1. Delete the no-op core math helper module

- [ ] Remove `src/backends/js/libraries/core/math.rs`.
- [ ] Remove `mod math;` from `src/backends/js/libraries/core/mod.rs`.
- [ ] Remove `self.emit_core_math_helpers();` from `emit_core_library_helpers`.
- [ ] Confirm no references remain:

```bash
rg "emit_core_math_helpers|mod math" src/backends/js
```

- [ ] Confirm `@core/math` tests still pass through inline-expression lowering.

### 2. Replace misleading JS lowering config constructor names

- [ ] In `src/backends/js/mod.rs`, replace `JsLoweringConfig::standard_html(release_build)` with explicit constructors.
- [ ] Add or keep a direct backend constructor with all-functions emission:

```rust
pub fn direct_js(release_build: bool) -> Self
```

- [ ] Add an HTML page-bundle constructor with reachable-only emission and glue enabled:

```rust
pub fn html_page_bundle(
    release_build: bool,
    external_package_registry: ExternalPackageRegistry,
) -> Self
```

- [ ] `direct_js` must preserve direct JS/backend test behavior:
  - [ ] `function_emission_policy: JsFunctionEmissionPolicy::AllFunctions`
  - [ ] `external_module_export_glue_enabled: false`
  - [ ] existing `auto_invoke_start` behavior preserved.
- [ ] `html_page_bundle` must encode the current HTML JS path:
  - [ ] `function_emission_policy: JsFunctionEmissionPolicy::ReachableFromStart`
  - [ ] `external_module_export_glue_enabled: true`
  - [ ] `auto_invoke_start: false`
  - [ ] supplied `external_package_registry` stored directly.
- [ ] Update `src/projects/html_project/js_path.rs` to call `JsLoweringConfig::html_page_bundle(...)` and remove manual field mutation.
- [ ] Update backend test support to call `direct_js` or construct the config explicitly when a test needs unusual settings.
- [ ] Remove old constructor references:

```bash
rg "standard_html" src tests docs
```

### 3. Update source comments

- [ ] Update comments in `src/backends/js/mod.rs` so the two constructors describe caller intent.
- [ ] Keep comments concise. Do not add user-facing documentation for internal constructor names.

## Audit / style guide review

- [ ] Confirm deleted math module had no emitted behavior.
- [ ] Confirm no placeholder file remains for possible future helpers.
- [ ] Confirm HTML JS lowering no longer mutates config fields after construction.
- [ ] Confirm no compatibility wrapper keeps `standard_html` alive.
- [ ] Confirm comments explain why the two constructor contracts differ.

## Validation

- [ ] `cargo fmt --all --check`
- [ ] `cargo test js`
- [ ] `cargo test emission_policy`
- [ ] `cargo test lower_hir_to_js`
- [ ] `cargo run -- tests --backend html`
- [ ] `git diff --check`

## Documentation

- [ ] Source comments only.
- [ ] Do not update `docs/language-overview.md`.
- [ ] Do not update `docs/src/docs/progress/#page.bst` unless a test or coverage claim changes.

---

# Phase 2 — Split the JS backend utility catch-all

## Summary

Replace `src/backends/js/utils.rs` with a few focused modules for output, lookups, reachability, and identifiers.

## Reasoning

`utils.rs` currently mixes output formatting, source-location comments, symbol lookup, block lookup, CFG reachability, temp identifier generation, identifier sanitization, and reserved-word handling. These are small but unrelated concerns. Splitting them is low risk and prevents the file from becoming a permanent misc bucket.

## Tasks

### 1. Create focused modules

Replace `src/backends/js/utils.rs` with:

```text
src/backends/js/output.rs
src/backends/js/lookups.rs
src/backends/js/reachability.rs
src/backends/js/identifiers.rs
```

- [ ] Move `emit_line`, `emit_location_comment`, and `with_indent` into `output.rs`.
- [ ] Move `function_name`, `local_name`, `field_name`, and `block_by_id` into `lookups.rs`.
- [ ] Move `collect_reachable_blocks` into `reachability.rs`.
- [ ] Move `next_temp_identifier`, `sanitize_identifier`, and `is_js_reserved` into `identifiers.rs`.
- [ ] Remove `src/backends/js/utils.rs`.
- [ ] Update `src/backends/js/mod.rs`:
  - [ ] add `mod output;`
  - [ ] add `mod lookups;`
  - [ ] add `mod reachability;`
  - [ ] add `mod identifiers;`
  - [ ] remove `mod utils;`
- [ ] Update imports in `src/backends/js/symbols.rs` to use `crate::backends::js::identifiers::{is_js_reserved, sanitize_identifier}`.
- [ ] Update any tests that import helpers directly.

### 2. Keep file docs useful

- [ ] Each new file must start with a module doc comment.
- [ ] `output.rs` must say it owns JS source text emission and indentation only.
- [ ] `lookups.rs` must say it owns checked backend symbol/block lookup only.
- [ ] `reachability.rs` must say it adapts HIR CFG reachability for JS function lowering only.
- [ ] `identifiers.rs` must say it owns generated JS identifier safety and uniqueness only.

## Audit / style guide review

- [ ] Confirm no broad `utils`, `helpers`, or `common` module was recreated.
- [ ] Confirm each moved function is in the module that names its responsibility.
- [ ] Confirm imports remain readable and avoid long inline paths.
- [ ] Confirm this phase is a pure move/split with no behavior changes.

## Validation

- [ ] `cargo fmt --all --check`
- [ ] `cargo test js`
- [ ] `cargo test symbols`
- [ ] `cargo test lower_hir_to_js`
- [ ] `git diff --check`

## Documentation

- [ ] Source docs only.
- [ ] No user-facing docs changes.

---

# Phase 3 — Centralize JS value-use lowering and extract call lowering

## Summary

Remove duplicated `Load` / `Copy` handling across expression, call, assignment, and return lowering. Extract call-target lowering into its own focused backend module, but avoid a broad statement-lowering directory split.

## Reasoning

The current duplicate handling is a correctness risk because Beanstalk calls, host/external calls, assignments, and returns use different ABI/value policies. A small shared value-use helper reduces repetition without adding a large abstraction layer. Moving call-target lowering out of `js_statement.rs` removes one distinct concern while keeping statement orchestration simple.

## Tasks

### 1. Add `value_use.rs`

Create:

```text
src/backends/js/value_use.rs
```

Suggested shape:

```rust
pub(crate) enum JsValueUse {
    PlainExpression,
    AssignmentValue,
    BeanstalkCallArgument,
    HostCallArgument,
    ReturnValue,
}
```

Names may differ if the code reads better, but the states must stay explicit.

- [ ] Add a `JsEmitter` helper that lowers a `HirExpression` according to the value-use context.
- [ ] Preserve current behavior exactly:
  - [ ] `PlainExpression` loads use `__bs_read(place)`.
  - [ ] `PlainExpression` copies use `__bs_clone_value(__bs_read(place))`.
  - [ ] `AssignmentValue` returns the concrete value to write/assign.
  - [ ] `BeanstalkCallArgument` passes existing places as refs and wraps rvalues in `__bs_binding(...)`.
  - [ ] `HostCallArgument` / external JS calls pass raw JS values, never binding wrappers.
  - [ ] `ReturnValue` preserves alias-return behavior where required.
  - [ ] Tuple return values recursively lower each element as a return value.
- [ ] Keep alias-return decision helpers named and visible. Do not hide them behind a vague boolean.
- [ ] Add comments for the Beanstalk call ABI vs host-call ABI boundary.

### 2. Replace duplicate lowering branches

Replace repeated `Load` / `Copy` branches in:

- [ ] `src/backends/js/js_expr.rs`:
  - [ ] `lower_return_value_expression`
  - [ ] `lower_call_argument`
  - [ ] `lower_host_call_argument`
- [ ] `src/backends/js/js_statement.rs`:
  - [ ] assignment value lowering
  - [ ] call argument preparation
  - [ ] result-local assignment behavior if duplicated after call extraction.

Keep `lower_expr` itself as the plain expression entrypoint. Do not split `js_expr.rs` in this phase unless the value-use extraction makes a very small local move unavoidable.

### 3. Extract call lowering to `js_calls.rs`

Create:

```text
src/backends/js/js_calls.rs
```

Move only call-specific code:

- [ ] `LoweredCallTarget`
- [ ] `lower_call_target`
- [ ] external package JS-lowering lookup for call targets
- [ ] external module export glue-name selection
- [ ] `substitute_inline_expression`
- [ ] any small helper used only by call lowering.

Then update `js_statement.rs` so `HirStatementKind::Call` delegates to a named call-emission helper.

Do not split `js_statement.rs` into a directory in this pass. Reassess after value-use cleanup; avoid file splitting if it would mostly move code without deleting duplication.

### 4. Preserve alias behavior

- [ ] Keep `local_is_alias_only_before_statement` and `local_is_alias_only_at_block_entry` near assignment/jump-transfer code.
- [ ] Keep comments explaining why alias-only locals use write-through assignment rather than rebinding.
- [ ] Confirm fallible source and external calls still assign fresh carrier values, not alias references.

## Audit / style guide review

- [ ] Confirm `value_use.rs` reduces duplication rather than becoming a broad lowering façade.
- [ ] Confirm `js_calls.rs` owns only call-target/call-expression lowering.
- [ ] Confirm `js_statement.rs` remains readable after delegation.
- [ ] Confirm no behavior-sensitive code was hidden in a clever iterator or broad generic helper.
- [ ] Confirm no user-facing diagnostics were routed through `CompilerError`.
- [ ] Confirm no trait-related files changed.

## Validation

- [ ] `cargo fmt --all --check`
- [ ] `cargo test js`
- [ ] `cargo test inline_expressions`
- [ ] `cargo test receiver_methods`
- [ ] `cargo test results`
- [ ] `cargo test lower_hir_to_js`
- [ ] `cargo run -- tests --backend html`
- [ ] `git diff --check`

## Required new tests

Add focused backend tests for value-use parity:

- [ ] `Load` and `Copy` in assignment contexts.
- [ ] `Load` and `Copy` in Beanstalk call arguments.
- [ ] `Load` and `Copy` in external/host call arguments.
- [ ] `Load` and `Copy` in return values.
- [ ] Tuple returns preserve return-value handling per element.
- [ ] Alias-returning source functions still use borrow assignment for result locals.
- [ ] Fallible source/external calls still assign carrier values, not alias references.
- [ ] Inline-expression placeholder validation still rejects duplicate and missing placeholders.

## Documentation

- [ ] Add source docs for `value_use.rs` and `js_calls.rs`.
- [ ] No user-facing docs changes.

---

# Phase 4 — Reduce runtime helper repetition without macro-style generation

## Summary

Centralize repeated JS runtime error-result and collection index-validation snippets. Keep the runtime prelude readable and avoid Rust macro abstractions.

## Reasoning

The runtime prelude is already split well. The remaining smell is repeated raw JS for collection error carriers and index checks. Small JS helper functions reduce repetition and make the emitted runtime contract easier to audit.

## Tasks

### 1. Add a small error-result helper

In `src/backends/js/runtime/errors.rs`, emit:

```js
function __bs_error_result(message, code) {
    return { tag: "err", value: __bs_make_error(message, code, null, null) };
}
```

- [ ] Keep `__bs_make_error` unchanged.
- [ ] Use `__bs_error_result(...)` in runtime helper bodies where it reduces repeated JS source.
- [ ] Prefer only collection helpers at first. Use it in cast helpers only if it clearly improves readability.
- [ ] Do not add Rust macros or a broad helper-source generation DSL.

### 2. Add collection index validation helper

In `src/backends/js/runtime/collections.rs`, emit:

```js
function __bs_collection_index_is_valid(collection, index) {
    return Number.isInteger(index) && index >= 0 && index < collection.length;
}
```

- [ ] Use it in `__bs_collection_get`.
- [ ] Use it in `__bs_collection_set`.
- [ ] Use it in `__bs_collection_remove`.
- [ ] Keep invalid collection and out-of-bounds error codes/messages unchanged.
- [ ] Keep `push` and `length` infallible.
- [ ] Keep `set` success returning `{ tag: "ok", value: null }`.
- [ ] Keep `remove` success returning the removed value.

### 3. Keep longer helpers explicit

- [ ] Do not restructure `__bs_cast_int` and `__bs_cast_float` unless the final code is more readable than the current explicit control flow.
- [ ] Keep `@core/time` helper emission explicit.
- [ ] Do not introduce a helper-plan abstraction in this phase.

## Audit / style guide review

- [ ] Confirm emitted JS remains readable.
- [ ] Confirm helper names use the `__bs_` backend-private namespace.
- [ ] Confirm runtime helper comments match implementation.
- [ ] Confirm no macro-heavy generation or string-template DSL was added.
- [ ] Confirm all behavior changes are covered by tests; intended behavior should be unchanged.

## Validation

- [ ] `cargo fmt --all --check`
- [ ] `cargo test runtime_helpers`
- [ ] `cargo test js`
- [ ] `cargo run -- tests --backend html`
- [ ] Run collection/cast integration cases if tag filtering is available.
- [ ] `git diff --check`

## Required new tests

- [ ] Invalid collection receiver returns the same error code/message.
- [ ] Negative index returns out-of-bounds error.
- [ ] Non-integer index returns out-of-bounds error.
- [ ] `index == length` returns out-of-bounds error.
- [ ] `set` success returns `{ tag: "ok", value: null }`.
- [ ] `remove` success returns the removed value.
- [ ] Cast fallback cases still return the same error behavior if cast helpers were touched.

## Documentation

- [ ] Source docs only.
- [ ] No user-facing docs changes.

---

# Phase 5 — Modest runtime glue module deepening

## Summary

Split `src/projects/html_project/external_js/runtime_glue.rs` into a directory module with a small number of focused files. Preserve all path, wrapper, import-map, and runtime-module behavior.

## Reasoning

`runtime_glue.rs` owns multiple separable HTML-builder concerns: per-module glue, referenced export collection, wrapper source generation, import maps, output paths, relative URL calculation, and build-level runtime module emission. This is worth deepening, but the split should be modest to avoid replacing one broad file with too many tiny files.

## Tasks

### 1. Create runtime glue directory module

Replace `runtime_glue.rs` with:

```text
src/projects/html_project/external_js/runtime_glue/
  mod.rs
  exports.rs
  source.rs
  import_map.rs
  paths.rs
  runtime_modules.rs
```

Responsibilities:

- [ ] `mod.rs`: public entrypoints, `ModuleGlueResult`, and per-module glue orchestration.
- [ ] `exports.rs`: `ReferencedExport`, referenced export collection, raw import names, and deterministic sort keys.
- [ ] `source.rs`: glue module source generation plus fallible/infallible wrapper source.
- [ ] `import_map.rs`: import-map HTML generation only.
- [ ] `paths.rs`: glue module path, runtime module path, safe runtime module names, and relative URL paths.
- [ ] `runtime_modules.rs`: build-level runtime module emission only.

Avoid a deeper split unless a file still mixes clearly unrelated concepts after this move.

### 2. Preserve caller API

- [ ] Keep these callable paths through `runtime_glue::mod.rs`:
  - [ ] `generate_module_glue`
  - [ ] `emit_build_runtime_modules`
- [ ] Update imports in:
  - [ ] `src/projects/html_project/html_project_builder.rs`
  - [ ] `src/projects/html_project/js_path.rs`
  - [ ] runtime glue tests.
- [ ] Use `pub(crate) use` re-exports only for the active API. Do not keep compatibility shims for old internal file shapes.
- [ ] Delete the old `runtime_glue.rs` file.

### 3. Preserve behavior exactly

- [ ] Glue module output path remains deterministic and stable.
- [ ] Glue imports JS runtime assets relative to the glue module path, not the HTML document path.
- [ ] HTML bundle import preamble remains relative to the HTML output path.
- [ ] Import maps remain HTML-builder-owned.
- [ ] Runtime modules are emitted once per build from accepted runtime imports.
- [ ] Debug fallible wrappers still throw for invalid wrapper shape.
- [ ] Release fallible wrappers still return an error carrier for invalid wrapper shape.
- [ ] Infallible wrappers still forward raw return values unchanged.

## Audit / style guide review

- [ ] Confirm no generic JS backend module imports HTML project paths.
- [ ] Confirm `runtime_glue/mod.rs` acts as a structural map and orchestration entrypoint.
- [ ] Confirm each submodule has a file-level WHAT/WHY doc.
- [ ] Confirm no function was copied and left in the old location.
- [ ] Confirm browser/import-map logic remains under `src/projects/html_project`.
- [ ] Confirm no path policy changed accidentally.

## Validation

- [ ] `cargo fmt --all --check`
- [ ] `cargo test runtime_glue`
- [ ] `cargo test build_import_tests`
- [ ] `cargo run -- tests --backend html`
- [ ] `git diff --check`

## Required new tests

Add or preserve focused runtime glue tests for:

- [ ] Nested HTML output paths resolve glue import paths correctly.
- [ ] Glue module import path to emitted JS runtime asset is relative to the glue module.
- [ ] Runtime module import-map entries are deterministic and deduplicated by specifier.
- [ ] Debug fallible wrapper invalid-shape behavior.
- [ ] Release fallible wrapper invalid-shape behavior.
- [ ] Infallible wrapper raw forwarding.
- [ ] Unreachable external module export does not generate glue or import-map entries in HTML page-bundle mode.

## Documentation

- [ ] Source docs only.
- [ ] Do not update language docs unless a behavior discrepancy is discovered.
- [ ] If a path/import-map behavior discrepancy is discovered and fixed, update `docs/compiler-design-overview.md` and `docs/src/docs/progress/#page.bst` in the same phase.

---

# Phase 6 — External JS parser test hardening and minimal scanner cleanup

## Summary

Harden the restricted external JS parser with tests for current behavior. Extract shared scanner cursor mechanics only if the extraction preserves current spans and reduces duplicated code without making the parser more abstract.

## Reasoning

The parser’s restricted design is correct. A full JS parser or diagnostic-payload overhaul is unnecessary for this cleanup pass. The useful work is to protect the current scanner behavior around comments, strings, templates, imports, and exports, then remove duplicated cursor code only if it stays straightforward.

## Tasks

### 1. Add parser behavior tests before refactoring

In `src/projects/html_project/external_js/parser/tests/**` or the existing parser test module:

- [ ] `export` inside a string literal is ignored.
- [ ] `export` inside a template literal is ignored.
- [ ] `import` inside a string literal is ignored.
- [ ] `import` inside a line/block comment is ignored.
- [ ] `import` inside a template literal is ignored.
- [ ] Braces inside strings/templates do not break statement-boundary scanning.
- [ ] Multiline named runtime import is accepted.
- [ ] Duplicate runtime imports are deduplicated deterministically.
- [ ] Default runtime import is rejected.
- [ ] Namespace runtime import is rejected.
- [ ] Aliased named runtime import is rejected.
- [ ] Unknown runtime import name is rejected with the current `JsDiagnosticKind`.
- [ ] `@bst.sig` missing export remains diagnosed.
- [ ] Unknown external type remains diagnosed.

### 2. Decide whether cursor extraction is worth doing

Before extracting, inspect duplication between:

- `parser/comment_extractor.rs`
- `parser/export_scanner.rs`

Proceed only if all are true:

- [ ] extracted code removes meaningful duplicated cursor/skip logic;
- [ ] the extracted API is small and parser-specific;
- [ ] spans and diagnostic behavior can be preserved;
- [ ] the result is easier to read than the current explicit scanners.

If any condition fails, skip cursor extraction and keep only the added tests.

### 3. Optional cursor extraction

If proceeding, create:

```text
src/projects/html_project/external_js/parser/source_cursor.rs
```

Keep it narrow:

- [ ] source text, byte position, line, column;
- [ ] `current_char`, `current_char_opt`, `peek_str`;
- [ ] `advance_char`, `advance_chars`, `advance_to_byte`, `is_at_end`;
- [ ] span construction helpers;
- [ ] simple shared skipping helpers only where both scanners need them.

Do not build a generic JS lexer. Do not change parser data structures unless required by the cursor extraction.

### 4. Preserve diagnostic boundary

- [ ] Keep parser output as `ParsedJsLibrary` plus parser-local diagnostics.
- [ ] Keep conversion to `CompilerDiagnostic` in `js_import_provider.rs`.
- [ ] Do not add a new diagnostic payload hierarchy in this pass.
- [ ] If a tiny conversion cleanup is needed, keep it local and preserve existing diagnostic codes/shape.

## Audit / style guide review

- [ ] Confirm parser remains independent from compiler diagnostics.
- [ ] Confirm no arbitrary JS syntax support was added.
- [ ] Confirm cursor helpers, if added, are parser-specific and small.
- [ ] Confirm line/column behavior is tested where moved.
- [ ] Confirm test files remain outside production code.
- [ ] Confirm no user-facing diagnostic shape changed unexpectedly.

## Validation

- [ ] `cargo fmt --all --check`
- [ ] `cargo test external_js::parser`
- [ ] `cargo test js_import_provider`
- [ ] `cargo test build_import_tests`
- [ ] `cargo run -- tests --backend html`
- [ ] `git diff --check`

## Documentation

- [ ] If `source_cursor.rs` is added, include a file-level WHAT/WHY doc.
- [ ] Update `external_js/parser/mod.rs` module layout docs if files change.
- [ ] Do not update user-facing docs.
- [ ] Update `docs/src/docs/progress/#page.bst` only if new tests materially change coverage wording.

---

# Phase 7 — Final stale-comment cleanup, docs check, and validation sweep

## Summary

Clean stale comments and update only the documentation that genuinely changed. Then run the full validation gate.

## Reasoning

Most changes are internal refactors. Documentation should not be churned unless a durable internal contract changed or a behavior discrepancy was discovered. Source comments must still match the new file layout.

## Tasks

### 1. Stale comment search

Run:

```bash
rg "standard_html|utils.rs|runtime_glue.rs|js_calls|value_use|placeholder|retained|helper plan|source_cursor" \
  src/backends/js \
  src/projects/html_project/external_js \
  src/projects/html_project/js_path.rs \
  docs
```

- [ ] Remove comments that describe deleted files or old flow.
- [ ] Remove comments that merely restate code.
- [ ] Add concise WHAT/WHY comments around:
  - [ ] value-use lowering policy;
  - [ ] call lowering boundary;
  - [ ] runtime glue module flow;
  - [ ] parser cursor role, if added;
  - [ ] HTML page-bundle JS config constructor.

### 2. Documentation decisions

- [ ] `docs/language-overview.md`: leave unchanged unless behavior changed.
- [ ] `docs/compiler-design-overview.md`: update only if a durable internal contract changed, such as HTML JS config/reachability wording becoming inaccurate.
- [ ] `docs/src/docs/progress/#page.bst`: update only if coverage/status wording is now inaccurate due to added tests.
- [ ] Do not regenerate docs/release artifacts.

### 3. Test pruning review

- [ ] Review newly added unit tests for duplicate coverage.
- [ ] Keep backend unit tests that pin backend contracts not visible through integration fixtures.
- [ ] Prefer integration fixtures for observable language behavior.
- [ ] Remove tests that assert incidental formatting unless exact emitted JS/HTML shape is the behavior under test.

## Audit / style guide review

- [ ] Every touched module has one clear owner responsibility.
- [ ] New files have file-level docs.
- [ ] `mod.rs` files are structural maps, not implementation dumps.
- [ ] No compatibility wrappers preserve obsolete internal APIs.
- [ ] No broad `utils`, `helpers`, or `common` catch-all file exists in the touched areas.
- [ ] No user-facing diagnostics route through `CompilerError`.
- [ ] No new user-input-driven `.unwrap()` exists.
- [ ] No broad generic helper hides compiler-stage ownership.
- [ ] No trait implementation scope was expanded.

## Validation

Run the full gate:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features
cargo test
cargo run -- tests --backend html
cargo run -- tests --backend html_wasm
cargo run -- tests
just validate
git diff --check
```

Run targeted isolation commands if the full gate fails:

```bash
cargo test js
cargo test runtime_glue
cargo test external_js::parser
cargo test emission_policy
cargo test lower_hir_to_js
cargo test build_import_tests
```

## Documentation deliverables

- [ ] Source-level docs updated for new/moved modules.
- [ ] `docs/compiler-design-overview.md` updated only if internal contract wording required it.
- [ ] `docs/src/docs/progress/#page.bst` updated only if coverage/status wording required it.
- [ ] `docs/language-overview.md` unchanged unless behavior changed.

---

# Final definition of done

- [ ] `src/backends/js/libraries/core/math.rs` is deleted.
- [ ] No no-op helper placeholder remains for `@core/math`.
- [ ] HTML JS lowering uses a constructor that directly encodes page-bundle reachability and glue support.
- [ ] `src/backends/js/utils.rs` no longer exists as a catch-all module.
- [ ] Utility responsibilities are split into focused modules.
- [ ] Repeated `Load` / `Copy` value-use lowering is centralized.
- [ ] Call-target lowering and inline-expression substitution live in a focused call-lowering module.
- [ ] Runtime collection helper repetition is reduced without macro-heavy Rust generation.
- [ ] Runtime glue is deepened into a modest directory module with focused responsibilities.
- [ ] External JS parser behavior is better covered; scanner cursor extraction is done only if it improves readability without behavior drift.
- [ ] Source comments match the new module layout.
- [ ] User-facing language behavior is unchanged unless an explicitly documented bug fix was required.
- [ ] Full validation passes.
- [ ] No trait implementation work was expanded.
