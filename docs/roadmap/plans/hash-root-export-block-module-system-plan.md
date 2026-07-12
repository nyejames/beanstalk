# Hash-root module files and `export:` blocks implementation plan

## Current state

ACTIVE_PLAN: `docs/roadmap/plans/hash-root-export-block-module-system-plan.md`
STATUS: active
CURRENT_SLICE: Phase 6B rename AST public-surface semantics and module-root diagnostics
LAST_ACCEPTED_COMMIT: `06699066f` (`refactor: remove legacy export path scanning`)
WORKTREE: main worktree `/Users/aneirinjames/projects/beanstalk/beanstalk` on branch `main` at `06699066f`; Phase 6A is accepted for its next checkpoint. The concurrent docs migration is committed separately and remains outside this plan's implementation commits.
REQUIRED_RELOADS_AFTER_COMPACTION:
- `AGENTS.md`
- mandatory docs named by `AGENTS.md`
- `docs/language-overview.md`
- compiler-design documents selected through the current `AGENTS.md` task-reading matrix
- `docs/src/docs/project-structure/#page.bst`
- `docs/src/docs/libraries/#page.bst`
- `benchmarks/README.md`
- this plan
- current source files before editing
RELEVANT_CONTEXT_NOW:
- docs: language overview, project-structure docs, libraries docs and progress matrix use canonical `config.bst`; module-root and inline `export` wording remains for later phases.
- code: `source_tree_index.rs` enforces one hash root per module directory and passes prepared `ModuleRootTable` identity forward. `FileRole` is now `ActiveModuleRoot`, `ImportedModuleRoot` or `Normal`. Header parsing alone emits active-root start bodies, discards imported-root body tokens and records root activity facts. Generic root identity uses prepared hash-map indexes. Inline `export` remains temporarily accepted only on module roots until Phase 5 replaces it atomically.
ACCEPTANCE_CRITERIA:
- One non-config `#*.bst` root file per module directory.
- `config.bst` is the only project config filename. No alternate filename receives config-specific handling or diagnostics.
- `export:` block is the only public API marker.
- Direct imports of hash root files and `config.bst` are rejected generically.
- Builders generate artifacts only for modules with builder-relevant root activity.
- Directory builds perform exactly one expensive source-tree scan for module-root discovery after config has established `entry_root`, library folders and output folders.
- Before/after benchmarks show Stage 0/source-tree discovery improvement and no broad compile-time regression.
- `just validate` passes after each accepted implementation phase.

DECISIONS_ALREADY_MADE:
- decision: Replace inline export syntax with a single strict `export:` block.
  - reason: removes repeated visibility noise; keeps public API in one readable facade section.
  - source/user/date: user agreement, 2026-07-03
- decision: Any non-config `#*.bst` file is a module root file; the hash filename after `#` has no semantic meaning.
  - reason: removes artificial `#page.bst`/`#mod.bst` split and moves semantics into language syntax.
  - source/user/date: user agreement, 2026-07-03
- decision: A source directory may contain at most one non-config hash root file.
  - reason: keeps module identity unambiguous and avoids filename-based role drift.
  - source/user/date: user agreement, 2026-07-03
- decision: Project config uses plain `config.bst` only.
  - reason: keeps the hash-root rule pure; config is not a module root and never produces HIR or exports.
  - source/user/date: user agreement, 2026-07-03; hard-break clarification, 2026-07-11
- decision: API-only module roots do not produce HTML artifacts by default.
  - reason: artifact policy belongs to builders; no top-level root/start output means no page artifact for the HTML builder.
  - source/user/date: user agreement, 2026-07-03
- decision: Module discovery performance is part of the feature, not optional cleanup.
  - reason: the compiler must not perform redundant expensive tree walks after this refactor.
  - source/user/date: user request, 2026-07-03

BLOCKERS / RISKS:
- Config lookup runs before module-root discovery because config determines `entry_root`, `library_folders`, `dev_folder` and `output_folder`. One cheap project-root probe for `config.bst` is allowed and must not be counted as the expensive source-tree scan.
- Imported module root files can contain top-level root/start code. Parser roles must distinguish the active root file from imported root files used only for public export surface.
- Source-library roots currently assume `#mod.bst`; they must move to generic hash root files without breaking built-in `libraries/html/#mod.bst` until renamed or accepted as a cosmetic hash root name.
- HTML builder currently compiles every module into artifacts; artifact filtering must not break homepage selection or tracked asset planning.
- Benchmarks may show Stage 0 improvement but no wall-clock movement due to noise; acceptance must use stage timing plus wall-clock regression checks.
- Phase 0A fixed the initially reported Stage 0 source-discovery `result_large_err` boundary by boxing `SourceDiscoveryError::Diagnostic`.
- Phase 0 diagnostic-baseline cleanup is complete. The squash integration preserved current `templates-refactor` TIR ownership while retaining the compatible boxed-diagnostic boundaries.
- Phase 1 review on 2026-07-11 confirmed `settings.rs` and Stage 0 use only `config.bst`, while `source_libraries/mod.rs` still owns the temporary `#mod.bst`/`#page.bst`/config special-file helpers. `path_resolution.rs` still discovers module roots internally, `module_inventory.rs` still runs `discover_root_entry_files`, and header/AST visibility still use `FileRole::ModuleFacade` plus facade-named diagnostics.
- Phase 4 removed the temporary duplicate-root evidence path and now rejects multiple `#*.bst` roots in one module directory with structured `BST-CONFIG-0001` diagnostics.

