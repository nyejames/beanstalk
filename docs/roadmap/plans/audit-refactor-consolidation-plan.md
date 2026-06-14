# Audit refactor consolidation implementation plan

## Goal

Implement the refactor plan from the broad compiler audit: consolidate backend feature validation,
split overgrown same-stage modules, remove duplicated traversal logic where ownership is clear, and
strengthen regression coverage without changing Beanstalk language semantics.

## User decisions

- Implementation scope: implement the audit refactor plan above.
- Plan shape: large multi-slice ad hoc work gets this durable plan before code changes.
- Roadmap handling: do not edit `docs/roadmap/roadmap.md` during this plan unless the user later
  explicitly requests it; that file already has unrelated local changes.

## Current repo anchors

- Relevant docs:
  - `AGENTS.md`: requires strict duplication cleanup, stage-boundary preservation, and final audit.
  - `docs/codebase-style-guide.md`: governs module splits, diagnostics, comments, validation, and
    test ownership.
  - `docs/compiler-design-overview.md`: defines HIR reachability, backend feature validation, AST
    template ownership, and TypeId-first type resolution boundaries.
  - `docs/language-overview.md`: keeps casts, templates, maps, generics, imports, and reactivity
    semantics stable.
  - `docs/memory-management-design.md`: confirms no borrow/ownership semantic changes are intended.
- Relevant implementation paths:
  - `src/compiler_frontend/hir/reachability.rs`: shared HIR reachability facts.
  - `src/backends/backend_feature_validation.rs`: target-specific pre-lowering feature validation.
  - `src/backends/external_package_validation.rs`: external package support validation.
  - `src/projects/html_project/html_project_builder.rs`: HTML builder validation orchestration.
  - `src/projects/html_project/wasm/artifacts.rs`: HTML-Wasm artifact planning and emission; Phase
    1 removed its duplicate runtime-cast/generic-runtime-value preflight scanners.
  - `src/compiler_frontend/ast/module_ast/finalization/reactive_templates.rs`: current mixed
    reactive metadata flow, collection, and annotation pass.
  - `src/compiler_frontend/ast/templates/template_types.rs`, `template.rs`, and
    `template_render_plan.rs`: existing template-owned metadata traversal helpers.
  - `src/compiler_frontend/ast/templates/template_folding.rs`: current mixed template folding,
    const loop, fold-binding, and render emission logic.
  - `src/compiler_frontend/ast/templates/template_control_flow/const_eval.rs`: existing owner for
    const-required template-control-flow checks.
  - `src/compiler_frontend/ast/type_resolution/resolve_type.rs` and `mod.rs`: current broad
    TypeId-first semantic type-resolution module.
- Relevant tests/fixtures:
  - `src/compiler_frontend/hir/tests/reachability_tests.rs`: reachability fact tests.
  - `src/compiler_frontend/hir/tests/hir_reactivity_tests.rs`: reactive HIR reachability tests.
  - `src/compiler_frontend/ast/module_ast/finalization/tests/reactive_templates_tests.rs`:
    reactive metadata propagation unit coverage.
  - `tests/cases/hashmap_wasm_*`, `tests/cases/reactive_wasm_*`,
    `tests/cases/html_wasm_reachable_generic_runtime_value_unsupported`,
    `tests/cases/html_wasm_unreachable_generic_runtime_value`, and `tests/cases/cast_*`:
    backend unsupported-feature integration coverage.
  - `tests/cases/manifest.toml`: must be updated for any new integration case.
- Roadmap/progress matrix state:
  - `docs/roadmap/roadmap.md`: already has unrelated local edits; this ad hoc plan should not
    update it automatically.
  - `docs/src/docs/progress/#page.bst`: no expected status change because this is a refactor.
    Update only if implementation discovers a support-status bug or changes feature behavior.

## Non-goals and deliberately deferred work

- Do not change language semantics, diagnostic codes, or backend capability status.
- Do not implement new Wasm lowering for casts, generic runtime values, maps, reactivity, or
  JS-backed external packages.
- Do not introduce broad generic AST/HIR visitor frameworks.
- Do not rewrite all mode-only integration fixtures in this plan; strengthen fixtures touched by
  the refactors and record any remaining broad cleanup separately.
- Do not edit generated documentation artifacts.

## Phase 1: Backend validation consolidation

