# Frontend representation cleanup implementation plan

## Goal

Implement the frontend representation cleanup from the audit: collapse duplicate type-alias metadata,
move executable AST string/path operator policy off diagnostic-only `DataType`, and move test-only
code out of production modules without changing Beanstalk language behavior or diagnostics.

## User decisions

- Worker choice: use fixed `kimi-2.7` through `kimi-beanstalk`.
- Fallback policy: no fallback worker is allowed unless the user explicitly authorizes it later.
- Semantics: preserve current language behavior, diagnostics, and generated artifacts.
- Documentation scope: this plan file is authorized; no other docs or progress-matrix updates are
  expected unless implementation discovers a language-surface status change or stale docs.

## Current repo anchors

- Relevant docs:
  - `docs/compiler-design-overview.md`: `TypeId` is canonical semantic type identity; `DataType`
    is parse-only and diagnostic-only once semantic IDs exist.
  - `docs/codebase-style-guide.md`: tests and test-only helpers must live outside production code.
  - `docs/language-overview.md`: aliases are transparent, string slices and templates remain
    distinct source/value surfaces, and this work must not change syntax or user semantics.
  - `docs/memory-management-design.md`: no direct ownership or borrow-model impact is expected.
- Relevant implementation paths:
  - `src/compiler_frontend/ast/module_ast/environment/*`,
    `src/compiler_frontend/ast/module_ast/scope_context*`, and
    `src/compiler_frontend/ast/type_resolution/*`: current alias maps and type-resolution context.
  - `src/compiler_frontend/ast/expressions/*`: expression value metadata, operator typing, and
    test-only call-argument parser entry point.
  - `src/compiler_frontend/pipeline.rs` and `src/build_system/create_project_modules/frontend_orchestration.rs`:
    production modules that currently contain test-only entry points or inline tests.
- Relevant tests/fixtures:
  - `src/compiler_frontend/ast/expressions/tests/*`: expression, operator, and call parsing tests.
  - `src/compiler_frontend/tests/parse_support.rs`: parser-stage frontend test support.
  - `src/build_system/tests/*`: build-system frontend orchestration tests.
- Roadmap/progress matrix state:
  - `docs/src/docs/progress/#page.bst`: no planned update because this is not intended to change
    feature support. Update it only if a language-surface status change or bug is discovered.

## Non-goals and deliberately deferred work

- Do not remove all `diagnostic_type` fields from AST expressions; this plan only removes the
  executable semantic string/path decision from `DataType`.
- Do not redesign operator policy, string coercion, templates, aliases, imports, traits, generics,
  or public diagnostics.
- Do not introduce compatibility wrappers, duplicate alias maps, transitional alias APIs, or broad
  utility modules.
- Do not edit docs outside this plan unless a required progress-matrix update is discovered.

## Phase 1: Collapse type-alias metadata

Resolve alias metadata through one owner map so semantic identity and diagnostic spelling remain
together. This removes the old parallel `DataType` alias map and keeps alias transparency anchored
in `ResolvedTypeAnnotation`.

- [x] Replace the two alias maps with one
  `resolved_type_aliases_by_path: FxHashMap<InternedPath, ResolvedTypeAnnotation>` in AST
  environment builder/lookups/scope context/type-resolution context.
- [x] Update alias resolution to store `source_ref`, `diagnostic_type`, and checked `type_id`
  when available in the single map.
- [x] Update environment, signatures, constants, traits, public-surface validation, nominal
  resolution, and emission call sites to use `annotation.type_id` for semantic checks and
  `annotation.diagnostic_type` only for diagnostic spelling or re-resolution.
- [x] Remove old `resolved_type_alias_annotations_by_path` names, staged-migration comments, and
  compatibility-style `with_resolved_type_alias_annotations` paths.
- [x] Audit/style-guide/validation checkpoint:
  - [x] Check style-guide compliance.
  - [x] Check architecture and stage-boundary compliance.
  - [x] Check language semantics and diagnostics.
  - [x] Check memory-model impact; no impact found.
  - [x] Check duplicated or obsolete alias logic.
  - [x] Run targeted validation: `cargo check`.
  - [x] Run focused validation: `cargo test alias_expanded_nested_optional_type_is_rejected`.
  - [x] Run search: `rg "resolved_type_alias_annotations_by_path|resolved_type_aliases_by_path.*DataType" src/compiler_frontend`.
  - [x] Record validation status and remaining risks.

Phase 1 status: accepted after parent review. Parent correction preserved checked fallback behavior
for alias public-surface and nominal-bound validation when an annotation unexpectedly lacks a
`TypeId`.