VALIDATION_STATE:
- `git diff --check`: passed for plan review
- Phase 0A focused patch: `cargo fmt` passed; `cargo test --lib build_system::create_project_modules --quiet` passed (133); `git diff --check` passed.
- Phase 0B project-config patch: `cargo fmt` passed; `cargo test --lib build_system::project_config --quiet` selected 0 tests; `cargo clippy --lib --quiet` exits 0 while warning; `git diff --check` passed.
- Phase 0B generic-bound patch: `cargo fmt` passed; `cargo test --lib compiler_frontend::ast::module_ast::environment --quiet` passed (8); `cargo clippy --lib --quiet` exits 0 while warning; `git diff --check` passed.
- Phase 0B generic-body patch: `cargo fmt` passed; `cargo test --lib compiler_frontend::ast::generic_functions --quiet` passed (9); `cargo clippy --lib --quiet` exits 0 while warning; `cargo clippy --all-targets --all-features -- -D warnings` failed at the next owner, `src/compiler_frontend/ast/module_ast/environment/type_resolution.rs:1065`; `git diff --check` passed.
- Phase 0B module-member patch: `cargo fmt` passed; `cargo test --lib compiler_frontend::ast::module_ast::environment --quiet` passed (8); `cargo clippy --lib --quiet` exits 0 while warning; `cargo clippy --all-targets --all-features -- -D warnings` failed at the next owner, `src/compiler_frontend/ast/module_ast/finalization/const_fact_collection.rs:54`; `git diff --check` passed.
- Phase 0B template-normalization patch: `cargo fmt` passed; `cargo test --lib compiler_frontend::ast::module_ast::finalization --quiet` passed (32); `cargo clippy --lib --quiet` exits 0 while warning; `cargo clippy --all-targets --all-features -- -D warnings` failed at the next owner, `src/compiler_frontend/ast/receiver_methods.rs:124`; `git diff --check` passed.
- Phase 0B receiver-method catalog patch: native subagent boxed `ReceiverMethodCatalogError` payloads and unboxed them at the existing `CompilerMessages` conversion boundary. Parent reran `cargo fmt`, `cargo test --lib compiler_frontend::ast::module_ast::environment --quiet` (8), `cargo clippy --lib --quiet` (exits 0 while warning), `cargo clippy --all-targets --all-features -- -D warnings` (failed at next owner `src/compiler_frontend/ast/type_resolution/collections.rs:31`) and `git diff --check`.
- Phase 0B collection-capacity patch: native subagent boxed collection-capacity diagnostics behind `CollectionCapacityDiagnostic`; parent removed an unnecessary implicit plain-diagnostic conversion and explicitly unboxed the shorthand declaration caller. Parent reran `cargo fmt`, `cargo test --lib compiler_frontend::ast::type_resolution --quiet` (20), `cargo test --lib compiler_frontend::ast::statements --quiet` (287), `cargo clippy --lib --quiet` (exits 0 while warning), `cargo clippy --all-targets --all-features -- -D warnings` (failed at next owner `src/compiler_frontend/ast/expressions/namespace_access/leaf_resolution.rs:60`) and `git diff --check`.
- Phase 0B namespace-access patch: native subagent reused the existing boxed `ExpressionParseError` boundary for namespace value leaf resolution and removed the redundant caller conversion. Parent reran `cargo fmt`, `cargo test --lib compiler_frontend::ast::expressions --quiet` (102), `cargo clippy --lib --quiet` (exits 0 while warning), `cargo clippy --all-targets --all-features -- -D warnings` (failed at next owner `src/compiler_frontend/ast/statements/asserts.rs:31`) and `git diff --check`.
- Phase 0B assert patch: native subagent reused the existing boxed `ExpressionParseError` boundary for assert statement parsing and unboxed at the statement-dispatch caller. Parent reran `cargo fmt`, `cargo test --lib compiler_frontend::ast::statements --quiet` (287), `cargo clippy --lib --quiet` (exits 0 while warning), `cargo clippy --all-targets --all-features -- -D warnings` (failed at next owner `src/compiler_frontend/ast/statements/body_dispatch.rs:71`) and `git diff --check`.
- Phase 0B statement-dispatch patch: native subagent boxed the local `parse_function_body_statements` diagnostic result boundary and kept `function_body_to_ast` as the plain `CompilerDiagnostic` adapter. Parent reran `cargo fmt`, `cargo test --lib compiler_frontend::ast::statements --quiet` (287), `cargo clippy --lib --quiet` (exits 0 while warning), `cargo clippy --all-targets --all-features -- -D warnings` (failed at next owner `src/compiler_frontend/ast/statements/body_expr_stmt.rs:103`) and `git diff --check`.
- Phase 0B expression-statement patch: Ollama boxed the local expression-statement diagnostic result boundary and direct symbol-statement adapters. Parent improved closure names and reran `cargo fmt --check`, `cargo test --lib compiler_frontend::ast::statements --quiet` (287), `cargo clippy --lib --quiet` (exits 0 while warning), `cargo clippy --all-targets --all-features -- -D warnings` (failed first at `src/compiler_frontend/ast/statements/body_return.rs:42`) and `git diff --check`.
- Phase 0B return-statement patch: Ollama boxed the return parser's diagnostic boundary and reused the existing boxed statement-dispatch result. Parent removed two worker-created scratch binaries, improved closure names and reran `cargo fmt --check`, `cargo test --lib compiler_frontend::ast::statements --quiet` (287), `cargo clippy --lib --quiet` (exits 0 while warning), `cargo clippy --all-targets --all-features -- -D warnings` (failed first at `src/compiler_frontend/ast/statements/body_symbol.rs:46`) and `git diff --check`.
- Phase 0B symbol-statement patch: Ollama boxed `push_accessed_symbol_statement`, `parse_this_statement` and `parse_symbol_statement` behind the existing statement-dispatch boundary. Parent introduced named mutation-node steps, improved closure names and reran `cargo fmt --check`, `cargo test --lib compiler_frontend::ast::statements --quiet` (287), `cargo clippy --lib --quiet` (exits 0 while warning), `cargo clippy --all-targets --all-features -- -D warnings` (failed first at `src/compiler_frontend/ast/statements/branching.rs:85`) and `git diff --check`.
- Phase 0B branching patch: Ollama boxed the `branching.rs` result family, kept statement dispatch boxed and unboxed the shared `parse_match_block` result at its two plain-diagnostic value-production boundaries. Parent improved closure names and reran `cargo fmt --check`, `cargo test --lib compiler_frontend::ast::statements --quiet` (287), `cargo clippy --lib --quiet` (exits 0 while warning), `cargo clippy --all-targets --all-features -- -D warnings` (failed first at `src/compiler_frontend/ast/statements/collections.rs:46`) and `git diff --check`.
- Phase 0B collection patch: Ollama boxed the `collections.rs` collection/map parser result family, reused the existing boxed `ExpressionParseError::Diagnostic` expression boundary and unboxed only at the shorthand declaration's plain-diagnostic boundary. Parent improved closure names and reran `cargo fmt --check`, `cargo test --lib compiler_frontend::ast::statements --quiet` (287), `cargo clippy --lib --quiet` (exits 0 while warning), `cargo clippy --all-targets --all-features -- -D warnings` (failed first at `src/compiler_frontend/ast/statements/condition_validation.rs:20`) and `git diff --check`.
- Phase 0B condition-validation patch: Ollama boxed the four shared condition helper results, unboxed at the existing plain-diagnostic statement/value-production boundaries and reused `ExpressionParseError::Diagnostic` directly for asserts. Parent kept the alias file-local and reran `cargo fmt --check`, `cargo test --lib compiler_frontend::ast::statements --quiet` (287), `cargo clippy --lib --quiet` (exits 0 while warning) and `git diff --check`; all-target Clippy first fails at `src/compiler_frontend/ast/statements/declarations.rs:134`.
- Phase 0B declaration patch: Ollama boxed the two body-local declaration entry points behind one file-local alias, removed redundant boxing at the existing boxed statement/environment callers and preserved typed declaration/coercion/const-folding boundaries. Parent reran `cargo fmt --check`, `cargo test --lib compiler_frontend::ast::statements --quiet` (287), `cargo test --lib compiler_frontend::ast::module_ast::environment --quiet` (8), `cargo clippy --lib --quiet` (exits 0 while warning) and `git diff --check`; all-target Clippy first fails at `src/compiler_frontend/ast/statements/functions.rs:149`.
- Phase 0B function-signature patch: Ollama boxed the seven `statements/functions.rs` signature/default/return-slot result boundaries behind one file-local alias and unboxed only at existing diagnostic accumulation boundaries. Parent ran `cargo test --lib compiler_frontend::ast --quiet` (1401) and `cargo clippy --all-targets --all-features -- -D warnings`; the cleanup is absent from the report and the first remaining owner is `src/compiler_frontend/ast/statements/if_headers.rs:55`.
- Phase 0B if-header patch: Ollama boxed the four shared `if_headers.rs` result boundaries behind one file-local alias, reused the existing boxed condition-validation result and unboxed only at plain diagnostic template/match-header callers. Parent ran `cargo test --lib compiler_frontend::ast --quiet` (1401) and `cargo clippy --all-targets --all-features -- -D warnings`; `if_headers.rs` is absent and the first remaining owner is `src/compiler_frontend/ast/statements/loop_headers.rs:95`.
- Phase 0B loop-header patch: Ollama boxed the complete 16-function `loop_headers.rs` parser/binding/type result family behind one file-local alias and unboxed only at the statement/template entry boundaries. Parent ran `cargo test --lib compiler_frontend::ast --quiet` (1401) and `cargo clippy --all-targets --all-features -- -D warnings`; `loop_headers.rs` is absent and the first remaining owner is `src/compiler_frontend/ast/statements/loops.rs:28`.
- Phase 0B loop-statement patch: Ollama boxed the two `loops.rs` construction/header-boundary results behind one file-local alias and reused the existing boxed loop-header and statement-dispatch boundaries directly. Parent ran `cargo test --lib compiler_frontend::ast --quiet` (1401); worker all-target Clippy confirmed `loops.rs` is absent and first fails at `src/compiler_frontend/ast/statements/match_exhaustiveness.rs:181`.
- Phase 0B match-exhaustiveness patch: Ollama boxed `enforce_match_exhaustiveness`, boxed its three locally constructed diagnostics once and removed the redundant boxing adapter at the already boxed `branching.rs` caller. Parent ran 287 statement tests, library Clippy and all-target Clippy; `match_exhaustiveness.rs` is absent and the first remaining owner is `src/compiler_frontend/ast/statements/match_headers.rs:77`.
- Phase 0B match-header patch: Ollama boxed the seven-function `match_headers.rs` result family behind one file-local alias, removed the redundant `branching.rs` boxing adapter and unboxed only at the existing inline value-match plain-diagnostic boundary. Parent improved the expression-error adapter name, reran 287 statement tests and all-target Clippy; `match_headers.rs` is absent and the first remaining owner is `src/compiler_frontend/ast/statements/match_patterns/choice.rs:38`.
- Phase 0B choice-pattern patch: Ollama boxed the five-function `match_patterns/choice.rs` result family behind one file-local alias while keeping the already boxed match-header caller direct. Parent named the converted compiler-invariant error step and reran 287 statement tests plus all-target Clippy; `choice.rs` is absent and the first remaining owner is `src/compiler_frontend/ast/statements/match_patterns/diagnostics.rs:21`.
- Phase 0B pattern-lead patch: Ollama changed the shared lead-token rejection helper to return `Option<CompilerDiagnostic>`, then boxed only at the choice-pattern boundary and kept the literal boundary plain. Parent reran 287 statement tests, library Clippy and all-target Clippy; `diagnostics.rs` is absent and the first remaining owner is `src/compiler_frontend/ast/statements/match_patterns/literal.rs:40`.
- Phase 0B literal-pattern patch: Ollama boxed the three-function `match_patterns/literal.rs` result family behind one file-local alias, reused the boxed match-header boundary and unboxed only at the still-plain option, relational and focused-test boundaries. Parent reran 287 statement tests, library Clippy and all-target Clippy; `literal.rs` is absent and the first remaining owner is `src/compiler_frontend/ast/statements/match_patterns/option.rs:30`.
- Phase 0B option-pattern patch: Ollama boxed the two-function `match_patterns/option.rs` result family behind one file-local alias, removed the redundant literal-pattern and if-header boxing adapters and kept the existing boxed match-header boundary direct. Parent reran formatting checks, 287 statement tests, library Clippy and all-target Clippy; `option.rs` is absent and the first remaining owner is `src/compiler_frontend/ast/statements/match_patterns/relational.rs:26`.
- Phase 0B relational-pattern patch: Ollama boxed the two-function `match_patterns/relational.rs` result family behind one file-local alias and removed redundant unbox/rebox adapters at both literal-pattern boundaries. Parent reran formatting checks, 287 statement tests and all-target Clippy; `relational.rs` is absent and the first remaining owner is `src/compiler_frontend/ast/statements/multi_bind.rs:61`.
- Phase 0B multi-bind patch: Ollama boxed the complete `statements/multi_bind.rs` parser, validation, RHS classification and target-resolution family behind one file-local alias, reused already boxed type-resolution results and removed the redundant `body_symbol.rs` adapter. Parent reran formatting checks, 287 statement tests, 10 parser error-recovery tests and all-target Clippy; `multi_bind.rs` is absent and the first remaining owner is `src/compiler_frontend/ast/statements/scoped_blocks.rs:31`.
- Phase 0B scoped-block patch: Ollama boxed `parse_scoped_block_statement` behind one file-local alias while keeping the shared reserved-keyword diagnostic helper plain and reusing the already boxed statement dispatcher directly. Parent reran formatting checks, 2 focused scoped-block tests, 287 statement tests and all-target Clippy; `scoped_blocks.rs` is absent and the first remaining owner is `src/compiler_frontend/ast/statements/value_production/multi_bind.rs:97`.
- Phase 0B multi-bind value-production patch: Ollama boxed the complete `value_production/multi_bind.rs` parsing, inference, validation, coercion and expression-construction family behind one file-local alias, reused boxed condition/match/statement dispatch boundaries and removed the redundant statement multi-bind adapter. Parent normalized imports and reran formatting checks, 287 statement tests, 10 parser error-recovery tests and all-target Clippy; the file is absent and the first remaining owner is `src/compiler_frontend/ast/statements/value_production/receiver/block_if.rs:31`.
- Phase 0B block value-if patch: Ollama boxed the complete `value_production/receiver/block_if.rs` result family behind one file-local alias. Parent reused the existing boxed statement dispatcher directly, kept plain result-type inference adapted at its narrow call site and unboxed once at the receiver boundary. Formatting checks, 287 statement tests and 10 parser error-recovery tests passed; all-target Clippy no longer reports `block_if.rs` and first fails at `src/compiler_frontend/ast/statements/value_production/receiver/full_match.rs:53`.
- Phase 0B full value-match patch: Ollama boxed the complete `value_production/receiver/full_match.rs` result family behind one file-local alias, reused the boxed match-block result and removed redundant boxing at the already boxed multi-bind caller. Parent tightened the expression-error adapter and reran formatting checks, 287 statement tests, 10 parser error-recovery tests and all-target Clippy; `full_match.rs` is absent and the first remaining owner is `src/compiler_frontend/ast/statements/value_production/receiver/inline_if.rs:22`.
- Phase 0B inline value-if patch: Ollama boxed `value_production/receiver/inline_if.rs` behind one file-local alias, adapted the still-plain inline then/else result once and unboxed at the receiver boundary. Parent reran formatting checks, 287 statement tests and 10 parser error-recovery tests; worker all-target Clippy confirmed `inline_if.rs` is absent and first fails at `src/compiler_frontend/ast/statements/value_production/receiver/inline_match.rs:155`.
- Phase 0B inline value-match patch: Ollama boxed `inline_match.rs::parse_inline_value_match` behind one file-local alias, adapted the still-plain inline then/else result once and unboxed at the speculative `Option<Result<...>>` boundary. Parent reran formatting checks, 287 statement tests and 10 parser error-recovery tests; worker all-target Clippy confirmed `inline_match.rs` is absent and first fails at `src/compiler_frontend/ast/statements/value_production/receiver/inline_then_else.rs:73`.
- Phase 0B inline branch patch: Ollama boxed the complete `inline_then_else.rs` result family behind one file-local alias, kept `same_logical_line` as a plain predicate and removed redundant adapters from the already boxed inline-if and inline-match callers. Parent reran formatting checks, 287 statement tests and 10 parser error-recovery tests; worker all-target Clippy confirmed `inline_then_else.rs` is absent and first fails at `src/compiler_frontend/ast/statements/value_production/receiver/result_type.rs:40`.
- Phase 0B result-type patch: Ollama boxed the three `receiver/result_type.rs` inference boundaries behind one file-local alias and removed redundant adapters from block value-if and full value-match callers. Parent reran formatting checks, 287 statement tests and 10 parser error-recovery tests; all-target Clippy no longer reports `result_type.rs` and first fails at `src/compiler_frontend/ast/statements/value_production/receiver/mod.rs:166`.
- Phase 0B receiver-boundary patch: Ollama boxed the private Bool value-if parser behind one file-local alias, kept the public speculative `Option<Result<...>>` API plain and removed redundant conversions from already boxed condition, inline-if and block-if helpers. Parent tightened the alias comment and reran formatting checks, 287 statement tests and 10 parser error-recovery tests; all-target Clippy no longer reports `receiver/mod.rs` and first fails at `src/compiler_frontend/ast/field_access/receiver_access.rs:34`.
- Phase 0B receiver-access patch: Ollama boxed the shared receiver-access validation family, consolidated identical non-place/immutable receiver rejection and reused the existing boxed `ExpressionParseError` boundary through a direct `From<Box<CompilerDiagnostic>>` conversion. Parent introduced one file-local result alias and reran formatting checks plus 102 expression tests; all-target Clippy no longer reports `receiver_access.rs` and first fails at `src/compiler_frontend/ast/templates/create_template_node.rs:91`.
- Phase 0B template-construction patch: Ollama boxed all five `Template` constructor entry points behind one file-local alias, reused boxed statement/expression callers and unboxed only at the emitter, nested-body parser and plain test-helper boundaries. Parent reran formatting checks, 287 constructor tests, 837 template tests and 10 parser error-recovery tests; all-target Clippy no longer reports `create_template_node.rs` and first fails at `src/compiler_frontend/ast/templates/template_body_parser.rs:67`.
- Phase 0B template-body patch: Ollama boxed the complete ten-function `template_body_parser.rs` result family behind one file-local generic alias, reused boxed template construction and if-header boundaries and unboxed only at direct sentinel returns. Parent reran formatting checks, 287 constructor tests, 837 template tests and 10 parser error-recovery tests; all-target Clippy no longer reports `template_body_parser.rs` and first fails at `src/compiler_frontend/ast/templates/template_body_sentinels.rs:192`.
- Phase 0B template-sentinel patch: Ollama boxed the six diagnostic-returning boundaries in `template_body_sentinels.rs` behind one file-local generic alias, removed redundant body-parser adapters and kept the one diagnostic-remapping boundary explicit. Parent reran repository formatting checks, 287 constructor tests, 837 template tests, 10 parser error-recovery tests and all-target Clippy; the sentinel file is absent and the first remaining owner is `src/compiler_frontend/ast/templates/template_control_flow/validation.rs:56`.
- Phase 0B template control-flow validation patch: Ollama boxed the complete 14-function const-required validation family behind one file-local generic alias, kept the runtime `TemplateError` family unchanged and adapted the existing boxed normalization boundary once. Parent reran formatting checks, 837 template tests, 10 parser error-recovery tests, library Clippy and all-target Clippy; `template_control_flow/validation.rs` is absent from the report and the next production owner is `src/compiler_frontend/traits/evidence/validation.rs:71`.
- Phase 0B trait-evidence validation patch: Ollama boxed `validate_trait_evidence` behind one file-local generic alias, preserved conformance/evidence mutation order and unboxed once at the existing environment diagnostic-accumulation boundary. Parent reran formatting checks, 8 environment tests and all-target Clippy; `traits/evidence/validation.rs:71` is absent and the next owner in that family is `src/compiler_frontend/traits/evidence/requirement_matching.rs:46`.
- Phase 0B trait-requirement matching patch: the requested Kimi alias was unavailable before edits and the native fallback stalled without edits, so the parent boxed the connected four-function requirement-validation family behind one file-local alias. Formatting checks, 8 environment tests and `git diff --check` passed; all-target Clippy still fails on the known broad baseline and no longer reports `requirement_matching.rs`.
- Phase 0B trait target-resolution patch: the exact user-requested Kimi model `kimi-k2.7-code-highspeed` failed cleanly before edits because its config entry is missing, and the native fallback again stalled without edits. The parent boxed both target/trait lookup results behind one file-local alias, preserving the already boxed evidence boundary. Formatting checks, 8 environment tests and `git diff --check` passed; all-target Clippy no longer reports `target_resolution.rs`.
- Phase 0B import-resolution patch: Ollama boxed `ImportPathResolutionError::Diagnostic`, reused the already boxed source-discovery boundary and unboxed only at the project-config diagnostic accumulation boundary. Parent reran formatting checks, 112 path tests, 133 create-project-module tests and `git diff --check`; worker all-target Clippy reported 209 remaining baseline errors, with import resolution absent and `compile_time_paths.rs` next.
- Phase 0B compile-time-path patch: Ollama boxed `CompileTimePathResolutionError::Diagnostic` in the same boundary-owned pattern and unboxed only through its existing `into_diagnostic` adapter. Parent reran formatting checks, 112 path tests, 133 create-project-module tests and `git diff --check`; worker all-target Clippy confirmed both path boundary files are absent and the remaining baseline moves into direct template/AST result families.
- Phase 0B children-directive patch: Ollama boxed the two-function `$children` parsing/normalization family behind one file-local alias and unboxed once at the existing plain-diagnostic core-directive boundary. Parent tightened the boundary comments and reran formatting checks, 287 create-template-node tests, 837 template tests, 10 parser error-recovery tests and `git diff --check`; worker all-target Clippy confirmed `children_directive.rs` is absent and `core_directives.rs:35` is next.
- Phase 0B core-directive patch: Ollama boxed the two-function slot/insert and generic core-style directive family behind one file-local alias, removed the redundant `$children` unbox/rebox adapter and unboxed only at the two plain-diagnostic head-parser callers. Parent reran formatting checks, 287 create-template-node tests, 837 template tests, 10 parser error-recovery tests and `git diff --check`; worker all-target Clippy confirmed `core_directives.rs` is absent and `control_flow_suffix.rs:34` is next.
- Phase 0B control-flow suffix patch: Ollama boxed the four-function `if`/`loop` suffix parsing family behind one file-local alias, removed redundant unboxing from already boxed header parsers and unboxed only at the two plain-diagnostic head-parser call sites. Parent reran formatting checks, 287 create-template-node tests, 837 template tests, 10 parser error-recovery tests and `git diff --check`; worker all-target Clippy confirmed `control_flow_suffix.rs` is absent and the remaining connected owner is `head_parser.rs:76`.
- Phase 0B template-head parser patch: Ollama boxed head compatibility, template-head parsing and local directive dispatch, removed four redundant adapters from already boxed helpers and kept plain handler directives adapted once. Parent consolidated the family behind one file-local alias and reran formatting checks, 287 create-template-node tests, 837 template tests, 10 parser error-recovery tests and `git diff --check`; worker all-target Clippy confirmed `head_parser.rs` is absent and `directive_args.rs` is the next connected owner.
- Phase 0B directive-argument patch: Ollama boxed the eight-function `directive_args.rs` parsing family behind one file-local alias, removed redundant adapters from already boxed core-directive callers and unboxed only at the handler and focused-test boundaries. Parent tightened the alias comment and adapter names, reran formatting checks, 287 create-template-node tests, 837 template tests, 10 parser error-recovery tests and `git diff --check`; all-target Clippy no longer reports `directive_args.rs` and first fails at `handler_directives.rs:42`.
- Phase 0B handler-directive patch: Ollama boxed the three-function `handler_directives.rs` parsing and normalization family behind one file-local alias, reused the boxed directive-argument result directly and removed the redundant adapter at the boxed head-parser dispatch. Parent tightened the alias comment and reran formatting checks, 287 create-template-node tests, 837 template tests, 10 parser error-recovery tests and `git diff --check`; worker all-target Clippy confirmed `handler_directives.rs` is absent and `head_expressions.rs:69` is next.
- Phase 0B head-expression patch: Ollama boxed the five-function `head_expressions.rs` validation, insertion, reactive-subscription and path-coercion family behind one file-local alias, reused the already boxed head-parser boundary and unboxed only at the still-plain reactive-subscription caller. Parent reran formatting checks, 837 template tests, 10 parser error-recovery tests and all-target Clippy; `head_expressions.rs` is absent and the first remaining owner is `reactive_subscriptions.rs:37`.
- Phase 0B reactive-subscription patch: Ollama boxed `parse_reactive_subscription` behind one file-local alias, reused the already boxed head-expression and head-parser boundaries directly and removed the obsolete unbox adapter. Parent reran formatting checks, 837 template tests and 10 parser error-recovery tests; worker all-target Clippy confirmed `reactive_subscriptions.rs` is absent and the next production owner is `tir/slot_composition/child_wrappers.rs:241`.
- Phase 0B child-wrapper patch: Ollama boxed all six production and test-only `child_wrappers.rs` boundaries behind one file-local alias, propagated the boxed diagnostic directly into `TemplateError` and unboxed only at the existing plain schema boundary. Parent reran formatting checks, 454 TIR tests and 837 template tests; worker all-target Clippy confirmed `child_wrappers.rs` is absent and the next production owner is `tir/slot_composition/contributions.rs:115`.
- Phase 0B slot-contribution patch: Ollama boxed the two-function `contributions.rs` routing family behind one file-local alias, preserved recursive insert routing and unboxed only at three existing plain composition boundaries. Parent improved adapter names and reran formatting checks, 454 TIR tests and 837 template tests; worker all-target Clippy confirmed `contributions.rs` is absent and the next production owner is `tir/slot_composition/head_chain.rs:93`.
- Phase 0B head-chain patch: Ollama boxed the complete ten-function `head_chain.rs` composition family behind one file-local alias and removed the temporary contribution unbox adapter. Parent propagated the boxed diagnostic directly into `TemplateError`, tightened the alias rationale and reran formatting checks, 454 TIR tests and 837 template tests; worker all-target Clippy confirmed `head_chain.rs` is absent and the next production owner is `tir/slot_composition/helpers.rs:131`.
- Phase 0B slot-helper patch: Ollama boxed the seven-function `helpers.rs` traversal and template-building family behind one file-local alias, kept diagnostic constructors plain and removed the redundant head-chain reboxing. Parent reran formatting checks, 454 TIR tests and 837 template tests; worker all-target Clippy confirmed `helpers.rs` is absent and the next production owner is `tir/slot_composition/overlays.rs:75`.
- Phase 0B slot-overlay patch: Ollama boxed the six-function `overlays.rs` materialization, composition, allocation and merge family behind one file-local alias and removed temporary unbox adapters from already boxed helper/contribution calls. Parent removed newly obsolete test dereferences and one leftover needless result adapter, then reran formatting checks, 454 TIR tests, 837 template tests and all-target Clippy; `overlays.rs` is absent and the next production owner is `tir/slot_composition/schema.rs:126`.
- Phase 0B slot-schema patch: Ollama boxed the twelve-function `schema.rs` discovery, placeholder collection/expansion, wrapper application and unresolved-slot family behind one file-local alias. Boxed diagnostics now propagate without reallocation into `TemplateError`, `TemplateSlotError` and already boxed composition boundaries. Parent corrected the boundary rationale and reran formatting checks, 454 TIR tests, 837 template tests, library Clippy and `git diff --check`; `schema.rs` is absent and the next production owner is `ast/mod.rs:380`.
- Phase 0B function-body boundary patch: Ollama boxed `function_body_to_ast`, removed redundant reboxing from already boxed statement/generic callers and unboxed only at the three AST-emitter accumulation boundaries. Parent reran formatting checks, 287 statement tests, 10 parser-recovery tests, library Clippy and `git diff --check`; `ast/mod.rs:380` is absent and the next production owner is `declaration_syntax/declaration_shell.rs:74`.
- Phase 0B declaration-shell patch: Ollama boxed the three-function declaration parsing, binding-target and marker-spacing family behind one file-local alias, removed redundant reboxing from multi-bind parsing and unboxed at the two remaining plain header/signature boundaries. Parent tightened the alias rationale and adapter names, then reran formatting checks, 54 declaration-syntax tests, 287 statement tests, 198 header tests, library Clippy and `git diff --check`; `declaration_shell.rs` is absent and the next production owner is `declaration_syntax/generic_parameters.rs:28`.
- Phase 0B generic-parameter patch: Ollama boxed the three-function generic list, trait-bound and trait-name validation family behind one file-local alias and unboxed once at the optional-header boundary. Parent tightened the alias rationale and adapter name, then reran formatting checks, 54 declaration-syntax tests, 198 header tests, library Clippy and `git diff --check`; `generic_parameters.rs` is absent and the next production owner is `declaration_syntax/record_body.rs:21`.
- Phase 0B record-body patch: Ollama boxed `parse_record_body` behind one file-local alias and unboxed once at each existing struct/choice boundary. Parent reran formatting checks, 141 declaration tests, 198 header tests and `git diff --check`; `record_body.rs` is absent and the next production owner is `declaration_syntax/signature_members.rs:152`.
- Phase 0B signature-member patch: an oversized Ollama attempt exhausted context without editing, so the parent split the family. The retry boxed the parameter/member half behind one file-local alias and adapted only the two plain callers. Parent tightened the alias and adapter names, then reran formatting checks, 54 declaration-syntax tests, 168 header tests and all-target Clippy; the assigned functions are absent and the remaining connected return-list family begins at `signature_members.rs:672`.
- Phase 0B signature-return patch: Ollama boxed the seven-function return-list, alias and validation family behind the existing signature result alias, removed the temporary in-file unbox and adapted the one plain trait-header caller. Parent reran formatting checks, 141 declaration tests, 198 header tests and all-target Clippy; `signature_members.rs` is absent and the first reported production owner is `declaration_syntax/struct.rs:33`.
- Phase 0B struct-shell patch: Ollama boxed the thin `parse_struct_shell` wrapper behind a file-local alias, propagated the already boxed record-body result directly and unboxed only at the plain header boundary. Parent tightened the adapter name and reran formatting checks, 54 declaration-syntax tests, 168 header tests and `git diff --check`; worker all-target Clippy confirmed `struct.rs` is absent and `type_syntax/parse.rs:27` is next.
- Phase 0B type-syntax front-half patch: Ollama boxed the eight-function annotation, atom, postfix, collection and capacity family behind one file-local alias, adapted the two still-plain back-half boundaries and one plain header caller. Parent generalized the alias rationale and reran formatting checks plus 349 focused tests; worker all-target Clippy moved the first connected owner to `type_syntax/parse.rs:528`.
- Phase 0B type-syntax back-half patch: Ollama boxed the seven-function map, type-slice, generic-argument, optional-suffix and `This` composition family behind the existing alias and removed all temporary split adapters. Parent kept bare diagnostic constructors plain, reran 53 type-syntax tests, 141 declaration tests, 198 header tests and all-target Clippy; `type_syntax/parse.rs` is absent and `headers/const_fragments.rs:25` is next.
- Phase 0B const-fragment patch: Ollama boxed top-level const-template header creation behind a file-local alias and unboxed only at the plain hash-item boundary. Parent tightened the alias rationale and adapter name, then reran 165 header tests and `git diff --check`; worker all-target Clippy confirmed `const_fragments.rs` is absent and `headers/facade_data.rs:77` is next.
- Phase 0B facade-build patch: Ollama boxed the five-function facade build/pass family behind one file-local alias, kept export-target helpers and `FacadeExportCollector::insert` plain with narrow adapters and unboxed once at the header diagnostic-bag boundary. Parent reran formatting checks, 165 header tests, `git diff --check` and all-target Clippy; the assigned build family is absent, 85 library owners remain and the coherent facade helper family at `facade_data.rs:372`, `:384`, `:404`, `:427` and `:572` is next.
- Phase 0B facade-helper patch: Ollama boxed the five remaining export-target and collector helpers behind the existing facade result alias, removed the temporary build-family adapters and kept three external import-environment boundaries explicit. Parent reran formatting checks, 165 header tests, `git diff --check` and all-target Clippy; `facade_data.rs` is absent and the four-function `headers/file_imports.rs` family is next.
- Phase 0B file-import patch: Ollama boxed the four-function import-clause parsing and recording family behind one file-local alias, kept const-path and normalization helpers plain with narrow adapters and unboxed at the three existing file-parser boundaries. Parent tightened the boundary rationale and adapter names, reran formatting checks, 165 header tests, `git diff --check` and all-target Clippy; `file_imports.rs` is absent and `file_parser.rs:51` is next.
- Phase 0B file-parser patch: Ollama boxed the seven-function file-level header orchestrator and helper family behind one file-local alias, reused the boxed import family directly and unboxed once at `HeaderFileParseState::into_error`. Parent tightened the boundary rationale and adapter name, reran formatting checks, 165 header tests, `git diff --check` and all-target Clippy; `file_parser.rs` is absent and `hash_items.rs:23` is next.
- Phase 0B hash-item patch: Ollama boxed both hash-item handlers behind one file-local alias, reused the boxed const-fragment and file-parser boundaries directly and preserved all hash syntax diagnostics and placement metadata. Parent reran formatting checks, 165 header tests, `git diff --check` and all-target Clippy; `hash_items.rs` is absent and `header_dispatch.rs:72` is next.
- Phase 0B header-dispatch patch: Ollama boxed the four-function declaration dispatch family behind one file-local alias, reused boxed declaration parsers directly and kept the internal diagnostic constructor plain. Parent tightened the boundary rationale, reran formatting checks, 165 header tests, `git diff --check` and all-target Clippy; `header_dispatch.rs` is absent and `import_environment/builder.rs:52` is next.
- Phase 0B import-builder patch: Ollama boxed the five-function visibility construction and grouped/bare resolution family behind one file-local alias, leaving narrow adapters at adjacent plain import registration boundaries. Parent tightened the rationale and adapter names, reran formatting checks, 165 header tests, `git diff --check` and all-target Clippy; `builder.rs` is absent and `external_imports.rs:20` is next.
- Phase 0B external-import patch: Ollama boxed external symbol registration, reused the boxed local-name derivation and builder callers directly and left one temporary provider adapter. Parent tightened the boundary rationale, reran formatting checks, 165 header tests, `git diff --check` and all-target Clippy; `external_imports.rs` is absent and `facade_resolution.rs` is next.
- Phase 0B facade-boundary patch: Ollama boxed source-library and module privacy checks and removed temporary adapters from the already boxed facade-data caller. Parent corrected the boundary rationale, reran formatting checks, 165 header tests, `git diff --check` and all-target Clippy; `facade_resolution.rs` is absent and `namespace_imports.rs` is next.
- Phase 0B namespace-import patch: Ollama boxed the nine-function namespace registration, recursive external record and privacy-check family, removed the builder adapter and left two provider adapters. Parent corrected the boundary rationale, reran formatting checks, 165 header tests, `git diff --check` and all-target Clippy; `namespace_imports.rs` is absent and `provider_imports.rs` is next.
- Phase 0B provider-import patch: Ollama boxed both provider-backed grouped and bare resolution functions, removed temporary external/namespace adapters and kept the visible-name registry boundary narrow. Parent reran formatting checks, 165 header tests, `git diff --check` and all-target Clippy; `provider_imports.rs` is absent and `source_imports.rs` is next.
- Phase 0B source-import patch: Ollama boxed source import registration behind one file-local alias, reused boxed local-name derivation and builder callers directly and kept `VisibleNameRegistry::register` plain with one narrow adapter. Parent removed an empty worker scratch file, reran formatting checks, 165 header tests and `git diff --check`; all-target Clippy no longer reports `source_imports.rs` and first fails at `import_environment/target_resolution.rs:117`.
- Phase 0B import-target patch: Ollama boxed `resolve_import_target`, removed the redundant facade caller adapter and reused the builder's existing boxed boundary directly. Parent reran formatting checks, 165 header tests and `git diff --check`; all-target Clippy no longer reports `target_resolution.rs` and first fails at `import_environment/visible_names.rs:111`.
- Phase 0B visible-name patch: Ollama boxed the collision registry boundary and removed redundant adapters from source, external, provider and namespace import registration. Parent replaced the lint-shaped comment with ownership rationale, reran formatting checks, 165 header tests and `git diff --check`; all-target Clippy no longer reports `visible_names.rs` and first fails at `headers/imports.rs:17`.
- Phase 0B import-normalization patch: Ollama boxed `normalize_import_dependency_path` and removed two redundant file-import adapters while keeping the still-plain clause-parser adapters explicit. Parent reran formatting checks, 165 header tests and `git diff --check`; all-target Clippy no longer reports `headers/imports.rs` and first fails at `headers/start_capture.rs:16`.
- Phase 0B start-capture patch: Ollama boxed the implicit-start runtime-template capture boundary while preserving balanced scanning and EOF diagnostics. Parent reran formatting checks, 165 header tests and `git diff --check`; all-target Clippy no longer reports `start_capture.rs` and first fails at `headers/trait_headers.rs:34`.
- Phase 0B trait-header patch: Ollama boxed the connected declaration, requirement, conformance, target, incompatibility and name-validation family and reused boxed signature/dispatch boundaries directly. Parent replaced implicit `.into()` conversions with explicit boundary boxing, reran formatting checks, 165 header tests and `git diff --check`; all-target Clippy no longer reports `trait_headers.rs` and first fails at `module_dependencies.rs:423`.
- Phase 0B dependency-visit patch: Ollama boxed recursive node/edge visitation behind one file-local result alias and unboxed once at the existing `DiagnosticBag` boundary. Parent replaced lint-shaped commentary with ownership rationale, reran 15 dependency tests and `git diff --check`; all-target Clippy no longer reports `module_dependencies.rs` and first fails at `tokenizer/lexer.rs:101`.
- Phase 0B lexer patch: Ollama boxed the connected spacing, dispatch, directive, identifier and public tokenize family, adapted still-plain numeric/text/path helpers narrowly and threaded the boxed boundary through production and focused test callers. Parent replaced lint-shaped commentary with lexer ownership rationale, reran 72 tokenizer, 165 header and 181 build-system tests plus `git diff --check`; all-target Clippy no longer reports `lexer.rs` or `pipeline.rs` and first fails at `tokenizer/numeric.rs:27`.
- Phase 0B numeric-token patch: Ollama boxed numeric literal tokenization and removed both temporary lexer adapters while preserving authored and normalized payloads. Parent replaced lint-shaped commentary with numeric/lexer boundary rationale, reran 72 tokenizer and 27 numeric-text tests plus `git diff --check`; all-target Clippy no longer reports `numeric.rs` and first fails at `tokenizer/text_modes.rs:20`.
- Phase 0B text-mode patch: Ollama boxed the connected raw, quoted and template-body mode family and removed five lexer adapters while preserving delimiter state and newline/bracket accounting. Parent replaced lint-shaped commentary with text-mode/lexer ownership rationale, reran 72 tokenizer tests and `git diff --check`; all-target Clippy no longer reports `text_modes.rs` and first fails at `builtins/casts/resolution.rs:63`.
- Phase 0B cast-resolution patch: Ollama boxed explicit cast resolution behind one file-local alias and reused the expression parser's boxed diagnostic family directly. Parent replaced lint-shaped commentary with ownership rationale, reran formatting checks, 46 cast tests, 102 expression tests and `git diff --check`; all-target Clippy no longer reports `builtins/casts/resolution.rs` and first fails at `symbols/identifier_policy.rs:159`.
- Phase 0B identifier-policy patch: Ollama boxed the shared reserved-keyword check, removed redundant adapters at boxed statement boundaries and unboxed only at plain declaration/diagnostic-bag boundaries. Parent replaced lint-shaped commentary with ownership rationale, reran formatting checks, 16 symbol tests, 165 header tests, 54 declaration-syntax tests, 287 statement tests, 102 expression tests and `git diff --check`; all-target Clippy no longer reports `identifier_policy.rs` and first fails at `datatypes/generic_parameters.rs:142`.
- Phase 0B generic-scope patch: Ollama boxed declaration-local generic-parameter scope validation behind one datatype-owned alias, removed the redundant AST type-resolution adapter and reused the declaration parser's boxed boundary directly. Parent reran formatting checks, 114 datatype tests, 54 declaration-syntax tests, 20 AST type-resolution tests and `git diff --check`; all-target Clippy no longer reports `datatypes/generic_parameters.rs` and first fails at `type_coercion/contextual.rs:34`.
- Phase 0B contextual-coercion patch: Ollama boxed the explicit typed-boundary coercion failure behind one file-local alias while all statement/expression callers consumed the boxed boundary directly. Parent reran formatting checks, 36 coercion tests, 287 statement tests, 102 expression tests and `git diff --check`; all-target Clippy no longer reports `type_coercion/contextual.rs` and first fails at `utilities/token_scan.rs:195`.
- Phase 0B token-scan patch: Ollama boxed declaration-initializer scanning's single EOF diagnostic in the same result family as its declaration-shell owner. Parent replaced lint-shaped commentary with ownership rationale, reran formatting checks, 10 utility tests, 54 declaration-syntax tests, 165 header tests and `git diff --check`; all-target Clippy no longer reports `utilities/token_scan.rs` and first fails at `paths/const_paths/components.rs:31`.
- Phase 0B const-path component patch: Ollama boxed the connected component, grouped-entry and ordinary-prefix parse/validation family, unboxing only at the public path boundary. Parent removed lint-shaped ownership comments, reran formatting checks, 61 const-path tests, 165 header tests, 54 declaration-syntax tests and `git diff --check`; all-target Clippy no longer reports `components.rs` or `grouped.rs` and first fails at `paths/const_paths/import_clauses.rs:22`, with the public `mod.rs:75` boundary also remaining.
- Phase 0B final Ollama const-path patch: Ollama boxed the import-clause/path-collection family and public path parser, removed redundant lexer/header adapters and adapted once at the Stage 0 discovery boundary. Parent replaced performance-shaped commentary with ownership rationale, reran formatting checks, 112 path tests, 72 tokenizer tests, 165 header tests, 133 module-creation tests, library Clippy with warnings denied and `git diff --check`; library Clippy is now clean and all-target Clippy first fails only in test helpers at `ast/statements/match_patterns/tests/literal_tests.rs:65` and `ast/templates/tests/create_template_node/directive_style_tests.rs:69`.
- Phase 0B test-helper patch: Codex CLI boxed the two remaining test-local diagnostic result families without changing production APIs. Parent reran 4 literal tests, 50 directive tests, formatting, all-target Clippy and `git diff --check`; all passed. `just validate` exited 0 after cross-target Clippy, 3308 unit tests, 1730 integration tests and bench-check 28/28, but the docs check emitted `BST-IMPORT-0011` while the separately owned codebase-docs refactor was mid-sync.
- Phase 0 baseline attempt: Codex CLI confirmed branch `codex/hash-root-export-block-module-system`, plan linkage and `src/compiler_frontend/paths/module_roots.rs` ownership. `just bench-frontend-check` could not capture timings because the synced user-owned docs refactor emits `BST-RULE-0034` for unresolved `codeblock`, `th` and `table` values. The remaining thread-count runs and `bench-report` are pending that external fix.
- Phase 0 baseline rerun after the completed docs and HTML `th` dependency sync: default frontend average ~50 ms; 1 thread ~125 ms; 2 threads ~80 ms; 4 threads ~58 ms. `just bench-check` passed 28/28 at ~16 ms average, `just bench-report` found no local history and full `just validate` passed with 3308 unit tests, 1730 integration tests and clean docs.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed after the final test-helper cleanup.
- Squash integration into `templates-refactor`: `cargo check --all-targets --message-format short`, all-target Clippy and full `just validate` passed before checkpoint commit `3690c3f57`.
- Phase 1 hard config switch: Codex CLI focused tests passed (135 Stage 0, 112 paths); parent `cargo run --quiet -- tests` passed 1735/1735 after correcting grouped config-import classification; full `just validate` passed cross-target Clippy, 3306 unit tests, 1735 integration cases, clean docs and bench-check 28/28.
- Phase 2 source-tree index: Codex CLI implemented the first pass. Parent removed transitional duplicate-root rejection, restored same-directory multi-entry coverage, converted hidden-scan test harnesses to prepared roots, removed stale counter names and audited all old discovery symbols. Focused Stage 0 (138), path (112) and Beandown (37) tests passed.
- Phase 2 performance: the post-slice frontend matrix was approximately 52 ms default, 122 ms with one thread, 82 ms with two and 63 ms with four versus the Phase 0 matrix of 50/125/80/58 ms. `just bench-check` passed 28/28 at -2 ms average. A 20-run direct comparison on `module-root-stress` measured the new single discovery at 0.312 ms versus two old timed scans averaging 0.224 ms each, plus the removed untimed entry walk. `BST_COUNTERS=summary` reported `source scans 1` for the directory build.
- Phase 2 final gate: `just validate` passed cross-target Clippy, 3309 unit tests, 1735 integration cases, clean docs and bench-check 28/28.
- Phase 3 generic import identity: Codex CLI added the generic root/config classifier, arbitrary hash-root direct-import rejection and exact dynamic diagnostic rendering. Parent moved the remaining temporary `#mod.bst` helpers into the same owner with no forwarding shim. Focused root-file (4), header (165) and Stage 0 (138) tests passed. Full `just validate` passed cross-target Clippy, 3314 unit tests, 1736 integration cases, clean docs and bench-check 28/28 at -2 ms average.
- Phase 3 generic source-library roots: Codex CLI added deterministic direct-child `#*.bst` discovery, threaded unique cosmetic roots through the existing temporary facade map and added structured missing/multiple-root diagnostics with sorted candidate paths. Parent reviewed stage ownership and temporary-map duplication, updated the progress matrix and ran full `just validate`: cross-target Clippy, 3319 unit tests, 1736 integration cases, clean docs and bench-check 28/28 at -2 ms average.
- Phase 3 entry-root export identity: Codex CLI renamed prepared entry-root facade maps to export-file identity and added `ModuleRootBoundary` so namespace resolution consumes the Stage 0-selected export path. Parent corrected the worker's dropped no-export-root boundary regression by making the carried export file optional, preserving missing-facade enforcement. Full `just validate` passed cross-target Clippy, 3320 unit tests, 1736 integration cases, clean docs and bench-check 28/28 at -2 ms average.
- Phase 4 root-role cutover: Codex CLI replaced entry/facade roles with active/imported/normal module-root roles, suppressed imported-root start output, preserved public surfaces and added header activity facts. The parent replaced a linear root-file identity scan with a prepared hash index, removed the obsolete duplicate-root counter and added end-to-end duplicate-root diagnostics coverage. `cargo fmt --check`, 3321 unit tests, 113 path tests, 140 Stage 0 tests and 1738 integration cases passed. `just validate` reached the docs check after code gates passed, then failed because concurrent user-owned `docs/src/**` edits already use Phase 5 `export:` syntax. This docs-only block is accepted temporarily by explicit user direction.
- Phase 5A strict export parser: Codex CLI added one file-level `export:` parser mode, rejected legacy inline forms, required grouped imports, rejected invalid runtime/evidence/receiver items and preserved public header/import carriers without adding a scope frame. The parent aligned module-root and receiver import/export diagnostic names. Empty `export:` is a structured error because an empty public API section is almost certainly accidental. `cargo fmt --check`, 174 header tests, 51 compiler-message tests and library Clippy with warnings denied passed. Full integration and validation are intentionally deferred to Phase 5B after production sources and fixtures migrate.
- Phase 5B fixture migration: Codex CLI migrated remaining integration and test-only sources to one strict `export:` block, added 11 stable-code export-block integration cases and passed `cargo fmt --check`, 1749 integration cases, all-target Clippy and full `just validate`. Parent renamed the stale page-root success case and reran formatting plus all 1749 integration cases. The grep audit found obsolete legacy-export recognition only in `paths/const_paths/import_clauses.rs`, which is isolated as Phase 5C before Phase 5 closes.
- Phase 5C legacy scanner removal: Codex CLI deleted the dedicated `export import` / `export @path` path collector and its sugar helper so strict export-block imports flow through the ordinary import scanner. Focused path coverage now proves strict block discovery and deliberate legacy-prefix skipping. Worker full `just validate` passed 3329 unit tests, 1749 integration cases and 28/28 benchmark checks. Parent reran formatting, 114 path tests and 140 Stage 0 module-creation tests.
- Phase 6A public export data ownership: Codex CLI replaced `facade_data.rs` with `public_exports.rs`, renamed the entry/target types and source-library/module-root maps and threaded the current names through header imports, dependency sorting and required AST references without compatibility aliases. Worker full `just validate` passed 3329 unit tests, 1749 integration cases and 28/28 benchmark checks. Parent reran formatting, 174 header tests, 14 dependency tests and 8 AST environment tests.

