
  7. Oversized test files mixing unrelated concerns
  Issue: Three test files exceed 2,000 lines and cover many distinct subsystems:
  • src/build_system/tests/build_tests.rs (2,709) — HTML builder, JS backend goldens, Wasm backend, config validation, receiver methods, constant folding, slot insertion, markdown wrappers.
  • src/compiler_frontend/ast/templates/tests/create_template_node_tests.rs (2,698) — head parsing, body parsing, slot resolution, style formatting, directives, constant folding.
  • src/compiler_frontend/hir/tests/hir_statement_lowering_tests.rs (2,145) — results, branching, loops, short-circuit, multi-bind, match lowering.
    Why it matters: Large mixed test files slow down compile times and make it hard to find where to add a new case.
    Recommended fix: Split each by concern. Example: build_tests.rs → build_html_tests.rs, build_js_tests.rs, build_wasm_tests.rs, build_config_tests.rs.