Consolidate unsupported-feature validation around shared HIR reachability so backend lowerers do not
own duplicate preflight scans.

- [x] Extend `HirReachability` or a backend-local reachable-feature fact helper with runtime cast
  facts, using source locations from the HIR side table and preserving existing
  map/reactive/external facts.
- [x] Keep TypeEnvironment-dependent generic-runtime-value detection out of pure callgraph
  reachability unless the API is explicitly shaped as a typed feature fact collector; prefer a
  backend validation helper that consumes `HirReachability.reachable_blocks` plus `TypeEnvironment`.
- [x] Move HTML-Wasm runtime cast and generic runtime value validation from
  `src/projects/html_project/wasm/artifacts.rs` into `src/backends/backend_feature_validation.rs`.
- [x] Preserve existing user-facing unsupported-feature diagnostic codes, source locations, and
  backend display labels for the affected cases unless a deliberate test update proves the current
  label was wrong.
- [x] Add a validation input/context that can validate from `start` or an explicit root-function
  export policy without collecting reachability more than once for the same validation boundary.
- [x] Keep HTML-Wasm artifact code focused on planning, JS bootstrap, Wasm lowering, and output
  emission; delete obsolete artifact-local scanner helpers.
- [x] Strengthen regression coverage for reachable and unreachable runtime casts and generic
  runtime values, preferably by reusing existing `cast_*` and `html_wasm_*` cases unless a new
  focused case is clearer.
- [x] Audit/style-guide/validation checkpoint:
  - [x] Check style-guide compliance.
  - [x] Check architecture and stage-boundary compliance.
  - [x] Check language semantics and diagnostics.
  - [x] Check memory-model impact.
  - [x] Check duplicated or obsolete logic.
  - [x] Run targeted validation: `cargo test reachability_tests`.
  - [x] Run targeted integration validation for affected `html_wasm`, `hashmap_wasm`, `reactive_wasm`,
        and `cast` cases, or document why the runner cannot filter narrowly.
  - [x] Record validation status and remaining risks.

Phase 1 accepted summary:
- `HirReachability` now records `reachable_runtime_casts`; backend feature validation consumes
  those facts for Wasm runtime-cast rejection instead of rescanning artifact-local HIR.
- `BackendFeatureValidationInput` selects either `start()` or explicit export roots. HTML-Wasm uses
  its export plan roots before artifact lowering, while JS validation keeps the start-root policy.
- Generic runtime value rejection now lives in `src/backends/backend_feature_validation.rs` and uses
  `HirReachability.reachable_blocks` plus the module `TypeEnvironment`, preserving HIR reachability
  as an untyped CFG/function feature pass.
- Removed duplicate runtime-cast and generic-runtime-value scanners from
  `src/projects/html_project/wasm/artifacts.rs`.
- Existing unsupported-feature diagnostic code `BST-RULE-0064` and source locations are preserved.
  Runtime-cast backend display text was standardized from `html_wasm` to `Wasm` to match the
  existing backend target labels used by hashmap, reactive, and external-package validation.
- Added `cast_wasm_unreachable_cast_ignored` and strengthened reachability unit coverage for
  reachable-only runtime cast facts.

Phase 1 validation:
- `cargo fmt`: passed.
- `cargo test reachability_tests`: passed.
- `cargo test hir_reactivity_tests`: passed.
- `cargo test html_project_builder`: passed.
- `cargo test wasm::artifacts`: passed.
- `cargo run -- tests --backend html_wasm`: passed, 99/99.
- `cargo run -- tests --backend html`: passed, 1496/1496.
- `just validate`: passed, including cross-target clippy, 2413 unit tests, 1595 integration cases,
  docs check, and benchmark check.

Phase 1 remaining notes:
- HTML-Wasm currently builds the export plan once for validation root selection and again during
  artifact planning. The plan is deterministic and cheap; unify this only if future export policy
  grows more complex.
- External-package support validation remains a separate registry-backed check. Current HTML-Wasm
  exports are rooted at `start()`, so this is behaviorally equivalent for Phase 1.

## Phase 2: Reactive template finalization split

Separate reactive metadata function-flow analysis, template-aware collection, and AST annotation
while keeping AST finalization as the owner of value flow.

- [x] Split `src/compiler_frontend/ast/module_ast/finalization/reactive_templates.rs` into a
  submodule with clear files such as `flow.rs`, `collector.rs`, and `annotation.rs`.
