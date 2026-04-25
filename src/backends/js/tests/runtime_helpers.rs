//! Runtime helper source contract tests for JavaScript output.

use super::support::*;
use crate::compiler_frontend::hir::blocks::HirBlock;
use crate::compiler_frontend::hir::expressions::{HirExpressionKind, ValueKind};
use crate::compiler_frontend::hir::functions::HirFunction;
use crate::compiler_frontend::hir::ids::{BlockId, FunctionId, LocalId, RegionId};
use crate::compiler_frontend::hir::statements::HirStatementKind;
use crate::compiler_frontend::hir::terminators::HirTerminator;

// Runtime helper contract tests
// ---------------------------------------------------------------------------

/// Returns the source text of a single JS helper function, bounded by the next
/// `function ` declaration or end of file. This keeps assertions focused on one
/// helper at a time instead of the whole prelude.
fn helper_source<'a>(source: &'a str, name: &str) -> &'a str {
    let prefix = format!("function {name}(");
    let start = source
        .find(&prefix)
        .unwrap_or_else(|| panic!("helper {name} must be present in emitted JS"));
    let rest = &source[start..];
    // Find the next top-level function declaration after this one.
    let end = rest[1..]
        .find("function ")
        .map(|i| i + 1)
        .unwrap_or(rest.len());
    &rest[..end]
}

/// Verifies that `__bs_result_propagate` unwraps ok values and throws a structured
/// sentinel for err values. [result]
#[test]
fn result_propagate_unwraps_ok_and_throws_sentinel_for_err() {
    let source = lower_minimal_module("main");
    let propagate = helper_source(&source, "__bs_result_propagate");

    assert!(
        propagate.contains("result.tag === \"ok\"") && propagate.contains("return result.value;"),
        "__bs_result_propagate must return the ok value"
    );
    assert!(
        propagate.contains("throw { __bs_result_propagate: true, value: result.value }")
            || propagate.contains("throw { __bs_result_propagate: true, value: result.value };"),
        "__bs_result_propagate must throw a structured sentinel for err"
    );
}

/// Verifies that `__bs_result_fallback` returns the ok value directly without calling
/// the fallback function. [result]
#[test]
fn result_fallback_returns_ok_value_without_calling_fallback() {
    let source = lower_minimal_module("main");
    let fallback = helper_source(&source, "__bs_result_fallback");

    assert!(
        fallback.contains("result.tag === \"ok\"") && fallback.contains("return result.value;"),
        "__bs_result_fallback must return the ok value directly"
    );
    // The fallback callback should only be invoked in the err branch.
    let ok_pos = fallback
        .find("if (result && result.tag === \"ok\")")
        .expect("ok branch must exist");
    let ok_branch_end = fallback[ok_pos..]
        .find("return result.value;")
        .map(|i| ok_pos + i)
        .expect("ok return must exist");
    let ok_branch = &fallback[ok_pos..ok_branch_end];
    assert!(
        !ok_branch.contains("fallback()"),
        "ok branch must not call the fallback callback"
    );
}

/// Verifies that `__bs_result_fallback` invokes the fallback callback for err carriers. [result]
#[test]
fn result_fallback_invokes_callback_for_err() {
    let source = lower_minimal_module("main");
    let fallback = helper_source(&source, "__bs_result_fallback");

    assert!(
        fallback.contains("result.tag === \"err\"") && fallback.contains("return fallback();"),
        "__bs_result_fallback must invoke fallback() for err carriers"
    );
}

/// Verifies that `__bs_clone_value` deep-copies arrays via `.map(__bs_clone_value)`. [clone]
#[test]
fn clone_value_uses_map_for_arrays() {
    let source = lower_minimal_module("main");
    let clone = helper_source(&source, "__bs_clone_value");

    assert!(
        clone.contains("Array.isArray(value)") && clone.contains("value.map(__bs_clone_value)"),
        "__bs_clone_value must deep-copy arrays using .map(__bs_clone_value)"
    );
}

/// Verifies that `__bs_clone_value` deep-copies plain objects key-by-key. [clone]
#[test]
fn clone_value_iterates_object_keys() {
    let source = lower_minimal_module("main");
    let clone = helper_source(&source, "__bs_clone_value");

    assert!(
        clone.contains("Object.keys(value)")
            && clone.contains("result[key] = __bs_clone_value(value[key])"),
        "__bs_clone_value must deep-copy objects by iterating Object.keys"
    );
}

/// Verifies that `__bs_collection_get` returns an err carrier for non-array inputs. [collection]
#[test]
fn collection_get_returns_err_for_non_array() {
    let source = lower_minimal_module("main");
    let get = helper_source(&source, "__bs_collection_get");

    assert!(
        get.contains("!Array.isArray(collection)") && get.contains("{ tag: \"err\", value: err }"),
        "__bs_collection_get must return a Result-typed err for non-array inputs"
    );
}

