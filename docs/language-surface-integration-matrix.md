# Language Surface Integration Matrix

This file is the repo-owned snapshot of which supported Beanstalk language features have:
- real implementation on `main`
- targeted parser / unit coverage
- canonical end-to-end integration coverage
- backend/runtime coverage where lowering matters

Use this together with `tests/cases/manifest.toml` when adding or reviewing language features.

## Status legend

- **Implemented** — supported for the Alpha surface and represented in the compiler today.
- **Implemented (incomplete)** — supported in a meaningful subset, but some sub-surfaces are intentionally deferred or still thinly covered.
- **Reserved / deferred** — syntax or surface is intentionally blocked with structured diagnostics and is not part of the supported Alpha surface.

## Coverage legend

- **Broad** — multiple focused cases plus clear success/failure coverage.
- **Targeted** — some dedicated coverage exists, but not across all important edges.
- **Thin** — only a small number of cases exist, or coverage is indirect.
- **None** — no clear canonical coverage found.

## Cross-platform / golden flag

`Yes` means the surface is especially sensitive to emitted output shape, newline normalization, path formatting, or golden drift.
It does **not** mean the surface is already fully hardened cross-platform.
It only marks where drift is more likely and where matrix reviews should pay attention.

## Alpha surface matrix

| Surface | Status | Parser / unit coverage | Integration coverage | Backend / runtime coverage | Golden-sensitive | Canonical cases | Explicit gaps |
| --- | --- | --- | --- | --- | --- | --- | --- |
| control flow | Implemented | **Broad** — `src/compiler_frontend/ast/statements/branching.rs`, `src/compiler_frontend/ast/statements/tests/branching_tests.rs`, `src/compiler_frontend/hir/tests/hir_statement_lowering_tests.rs` | **Broad** | **JS + HTML-Wasm** | Yes | `control_flow`, `function_if_loop_smoke`, `loop_*`, `simple_if_test`, `dynamic_if_test`, `literal_match_with_else_success`, `choice_match_exhaustive_success` | Short-circuit fixtures now expose one active borrow-checker fallout (`logical_short_circuit_and`, `logical_short_circuit_or`) from stricter artifact-shaped coverage |
| functions / calls | Implemented | **Broad** — `src/compiler_frontend/ast/expressions/function_calls.rs`, `src/compiler_frontend/ast/expressions/call_validation.rs`, `src/compiler_frontend/ast/expressions/tests/function_call_tests.rs`, `src/compiler_frontend/ast/statements/tests/function_parsing_tests.rs` | **Broad** | **JS** | Yes | `functions`, `function_calls`, `function_single_call_smoke`, `function_return_smoke`, `function_call_arg_type_*`, `host_function_integration`, `host_function_with_control_flow`, `imported_start_function_callable_not_auto_run`, `js_function_param_passing` | Normal function-call semantics are now artifact-shaped; backend/runtime contract checks remain in dedicated host/js buckets |
| templates / style directives | Implemented | **Broad** — `src/compiler_frontend/ast/templates/template.rs`, `src/compiler_frontend/ast/templates/template_head_parser/core_directives.rs`, `src/compiler_frontend/ast/templates/tests/create_template_node_tests.rs`, `src/compiler_frontend/ast/templates/tests/slots_tests.rs` | **Broad** | **HTML + HTML-Wasm + runtime fragment lowering** | Yes | `template_*`, `top_level_const_template*`, `template_html_directives_html_project_success`, `html_wasm_runtime_template`, `html_wasm_multi_fragment_string` | Cross-platform newline / golden drift is still a later roadmap hardening area |
| structs / records / methods | Implemented | **Targeted** — `src/compiler_frontend/ast/expressions/struct_instance.rs`, `src/compiler_frontend/ast/receiver_methods.rs`, `src/compiler_frontend/ast/module_ast/pass_function_signatures.rs` | **Broad** | **JS** | No | `structs_and_collections`, `struct_using_constant`, `struct_constructor_*`, `struct_nested_field_mutation_success`, `receiver_method_basic_call`, `receiver_method_exported_cross_file_success`, `struct_chained_immutable_receiver_method_call`, `js_struct_field_mutation`, `js_nested_struct_field` | More backend-facing receiver / field-mutation cases are still useful, especially outside JS |
| choices | Implemented (incomplete) | **Targeted** — `src/compiler_frontend/headers/parse_file_headers.rs`, `src/compiler_frontend/headers/tests/parse_file_headers_tests.rs`, `src/compiler_frontend/ast/module_ast/pass_declarations.rs`, `src/compiler_frontend/ast/statements/branching.rs` | **Targeted** | **Thin backend-specific coverage** | No | `choice_basic_declaration_and_use`, `choice_import_visibility_exported`, `choice_match_exhaustive_success`, `choice_match_non_exhaustive_failure`, `choice_match_unknown_variant_failure`, `choice_*_deferred` | Payload declarations, tagged/default declarations, and constructor use are intentionally deferred; dedicated backend/runtime choice cases are still thin |
| pattern matching | Implemented (incomplete) | **Broad** — `src/compiler_frontend/ast/statements/branching.rs`, `src/compiler_frontend/ast/statements/tests/branching_tests.rs` | **Broad** | **JS** | No | `choice_match_*`, `literal_match_*`, `diagnostic_match_*`, `js_dispatcher_loop_with_match`, `adversarial_loop_match_result_chain` | Wildcard, relational, negated, and capture/tagged patterns are intentionally deferred for Alpha |
| arrays / collections | Implemented | **Targeted** — `src/compiler_frontend/ast/field_access/collection_builtin.rs`, `src/compiler_frontend/ast/statements/tests/collections_tests.rs`, `src/compiler_frontend/hir/hir_expression/calls.rs` | **Broad** | **JS** | Yes | `collection_literal_smoke`, `collection_methods_end_to_end`, `collection_methods_backend_contract`, `collection_indexed_write_end_to_end`, `collection_get_out_of_bounds`, `collection_mutating_method_requires_explicit_receiver_tilde`, `loop_collection_iteration*` | HTML-Wasm-specific collection runtime checks are still thinner than html/js coverage |
| results / options / multiple returns / multiple assignment | Implemented | **Targeted** — `src/compiler_frontend/ast/statements/result_handling.rs`, `src/compiler_frontend/ast/statements/multi_bind.rs`, `src/compiler_frontend/hir/tests/hir_statement_lowering_tests.rs` | **Broad** | **Thin dedicated backend/runtime coverage** | No | `none_*`, `multi_bind_*`, `result_*`, `adversarial_borrow_after_result_handler`, `adversarial_nested_named_error_handlers`, `adversarial_loop_match_result_chain` | Propagation / fallback / named-handler behavior is well covered frontend-to-frontend, but still lacks enough explicit emitted-output runtime cases |
| type checking | Implemented | **Broad** — `src/compiler_frontend/type_coercion/*`, `src/compiler_frontend/ast/expressions/tests/function_call_tests.rs`, `src/compiler_frontend/tests/type_syntax_tests.rs` | **Broad** | **Mostly frontend-owned** | No | `function_call_arg_type_*`, `function_call_arg_errorKind_accepts_string`, `int_promotion_to_float_*`, `float_declaration_from_bool_rejected`, `int_declaration_from_float_rejected`, `struct_nominal_type_mismatch_rejected`, `if_condition_requires_bool`, `logical_invalid_operand_types`, `not_requires_bool` | The matrix should keep distinguishing contextual coercion coverage from strict expression typing coverage |
| paths / imports | Implemented | **Targeted** — `src/compiler_frontend/headers/parse_file_headers.rs`, `src/build_system/create_project_modules/*`, `src/build_system/tests/build_tests.rs` | **Broad** | **HTML builder + path rendering** | Yes | `import_syntax_test`, `relative_import_dot_segments`, `multi_file_module`, `path_*`, `complex_import_errors`, `circular_dependency` | Cross-platform path normalization / output formatting remains a drift-sensitive area |
| html project builds | Implemented | **Targeted** — `src/build_system/tests/build_tests.rs`, `src/compiler_tests/integration_test_runner/fixture.rs` | **Broad** | **HTML + HTML-Wasm** | Yes | `html_builder_*`, `html_tracked_asset_*`, `html_duplicate_route_reports_config_error`, `html_missing_homepage_reports_config_error`, `template_html_directives_html_project_success`, `html_wasm_*` | Still one of the most golden-sensitive surfaces in the repo |
| logical expressions | Implemented | **Targeted** — `src/compiler_frontend/ast/expressions/parse_expression.rs`, `src/compiler_frontend/ast/expressions/eval_expression.rs` | **Broad** | **JS** | Yes | `comparison_and_logical`, `if_logical_precedence`, `if_nested_boolean_conditions`, `logical_parenthesized_grouping`, `logical_short_circuit_and`, `logical_short_circuit_or`, `logical_invalid_operand_types`, `not_requires_bool`, `js_operator_mapping` | Artifact-shaped short-circuit fixtures currently expose borrow-checker ownership divergence in `logical_short_circuit_and`/`logical_short_circuit_or` |
| if statements / conditions | Implemented | **Broad** — `src/compiler_frontend/ast/statements/branching.rs`, `src/compiler_frontend/ast/statements/tests/branching_tests.rs` | **Broad** | **JS + HTML-Wasm** | No | `simple_if_test`, `dynamic_if_test`, `if_condition_requires_bool`, `if_nested_boolean_conditions`, `js_structured_if_no_dispatcher`, `html_wasm_bool_conditional` | No major matrix gap beyond keeping runtime lowering checks explicit |
| char | Implemented | **Targeted** — tokenizer / expression / receiver support is present; `Char` is included in receiver lookup support | **Broad** | **Thin dedicated backend/runtime coverage** | Yes | `char_basic`, `char_equality`, `char_ordering`, `char_receiver_method`, `char_in_template` | Success coverage is now artifact-shaped; dedicated failure/diagnostic coverage is still thin |
| named arguments (`parameter = value`, call-site `~` on the value expression) | Implemented | **Broad** — `src/compiler_frontend/ast/expressions/call_argument.rs`, `src/compiler_frontend/ast/expressions/function_calls.rs`, `src/compiler_frontend/ast/expressions/call_validation.rs`, `src/compiler_frontend/ast/expressions/struct_instance.rs`, `src/compiler_frontend/ast/expressions/tests/function_call_tests.rs` | **Broad** | **Function and constructor paths covered; host/builtin calls intentionally positional-only** | No | `function_call_named_args_*`, `struct_constructor_named_args_*`, `function_call_mutable_param_requires_explicit_tilde`, `function_call_tilde_on_immutable_place`, `function_call_tilde_on_non_place_expression` | No dedicated HTML / HTML-Wasm runtime case for named args; host calls and builtin member calls remain positional-only by design |