- [x] Move shared structural template metadata traversal behind template-owned helper APIs when the
  traversal only depends on template shape; keep flow-aware expression resolution in finalization.
- [x] Delete duplicated structural walkers that become unnecessary after the template-owned helper is
  the single owner for template/control-flow/render-plan shape traversal.
- [x] Preserve `propagate_reactive_template_metadata_in_ast` as the public testable entry point
  unless a clearer current API fully replaces it.
- [x] Update or split existing reactive metadata unit tests only when needed to match new module
  boundaries; do not add implementation-shaped assertions.
- [x] Audit/style-guide/validation checkpoint:
  - [x] Check style-guide compliance.
  - [x] Check architecture and stage-boundary compliance.
  - [x] Check language semantics and diagnostics.
  - [x] Check memory-model impact.
  - [x] Check duplicated or obsolete logic.
  - [x] Run targeted validation: `cargo test reactive_templates_tests`.
  - [x] Record validation status and remaining risks.

Phase 2 slice 1 accepted summary:
- Split the former 1,316-line `reactive_templates.rs` into
  `reactive_templates/mod.rs`, `flow.rs`, `collector.rs`, `annotation.rs`, and `types.rs`.
- Kept `propagate_reactive_template_metadata_in_ast` as the testable entry point and preserved
  `AstFinalizer::propagate_reactive_template_metadata` for `finalizer.rs`.
- Kept the existing unit test file at
  `src/compiler_frontend/ast/module_ast/finalization/tests/reactive_templates_tests.rs`; the new
  directory module loads it with a relative `#[path]` so tests stay in the established finalization
  test directory.
- Parent review corrected one inline import introduced during the split.

Phase 2 slice 1 validation:
- `cargo fmt`: passed.
- `cargo test reactive_templates_tests`: passed, 5/5.
- `cargo test --lib compiler_frontend::ast::module_ast::finalization`: passed, 5/5.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `just validate`: passed, including cross-target clippy, 2413 unit tests, 1595 integration cases,
  docs check, and benchmark check.

Phase 2 slice 2 accepted summary:
- Added `src/compiler_frontend/ast/templates/reactive_template_metadata.rs` as the single
  template-owned structural traversal helper for reactive metadata. It walks content,
  template control flow, render plans, aggregate render plans, and runtime slot application plans.
- The helper accepts a caller-supplied expression resolver. `Template::reactive_template_metadata`
  uses the default resolver that reads already-computed expression metadata, while AST
  finalization supplies its flow-aware resolver from `reactive_templates/collector.rs`.
- Removed duplicate structural walkers from `collector.rs`, `template_types.rs`, `template.rs`,
  and `template_render_plan.rs`. `collector.rs` now owns expression/function-call/value-flow
  resolution only.
- Parent review corrected one non-ASCII punctuation mark in the new helper docs.

Phase 2 slice 2 validation:
- `cargo fmt`: passed.
- `cargo test reactive_templates_tests`: passed, 5/5.
- `cargo test --lib compiler_frontend::ast::templates`: passed, 299/299.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `just validate`: passed, including cross-target clippy, 2413 unit tests, 1595 integration cases,
  docs check, and benchmark check with no measurable change.

Phase 2 remaining notes:
- Phase 2 is complete. No language semantics, diagnostics, memory-model behavior, or progress-matrix
  status changed.

## Phase 3: Template const-folding ownership cleanup

Move const loop and fold-binding mechanics closer to the template-control-flow const owner, leaving
render emission orchestration in `template_folding.rs`.

- [x] Create a focused template-control-flow const folding/evaluation module for `ConstRangeCursor`,
  numeric const range validation, const collection extraction, and iteration binding construction.
- [x] Move fold-binding expression substitution helpers out of `template_folding.rs` if the new
  owner makes the call flow clearer and does not blur stage boundaries.
- [x] Keep render-plan folding and emission-specific wrapper behavior local to `template_folding.rs`.
- [x] Preserve all existing template const loop diagnostics, source locations, and expansion-limit
  behavior.
- [x] Add or update targeted tests only if moving the helpers exposes a missing invariant; otherwise
  rely on existing const template loop integration/unit coverage.