## Phase 2: Add expression value-shape metadata

Make string/path/template operator policy depend on explicit AST value metadata rather than
diagnostic-only type spelling.

- [x] Add an AST-local enum such as
  `ExpressionValueShape::{Ordinary, PlainStringSlice, CompileTimePath, TemplateString}` near the
  existing expression value metadata.
- [x] Add the shape to `Expression` and carry it into `ExpressionResultType`.
- [x] Populate the shape through expression constructors and literal/template/path creation paths,
  preserving current behavior for ordinary strings, raw strings, compile-time paths, templates,
  folded templates, field accesses, and call results.
- [x] Update `both_plain_string_slices` to check canonical string `TypeId` plus
  `ExpressionValueShape::PlainStringSlice`; remove semantic matches on `diagnostic_type`.
- [x] Add or update focused tests proving ordinary strings still support the existing string
  operators and compile-time path/template-shaped values do not accidentally become plain string
  operands.
- [x] Audit/style-guide/validation checkpoint:
  - [x] Check style-guide compliance.
  - [x] Check architecture and stage-boundary compliance.
  - [x] Check language semantics and diagnostics.
  - [x] Check memory-model impact; no impact found.
  - [x] Check duplicated or obsolete value-shape logic.
  - [x] Run targeted validation: `cargo check` and `cargo test -p beanstalk --lib eval_expression_tests`.
  - [x] Run search: `rg "diagnostic_type.*StringSlice|StringSlice.*diagnostic_type" src/compiler_frontend/ast/expressions/eval_expression` returned no matches.
  - [x] Record validation status and remaining risks: chained plain-string concatenation,
    function-result conversion, and explicit copy propagation are covered by targeted tests.
    Full `just validate` remains part of final validation.

## Phase 3: Move test-only code out of production modules

Keep production modules free of inline tests and one-off test-only APIs while preserving current
test coverage.

- [x] Move inline tests from
  `src/build_system/create_project_modules/frontend_orchestration.rs` into the existing
  `src/build_system/tests/` test area and wire them through `mod.rs` with an external test module.
- [x] Move `CompilerFrontend::source_to_tokens` test usage into `src/compiler_frontend/tests/parse_support.rs`
  or a focused frontend test-support module.
- [x] Move the test-only `parse_call_arguments` wrapper out of `function_calls.rs` into expression
  test support, keeping production callers on `parse_call_arguments_typed_with_expectations`.
- [x] Keep only external `#[cfg(test)] #[path = "..."] mod ...;` declarations in production
  module files when Rust module wiring requires them.
- [x] Audit/style-guide/validation checkpoint:
  - [x] Check style-guide compliance.
  - [x] Check architecture and stage-boundary compliance.
  - [x] Check language semantics and diagnostics; no behavior change intended or found.
  - [x] Check memory-model impact; no impact found.
  - [x] Check duplicated or obsolete test support.
  - [x] Run targeted validation for build-system and expression tests.
  - [x] Run search: `rg "source_to_tokens|parse_call_arguments|#\\[cfg\\(test\\)\\]\\s*mod tests" src/compiler_frontend src/build_system/create_project_modules`.
  - [x] Record validation status and remaining risks.

Phase 3 status: accepted after parent review. The tokenizer-only helper now lives in
`src/compiler_frontend/tests/parse_support.rs`, and the existing frontend pipeline test caller was
updated as a directly required test call site. The call-argument syntax tests use a test-local
helper that calls the production inner parser with no expectations, preserving the old
syntax-only test intent without keeping a production wrapper.

## Final audit and validation

- [x] Check style-guide compliance.
- [x] Check architecture/stage-boundary compliance.
- [x] Check language-semantics compliance.
- [x] Check memory-model compliance; no impact found.
- [x] Check diagnostics quality.
- [x] Check test coverage.
- [x] Check duplicated or obsolete logic.
- [x] Check progress matrix accuracy.
- [x] Check validation status.
- [x] Check stale code/docs that should be removed or updated.
- [x] Check duplicated logic to consolidate or intentionally leave local.
- [x] Run final validation: `just validate`.

Final status: Kimi audit completed with one required correction, preserving expression metadata
when struct-field default constant inlining rebuilds wrapper expressions. Parent applied the fix,
added a struct default value-shape regression test, reran targeted struct parsing tests, and ran
`just validate` successfully. Optional audit suggestions around broader `Expression::new`
defaults and alias fallback consolidation are deferred because the required root-cause fix is
localized and the remaining duplication is small.
