//! Runtime prelude presence and ordering tests for JavaScript output.

use super::support::*;

// Prelude helper presence tests [binding] [alias] [computed] [clone]
// ---------------------------------------------------------------------------

/// Verifies that all six binding helpers are present in the emitted prelude. [binding]
#[test]
fn runtime_prelude_contains_all_binding_helpers() {
    let source = lower_minimal_module("main");

    assert!(
        source.contains("function __bs_is_ref("),
        "prelude must contain __bs_is_ref"
    );
    assert!(
        source.contains("function __bs_binding("),
        "prelude must contain __bs_binding"
    );
    assert!(
        source.contains("function __bs_param_binding("),
        "prelude must contain __bs_param_binding"
    );
    assert!(
        source.contains("function __bs_resolve("),
        "prelude must contain __bs_resolve"
    );
    assert!(
        source.contains("function __bs_read("),
        "prelude must contain __bs_read"
    );
    assert!(
        source.contains("function __bs_write("),
        "prelude must contain __bs_write"
    );
}

/// Verifies that both alias helpers are present in the emitted prelude. [alias]
#[test]
fn runtime_prelude_contains_alias_helpers() {
    let source = lower_minimal_module("main");

    assert!(
        source.contains("function __bs_assign_borrow("),
        "prelude must contain __bs_assign_borrow"
    );
    assert!(
        source.contains("function __bs_assign_value("),
        "prelude must contain __bs_assign_value"
    );
}

/// Verifies that both computed-place helpers are present in the emitted prelude. [computed]
#[test]
fn runtime_prelude_contains_computed_place_helpers() {
    let source = lower_minimal_module("main");

    assert!(
        source.contains("function __bs_field("),
        "prelude must contain __bs_field"
    );
    assert!(
        source.contains("function __bs_index("),
        "prelude must contain __bs_index"
    );
}

/// Verifies that the deep-copy helper is present in the emitted prelude. [clone]
#[test]
fn runtime_prelude_contains_clone_helper() {
    let source = lower_minimal_module("main");

    assert!(
        source.contains("function __bs_clone_value("),
        "prelude must contain __bs_clone_value"
    );
}

// ---------------------------------------------------------------------------
// Prelude ordering tests [prelude-order]
// ---------------------------------------------------------------------------

/// Verifies that binding helpers precede alias helpers in emitted output. [prelude-order]
#[test]
fn binding_helpers_appear_before_alias_helpers() {
    let source = lower_minimal_module("main");

    let binding_pos = source
        .find("function __bs_binding(")
        .expect("__bs_binding must be present");
    let alias_pos = source
        .find("function __bs_assign_borrow(")
        .expect("__bs_assign_borrow must be present");

    assert!(
        binding_pos < alias_pos,
        "binding helpers must appear before alias helpers in emitted JS"
    );
}

/// Verifies that alias helpers precede computed-place helpers in emitted output. [prelude-order]
#[test]
fn alias_helpers_appear_before_computed_place_helpers() {
    let source = lower_minimal_module("main");

    let alias_pos = source
        .find("function __bs_assign_value(")
        .expect("__bs_assign_value must be present");
    let computed_pos = source
        .find("function __bs_field(")
        .expect("__bs_field must be present");

    assert!(
        alias_pos < computed_pos,
        "alias helpers must appear before computed-place helpers in emitted JS"
    );
}

/// Verifies that computed-place helpers precede the clone helper in emitted output. [prelude-order]
#[test]
fn computed_place_helpers_appear_before_clone_helper() {
    let source = lower_minimal_module("main");

    let computed_pos = source
        .find("function __bs_index(")
        .expect("__bs_index must be present");
    let clone_pos = source
        .find("function __bs_clone_value(")
        .expect("__bs_clone_value must be present");

    assert!(
        computed_pos < clone_pos,
        "computed-place helpers must appear before the clone helper in emitted JS"
    );
}

// ---------------------------------------------------------------------------
// Prelude helper group presence tests [prelude-presence]
// ---------------------------------------------------------------------------

/// Verifies that the error helper group is present in the emitted prelude. [prelude-presence]
#[test]
fn runtime_prelude_contains_error_helpers() {
    let source = lower_minimal_module("main");

    assert!(
        source.contains("function __bs_make_error("),
        "prelude must contain __bs_make_error"
    );
}

/// Verifies that the result helper group is present in the emitted prelude. [prelude-presence]
#[test]
fn runtime_prelude_contains_result_helpers() {
    let source = lower_minimal_module("main");

    assert!(
        source.contains("function __bs_result_propagate("),
        "prelude must contain __bs_result_propagate"
    );
}

/// Verifies that the string helper group is present in the emitted prelude. [prelude-presence]
#[test]
fn runtime_prelude_contains_string_helpers() {
    let source = lower_minimal_module("main");

    assert!(
        source.contains("function __bs_value_to_string("),
        "prelude must contain __bs_value_to_string"
    );
    assert!(
        source.contains("function __bs_io("),
        "prelude must contain __bs_io"
    );
}

/// Verifies that the collection helper group is present in the emitted prelude. [prelude-presence]
#[test]
fn runtime_prelude_contains_collection_helpers() {
    let source = lower_minimal_module("main");

    assert!(
        source.contains("function __bs_collection_get("),
        "prelude must contain __bs_collection_get"
    );
}

/// Verifies that the cast helper group is present in the emitted prelude. [prelude-presence]
#[test]
fn runtime_prelude_contains_cast_helpers() {
    let source = lower_minimal_module("main");

    assert!(
        source.contains("function __bs_cast_int("),
        "prelude must contain __bs_cast_int"
    );
    assert!(
        source.contains("function __bs_cast_float("),
        "prelude must contain __bs_cast_float"
    );
}

// ---------------------------------------------------------------------------