- [x] Audit/style-guide/validation checkpoint:
  - [x] Check style-guide compliance.
  - [x] Check architecture and stage-boundary compliance.
  - [x] Check language semantics and diagnostics.
  - [x] Check memory-model impact.
  - [x] Check duplicated or obsolete logic.
  - [x] Run targeted validation for template head/control-flow tests and affected integration cases.
  - [x] Record validation status and remaining risks.

Phase 3 accepted summary:
- Added `src/compiler_frontend/ast/templates/template_control_flow/const_folding.rs` as the
  focused owner for const-loop range cursoring, numeric range validation, const collection source
  extraction, and per-iteration fold-binding construction.
- Moved `TemplateFoldBinding` with the const-loop helper so `TemplateFoldContext` can keep using
  the same binding stack without making `template_control_flow` depend on `template_folding.rs`.
- Kept render-plan folding, emission-specific wrapper behavior, and expression substitution in
  `template_folding.rs`. The substitution helpers are tied to render-piece folding and constant
  folding, so moving them would broaden the control-flow helper beyond loop mechanics.
- Parent review removed an unnecessary unused-import suppression from the worker patch.

Phase 3 validation:
- `cargo fmt`: passed.
- `cargo test --lib compiler_frontend::ast::templates::create_template_node::create_template_node_tests::head_tests`:
  passed, 78/78.
- `cargo test --lib compiler_frontend::ast::templates`: passed, 299/299.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `just validate`: passed, including cross-target clippy, 2413 unit tests, 1595 integration cases,
  docs check, and benchmark check with no measurable change.

Phase 3 remaining notes:
- Phase 3 is complete. No language semantics, diagnostics, memory-model behavior, generated docs,
  or progress-matrix status changed.

## Phase 4: AST type-resolution module split

Preserve AST ownership and TypeId-first semantics while splitting `resolve_type.rs` into smaller
stage-local responsibility files.

- [x] Split context/input structs into a `context` module.
- [x] Split fixed collection capacity folding into a `collections` module.
- [x] Split map nesting and map-key validation into a `maps` module.
- [x] Split type alias lookup and alias re-resolution helpers into an `aliases` module.
- [x] Split generic nominal instantiation and bound-evidence checks into a `generics` module.
- [x] Split named/namespaced lookup and trait-name rejection into a `lookup` module when this keeps
  imports clearer than leaving lookup local.
- [x] Keep `resolve_type.rs` as the semantic entrypoint for `resolve_parsed_type_annotation`,
  diagnostic-type-to-TypeId conversion helpers, and `resolve_type` orchestration.
- [x] Update `mod.rs` re-exports so callers do not gain compatibility shims or parallel APIs.
- [x] Audit/style-guide/validation checkpoint:
  - [x] Check style-guide compliance.
  - [x] Check architecture and stage-boundary compliance.
  - [x] Check language semantics and diagnostics.
  - [x] Check memory-model impact.
  - [x] Check duplicated or obsolete logic.
  - [x] Run targeted validation for type-resolution/generic/map/fixed-capacity tests.
  - [x] Record validation status and remaining risks.

Phase 4 slice 1 accepted summary:
- Added `src/compiler_frontend/ast/type_resolution/context.rs` for
  `TypeResolutionContext`, `TypeResolutionContextInputs`, `ResolvedTypeAnnotation`, and the context
  construction helpers.
- Updated `type_resolution/mod.rs` to re-export those types from the new owner so existing callers
  continue using `ast::type_resolution` without compatibility shims.
- Kept `resolve_type.rs` as the semantic entrypoint for parsed annotation resolution,
  diagnostic-type-to-`TypeId` conversion, and `resolve_type` orchestration.
- No type-resolution behavior, diagnostics, `TypeId` semantics, or caller-visible API names changed.

Phase 4 slice 1 validation:
- `cargo fmt`: passed.
- `cargo test --lib compiler_frontend::ast::type_resolution_tests`: passed, 20/20.
- `cargo test --lib compiler_frontend::declaration_syntax::type_syntax::type_syntax_tests`:
  passed, 49/49.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `just validate`: passed, including cross-target clippy, 2413 unit tests, 1595 integration cases,
  docs check, and benchmark check with no measurable change.

Phase 4 remaining notes:
- Phase 4 structural splits are complete. The next Phase 4 checkpoint is the audit/style-guide and
  validation review over the split type-resolution module.

Phase 4 slice 2 accepted summary:
- Added `src/compiler_frontend/ast/type_resolution/collections.rs` as the focused owner for fixed
  collection capacity folding.
