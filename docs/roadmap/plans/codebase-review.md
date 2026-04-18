Audit Report — Beanstalk Codebase

  Scope: Full codebase audit (src/ + tests/cases/) against docs/codebase-style-guide.md and docs/compiler-design-overview.md, focused on duplication, stage-boundary leaks, legacy code, mod.r
  s bloat, and dead code.
  Overall state: The codebase is well-structured at the module level but carries significant duplication across backends, duplicated test infrastructure, stage-boundary leaks, and one large
  unused backend subtree. No user-input panics were found in active paths.

  Top Findings
  ════════════

  1. Backend error types live in the frontend
  Issue: ErrorType::LirTransformation and ErrorType::WasmGeneration (plus their macros return_lir_transformation_error!, return_wasm_generation_error!) are defined in src/compiler_frontend/c
  ompiler_messages/compiler_errors.rs.
  Why it matters: Backends import these from compiler_frontend. Error categories for backend codegen should live in the backend layer or a shared core module, not the frontend. This is a cle
  ar stage-boundary leak.
  Evidence: compiler_errors.rs:375,435,449,450 and macro definitions at lines 1149,1204.
  Recommended fix: Move LirTransformation and WasmGeneration variants (and their macros) to src/backends/wasm/ or a shared compiler_core module. Frontend should only own Syntax, Type, Rule,
  BorrowChecker, HirTransformation, Compiler, File, Config, DevServer.

  2. HTML markup generation inside frontend AST template folding
  Issue: ast/templates/styles/markdown.rs and ast/templates/styles/code.rs emit raw HTML (<ul>, <ol>, <p>, <code class='codeblock'>, <span class='bst-code-keyword'>, HTML entities like &lt;)
  .
  Why it matters: Compile-time template folding should produce a target-agnostic IR or plain text. Embedding HTML markup in the frontend AST hardcodes a backend output format into a frontend
  stage.
  Evidence: markdown.rs:33–45, code.rs:81–98,152+,170–491.
  Recommended fix: Replace HTML string emission with a structured formatter output (FormatterOutputPiece already exists). Let the HTML builder/backend inject markup when rendering fragments.

  3. Borrow-checker engine lives in mod.rs
  Issue: src/compiler_frontend/analysis/borrow_checker/mod.rs is 547 lines and contains the full BorrowChecker struct, fixed-point worklist algorithm, state joins, drop-site analysis, and su
  ccessors().
  Why it matters: The style guide says mod.rs should be a structural map. ~450 lines of core algorithm blur module structure.
  Evidence: borrow_checker/mod.rs:49–544.
  Recommended fix: Extract BorrowChecker, run(), analyze_function(), and the worklist loop into borrow_checker/engine.rs or driver.rs. Leave mod.rs with pub(crate) use, check_borrows thin en
  try point, and submodule declarations.

  4. Duplicate expression-tree traversal in borrow checker
  Issue: record_shared_reads_in_expression (lines 964–1099) and collect_expression_roots (lines 1102–1184) in access.rs match on the same HirExpressionKind variants with nearly identical arm
  s. The first function calls the second at its end, so the tree is traversed twice with the same shape.
  Why it matters: Double traversal of the same IR tree is wasteful and the match arms are a maintenance hazard (any new variant must be updated in both).
  Evidence: borrow_checker/transfer/access.rs:964–1184.
  Recommended fix: Restructure so record_shared_reads_in_expression collects roots while recording reads in a single pass, or introduce a generic fold_expression helper parameterized by the
  leaf action.

  5. Circular dependency + forwarding wrappers in expression parsing
  Issue: ast/expressions/parse_expression.rs (22–36, 118–134) contains thin wrappers that forward to parse_expression_lists.rs. Meanwhile parse_expression_lists.rs:7 imports create_expressio
  n and create_expression_with_trailing_newline_policy from parse_expression.rs.
  Why it matters: Circular module dependencies are architectural debt. The wrappers exist only to paper over the cycle.
  Evidence: parse_expression.rs:29,126 and parse_expression_lists.rs:7.
  Recommended fix: Move create_multiple_expressions and create_expression_until into parse_expression.rs (they are list variants of expression parsing) and remove the re-export wrappers. Or
  extract a shared expression_parsing_core module both files depend on.

  7. Duplicated backend lowering helpers
  Issue: Several small helpers are copy-pasted across backends with identical or near-identical logic:
  • collect_reachable_blocks/collect_reachable_block_ids: 4+ independent implementations in js/utils.rs:67, wasm/hir_to_lir/function.rs:205, rust_interpreter/lowering/functions.rs:170, and b
    row_checker/metadata.rs:516. The JS and Wasm versions even reimplement block_successors/terminator_targets.
  • is_unit_expression: js/js_expr.rs:215 checks tuple-empty + type context; rust_interpreter/lowering/terminators.rs:121 checks only tuple-empty. Behavior diverges.
  • lower_type_to_abi / lower_storage_type: wasm/hir_to_lir/context.rs:171 and rust_interpreter/lowering/context.rs:51 map the same HirTypeKind variants to backend-specific enums.
    Why it matters: Duplicated CFG reachability and type-classification logic drifts when HIR changes.
    Recommended fix: Extract hir::utils::collect_reachable_blocks (it already exists in metadata.rs) and promote it to a public HIR utility used by all backends. Extract a shared classify_hi
    type helper that returns a frontend-owned TypeClass enum (scalar / heap / void) so backends only map their own ABI types.

  8. Duplicated build-system utilities
  Issue: should_skip_unchanged_write appears verbatim in build_system/build.rs:437 and build_system/output_cleanup.rs:405. file_error_messages appears verbatim in 5 files (build.rs, output_c
  leanup.rs, html_project_builder.rs, path_policy.rs, tracked_assets.rs).
  Why it matters: These are one-liner wrappers, but 5 copies of the same wrapper is noise.
  Evidence: See grep results.
  Recommended fix: Move both to build_system/common.rs or build_system/utils.rs and import everywhere.

  9. Duplicated test infrastructure
  Issue:
  • temp_dir() pattern (timestamp + std::env::temp_dir().join(...)) appears in 12+ test files with only the prefix string changed.
  • build_ast() + lower_ast() + assert_no_placeholder_terminators() are duplicated across hir_statement_lowering_tests.rs, loop_lowering_tests.rs, hir_validation_tests.rs, hir_function_origi
    tests.rs.
  • file_error_messages wrappers duplicated in HTML project tests.
    Why it matters: Test boilerplate duplication makes harness changes expensive.
    Evidence: build_system/tests/build_tests.rs:30, build_system/tests/create_project_modules_tests.rs:14, compiler_tests/integration_test_runner/tests.rs:21, projects/dev_server/tests/*, co
    iler_frontend/hir/tests/*.
    Recommended fix: Create src/test_support/temp_dir.rs (or use a test-support crate module) shared by all test modules. Extend src/compiler_frontend/hir/tests/hir_builder_test_support.rs w
    h build_ast, lower_ast, and assert_no_placeholder_terminators.

  10. Oversized test files mixing unrelated concerns
  Issue: Three test files exceed 2,000 lines and cover many distinct subsystems:
  • src/build_system/tests/build_tests.rs (2,709) — HTML builder, JS backend goldens, Wasm backend, config validation, receiver methods, constant folding, slot insertion, markdown wrappers.
  • src/compiler_frontend/ast/templates/tests/create_template_node_tests.rs (2,698) — head parsing, body parsing, slot resolution, style formatting, directives, constant folding.
  • src/compiler_frontend/hir/tests/hir_statement_lowering_tests.rs (2,145) — results, branching, loops, short-circuit, multi-bind, match lowering.
    Why it matters: Large mixed test files slow down compile times and make it hard to find where to add a new case.
    Recommended fix: Split each by concern. Example: build_tests.rs → build_html_tests.rs, build_js_tests.rs, build_wasm_tests.rs, build_config_tests.rs.

  11. Duplicate integration-test fixtures
  Issue: Many tests/cases/ subdirectories contain byte-identical expect.toml, #config.bst, and #page.bst stubs. One expect.toml hash matches 22 different cases.
  Why it matters: Boilerplate fixtures create maintenance surface area.
  Evidence: Identical hashes across tests/cases/*/expect.toml, tests/cases/*/#config.bst, etc.
  Recommended fix: Teach the integration runner to apply a default expect.toml when a case omits one, and provide shared stub fixtures in tests/fixtures/stubs/.

  12. #[allow(dead_code)] without clear justification
  Issue: Several items are suppressed without the explanatory comments required by the style guide:
  • SlotOutput.id in compiler_tests/integration_test_runner/assertions.rs:914 — zero justification comment (the only occurrence in the codebase with none).
  • rebuild_content() in template_render_plan.rs:170 — 30-line unreferenced function, justification is speculative.
  • BuiltinErrorManifest.reserved_symbol_paths in builtins/error_type.rs:83 — never read, speculative "future checks".
  • hir_display.rs — 9 #[allow(dead_code)] annotations on debug-dump impl blocks. Should be behind #[cfg(test)] or a debug_dump feature gate.
    Evidence: See lines cited above.
    Recommended fix: Remove unreferenced speculative code. Move debug dumps behind feature gates or test cfg. Add missing justification comments or delete the fields.
  ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
  Refactor Plan
  ═════════════
   #    Target area                                                  Concrete change                                               Action
  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
   1    compiler_errors.rs                                           Move WasmGeneration, LirTransformation variants and macros    Move to src/backends/wasm/ or new src/compiler_core/errors
                                                                     to backend layer                                              .rs
   2    ast/templates/styles/                                        Remove HTML string emission from markdown.rs and code.rs      Rewrite to emit structured FormatterOutputPiece only; HTML
                                                                                                                                   builder injects tags
   3    borrow_checker/mod.rs                                        Extract engine algorithm to submodule                         Move BorrowChecker, run(), analyze_function(), worklist to
                                                                                                                                   engine.rs; leave entry point in mod.rs
   4    borrow_checker/transfer/access.rs                            Unify record_shared_reads_in_expression and collect_express   Merge into single traversal or extract generic fold_expres
                                                                     ion_roots                                                     sion helper
   5    ast/expressions/parse_expression.rs ↔ parse_expression_lis   Break circular dependency                                     Restructure: move list helpers into parse_expression.rs an
        ts.rs                                                                                                                      d delete wrappers, or extract shared core module
   6    backends/rust_interpreter/                                   Decide fate of unused backend                                 Remove entire tree, or wire it into a test path and remove
                                                                                                                                   blanket #![allow(dead_code)]
   7    Backend lowering                                             Deduplicate collect_reachable_blocks, is_unit_expression, t   Extract shared hir::utils for CFG reachability; extract hi
                                                                     ype-classification                                            r_type_class helper for ABI mapping
   8    build_system/                                                Deduplicate should_skip_unchanged_write and file_error_mess   Move to build_system/common.rs; update all call sites
                                                                     ages
   9    Test support                                                 Deduplicate temp_dir, build_ast, lower_ast, assert_no_place   Extract into src/test_support/ and src/compiler_frontend/h
                                                                     holder_terminators                                            ir/tests/test_support.rs
   10   Test files >2k lines                                         Split by concern                                              Split build_tests.rs, create_template_node_tests.rs, hir_s
                                                                                                                                   tatement_lowering_tests.rs into focused modules
   11   Integration fixtures                                         Reduce boilerplate duplication                                Restructure harness to apply default expectations; share s
                                                                                                                                   tub fixtures
   12   Dead code cleanup                                            Address weak #[allow(dead_code)]                              Remove SlotOutput.id (or justify), rebuild_content(), rese
                                                                                                                                   rved_symbol_paths; gate debug dumps behind cfg
  ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
  Test Plan
  ═════════
  • Missing coverage: No tests enforce that collect_reachable_blocks behaves identically across JS, Wasm, and borrow-checker metadata. Add a cross-consumer HIR utility test.
  • Redundant coverage: Merge identical expect.toml boilerplate into harness defaults; prune duplicate stub fixtures.
  • Integration cases to add: Add a case that asserts the compiler rejects backend-specific error types being imported from frontend (once moved).
  • Artifact assertions: Wasm lowering tests currently use goldens; strengthen with artifact_assertions checking exact exports (memory, bst_str_ptr, etc.) where not already present.
  • Regression tests: Add a test ensuring parse_expression.rs and parse_expression_lists.rs do not reintroduce a circular dependency (can be a simple cargo check / lint rule).
  ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
  Doc Drift
  ═════════
  • docs/compiler-design-overview.md states: "Error types for WASM and LIR should live in the backend layer or a shared compiler-core module" — the current code contradicts this by placing t
    m in compiler_frontend. Callout: compiler_errors.rs needs to align with the design doc.
  • docs/codebase-style-guide.md says mod.rs should be the module entry point and structural map — borrow_checker/mod.rs and dev_server/mod.rs violate this. They are implementation-heavy.
  ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
  Immediate Priorities
  ════════════════════
  1. Move backend error types out of frontend (WasmGeneration, LirTransformation) — highest-value boundary cleanup.
  2. Extract borrow-checker engine from mod.rs — improves module clarity and follows the style guide.
  3. Delete or wire the rust_interpreter backend — removes a 2,500-line dead-code blind spot.
  4. Deduplicate collect_reachable_blocks and type-classification helpers across backends — reduces drift risk when HIR evolves.
  5. Break parse_expression.rs ↔ parse_expression_lists.rs circular dependency — remove forwarding wrappers.
  6. Consolidate test support helpers (temp_dir, build_ast, lower_ast) — reduces test boilerplate before splitting oversized test files.