DOCS_IMPACT:
- progress matrix: updated for generic Stage 0 source-library discovery and the remaining temporary `#mod.bst` file-role limit
- other docs stale: `docs/language-overview.md`, `docs/compiler-design-overview.md`, docs-site project-structure/libraries/getting-started pages, `README.md` examples, scaffold docs and benchmark docs if metric names change
- authorized docs updates: yes, update docs in the same phase that changes behavior; do not leave source semantics undocumented

NEXT_ACTION:
- Commit Phase 6A, then delegate Phase 6B AST public-surface and module-root diagnostic naming through the verified `codex-cli-beanstalk` wrapper.
DELEGATION_DECISION: codex-cli - explicit user override for every implementation and audit slice; the reviewed wrapper now resolves through the repo-tracked script
NEXT_WORKER_ORDER: codex-cli only for this run
STOP_REASON: none
NEXT_RESUME_ACTION: inspect Phase 6B diagnostic and AST owners, then create its bounded codex-cli task

---

## Current repo snapshot at plan creation

Observed branch state:
- Repository: `nyejames/beanstalk`
- Branch: `templates-refactor`
- Observed head: `e23773082cabfc94bd39c6e4e15167e42dc9922f`
- Compared to `main`: ahead by 64 commits, behind by 0 at plan creation.