## Compiler-owned builtins and method-like surfaces

| Surface | Status | Parser / unit coverage | Integration coverage | Backend / runtime coverage | Golden-sensitive | Canonical cases | Explicit gaps |
| --- | --- | --- | --- | --- | --- | --- | --- |
| collection methods | Implemented | **Targeted** — `src/compiler_frontend/ast/field_access/collection_builtin.rs`, `src/compiler_frontend/ast/statements/tests/collections_tests.rs` | **Broad** | **JS** | Yes | `collection_methods_end_to_end`, `collection_methods_backend_contract`, `collection_get_out_of_bounds`, `collection_indexed_write_end_to_end`, `collection_mutating_method_requires_explicit_receiver_tilde` | HTML-Wasm-specific collection helper/runtime contract coverage is still light |
| error helper methods (`with_location`, `push_trace`, `bubble`) | Implemented (coverage thin) | **Targeted** — `src/compiler_frontend/ast/field_access/error_builtin.rs`, `src/compiler_frontend/builtins/error_type.rs` | **None found** | **JS host-function path exists, but end-to-end proof is missing here** | No | _No canonical integration case found in `tests/cases/manifest.toml`_ | This is the clearest immediate matrix gap |
| receiver methods | Implemented | **Targeted** — `src/compiler_frontend/ast/receiver_methods.rs`, `src/compiler_frontend/ast/module_ast/pass_function_signatures.rs` | **Broad** | **JS** | No | `receiver_method_basic_call`, `receiver_method_cross_file_rejected`, `receiver_method_field_name_conflict_rejected`, `receiver_method_free_function_call_rejected`, `receiver_method_exported_cross_file_success`, `struct_mutable_receiver_immutable_binding_rejected`, `receiver_this_not_first_parameter_rejected`, `receiver_unsupported_receiver_type_rejected` | HTML / HTML-Wasm specific runtime cases are still light |
| result suffix handling (`!`, fallback values, named handlers) | Implemented | **Targeted** — `src/compiler_frontend/ast/statements/result_handling.rs` | **Broad** | **Thin dedicated backend/runtime coverage** | No | `result_multi_bind_fallback`, `result_multi_bind_propagate`, `result_named_handler_*`, `result_handler_without_fallback_fallthrough_rejected`, `error_field_access_in_handler`, `adversarial_nested_named_error_handlers` | Add more emitted-output cases for propagation / fallback behavior |