/// Verifies that `__bs_collection_get` returns an err carrier for out-of-bounds indices. [collection]
#[test]
fn collection_get_returns_err_for_out_of_bounds() {
    let source = lower_minimal_module("main");
    let get = helper_source(&source, "__bs_collection_get");

    assert!(
        get.contains("index < 0 || index >= collection.length")
            && get.contains("{ tag: \"err\", value: err }"),
        "__bs_collection_get must return a Result-typed err for out-of-bounds indices"
    );
}

/// Verifies that `__bs_collection_get` returns an ok carrier for valid inputs. [collection]
#[test]
fn collection_get_returns_ok_for_valid_index() {
    let source = lower_minimal_module("main");
    let get = helper_source(&source, "__bs_collection_get");

    assert!(
        get.contains("{ tag: \"ok\", value: collection[index] }"),
        "__bs_collection_get must return a Result-typed ok for valid indices"
    );
}

/// Verifies that `__bs_collection_push` returns an err carrier for non-array inputs. [collection]
#[test]
fn collection_push_returns_err_for_non_array() {
    let source = lower_minimal_module("main");
    let push = helper_source(&source, "__bs_collection_push");

    assert!(
        push.contains("!Array.isArray(collection)")
            && push.contains("{ tag: \"err\", value: err }"),
        "__bs_collection_push must return a Result-typed err for non-array inputs"
    );
}

/// Verifies that `__bs_collection_push` returns an ok carrier for valid inputs. [collection]
#[test]
fn collection_push_returns_ok_for_valid_array() {
    let source = lower_minimal_module("main");
    let push = helper_source(&source, "__bs_collection_push");

    assert!(
        push.contains("collection.push(value)") && push.contains("{ tag: \"ok\", value: null }"),
        "__bs_collection_push must return a Result-typed ok after pushing"
    );
}

/// Verifies that `__bs_collection_remove` returns an err carrier for non-array inputs. [collection]
#[test]
fn collection_remove_returns_err_for_non_array() {
    let source = lower_minimal_module("main");
    let remove = helper_source(&source, "__bs_collection_remove");

    assert!(
        remove.contains("!Array.isArray(collection)")
            && remove.contains("{ tag: \"err\", value: err }"),
        "__bs_collection_remove must return a Result-typed err for non-array inputs"
    );
}

/// Verifies that `__bs_collection_remove` returns an err carrier for out-of-bounds indices. [collection]
#[test]
fn collection_remove_returns_err_for_out_of_bounds() {
    let source = lower_minimal_module("main");
    let remove = helper_source(&source, "__bs_collection_remove");

    assert!(
        remove.contains("index < 0 || index >= collection.length")
            && remove.contains("{ tag: \"err\", value: err }"),
        "__bs_collection_remove must return a Result-typed err for out-of-bounds indices"
    );
}

/// Verifies that `__bs_collection_remove` returns an ok carrier for valid inputs. [collection]
#[test]
fn collection_remove_returns_ok_for_valid_index() {
    let source = lower_minimal_module("main");
    let remove = helper_source(&source, "__bs_collection_remove");

    assert!(
        remove.contains("collection.splice(index, 1)")
            && remove.contains("{ tag: \"ok\", value: null }"),
        "__bs_collection_remove must return a Result-typed ok after removing"
    );
}

/// Verifies that `__bs_collection_length` returns an err carrier for non-array inputs. [collection]
#[test]
fn collection_length_returns_err_for_non_array() {
    let source = lower_minimal_module("main");
    let length = helper_source(&source, "__bs_collection_length");

    assert!(
        length.contains("!Array.isArray(collection)")
            && length.contains("{ tag: \"err\", value: err }"),
        "__bs_collection_length must return a Result-typed err for non-array inputs"
    );
}

/// Verifies that `__bs_collection_length` returns an ok carrier for valid inputs. [collection]
#[test]
fn collection_length_returns_ok_for_valid_array() {
    let source = lower_minimal_module("main");
    let length = helper_source(&source, "__bs_collection_length");

    assert!(
        length.contains("{ tag: \"ok\", value: collection.length }"),
        "__bs_collection_length must return a Result-typed ok with the length"
    );
}

/// Verifies that emitted `__bs_collection_push` calls are wrapped with `__bs_result_propagate`. [collection]
#[test]
fn collection_push_call_is_wrapped_with_result_propagate() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    let push_path = InternedPath::from_single_str("__bs_collection_push", &mut string_table);

    let call_statement = statement(
        1,
        HirStatementKind::Call {
            target: CallTarget::ExternalFunction(push_path),
            args: vec![
                expression(
                    1,
                    HirExpressionKind::Collection(vec![]),
                    types.int,
                    RegionId(0),
                    ValueKind::RValue,
                ),
                int_expression(2, 42, types.int, RegionId(0)),
            ],
            result: None,
        },
        1,
    );

    let block = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![],
        statements: vec![call_statement],
        terminator: HirTerminator::Return(unit_expression(3, types.unit, RegionId(0))),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.unit,
        return_aliases: vec![],
    };

    let module = build_module(
        &mut string_table,
        "main",
        vec![block],
        function,
        &[],
        type_context,
    );

    let output = lower_hir_to_js(
        &module,
        &BorrowCheckReport::default(),
        &string_table,
        default_config(),
    )
    .expect("JS lowering should succeed");

    assert!(
        output
            .source
            .contains("__bs_result_propagate(__bs_collection_push("),
        "__bs_collection_push host call must be wrapped with __bs_result_propagate"
    );
}