Current architecture facts that this plan depends on:
- `ProjectPathResolver::new_with_module_root_policy` currently performs eager module-root discovery when `ModuleRootDiscoveryPolicy::Full` is selected.
- At plan creation, `discover_module_roots` walked the entry root, excluded the old config filename from `#*.bst` module roots and stored `#mod.bst` as the facade when present.
- At plan creation, `discover_root_entry_files` performed a separate BFS for `#*.bst` entry files, excluding the old config filename and `#mod.bst`.
- `FileRole` currently separates `Entry`, `ModuleFacade`, and `Normal`; `#mod.bst` is the only facade-capable filename.
- `export` is already a tokenizer keyword and `HeaderExportMode::{Private, Public}` already exists.
- `facade_data.rs` already builds export maps from public authored headers and public import records.
- `build.rs::Module` already carries const top-level fragments and runtime fragment count, but not an explicit root activity summary.
- The HTML builder currently iterates every compiled module and emits HTML artifacts for each module.
- Benchmarks already include Stage 0/path-resolution/frontend timings and parallelism/module-root stress fixtures.
- `src/compiler_frontend/source_libraries/mod.rs`, not the old planned `mod_file.rs`, currently owns the hard-coded `#mod.bst`, `#page.bst` and canonical config helper constants.
- `src/build_system/create_project_modules/facade_validation.rs` still preflights source libraries by requiring `#mod.bst`.
- `src/projects/html_project/new_html_project/scaffold.rs`, dev-server watch/config tests, HTML output routing and docs-site getting-started/project-structure pages used the old config filename and `src/#page.bst` at plan creation. Phase 1 has corrected the config side.