## Reserved / deferred surfaces that are intentionally **not** counted as supported Alpha surface

These should stay visible so the matrix does not accidentally misclassify them as missing implementation work:

- trait / interface syntax reservation and related receiver syntax reservation
  - canonical diagnostics: `trait_declaration_reserved`, `trait_this_reserved`
- deferred choice sub-surfaces
  - canonical diagnostics: `choice_payload_decl_deferred`, `choice_default_decl_deferred`, `choice_tagged_decl_deferred`, `choice_constructor_use_deferred`
- deferred match sub-surfaces
  - canonical diagnostics: `diagnostic_match_deferred_capture_pattern`, `diagnostic_match_deferred_relational_pattern`, `diagnostic_match_wildcard_arm`

## Immediate gaps to keep visible

These are the clearest missing or thinly-covered areas exposed by the current matrix:

1. **Error helper methods**
   - implementation exists
   - canonical integration coverage is missing
   - should get both success and failure fixtures

2. **Backend-facing result handling**
   - propagation / fallback / named-handler semantics need more explicit emitted-output coverage

3. **Choice and match runtime coverage**
   - frontend coverage is good
   - backend/runtime-specific coverage is still lighter than frontend coverage

4. **Char diagnostics**
   - success paths are represented
   - dedicated failure diagnostics are still thin

## Maintenance rule

When a new language feature is added or an old one changes shape:

1. update this matrix
2. update `tests/cases/manifest.toml`
3. add or rewrite canonical cases instead of layering ad hoc temporary coverage
4. mark deferred syntax as **deferred**, not as a vague “missing test”
