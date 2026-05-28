//! Variable declaration parsing regression tests.
//!
//! WHAT: validates mutability, explicit types, and named-type annotations in declarations.
//! WHY: declaration parsing is the entrypoint for most AST values and must preserve type intent.

use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::compiler_messages::{
    DiagnosticKind, DiagnosticPayload, ReservedNameOwner, RuleDiagnosticKind, TypeMismatchContext,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::tests::test_support::{
    parse_single_file_ast, parse_single_file_ast_diagnostic, start_function_body,
};
use crate::compiler_frontend::value_mode::ValueMode;

// --------------------------
//  Basic declarations
// --------------------------

#[test]
fn parses_mutable_and_explicitly_typed_declarations() {
    let (ast, string_table) = parse_single_file_ast("count ~= 1\nname String = \"Ada\"\n");

    let body = start_function_body(&ast, &string_table);

    let NodeKind::VariableDeclaration(count_decl) = &body[0].kind else {
        panic!("expected mutable declaration");
    };
    assert_eq!(count_decl.value.diagnostic_type, DataType::Int);
    assert_eq!(count_decl.value.value_mode, ValueMode::MutableOwned);

    let NodeKind::VariableDeclaration(name_decl) = &body[1].kind else {
        panic!("expected explicit string declaration");
    };
    assert_eq!(name_decl.value.diagnostic_type, DataType::StringSlice);
    assert!(matches!(
        name_decl.value.kind,
        ExpressionKind::StringSlice(..)
    ));
}

#[test]
fn resolves_named_type_annotations_against_prior_structs() {
    let (ast, string_table) =
        parse_single_file_ast("Point = |\n    x Int,\n|\n\norigin Point = Point(0)\n");

    let body = start_function_body(&ast, &string_table);

    let NodeKind::VariableDeclaration(origin_decl) = &body[0].kind else {
        panic!("expected typed declaration");
    };
    assert!(matches!(
        origin_decl.value.diagnostic_type,
        DataType::Struct {
            const_record: false,
            ..
        }
    ));
    assert!(matches!(
        origin_decl.value.kind,
        ExpressionKind::StructInstance(..)
    ));
}

// --------------------------
//  Reserved name rejections
// --------------------------

#[test]
fn rejects_user_declarations_named_error() {
    let diagnostic = parse_single_file_ast_diagnostic("Error = 1\n");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::ReservedNameCollision {
            reserved_by: ReservedNameOwner::BuiltinType,
            ..
        }
    ));
}

#[test]
fn rejects_struct_redefinition_of_reserved_error_symbol() {
    let diagnostic = parse_single_file_ast_diagnostic("Error = |\n    message String,\n|\n");

    assert!(matches!(
        diagnostic.kind,
        DiagnosticKind::Rule(RuleDiagnosticKind::ReservedBuiltinName)
    ));
    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::UnusedName { .. }
    ));
}

#[test]
fn allows_user_declarations_named_old_error_family_symbols() {
    parse_single_file_ast(
        "ErrorKind = |\n    message String,\n|\n\n\
         ErrorLocation = |\n    line Int,\n|\n\n\
         StackFrame = |\n    name String,\n|\n\n\
         kind = ErrorKind(\"custom\")\n\
         location = ErrorLocation(12)\n\
         frame = StackFrame(\"main\")\n",
    );
}

#[test]
fn rejects_keyword_shadow_variable_declarations() {
    let diagnostic = parse_single_file_ast_diagnostic("_true = 1\n");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::ReservedNameCollision {
            reserved_by: ReservedNameOwner::Keyword,
            ..
        }
    ));
}

// --------------------------
//  Type mismatch diagnostics
// --------------------------

#[test]
fn rejects_initializer_type_mismatch_with_target_and_value_details() {
    let diagnostic = parse_single_file_ast_diagnostic("result Float = true\n");

    let DiagnosticPayload::TypeMismatch {
        expected,
        found,
        context,
    } = &diagnostic.payload
    else {
        panic!("expected typed TypeMismatch diagnostic payload");
    };

    assert_eq!(*context, TypeMismatchContext::Declaration);
    assert_eq!(expected.0, 2);
    assert_eq!(found.0, 0);
}

#[test]
fn declaration_int_context_reports_targeted_guidance_for_regular_division() {
    assert_declaration_type_mismatch("result Int = 5 / 2\n");
}

#[test]
fn rejects_multiline_regular_division_with_operator_on_next_line() {
    assert_declaration_type_mismatch("result Int = 5\n / 2\n");
}

#[test]
fn rejects_multiline_regular_division_with_operator_at_end_of_line() {
    assert_declaration_type_mismatch("result Int = 5 /\n 2\n");
}

fn assert_declaration_type_mismatch(source: &str) {
    let diagnostic = parse_single_file_ast_diagnostic(source);

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::TypeMismatch {
            context: TypeMismatchContext::Declaration,
            ..
        }
    ));
}
