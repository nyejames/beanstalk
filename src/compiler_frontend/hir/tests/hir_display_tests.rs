//! HIR display regression tests.
//!
//! WHAT: pins debug-display rendering for HIR-only constructs.
//! WHY: display output is used while auditing lowering and borrow behavior, so embedded message
//! text must remain unambiguous when it contains quotes or control characters.

use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::hir::expressions::{HirExpression, HirExpressionKind, ValueKind};
use crate::compiler_frontend::hir::hir_display::{HirDisplayContext, HirDisplayOptions};
use crate::compiler_frontend::hir::ids::{HirNodeId, HirValueId, LocalId, RegionId};
use crate::compiler_frontend::hir::numeric::NumericFailureMode;
use crate::compiler_frontend::hir::statements::{HirStatement, HirStatementKind};
use crate::compiler_frontend::hir::terminators::HirTerminator;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

fn render_collection_type(type_environment: &TypeEnvironment, type_id: TypeId) -> String {
    let string_table = StringTable::new();
    let display = HirDisplayContext::new(&string_table).with_type_environment(type_environment);

    let expression = HirExpression {
        id: HirValueId(0),
        kind: HirExpressionKind::Collection(vec![]),
        ty: type_id,
        value_kind: ValueKind::RValue,
        region: RegionId(0),
    };

    display.render_expression(&expression)
}

#[test]
fn assertion_failure_message_display_escapes_debug_text() {
    let string_table = StringTable::new();
    let display = HirDisplayContext::new(&string_table);

    let rendered = display.render_terminator(&HirTerminator::AssertFailure {
        message: Some("quoted \"message\"\nnext".to_owned()),
    });

    assert_eq!(rendered, "assert_failure \"quoted \\\"message\\\"\\nnext\"");
}

#[test]
fn runtime_failure_message_display_escapes_debug_text() {
    let string_table = StringTable::new();
    let display = HirDisplayContext::new(&string_table);

    let rendered = display.render_terminator(&HirTerminator::RuntimeFailure {
        message: "quoted \"message\"\nnext".to_owned(),
    });

    assert_eq!(
        rendered,
        "runtime_failure \"quoted \\\"message\\\"\\nnext\""
    );
}

#[test]
fn hir_display_renders_fixed_collection_type() {
    let mut type_environment = TypeEnvironment::new();
    let int_type = type_environment.builtins().int;
    let fixed_collection = type_environment.intern_collection(int_type, Some(4));

    let rendered = render_collection_type(&type_environment, fixed_collection);
    assert!(
        rendered.contains("{4 Int}"),
        "expected HIR display to contain '{{4 Int}}', got: {}",
        rendered
    );
}

#[test]
fn hir_display_renders_growable_collection_type() {
    let mut type_environment = TypeEnvironment::new();
    let int_type = type_environment.builtins().int;
    let growable_collection = type_environment.intern_collection(int_type, None);

    let rendered = render_collection_type(&type_environment, growable_collection);
    assert!(
        rendered.contains("{Int}"),
        "expected HIR display to contain '{{Int}}', got: {}",
        rendered
    );
}

#[test]
fn hir_display_renders_nested_fixed_collection_type() {
    let mut type_environment = TypeEnvironment::new();
    let int_type = type_environment.builtins().int;
    let inner_fixed = type_environment.intern_collection(int_type, Some(8));
    let outer_fixed = type_environment.intern_collection(inner_fixed, Some(4));

    let rendered = render_collection_type(&type_environment, outer_fixed);
    assert!(
        rendered.contains("{4 {8 Int}}"),
        "expected HIR display to contain '{{4 {{8 Int}}}}', got: {}",
        rendered
    );
}

fn float_expression(value: f64) -> HirExpression {
    HirExpression {
        id: HirValueId(0),
        kind: HirExpressionKind::Float(value),
        ty: TypeId(0),
        value_kind: ValueKind::RValue,
        region: RegionId(0),
    }
}

fn float_statement(kind: HirStatementKind) -> HirStatement {
    HirStatement {
        id: HirNodeId(0),
        kind,
        location: SourceLocation::default(),
    }
}

fn terse_display_context(string_table: &StringTable) -> HirDisplayContext<'_> {
    HirDisplayContext::new(string_table).with_options(HirDisplayOptions {
        include_ids: false,
        include_types: false,
        include_value_kinds: false,
        include_regions: false,
        multiline_match_arms: false,
    })
}

#[test]
fn hir_display_renders_format_float_trap() {
    let string_table = StringTable::new();
    let display = terse_display_context(&string_table);

    let rendered = display.render_statement(&float_statement(HirStatementKind::FormatFloat {
        source: float_expression(1.5),
        failure_mode: NumericFailureMode::Trap,
        result: LocalId(9000),
    }));

    assert_eq!(rendered, "l9000 = format_float_trap(1.5)");
}

#[test]
fn hir_display_renders_format_float_return_error() {
    let string_table = StringTable::new();
    let display = terse_display_context(&string_table);

    let rendered = display.render_statement(&float_statement(HirStatementKind::FormatFloat {
        source: float_expression(-0.25),
        failure_mode: NumericFailureMode::ReturnError,
        result: LocalId(9001),
    }));

    assert_eq!(rendered, "l9001 = format_float_err(-0.25)");
}

#[test]
fn hir_display_renders_validate_float_trap() {
    let string_table = StringTable::new();
    let display = terse_display_context(&string_table);

    let rendered = display.render_statement(&float_statement(HirStatementKind::ValidateFloat {
        source: float_expression(2.5),
        failure_mode: NumericFailureMode::Trap,
        result: LocalId(9002),
    }));

    assert_eq!(rendered, "l9002 = validate_float_trap(2.5)");
}

#[test]
fn hir_display_renders_validate_float_return_error() {
    let string_table = StringTable::new();
    let display = terse_display_context(&string_table);

    let rendered = display.render_statement(&float_statement(HirStatementKind::ValidateFloat {
        source: float_expression(0.0),
        failure_mode: NumericFailureMode::ReturnError,
        result: LocalId(9003),
    }));

    assert_eq!(rendered, "l9003 = validate_float_err(0)");
}