- Moved `fold_collection_capacity` out of `resolve_type.rs` while preserving the
  `ast::type_resolution::fold_collection_capacity` re-export used by existing callers.
- Parent review consolidated duplicated literal and bare-constant capacity value validation inside
  the new module.
- Kept collection type interning and fallback-to-diagnostic-type decisions in
  `resolve_parsed_type_annotation`, so `resolve_type.rs` remains the semantic entrypoint.
- No collection semantics, diagnostics, diagnostic locations, `TypeId` behavior, or caller-visible
  API names changed.

Phase 4 slice 2 validation:
- `cargo fmt`: passed.
- `cargo test --lib compiler_frontend::ast::type_resolution_tests`: passed, 20/20.
- `cargo test --lib compiler_frontend::declaration_syntax::type_syntax::type_syntax_tests`:
  passed, 49/49.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `just validate`: passed, including cross-target clippy, 2413 unit tests, 1595 integration cases,
  docs check, and benchmark check with no measurable change (average +2ms across 16 cases).

Phase 4 slice 3 accepted summary:
- Added `src/compiler_frontend/ast/type_resolution/maps.rs` as the focused owner for source-authored
  map type policy: inline nesting readability validation and V1 scalar key validation.
- Moved `map_nesting_depth` and `validate_map_key_type` out of `resolve_type.rs`.
- Preserved `ast::type_resolution::validate_map_key_type` through the module re-export without a
  forwarding wrapper, while narrowing `map_nesting_depth` to the type-resolution module tree.
- Kept map interning, parsed annotation orchestration, and diagnostic-type conversion in
  `resolve_type.rs`.
- No map semantics, diagnostics, diagnostic locations, `TypeId` behavior, or caller-visible API
  names changed.

Phase 4 slice 3 validation:
- `cargo fmt`: passed.
- `cargo test --lib compiler_frontend::ast::type_resolution_tests`: passed, 20/20.
- `cargo test --lib compiler_frontend::declaration_syntax::type_syntax::type_syntax_tests`:
  passed, 49/49.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- Worker integration validation: `cargo run -- tests --backend html_wasm` passed, 99/99, and
  `cargo run -- tests --backend html` passed, 1496/1496, covering the existing hashmap cases.
- `just validate`: passed, including cross-target clippy, 2413 unit tests, 1595 integration cases,
  docs check, and benchmark check with no measurable change (average 0ms across 16 cases).

Phase 4 slice 4 accepted summary:
- Added `src/compiler_frontend/ast/type_resolution/aliases.rs` as the focused owner for type alias
  lookup and alias-target re-resolution.
- Moved bare and namespaced alias lookup, fixed-capacity alias-target detection, and alias source
  scope reconstruction out of `resolve_type.rs`.
- Kept `resolve_parsed_type_annotation` in `resolve_type.rs` as the semantic entrypoint; it now
  delegates alias handling to the new module.
- Preserved fixed-capacity alias behavior by re-resolving such targets through the alias
  declaration file's visibility and clearing use-site body-local declarations.
- No alias semantics, diagnostics, diagnostic locations, `TypeId` behavior, or caller-visible API
  names changed.

Phase 4 slice 4 validation:
- `cargo fmt`: passed.
- `cargo test --lib compiler_frontend::ast::type_resolution_tests`: passed, 20/20.
- `cargo test --lib compiler_frontend::declaration_syntax::type_syntax::type_syntax_tests`:
  passed, 49/49.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- Worker integration validation: `cargo run -- tests --backend html_wasm` passed, 99/99, and
  `cargo run -- tests --backend html` passed, 1496/1496, covering existing alias and
  fixed-collection cases.
- `just validate`: passed, including cross-target clippy, 2413 unit tests, 1595 integration cases,
  docs check, and benchmark check with no measurable change (average +1ms across 16 cases).

Phase 4 slice 5 accepted summary:
- Added `src/compiler_frontend/ast/type_resolution/generics.rs` as the focused owner for lazy
  nominal generic struct/choice instantiation and bound-evidence validation.
- Moved `instantiate_generic_nominal` and nominal bound-evidence validation out of
  `resolve_type.rs`.
- Consolidated the repeated struct/choice generic instance `TypeId` interning into a small
  stage-local helper in the new module.