Agents must refresh this section after each accepted slice and before compaction if the branch moves.

---

## Target design

A module directory is represented by exactly one non-config hash-prefixed root file:

```text
src/
  #home.bst
```

The filename after `#` is cosmetic. These are equivalent module root files:

```text
#page.bst
#mod.bst
#home.bst
#whatever.bst
```

The module directory path is the module identity. The root file may define both the module public API and builder-consumable top-level root/start content.

```beanstalk
import @core/text {length as private_length}

private_prefix #= "internal"

export:
    import @components/card {
        CardData as Card,
        render_card,
    }

    PageData = |
        title String,
    |

    title_length |title String| -> Int:
        return private_length(title)
    ;
;

[: top-level page/root output]
```

### Strict language rules

- `config.bst` is the project config file.
- No other filename has config meaning, compatibility handling or a config-specific diagnostic.
- Any non-config `#*.bst` file marks its containing directory as a module root.
- A directory may contain at most one non-config `#*.bst` file.
- The hash filename is never part of import identity.
- Direct imports of hash files are invalid.
- Direct imports of `config.bst` or `config` are invalid; project config is not normal source.
- Importing a module never runs the imported module root's top-level root/start body.
- A module with no `export:` block exports nothing.
- A module root with no builder-relevant top-level root/start activity is API-only for the HTML builder and does not emit a page artifact.

### `export:` block rules

- `export:` is valid only in a module root file.
- A module root file may contain at most one `export:` block.
- The block is not a lexical scope.
- Items inside are ordinary top-level module-root items marked public.
- Items outside remain private to the module root file.
- The block ends with normal Beanstalk `;`.

Allowed inside `export:`:
- grouped source imports;
- grouped external imports;
- functions;
- structs;
- choices;
- type aliases;
- traits;
- compile-time constants using `#`.

Rejected inside `export:`:
- runtime bindings;
- mutable bindings;
- top-level executable statements;
- runtime templates;
- top-level const page fragments `#[...]`;
- trait conformances;
- trait incompatibility declarations;
- receiver methods as directly exported symbols;
- bare namespace imports/exports;
- wildcard exports;
- export lists;
- function alias exports;
- nested `export:` blocks;
- all legacy inline export forms.

Trait declarations may be exported. Trait conformances stay outside `export:` because they are evidence, not public symbols.

---

## Performance requirements

This refactor must improve module-root discovery or it is incomplete.

Strict requirements:
- There must be exactly one expensive project tree scan for module root discovery in a directory build. The project-root config probe happens first and stays a bounded file check because config determines the scan roots and skip policy.
- `discover_module_roots` and `discover_root_entry_files` must not remain as two independent entry-root BFS scans.
- The new source-tree index must provide both module-root data and root module entry candidates.
- Discovery must skip known irrelevant directories by policy.
- Nearest module-root lookup must use parent-walk/hash lookup or equivalent `O(path depth)` lookup, not a linear scan over every module root.
- Ordinary file paths must not be canonicalized during tree discovery unless they become indexed roots/configs or are needed for diagnostics.
- Benchmark and profiling evidence must be recorded before and after the discovery refactor.
- If benchmarks show no Stage 0/source-tree discovery improvement, the phase is not accepted until profiling explains why and the plan is updated.
- If total compile time regresses outside expected noise, stop and profile before continuing.

Minimum benchmark protocol:

```bash
just validate
just bench-frontend-check
RAYON_NUM_THREADS=1 just bench-frontend-check
RAYON_NUM_THREADS=2 just bench-frontend-check
RAYON_NUM_THREADS=4 just bench-frontend-check
just bench-check
just bench-report
```

Use targeted profiling only after `just bench-report` identifies a relevant case/stage:

```bash
just profile-case module-root-stress terse
just profile-case many-modules-one-file-each terse
```

Do not commit raw local benchmark history, raw profiles, or expanded counter tables. Record concise before/after conclusions in `benchmarks/frontend-optimization-results.md` only when a phase intentionally changes performance.

---

## Complexity reduction opportunities to enforce

Use this refactor to remove old indirection rather than preserving it under new names.

Required simplifications:
- Replace hard-coded special file identity in `src/compiler_frontend/source_libraries/mod.rs` or a new focused root-file helper owner with generic root/config helpers.
- Remove separate entry discovery once the source tree index owns root discovery.
- Rename facade-oriented maps/types where they now represent generic module root public exports.
- Remove legacy inline `export` parser branches rather than keeping compatibility paths.
- Replace boolean-heavy discovery/config flags with named enums/structs where behavior differs.
- Keep Stage 0 discovery as one owner; do not let `ProjectPathResolver` perform hidden filesystem scans during construction.
- Do not add a second artifact discovery pass. Builder artifact relevance must come from metadata already produced by header/frontend work.
- Prefer one `ModuleRootRecord` / `ModuleRootTable` model over parallel `module_roots`, `module_root_facades`, and entry file lists.
- Keep path resolver focused on resolution and boundary lookup, not source tree scanning.

