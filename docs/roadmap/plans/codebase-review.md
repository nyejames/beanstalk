Audit Report — Beanstalk Codebase

  Scope: Full codebase audit (src/ + tests/cases/) against docs/codebase-style-guide.md and docs/compiler-design-overview.md, focused on duplication, stage-boundary leaks, legacy code, mod.r
  s bloat, and dead code.
  Overall state: The codebase is well-structured at the module level but carries significant duplication across backends, duplicated test infrastructure, stage-boundary leaks, and one large
  unused backend subtree. No user-input panics were found in active paths.

  Top Findings
  ════════════

  2. Duplicate expression-tree traversal in borrow checker
  Issue: record_shared_reads_in_expression (lines 964–1099) and collect_expression_roots (lines 1102–1184) in access.rs match on the same HirExpressionKind variants with nearly identical arm
  s. The first function calls the second at its end, so the tree is traversed twice with the same shape.
  Why it matters: Double traversal of the same IR tree is wasteful and the match arms are a maintenance hazard (any new variant must be updated in both).
  Evidence: borrow_checker/transfer/access.rs:964–1184.
  Recommended fix: Restructure so record_shared_reads_in_expression collects roots while recording reads in a single pass, or introduce a generic fold_expression helper parameterized by the
  leaf action.

  7. Oversized test files mixing unrelated concerns
  Issue: Three test files exceed 2,000 lines and cover many distinct subsystems:
  • src/build_system/tests/build_tests.rs (2,709) — HTML builder, JS backend goldens, Wasm backend, config validation, receiver methods, constant folding, slot insertion, markdown wrappers.
  • src/compiler_frontend/ast/templates/tests/create_template_node_tests.rs (2,698) — head parsing, body parsing, slot resolution, style formatting, directives, constant folding.
  • src/compiler_frontend/hir/tests/hir_statement_lowering_tests.rs (2,145) — results, branching, loops, short-circuit, multi-bind, match lowering.
    Why it matters: Large mixed test files slow down compile times and make it hard to find where to add a new case.
    Recommended fix: Split each by concern. Example: build_tests.rs → build_html_tests.rs, build_js_tests.rs, build_wasm_tests.rs, build_config_tests.rs.

  9. #[allow(dead_code)] without clear justification
  Issue: Several items are suppressed without the explanatory comments required by the style guide:
  • SlotOutput.id in compiler_tests/integration_test_runner/assertions.rs:914 — zero justification comment (the only occurrence in the codebase with none).
  • rebuild_content() in template_render_plan.rs:170 — 30-line unreferenced function, justification is speculative.
  • BuiltinErrorManifest.reserved_symbol_paths in builtins/error_type.rs:83 — never read, speculative "future checks".
  • hir_display.rs — 9 #[allow(dead_code)] annotations on debug-dump impl blocks. Should be behind #[cfg(test)] or a debug_dump feature gate.
    Evidence: See lines cited above.
    Recommended fix: Remove unreferenced speculative code. Move debug dumps behind feature gates or test cfg. Add missing justification comments or delete the fields.
  ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────


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
  ─────────────────────────────────────────────────────────────────────────────────────────────