- Kept generic base lookup, bare generic name rejection, and the high-level
  `DataType::GenericInstance` orchestration in `resolve_type.rs`.
- Parent review removed a stale unused `StringTable` parameter from the moved helper.
- No generic semantics, diagnostics, diagnostic locations, bound-evidence behavior, `TypeId`
  behavior, or caller-visible API names changed.

Phase 4 slice 5 validation:
- `cargo fmt`: passed.
- `cargo test --lib compiler_frontend::ast::type_resolution_tests`: passed, 20/20.
- `cargo test --lib compiler_frontend::declaration_syntax::type_syntax::type_syntax_tests`:
  passed, 49/49.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- Worker integration validation: `cargo run -- tests --backend html` passed, 1496/1496, and
  `cargo run -- tests --backend html_wasm` passed, 99/99, covering existing generics cases.
- `just validate`: passed, including cross-target clippy, 2413 unit tests, 1595 integration cases,
  docs check, and benchmark check with no measurable change (average +1ms across 16 cases).

Phase 4 slice 6 accepted summary:
- Added `src/compiler_frontend/ast/type_resolution/lookup.rs` as the focused owner for
  source-visible type-name lookup and trait-name rejection.
- Moved bare named type lookup, namespace-qualified type lookup, generic application base lookup,
  trait-name rejection, declaration lookup helpers, builtin-name lookup, and public
  `Option`/`Result` deferred syntax checks out of `resolve_type.rs`.
- Reused the aliases module's bare alias lookup helper in named type resolution instead of leaving
  duplicate alias lookup logic in the lookup path.
- Kept alias target re-resolution, lazy generic instance materialization, diagnostic-type-to-`TypeId`
  conversion, and the high-level `resolve_type` orchestration with their existing owners.
- Parent review removed stale unused `StringTable` parameters from moved lookup helpers and narrowed
  helper visibility where practical.
- No lookup semantics, diagnostics, diagnostic locations, namespace type/value misuse behavior,
  generic application diagnostics, `TypeId` behavior, or caller-visible API names changed.

Phase 4 slice 6 validation:
- `cargo fmt`: passed.
- `cargo test --lib compiler_frontend::ast::type_resolution_tests`: passed, 20/20.
- `cargo test --lib compiler_frontend::declaration_syntax::type_syntax::type_syntax_tests`:
  passed, 49/49.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- Worker integration validation: `cargo run -- tests --backend html` passed, 1496/1496, and
  `cargo run -- tests --backend html_wasm` passed, 99/99, covering existing generic, trait, and
  namespace cases.
- `just validate`: passed, including cross-target clippy, 2413 unit tests, 1595 integration cases,
  docs check, and benchmark check with no measurable change (average -1ms across 16 cases).

Phase 4 audit checkpoint summary:
- Reviewed the split type-resolution module surface after the `context`, `collections`, `maps`,
  `aliases`, `generics`, and `lookup` extractions. `resolve_type.rs` remains the semantic entrypoint
  for parsed annotation orchestration, recursive diagnostic `DataType` resolution, and
  diagnostic-type-to-`TypeId` conversion.
- Corrected stale file-level ownership comments in `aliases.rs`, `collections.rs`, and
  `generics.rs` so they point at the current `generic_parameters`, `lookup`, `generics`, and
  `resolve_type` owners.
- No language semantics, diagnostic codes, source locations, TypeId identity behavior,
  memory-model behavior, generated docs, roadmap state, or progress-matrix status changed.
- Remaining type-resolution module sizes are comfortably below the style-guide split threshold;
  the largest files are now `resolve_type.rs` at 618 lines and `lookup.rs` at 486 lines.

Phase 4 audit checkpoint validation:
- `cargo fmt`: passed.
- `cargo test --lib compiler_frontend::ast::type_resolution_tests`: passed, 20/20.
- `cargo test --lib compiler_frontend::declaration_syntax::type_syntax::type_syntax_tests`:
  passed, 49/49.
- `cargo test --lib generics_tests`: passed, 25/25.
- `cargo test --lib capacity_reference`: passed, 6/6.
- `cargo test --lib map_type`: passed, 18/18.

## Phase 5: Coverage and stale-path cleanup

Close refactor-induced coverage gaps and avoid carrying obsolete scaffolding forward.

- [x] Review the 46 currently mode-only `expect.toml` cases identified by the audit and strengthen
  only cases directly touched by these refactors or cases whose weak assertion hides changed
  behavior.