Preferred new structure:

```rust
pub(crate) struct SourceTreeIndex {
    pub(crate) entry_root: PathBuf,
    pub(crate) module_roots: ModuleRootTable,
    pub(crate) stats: SourceTreeDiscoveryStats,
}

pub(crate) struct ModuleRootRecord {
    pub(crate) root_directory: PathBuf,
    pub(crate) root_file: PathBuf,
}

pub(crate) struct ModuleRootTable {
    records: Vec<ModuleRootRecord>,
    by_directory: HashMap<PathBuf, ModuleRootId>,
    by_root_file: HashMap<PathBuf, ModuleRootId>,
}
```

Keep names exact and descriptive. Avoid aliases unless they materially reduce noise.

---

# Phased implementation checklist

## Phase 0A — fix Stage 0 source-discovery error boundary

### Context

Phase 0 benchmark capture is blocked because `just validate` fails before any module/export behavior changes. The failure is a clippy `result_large_err` lint in Stage 0 source discovery, first reported at `src/build_system/create_project_modules/import_scanning.rs:37`.

This phase is a validation-baseline cleanup only. It must not implement the hash-root/config/export behavior changes.

### Checklist

- [x] Fix the Stage 0 discovery error boundary so `Result<_, SourceDiscoveryError>` no longer trips `clippy::result_large_err`.
- [x] Keep user-facing diagnostics on `CompilerDiagnostic` and filesystem/tooling failures on `CompilerError`.
- [x] Prefer boxing the large diagnostic payload inside the local error enum over weakening the diagnostic boundary.
- [x] Update conversions in `src/build_system/create_project_modules/source_discovery_error.rs` and only the call sites/tests required by the enum shape.
- [x] Do not add lint suppressions unless the root-cause fix is impossible; if impossible, stop and document why.

### Review / audit / validation

- [x] Run `cargo fmt`.
- [x] Run `cargo test --lib build_system::create_project_modules --quiet`.
- [x] Run `cargo clippy --lib --quiet`.
- [ ] Run `just validate` if focused clippy passes.
  - Blocked: broader `result_large_err` warnings outside this Stage 0 boundary fail all-target clippy with `-D warnings`.
- [x] Confirm no source-language behavior, diagnostics or module/export semantics changed.
- [x] Update the current-state block with the validation result before Phase 0 benchmark capture resumes.

---

## Phase 0B — clean broader `result_large_err` validation baseline

### Context

Phase 0A fixed the initially reported Stage 0 source-discovery boundary, but `just validate` still fails before benchmark capture because current Clippy reports broader `result_large_err` warnings across local diagnostic result boundaries.

This is still validation-baseline work only. It must not implement the hash-root/config/export behavior changes.

### Checklist

- [x] Capture the current `just validate` / all-target clippy blocker list in a compact owner map.
- [x] Fix local error enums that carry `CompilerDiagnostic` by boxing only the diagnostic payload where that preserves boundary ownership.
  - `ExtractConfigValueError::Diagnostic` in `src/build_system/project_config/validation.rs` is boxed.
- [x] For direct `Result<_, CompilerDiagnostic>` helper families, prefer stage-local boxed diagnostic aliases only when they keep the owner readable and do not leak into `DiagnosticBag` or `CompilerMessages`.
  - Owner-local boxed aliases now unbox only at plain diagnostic accumulation or render boundaries.
- [x] Keep `DiagnosticBag` and `CompilerMessages` owning plain `CompilerDiagnostic` values at accumulation and render/build boundaries.
- [x] Do not globally box `CompilerDiagnostic`.
- [x] Do not add lint suppressions unless a specific boundary cannot be corrected without making code worse; if so, stop and document the exact reason.
- [x] Split cleanup into owner-bounded worker slices rather than one repository-wide churn pass.
- [x] First likely owners:
  - [x] `src/build_system/project_config/validation.rs`
  - [x] `src/compiler_frontend/ast/generic_bounds.rs`
  - [x] `src/compiler_frontend/ast/generic_functions/body_rules.rs`
  - [x] `src/compiler_frontend/ast/module_ast/environment/type_resolution.rs`
  - [x] `src/compiler_frontend/ast/module_ast/finalization/normalize_ast.rs`
  - [x] `src/compiler_frontend/ast/receiver_methods.rs`
  - [x] `src/compiler_frontend/ast/module_ast/environment/function_signatures.rs`
  - [x] `src/compiler_frontend/ast/type_resolution/collections.rs`
    - Parent explicitly unboxed the affected shorthand declaration caller in `src/compiler_frontend/ast/statements/declarations.rs`.
  - [x] `src/compiler_frontend/ast/expressions/namespace_access/leaf_resolution.rs`
  - [x] `src/compiler_frontend/ast/statements/asserts.rs`
  - [x] `src/compiler_frontend/ast/statements/body_dispatch.rs`
  - [x] `src/compiler_frontend/ast/statements/body_expr_stmt.rs`
  - [x] `src/compiler_frontend/ast/statements/body_return.rs`
  - [x] `src/compiler_frontend/ast/statements/body_symbol.rs`
  - [x] `src/compiler_frontend/ast/statements/branching.rs`
  - [x] `src/compiler_frontend/ast/statements/collections.rs`
  - [x] `src/compiler_frontend/ast/statements/condition_validation.rs`
  - [x] `src/compiler_frontend/ast/statements/declarations.rs`
  - [x] `src/compiler_frontend/ast/statements/functions.rs`
  - [x] `src/compiler_frontend/ast/statements/if_headers.rs`
  - [x] `src/compiler_frontend/ast/statements/loop_headers.rs`
  - [x] `src/compiler_frontend/ast/statements/loops.rs`
  - [x] `src/compiler_frontend/ast/statements/match_exhaustiveness.rs`
  - [x] `src/compiler_frontend/ast/statements/match_headers.rs`
  - [x] `src/compiler_frontend/ast/statements/match_patterns/choice.rs`
  - [x] `src/compiler_frontend/ast/statements/match_patterns/diagnostics.rs`
  - [x] `src/compiler_frontend/ast/statements/match_patterns/literal.rs`
  - [x] `src/compiler_frontend/ast/statements/match_patterns/option.rs`
  - [x] `src/compiler_frontend/ast/statements/match_patterns/relational.rs`
  - [x] `src/compiler_frontend/ast/statements/multi_bind.rs`
  - [x] `src/compiler_frontend/ast/statements/scoped_blocks.rs`
  - [x] `src/compiler_frontend/ast/statements/value_production/multi_bind.rs`
  - [x] `src/compiler_frontend/ast/statements/value_production/receiver/block_if.rs`
  - [x] `src/compiler_frontend/ast/statements/value_production/receiver/full_match.rs`
  - [x] `src/compiler_frontend/ast/statements/value_production/receiver/inline_if.rs`
  - [x] `src/compiler_frontend/ast/statements/value_production/receiver/inline_match.rs`
  - [x] `src/compiler_frontend/ast/statements/value_production/receiver/inline_then_else.rs`
  - [x] `src/compiler_frontend/ast/statements/value_production/receiver/result_type.rs`
  - [x] `src/compiler_frontend/ast/statements/value_production/receiver/mod.rs`
  - [x] `src/compiler_frontend/ast/field_access/receiver_access.rs`
  - [x] `src/compiler_frontend/ast/templates/create_template_node.rs`
  - [x] `src/compiler_frontend/ast/templates/template_body_parser.rs`
  - [x] `src/compiler_frontend/ast/templates/template_body_sentinels.rs`
  - [x] `src/compiler_frontend/ast/templates/template_control_flow/validation.rs`
  - [x] `src/compiler_frontend/traits/evidence/validation.rs`
  - [x] `src/compiler_frontend/traits/evidence/requirement_matching.rs`
  - [x] `src/compiler_frontend/paths/import_resolution.rs`
  - [x] `src/compiler_frontend/paths/compile_time_paths.rs`
  - [x] direct `Result<_, CompilerDiagnostic>` helper families surfaced by all-target clippy.

### Review / audit / validation

- [x] Run `cargo fmt`.
- [x] Run focused tests for each touched owner.
- [x] Run `cargo clippy --all-targets --all-features -- -D warnings`.
- [x] Run `just validate` before Phase 0 benchmark capture.
- [x] Confirm no source-language behavior, diagnostics or module/export semantics changed.
- [x] Update the current-state block with the validation result before Phase 0 benchmark capture resumes.

---

## Phase 0 — refresh plan context and record baseline

### Context

This phase refreshes the reloadable plan artifact and captures the current behavior/performance baseline. No compiler behavior should change.

### Checklist

- [x] Run `git status --short` and update `WORKTREE` in the current-state block.
- [x] Confirm branch and commit:
  - [x] `git branch --show-current`
  - [x] `git rev-parse HEAD`
- [x] Read/refresh:
  - [x] `AGENTS.md` and its mandatory worktree-local references
  - [x] project-structure, build/frontend-boundary and parallelism compiler references selected by the task-reading matrix
  - [x] `docs/language-overview.md`
  - [x] `benchmarks/README.md`
- [x] Confirm this plan is still linked from `docs/roadmap/roadmap.md` after TIR and remains ordered after the TIR plan.
- [x] Confirm current code references in this plan exist; correct stale paths before launching implementation workers.
  - Plan review on 2026-07-09 found the listed owners still exist. Add `src/compiler_frontend/headers/import_environment/namespace_imports.rs`, `src/compiler_frontend/headers/import_environment/builder.rs`, `src/compiler_frontend/headers/facade_data.rs`, `src/compiler_frontend/headers/symbol_collection.rs`, `src/compiler_frontend/headers/hash_items.rs`, and `src/compiler_frontend/ast/module_ast/environment/public_surface.rs` as explicit migration owners for later phases.
- [x] Run baseline validation:
  - [x] `just validate`
- [x] Run baseline benchmark/profiling protocol:
  - [x] `just bench-frontend-check`
  - [x] `RAYON_NUM_THREADS=1 just bench-frontend-check`
  - [x] `RAYON_NUM_THREADS=2 just bench-frontend-check`
  - [x] `RAYON_NUM_THREADS=4 just bench-frontend-check`
  - [x] `just bench-check`
  - [x] `just bench-report`
- [x] Record concise baseline conclusions in this plan or `benchmarks/frontend-optimization-results.md` if the repo convention requires it.
- [x] Do not commit raw local benchmark/profiling artifacts.

### Review / audit / validation

- [x] Confirm no behavior/source changes except plan review updates.
- [x] Confirm active context capsule is updated.
- [x] Confirm baseline command results are recorded with known unrelated failures, if any.

---

## Phase 1 — hard-switch project config to `config.bst`

### Context

Config is not a module root, does not produce HIR, and does not export language-visible declarations. Moving it to `config.bst` keeps the hash-root rule pure before generic hash root behavior lands.

### Checklist

- [x] Replace config filename constants:
  - [x] add `config.bst` as the canonical config name;
  - [x] remove every alternate config filename constant and compatibility path.
- [x] Update config loading in:
  - [x] `src/projects/settings.rs`
  - [x] `src/build_system/project_config.rs`
  - [x] `src/build_system/project_config/parsing.rs`
  - [x] `src/build_system/project_config/validation.rs`
  - [x] any project-root/config discovery call sites.
- [x] Update config-adjacent project tooling:
  - [x] dev-server watch setup and tests;
  - [x] `src/projects/routing.rs`;
  - [x] `src/projects/html_project/document_config.rs`;
  - [x] `src/projects/html_project/new_html_project/scaffold.rs` and scaffold tests.
- [x] Remove alternate-filename config probing, migration diagnostics and config-specific import recognition.
- [x] Reject direct imports of canonical `config.bst` / `config` through the config-is-not-source policy.
- [x] Update tests and fixtures:
  - [x] rename every project config fixture to `config.bst`;
  - [x] add negative test for direct `config.bst` import;
  - [x] update all benchmark config fixtures.
- [x] Update docs:
  - [x] `docs/language-overview.md`
  - [x] `docs/compiler-design-overview.md`
  - [x] docs-site project-structure page
  - [x] docs-site getting-started page
  - [x] README audit found no alternate config filename examples
  - [x] progress matrix.

### Review / audit / validation