/// Verifies that emitted `__bs_collection_remove` calls are wrapped with `__bs_result_propagate`. [collection]
#[test]
fn collection_remove_call_is_wrapped_with_result_propagate() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    let remove_path = InternedPath::from_single_str("__bs_collection_remove", &mut string_table);

    let call_statement = statement(
        1,
        HirStatementKind::Call {
            target: CallTarget::ExternalFunction(remove_path),
            args: vec![
                expression(
                    1,
                    HirExpressionKind::Collection(vec![]),
                    types.int,
                    RegionId(0),
                    ValueKind::RValue,
                ),
                int_expression(2, 0, types.int, RegionId(0)),
            ],
            result: None,
        },
        1,
    );

    let block = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![],
        statements: vec![call_statement],
        terminator: HirTerminator::Return(unit_expression(3, types.unit, RegionId(0))),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.unit,
        return_aliases: vec![],
    };

    let module = build_module(
        &mut string_table,
        "main",
        vec![block],
        function,
        &[],
        type_context,
    );

    let output = lower_hir_to_js(
        &module,
        &BorrowCheckReport::default(),
        &string_table,
        default_config(),
    )
    .expect("JS lowering should succeed");

    assert!(
        output
            .source
            .contains("__bs_result_propagate(__bs_collection_remove("),
        "__bs_collection_remove host call must be wrapped with __bs_result_propagate"
    );
}

/// Verifies that emitted `__bs_collection_length` calls are wrapped with `__bs_result_propagate`. [collection]
#[test]
fn collection_length_call_is_wrapped_with_result_propagate() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    let length_path = InternedPath::from_single_str("__bs_collection_length", &mut string_table);

    let call_statement = statement(
        1,
        HirStatementKind::Call {
            target: CallTarget::ExternalFunction(length_path),
            args: vec![expression(
                1,
                HirExpressionKind::Collection(vec![]),
                types.int,
                RegionId(0),
                ValueKind::RValue,
            )],
            result: Some(LocalId(0)),
        },
        1,
    );

    let block = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![local(0, types.int, RegionId(0))],
        statements: vec![call_statement],
        terminator: HirTerminator::Return(unit_expression(2, types.unit, RegionId(0))),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.unit,
        return_aliases: vec![],
    };

    let module = build_module(
        &mut string_table,
        "main",
        vec![block],
        function,
        &[(LocalId(0), "len")],
        type_context,
    );

    let output = lower_hir_to_js(
        &module,
        &BorrowCheckReport::default(),
        &string_table,
        default_config(),
    )
    .expect("JS lowering should succeed");

    assert!(
        output
            .source
            .contains("__bs_result_propagate(__bs_collection_length("),
        "__bs_collection_length host call must be wrapped with __bs_result_propagate"
    );
}

/// Verifies that `__bs_cast_int` rejects non-numeric strings with a Parse error. [cast]
#[test]
fn cast_int_rejects_non_numeric_string() {
    let source = lower_minimal_module("main");
    let cast = helper_source(&source, "__bs_cast_int");

    assert!(
        cast.contains("Cannot parse Int from text") && cast.contains("{ tag: \"err\""),
        "__bs_cast_int must return a Parse err for non-numeric strings"
    );
}

/// Verifies that `__bs_cast_int` accepts integer strings via parseInt. [cast]
#[test]
fn cast_int_accepts_integer_string() {
    let source = lower_minimal_module("main");
    let cast = helper_source(&source, "__bs_cast_int");

    assert!(
        cast.contains("Number.parseInt(normalized, 10)") && cast.contains("{ tag: \"ok\""),
        "__bs_cast_int must parse integer strings and return ok"
    );
}

/// Verifies that `__bs_cast_float` rejects invalid strings with a Parse error. [cast]
#[test]
fn cast_float_rejects_invalid_string() {
    let source = lower_minimal_module("main");
    let cast = helper_source(&source, "__bs_cast_float");

    assert!(
        cast.contains("Cannot parse Float from text") && cast.contains("{ tag: \"err\""),
        "__bs_cast_float must return a Parse err for invalid strings"
    );
}

/// Verifies that `__bs_error_bubble` normalizes the file path and builds a trace frame. [error]
#[test]
fn error_bubble_normalizes_file_and_builds_trace() {
    let source = lower_minimal_module("main");
    let bubble = helper_source(&source, "__bs_error_bubble");

    assert!(
        bubble.contains("__bs_error_normalize_file(file)")
            && bubble.contains("const frame = { function: safeFunction, location }")
            && bubble.contains("__bs_error_push_trace"),
        "__bs_error_bubble must normalize file paths and push a trace frame"
    );
}

// ---------------------------------------------------------------------------