- [x] Prefer `diagnostic_codes`, `rendered_output_contains`, and backend-specific
  `artifact_assertions` over broad compile-success assertions.
- [x] Remove obsolete helper functions, imports, comments, or test fixtures made redundant by the
  refactors.
- [x] Update `docs/src/docs/progress/#page.bst` only if a feature status, deliberately deferred
  behavior, or discovered bug changes.
- [x] Audit/style-guide/validation checkpoint:
  - [x] Check style-guide compliance.
  - [x] Check architecture and stage-boundary compliance.
  - [x] Check language semantics and diagnostics.
  - [x] Check memory-model impact.
  - [x] Check duplicated or obsolete logic.
  - [x] Run targeted validation for strengthened cases.
  - [x] Record validation status and remaining risks.

Phase 5 accepted summary:
- Reviewed the current weak `expect.toml` surface using the integration expectation parser's
  assertion model. The current backend-variant count differs from the original audit wording
  because backend baseline contracts and backend matrix variants are counted separately, but the
  directly touched Phase 4 namespace/alias fixtures were still weak.
- Strengthened `module_facade_reexport_namespace_success` and
  `namespace_import_module_facade_success` with explicit HTML-Wasm `page.wasm` artifact assertions.
  HTML-Wasm fixtures cannot use `rendered_output_contains` because the harness only runs inline
  HTML scripts and these builds emit external `page.js`.
- Strengthened `type_alias_via_imported_source_alias` and `private_type_alias_import_rejected`
  with HTML `rendered_output_contains` assertions.
- Reviewed stale helper/comment paths in the refactored backend validation, reactive template,
  template folding, and type-resolution areas. No additional obsolete helpers, imports, fixtures,
  or comments needed removal beyond the ownership-comment cleanup already completed in Phase 4.
- No language semantics, diagnostics, memory-model behavior, feature support state, generated docs,
  or progress-matrix status changed; `docs/src/docs/progress/#page.bst` does not need an update.

Phase 5 validation:
- `cargo run -- tests --backend html_wasm`: passed, 99/99.
- `cargo run -- tests --backend html`: passed, 1496/1496.

## Final audit and validation

- [x] Check style-guide compliance.
- [x] Check architecture/stage-boundary compliance.
- [x] Check language-semantics compliance.
- [x] Check memory-model compliance.
- [x] Check diagnostics quality.
- [x] Check test coverage.
- [x] Check duplicated or obsolete logic.
- [x] Check progress matrix accuracy.
- [x] Check validation status.
- [x] Check stale code/docs that should be removed or updated.
- [x] Check duplicated logic to consolidate or intentionally leave local.
- [x] Run final validation, preferably `just validate`.

Final audit summary:
- Parent audit scope covered the full active refactor commit series from backend feature validation
  through reactive template finalization, shared template metadata traversal, const-loop folding,
  type-resolution module splits, refactor-adjacent fixture strengthening, and this plan.
- Required finding corrected: the first Phase 5 HTML alias assertions used short fragments (`"1"`
  and `"Ada"`). The fixtures now emit unique labels and assert the full rendered output strings.
- Style and architecture review found the final ownership boundaries aligned with the style guide
  and compiler design overview: backend feature validation consumes HIR reachability facts,
  template-owned structural traversal is separated from AST flow-aware resolution, const-loop
  mechanics live with template control-flow const support, and `resolve_type.rs` remains the
  semantic type-resolution entrypoint while focused submodules own lookup, aliases, collections,
  maps, context, and generic nominal instantiation.
- No required additional consolidation was found. Similar logic intentionally remains local where
  the behavior is stage-specific: backend feature validation still stays separate from external
  package registry validation, and HTML-Wasm artifact planning still builds deterministic output
  plans rather than owning unsupported-feature scans.
- No user-facing language semantics, diagnostic codes, memory-model behavior, generated docs, or
  progress-matrix status changed. The progress matrix remains accurate and was not edited.

Final validation:
- `cargo fmt`: passed.
- `cargo run -- tests --backend html`: passed, 1496/1496, after the final alias-fixture audit
  correction.
- `just validate`: passed, including cross-target clippy, 2413 unit tests, 1595 integration cases,
  docs check, and benchmark check.
- Benchmark check result: no measurable change, average 0ms across 16/16 cases.