- [x] Run `cargo test project_config` or closest focused test command.
- [x] Run `cargo run -- tests` for integration coverage touching config.
- [x] Run `just validate`.
- [x] Manual stage-boundary review:
  - [x] config mistakes use `CompilerDiagnostic`;
  - [x] filesystem/tooling failures use `CompilerError`;
  - [x] canonical config syntax is not treated as module source syntax.
- [x] Update active context capsule.

---

## Phase 2 — add a single source tree index for module roots

### Context

This is the primary performance phase. It must remove duplicate expensive directory walks and create one source of truth for module-root discovery, root entry candidates, duplicate hash-file diagnostics, and skip-policy evidence. The project config has already been loaded by this point.

Course correction: Phase 2 records duplicate hash-root evidence but defers rejection until Phase 3 removes the current `#page.bst`/`#404.bst`/`#mod.bst` role split. Rejecting duplicates here would break the live docs and route model before its replacement exists. Phase 3 owns the structured diagnostic and integration failure fixture alongside the root-role transition.

### Checklist

- [x] Add `src/build_system/create_project_modules/source_tree_index.rs` as the Stage 0 source-tree owner.
- [x] Define clear data types:
  - [x] `SourceTreeIndex`
  - [x] `ModuleRootTable`
  - [x] `ModuleRootRecord`
  - [x] `ModuleRootId`
  - [x] `SourceTreeDiscoveryStats`
  - [x] `SourceTreeSkipPolicy`
- [x] The index scan collects:
  - [x] canonical entry root;
  - [x] all non-config `#*.bst` module root files;
  - [x] module root directory for each root file;
  - [x] duplicate non-config hash files per directory as transition evidence;
  - [x] configured `dev_folder` and `output_folder` skip decisions;
  - [x] skipped directory counts.
- [x] Add skip policy:
  - [x] `.git`
  - [x] `target`
  - [x] `node_modules`
  - [x] `release`
  - [x] `dev`
  - [x] `dist`
  - [x] `build`
  - [x] `.cache`
  - [x] configured `dev_folder`
  - [x] configured `output_folder`
- [x] Keep registered source-library traversal in its existing explicit setup, outside the entry-root skip policy.
- [x] Canonicalize only:
  - [x] entry root;
  - [x] no config path, because config is already loaded and is not rediscovered by the index;
  - [x] module root directories;
  - [x] module root files;
  - [x] source-library roots handled by existing source-library setup.
- [x] Add instrumentation:
  - [x] `stage0.source_tree_index.discovery`
  - [x] `source_tree_index.discovery_runs`
  - [x] `source_tree_index.dirs_visited`
  - [x] `source_tree_index.dirs_skipped`
  - [x] `source_tree_index.files_seen`
  - [x] `source_tree_index.hash_root_files_seen`
  - [x] `source_tree_index.module_roots_found`
  - [x] `source_tree_index.duplicate_hash_root_dirs`
- [x] Ensure directory builds call the source tree index exactly once.
- [x] Replace `discover_root_entry_files` consumers with `SourceTreeIndex.entry_candidates`.
- [x] Remove the old entry discovery owner and frontend-owned module-root traversal after moving all callers.

### Performance gate

- [x] Run the baseline benchmark protocol again after this phase.
- [x] Confirm there is exactly one source-tree discovery run per directory build via counters or logs.
- [x] Confirm `discover_module_roots` and `discover_root_entry_files` are removed.
- [x] Confirm Stage 0/source-tree discovery improves on `module-root-stress`.
- [x] Targeted profiling was not required because the direct before/after scan comparison showed improvement and the wall-clock matrix stayed broadly stable.

### Review / audit / validation

- [x] Run focused source tree index tests.
- [x] Run Stage 0/project structure integration tests.
- [x] Run `just validate`.
- [x] Manual stage-boundary review:
  - [x] duplicate policy diagnostics are deferred with the root-role transition instead of introducing an early behavior break;
  - [x] filesystem traversal failures remain `CompilerError`;
  - [x] `ProjectPathResolver` no longer hides expensive tree scanning;
  - [x] config loading still owns the project-root config probe before source-tree indexing.
- [x] Update active context capsule with benchmark result summary.

---

## Phase 3 — refactor path resolver and hash-root identity

### Context

After Phase 2, path resolution should consume module-root data instead of discovering it. This phase removes filename-specific `#mod`/`#page` logic from path identity and import validation.

### Checklist

- [x] Enforce one non-config hash root per directory while replacing the current filename roles:
  - [x] add a structured diagnostic with all conflicting files where practical;
  - [x] assign a stable diagnostic code;
  - [x] add an integration failure fixture;
  - [x] remove the temporary duplicate-evidence-only path once the diagnostic consumes it.

- [x] Replace the broad `src/compiler_frontend/source_libraries/mod.rs` special-file helpers with `src/compiler_frontend/source_libraries/root_file.rs`.
- [x] Provide generic helpers:
  - [x] `file_name_is_hash_root_file(name: &str) -> bool`
  - [x] `file_name_is_config_file(name: &str) -> bool`
  - [x] `import_path_references_hash_root_file(path, from_grouped, string_table) -> bool`
  - [x] `import_path_references_config_file(path, from_grouped, string_table) -> bool`
  - [x] `hash_root_file_name_from_import_component(...)` for exact diagnostic rendering.
- [x] Keep `#mod.bst` and `#page.bst` accepted as cosmetic root filenames, not semantic import identities.
- [x] Reject direct imports of any hash file:
  - [x] `@x/#page`
  - [x] `@x/#mod`
  - [x] `@x/#home`
  - [x] `@x/#anything.bst`
- [x] Reject direct imports of config:
  - [x] `@./config`
  - [x] `@./config.bst`
- [x] Update `ProjectPathResolver` construction:
  - [x] accept precomputed `ModuleRootTable` / `SourceTreeIndex` data;
  - [x] replace `ModuleRootDiscoveryPolicy::Disabled` with an explicit empty or bounded table for single-file mode;
  - [x] do not call filesystem discovery internally during normal directory builds.
- [x] Replace `module_root_facades` with root-file/export-surface naming.
- [x] Implement nearest-root lookup via parent walk + map lookup, not linear `Vec` scan.
- [x] Update namespace/root lookup owners that currently synthesize or check hard-coded facade paths:
  - [x] `src/compiler_frontend/headers/import_environment/namespace_imports.rs`
  - [x] `src/compiler_frontend/headers/import_environment/builder.rs`
  - [x] `src/compiler_frontend/headers/facade_data.rs`
  - [x] `src/compiler_frontend/headers/symbol_collection.rs` (reviewed; no synthesized facade path remained)
- [x] Update source-library facade/root discovery:
  - [x] each source-library root may contain one non-config `#*.bst` root file;
  - [x] existing `libraries/html/#mod.bst` remains valid as a cosmetic root name;
  - [x] `#mod.bst` is no longer the only accepted source-library facade filename.
- [x] Update `src/build_system/create_project_modules/facade_validation.rs` so source-library preflight validates a generic root file instead of hard-coded `#mod.bst`.

### Review / audit / validation

- [x] Run path resolver tests.
- [x] Run import/facade/source-library tests.
- [x] Run `just validate`.
- [x] Grep audit:
  - [x] remaining `MOD_FILE_NAME` usages are gone or migration-only;
  - [x] remaining `PAGE_FILE_NAME` usages are gone;
  - [x] remaining `CONFIG_FILE_NAME` usages point only to canonical `config.bst` handling;
  - [x] no direct hash-file import bypass exists in the current import classifier.
- [x] Update active context capsule.

---

## Phase 4 — replace file role split with active/imported module root roles

### Context

A root file can now be both export-capable and root/start-body-capable. However, the same root file may also be included in another module compilation only to expose its public API. That imported surface must not execute or validate the imported module's start body in the importing module.

### Required role model

Use explicit roles, not booleans:

```rust
pub(crate) enum FileRole {
    ActiveModuleRoot,
    ImportedModuleRoot,
    Normal,
}
```

Meaning:
- `ActiveModuleRoot`: the root file for the module currently being compiled; `export:` and top-level root/start body are allowed.
- `ImportedModuleRoot`: a foreign root file included only for export/public-surface validation; `export:` and declarations are allowed, but top-level root/start body is not emitted into this module.
- `Normal`: ordinary source file; no `export:` and no top-level runtime/root body.

### Checklist

- [x] Update role assignment in header preparation:
  - [x] active entry file path + root table => `ActiveModuleRoot`;
  - [x] non-active hash root file in the reachable file set => `ImportedModuleRoot`;
  - [x] ordinary `.bst` source => `Normal`.
- [x] Update header parser behavior:
  - [x] active/imported module root roles are export-capable; Phase 5 changes the syntax from inline export to `export:`;
  - [x] top-level root/start body emitted only for `ActiveModuleRoot`;
  - [x] top-level root/start body in `ImportedModuleRoot` is captured and discarded or skipped without becoming `StartFunction`;
  - [x] top-level runtime in `Normal` remains invalid.
- [x] Capture header-level root activity facts needed by the later builder handoff without adding the final backend-facing type yet:
  - [x] `has_non_trivial_root_body`
  - [x] `runtime_fragment_count`
  - [x] `const_fragment_count`
  - [x] final `ModuleRootActivity` handoff remains Phase 7.
- [x] Ensure imported root files still parse private declarations required by exported signatures.
- [x] Ensure imported root files still build public export maps.
- [x] Ensure AST never receives a `StartFunction` header from `ImportedModuleRoot` files.
- [x] Update `HeaderParseOptions` to pass root table/active root identity instead of only entry ID where necessary.

### Review / audit / validation

- [x] Add tests for importing a module root that also has top-level templates/code; importer must not execute or emit imported root output.
- [x] Add tests for active root output still emitting normally.
- [x] Run header parser tests.
- [x] Run integration module import tests.
- [x] Run `just validate`; code gates passed and the docs gate is temporarily blocked by user-owned Phase 5 syntax.
- [x] Manual stage-boundary review:
  - [x] header parser owns start-body separation;
  - [x] AST does not rediscover root role from raw tokens;
  - [x] imported root body suppression is documented and explicit.
- [x] Update active context capsule.

---

## Phase 5 — implement strict `export:` blocks and remove inline export syntax

### Context

The export block should be the only public API marker. This phase should delete legacy export surface rather than adding compatibility branches.

### Checklist

- [x] Add parser support for `export:` at top-level statement boundary.
- [x] Add per-file state tracking:
  - [x] `seen_export_block: Option<SourceLocation>`
  - [x] current export block mode / parser state.
- [x] Parse block contents until the matching top-level `;`.
- [x] Update `top_level_classifier.rs` / `file_parser.rs` so `export:` is distinguished from legacy inline `export ...` before dispatching to declaration parsing.
- [x] Inside the block:
  - [x] grouped `import @path { ... }` => public import record;
  - [x] authored exportable declaration => public header;
  - [x] invalid item => structured diagnostic.
- [x] Reject non-grouped imports inside `export:`.
- [x] Reject trait conformance and trait incompatibility inside `export:`.
- [x] Reject nested `export:`.
- [x] Reject empty `export:` as a structured error because an empty public API section is accidental and has no semantic effect.
- [x] Remove support for legacy forms:
  - [x] `export name #= ...`
  - [x] `export function ...`
  - [x] `export import @path { ... }`
  - [x] `export @path { ... }`
- [x] Keep JavaScript `export` scanning under `src/projects/html_project/external_js/**` unchanged; it is a JS provider concern, not Beanstalk module syntax.
- [x] Keep `HeaderExportMode::Public` as the underlying visibility carrier.
- [x] Keep export block as parser/header mode, not a new scope.

### Diagnostics checklist

- [x] `export:` outside module root file.
- [x] missing `:` after `export`.
- [x] duplicate `export:` block.
- [x] invalid runtime item inside `export:`.
- [x] non-grouped import inside `export:`.
- [x] conformance/incompatibility inside `export:`.
- [x] nested `export:`.
- [x] old inline export syntax.
- [x] duplicate public export names.
- [x] public API leaking private facade/root-only types.

### Review / audit / validation

- [x] Run header/export parser tests.
- [x] Run integration tests for public imports/re-exports.
- [x] Run failure fixtures using stable diagnostic codes.
- [x] Run `just validate`.
- [x] Grep audit:
  - [x] no compatibility parser path for old inline export syntax;
  - [x] no `export @path` sugar remains;
  - [x] `export:` block does not create a scope frame.
- [x] Update active context capsule.

---

## Phase 6 — rebuild module public export maps from generic root files

### Context

The public API map remains the same concept, but its source is now the root file's `export:` block rather than `#mod.bst`. This phase should mostly rename and simplify facade-specific code.

### Checklist

- [x] Rename facade-oriented data where it now means module root public exports:
  - [x] `module_root_facades` -> `module_root_files` or `module_root_export_files`;
  - [x] `facade_exports` -> keep only if still accurate for source libraries, otherwise rename to `public_exports` / `module_public_exports`.
- [x] Update `facade_data.rs` or split it if naming becomes misleading:
  - [x] file-level docs explain module-root public export map construction;
  - [x] source-library roots use same generic root-file logic.
- [ ] Update AST public-surface validation naming in `src/compiler_frontend/ast/module_ast/environment/public_surface.rs` after export maps are root-based, without changing semantic ownership.
- [ ] Rename facade-specific diagnostics and payload reasons only when the behavior has moved to generic module roots:
  - [ ] `MissingModuleFacade`
  - [ ] `ExportOutsideModuleFacade`
  - [ ] `RuntimeTemplateInModuleFacade`
  - [ ] `InvalidTraitConformanceReason::ModuleFacade`
  - [ ] rendered wording that says `#mod.bst` is the only public surface.
- [x] Preserve two-pass export construction:
  - [x] authored public headers first;
  - [x] public import re-exports second.
- [x] Preserve duplicate public name detection.
- [x] Preserve reserved core cast trait collision checks.
- [x] Preserve receiver-method direct-export rejection.
- [ ] Preserve public API private-type leakage validation in AST/environment.
- [ ] Ensure root modules with no `export:` produce an empty public export map.
- [ ] Ensure imports across module boundaries require the target module root to have a matching export.

### Review / audit / validation

- [ ] Run source-library facade/export tests.
- [ ] Run cross-module import tests.
- [ ] Run public API leakage diagnostics tests.
- [ ] Run `just validate`.
- [ ] Manual stage-boundary review:
  - [ ] header/import preparation builds visibility;
  - [ ] AST consumes visibility and validates semantic leakage;
  - [ ] no AST rediscovery of export syntax.
- [ ] Update active context capsule.

---

## Phase 7 — add root activity metadata and builder artifact filtering

### Context

Root files can be API-only. Builders must decide output policy from explicit metadata, not from filename conventions or ad-hoc HIR scanning.

### Checklist

- [ ] Add `ModuleRootActivity` or equivalent to backend handoff:

```rust
pub(crate) struct ModuleRootActivity {
    pub(crate) has_non_trivial_root_body: bool,
    pub(crate) const_fragment_count: usize,
    pub(crate) runtime_fragment_count: usize,
}
```

- [ ] Add helper:

```rust
impl ModuleRootActivity {
    pub(crate) fn has_html_artifact_activity(&self) -> bool {
        self.has_non_trivial_root_body
            || self.const_fragment_count > 0
            || self.runtime_fragment_count > 0
    }
}
```

- [ ] Populate metadata from header parsing / sorted headers / AST finalization without HIR rescans.
- [ ] Add the metadata to `build.rs::Module`.
- [ ] Update HTML builder:
  - [ ] review `src/projects/html_project/output_plan.rs` and `src/projects/html_project/path_policy.rs`, which currently special-case `#page`;
  - [ ] skip page artifact compilation for modules with no HTML artifact activity;
  - [ ] keep skipped modules available for import/export validation and external package metadata where relevant;
  - [ ] homepage selection ignores skipped API-only modules;
  - [ ] if all modules are API-only, produce a clear diagnostic or empty project according to existing project policy. Recommended: diagnostic for HTML directory builds with no page roots.
- [ ] Ensure runtime asset/tracked asset planning only uses modules that emitted page artifacts unless a future builder explicitly asks for API-only artifact emission.
- [ ] Do not add a second discovery scan to classify artifact relevance.

### Review / audit / validation

- [ ] Add integration test: API-only module root emits no HTML output.
- [ ] Add integration test: module root with top-level template emits HTML output.
- [ ] Add integration test: module root with top-level runtime code emits output if current HTML semantics support it.
- [ ] Add integration test: imported API-only module contributes functions/types to a page root.
- [ ] Run HTML builder tests.
- [ ] Run `just validate`.
- [ ] Run focused benchmark check for API-only module projects if fixtures are added.
- [ ] Update active context capsule.

---

## Phase 8 — update reachable-file traversal for generic root files

### Context

Current reachable traversal queues source library facades and cross-module `#mod.bst` facades. After this change, it must queue module root files for public-surface validation while respecting active/imported root roles.

### Checklist

- [ ] Update source library root queueing:
  - [ ] queue each source library root file from generic root metadata;
  - [ ] stop looking specifically for `#mod.bst`.
- [ ] Update cross-module import handling:
  - [ ] when an import crosses module root boundary, queue the target module root file as `ImportedModuleRoot` surface;
  - [ ] avoid queueing if already reachable;
  - [ ] preserve source kind for Beandown/Markdown imports.
- [ ] Ensure same-module imports do not require public export checks.
- [ ] Preserve provider-capable and provider-free traversal paths.
- [ ] Preserve source-cache reuse during import scanning.
- [ ] Preserve Beandown same-directory root-file queueing for restricted same-directory compile-time constants, updated from facade naming to root-file naming.
- [ ] Update `src/projects/html_project/beandown/scope.rs` and related Beandown tests if they still name same-directory `#mod.bst` constants.

### Review / audit / validation

- [ ] Run reachable file discovery tests.
- [ ] Run Beandown/Markdown import tests.
- [ ] Run provider-backed external import tests if available.
- [ ] Run `just validate`.
- [ ] Run `just bench-frontend-check` to ensure traversal changes did not regress parallelism/source-loading cases.
- [ ] Update active context capsule.

---

## Phase 9 — documentation, generated docs, and progress matrix

### Context

Language semantics and project structure changed. Docs must be updated in the same plan before declaring completion.

### Checklist

- [ ] Update `docs/language-overview.md`:
  - [ ] syntax summary: `export:` block;
  - [ ] config file: `config.bst`;
  - [ ] module roots: one non-config `#*.bst` file per directory;
  - [ ] imports: no direct hash-file imports;
  - [ ] root code/import execution separation;
  - [ ] API-only module behavior.
- [ ] Update `docs/compiler-design-overview.md`:
  - [ ] Stage 0 source tree index;
  - [ ] path resolver no longer owns expensive discovery;
  - [ ] active/imported module root roles;
  - [ ] header parsing/export block ownership;
  - [ ] builder artifact policy from root activity metadata.
- [ ] Update docs-site pages:
  - [ ] project structure;
  - [ ] libraries/imports;
  - [ ] getting started / `bean new`;
  - [ ] config/getting started if relevant;
  - [ ] progress matrix.
- [ ] Update `README.md` examples if they use alternate config filenames, or treat `#page.bst` or `#mod.bst` as semantic names.
- [ ] Update `benchmarks/README.md` if metric names or benchmark protocol changed.
- [ ] Update `bean new html` scaffold output and tests if not already completed in Phase 1:
  - [ ] `src/projects/html_project/new_html_project/scaffold.rs`
  - [ ] scaffold summary/replacement tests
  - [ ] docs-site getting-started scaffold tree.
- [ ] Add breaking-change notes:
  - [ ] `config.bst` is the only project config filename, with no compatibility path;
  - [ ] merge `#mod.bst` and `#page.bst` into one root file if both existed;
  - [ ] replace inline `export` with `export:` block.

### Review / audit / validation

- [ ] Run docs build/check if available.
- [ ] Run `just validate`.
- [ ] Grep docs for stale terms:
  - [ ] alternate project config filenames
  - [ ] `#mod.bst` as the only facade
  - [ ] `#page.bst` as special required entry filename
  - [ ] `export import`
  - [ ] inline `export ` examples.
- [ ] Update active context capsule.

---

## Phase 10 — final performance validation and cleanup audit

### Context

This phase proves the refactor achieved its performance and simplification goals. It also removes obsolete code after all behavior is wired.

### Checklist

- [ ] Delete obsolete files/functions after replacement:
  - [ ] old `discover_root_entry_files` implementation;
  - [ ] old filename-specific module root/facade helpers;
  - [ ] old inline export parser branches;
  - [ ] compatibility wrappers or forwarding shims introduced during migration.
- [ ] Run grep audit:
  - [ ] no alternate project config filename, compatibility branch or migration diagnostic remains;
  - [ ] `MOD_FILE_NAME` removed or no longer semantic;
  - [ ] `PAGE_FILE_NAME` removed or no longer semantic;
  - [ ] `export import` only appears in migration notes/tests for rejection;
  - [ ] no second expensive discovery pass remains.
- [ ] Run full validation:
  - [ ] `just validate`
- [ ] Run final benchmark protocol:
  - [ ] `just bench-frontend-check`
  - [ ] `RAYON_NUM_THREADS=1 just bench-frontend-check`
  - [ ] `RAYON_NUM_THREADS=2 just bench-frontend-check`
  - [ ] `RAYON_NUM_THREADS=4 just bench-frontend-check`
  - [ ] `just bench-check`
  - [ ] `just bench-report`
- [ ] Confirm acceptance gates:
  - [ ] exactly one source-tree discovery run per directory build;
  - [ ] module-root stress/source-tree discovery stage improved versus baseline;
  - [ ] no broad compile-time regression;
  - [ ] no hidden path resolver scan;
  - [ ] no artifact emitted for API-only HTML modules;
  - [ ] imported root start bodies do not execute in importers.
- [ ] If benchmark results are mixed:
  - [ ] run `just profile-case <case> terse` for regressed cases;
  - [ ] record concise explanation;
  - [ ] either fix or explicitly update the plan with accepted tradeoff before completion.

### Final review / audit / validation

- [ ] Manual style-guide review:
  - [ ] clear stage ownership;
  - [ ] no duplicated boundary paths;
  - [ ] no obsolete APIs left behind;
  - [ ] files have clear file-level docs;
  - [ ] functions use named steps and explicit control flow;
  - [ ] no user-input `panic!`, `todo!`, or unsafe `.unwrap()` paths.
- [ ] Manual compiler-boundary review:
  - [ ] Stage 0 owns tree discovery;
  - [ ] header parsing owns export/root-body splitting;
  - [ ] AST owns semantic validation but not syntax rediscovery;
  - [ ] HIR/backend receive explicit root activity metadata.
- [ ] Update active context capsule one final time with final commit and validation state.

---

## Test matrix to add or update

Integration cases under `tests/cases/` should cover:

- [ ] arbitrary hash root file, e.g. `#home.bst`;
- [ ] same behavior for existing `#page.bst` and `#mod.bst` as cosmetic names;
- [ ] duplicate hash files in one directory;
- [ ] `config.bst` accepted;
- [ ] alternate filenames receive no config-specific handling;
- [ ] direct hash-file import rejected;
- [ ] direct `config.bst` / `config` import rejected;
- [ ] module directory import resolves through `export:`;
- [ ] module with no `export:` exports nothing;
- [ ] API-only module produces no HTML artifact;
- [ ] module with top-level template produces HTML artifact;
- [ ] imported module root with top-level template does not execute in importer;
- [ ] old inline `export` syntax rejected;
- [ ] conformance inside `export:` rejected;
- [ ] public API private type leakage still rejected;
- [ ] source-library root can use any single hash root filename;
- [ ] Beandown same-directory root constants still work through root export/scope rules if currently supported.

Unit/focused tests should cover:

- [ ] source tree index skip policy;
- [ ] source tree index duplicate detection;
- [ ] source tree index single discovery-run counter;
- [ ] parent-walk nearest-root lookup;
- [ ] generic hash-file import rejection;
- [ ] active/imported module root role assignment;
- [ ] export block parser invalid item cases.

Benchmark fixtures should cover:

- [ ] many irrelevant skipped directories under entry root;
- [ ] many module roots with one file each;
- [ ] few modules with many files each;
- [ ] API-only roots mixed with page/output roots;
- [ ] source-library root with cosmetic non-`#mod.bst` hash filename.

---

## Agent implementation rules

- Complete one phase per slice unless the phase is explicitly split in the active context capsule.
- At the start of every slice, refresh the active context capsule.
- At the end of every slice, run the phase validation commands or record why they could not run.
- Do not preserve old APIs for compatibility unless the plan is updated and approved.
- Do not add new broad abstractions unless they replace existing duplication.
- Prefer deleting old code after migration over leaving parallel paths.
- Use structured `CompilerDiagnostic` for source/config/import/rule errors.
- Use `CompilerError` only for filesystem, tooling, backend, or internal infrastructure failures.
- Add stable diagnostic codes for new failure cases and use them in integration fixtures.
- Keep benchmark raw data and profiles local-only.
