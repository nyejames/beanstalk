//! Function call argument parsing regression tests.
//!
//! WHAT: validates positional and named argument parsing, call-access mode classification, and
//!       argument validation against signatures.
//! WHY: call parsing spans syntax, dispatch, and access-mode intent; focused tests prevent
//!      subtle regressions in how arguments are bound and passed.

use crate::compiler_frontend::ast::expressions::call_argument::{CallAccessMode, CallArgument};
use crate::compiler_frontend::ast::expressions::error::ExpressionParseError;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext, TopLevelDeclarationTable};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DiagnosticKind, DiagnosticPayload, InvalidCallShapeReason,
    SyntaxDiagnosticKind,
};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tests::parse_support::{
    parse_single_file_ast, parse_single_file_ast_diagnostic,
};
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind, TokenizerEntryMode};
use crate::compiler_frontend::type_coercion::compatibility::TypeCompatibilityCache;
use std::rc::Rc;
use std::sync::Arc;

fn parse_args(
    source: &str,
) -> Vec<crate::compiler_frontend::ast::expressions::call_argument::CallArgument> {
    let mut string_table = StringTable::new();
    let file_path = InternedPath::from_single_str("#page.bst", &mut string_table);
    let mut tokens = tokenize(
        source,
        &file_path,
        TokenizerEntryMode::SourceFile,
        &crate::compiler_frontend::style_directives::StyleDirectiveRegistry::built_ins(),
        &mut string_table,
        None,
    )
    .expect("tokenization should succeed");

    while tokens.current_token_kind() != &TokenKind::OpenParenthesis {
        tokens.advance();
    }

    let context = ScopeContext::new(
        ContextKind::Function,
        InternedPath::new(),
        Rc::new(TopLevelDeclarationTable::new(vec![])),
        Arc::new(ExternalPackageRegistry::new()),
        vec![],
        0,
    );

    let mut type_environment = TypeEnvironment::new();
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);
    parse_raw_call_args_for_test(&mut tokens, &context, &mut type_interner, &mut string_table)
        .expect("call arguments should parse")
}

/// Parses raw call arguments without parameter expectations for syntax-level tests.
///
/// WHAT: calls the production argument parser with no expectations so syntax-only tests are not
///       coupled to parameter counts or types.
/// WHY: keeping this local to the test module avoids a test-only production API while preserving
///      the exact behavior these syntax tests need.
fn parse_raw_call_args_for_test(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<Vec<CallArgument>, ExpressionParseError> {
    super::parse_call_arguments_inner(
        token_stream,
        context,
        type_interner,
        string_table,
        super::CallArgumentSyntaxContext::Ordinary,
        super::NamedArgumentSyntax::Supported { callee_name: None },
        None,
    )
}

fn parse_args_diagnostic(source: &str) -> CompilerDiagnostic {
    let mut string_table = StringTable::new();
    let file_path = InternedPath::from_single_str("#page.bst", &mut string_table);
    let mut tokens = tokenize(
        source,
        &file_path,
        TokenizerEntryMode::SourceFile,
        &crate::compiler_frontend::style_directives::StyleDirectiveRegistry::built_ins(),
        &mut string_table,
        None,
    )
    .expect("tokenization should succeed");

    while tokens.current_token_kind() != &TokenKind::OpenParenthesis {
        tokens.advance();
    }

    let context = ScopeContext::new(
        ContextKind::Function,
        InternedPath::new(),
        Rc::new(TopLevelDeclarationTable::new(vec![])),
        Arc::new(ExternalPackageRegistry::new()),
        vec![],
        0,
    );

    let mut type_environment = TypeEnvironment::new();
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);
    let error =
        parse_raw_call_args_for_test(&mut tokens, &context, &mut type_interner, &mut string_table)
            .expect_err("call arguments should fail");

    CompilerDiagnostic::from(error)
}

fn assert_invalid_call_shape(
    source: &str,
    reason_matches: impl FnOnce(&InvalidCallShapeReason) -> bool,
) {
    let diagnostic = parse_single_file_ast_diagnostic(source);

    let DiagnosticPayload::InvalidCallShape { reason, .. } = &diagnostic.payload else {
        panic!(
            "expected InvalidCallShape diagnostic, got {:?}",
            diagnostic.payload
        );
    };

    assert!(reason_matches(reason));
}

// ── Parser-level tests (syntax-only call arguments) ──────────────────────────

#[test]
fn parses_positional_and_named_call_arguments_with_equals_syntax() {
    let args = parse_args("sum(1, b = 2)");

    assert_eq!(args.len(), 2);
    assert!(args[0].target_param.is_none());
    assert_eq!(args[0].access_mode, CallAccessMode::Shared);
    assert!(args[1].target_param.is_some());
}

#[test]
fn parses_named_mutable_argument_on_value_side() {
    let args = parse_args("take(value = ~1)");

    assert_eq!(args.len(), 1);
    assert!(args[0].target_param.is_some());
    assert_eq!(args[0].access_mode, CallAccessMode::Mutable);
}

#[test]
fn call_argument_locations_keep_named_target_value_and_marker_distinct() {
    // `parameter = ~value` must keep the named-target token, the value expression and the
    // authored `~` marker at three distinct source locations so diagnostics can label the
    // source the author must change. The value here is a literal, not a binding.
    let args = parse_args("take(value = ~1)");

    assert_eq!(args.len(), 1);
    let argument = &args[0];
    assert_eq!(argument.access_mode, CallAccessMode::Mutable);

    let target_location = argument
        .target_location
        .clone()
        .expect("named argument should carry a target location");
    let marker_location = argument
        .marker_location
        .clone()
        .expect("authored ~ should carry a marker location");

    // `location` is the value-expression location, not the named-target token.
    assert_ne!(argument.location, target_location);
    assert_ne!(argument.location, marker_location);
    assert_ne!(target_location, marker_location);
}

#[test]
fn call_argument_marker_location_is_absent_without_authored_tilde() {
    let args = parse_args("take(value = 1)");

    assert_eq!(args.len(), 1);
    assert_eq!(args[0].access_mode, CallAccessMode::Shared);
    assert!(
        args[0].marker_location.is_none(),
        "absent ~ must not synthesize a marker location",
    );
}

#[test]
fn parses_all_named_arguments() {
    let args = parse_args("sum(a = 1, b = 2)");

    assert_eq!(args.len(), 2);
    assert!(args[0].target_param.is_some());
    assert!(args[1].target_param.is_some());
}

#[test]
fn parses_mixed_positional_then_named() {
    let args = parse_args("sum(1, b = 2, c = 3)");

    assert_eq!(args.len(), 3);
    assert!(args[0].target_param.is_none());
    assert!(args[1].target_param.is_some());
    assert!(args[2].target_param.is_some());
}

#[test]
fn rejects_mutable_marker_on_named_argument_target() {
    let diagnostic = parse_single_file_ast_diagnostic(
        r#"
take |value ~Int|:
;

value ~= 1
take(~value = value)
"#,
    );

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::UnexpectedToken { .. }
    ));
}

#[test]
fn rejects_positional_after_named() {
    assert_invalid_call_shape(
        r#"
sum |a Int, b Int| -> Int:
    return a + b
;

sum(a = 1, 2)
"#,
        |reason| matches!(reason, InvalidCallShapeReason::PositionalAfterNamed),
    );
}

#[test]
fn rejects_duplicate_named_target() {
    assert_invalid_call_shape(
        r#"
sum |a Int, b Int| -> Int:
    return a + b
;

sum(a = 1, a = 2)
"#,
        |reason| matches!(reason, InvalidCallShapeReason::DuplicateArgument { .. }),
    );
}

#[test]
fn rejects_unknown_named_parameter() {
    assert_invalid_call_shape(
        r#"
sum |a Int, b Int| -> Int:
    return a + b
;

sum(a = 1, unknown = 2)
"#,
        |reason| matches!(reason, InvalidCallShapeReason::NamedArgumentNotFound { .. }),
    );
}

#[test]
fn rejects_missing_required_parameter() {
    assert_invalid_call_shape(
        r#"
sum |a Int, b Int| -> Int:
    return a + b
;

sum(a = 1)
"#,
        |reason| matches!(reason, InvalidCallShapeReason::MissingArgument { .. }),
    );
}

#[test]
fn rejects_tilde_on_left_side_of_named_arg() {
    // ~name = value is explicitly rejected at the parse level
    let diagnostic = parse_args_diagnostic("take(~value = 1)");

    assert_eq!(
        diagnostic.kind,
        DiagnosticKind::Syntax(SyntaxDiagnosticKind::UnexpectedToken)
    );
    assert_eq!(diagnostic.kind.code(), "BST-SYNTAX-0002");
    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::UnexpectedToken {
            found: TokenKind::Mutable
        }
    ));
}

#[test]
fn rejects_missing_tilde_for_mutable_positional_parameter() {
    assert_invalid_call_shape(
        r#"
mutate |value ~Int|:
    value = 5
;

x ~= 1
mutate(x)
"#,
        |reason| matches!(reason, InvalidCallShapeReason::MutableAccessRequired { .. }),
    );
}

#[test]
fn accepts_fresh_rvalue_for_mutable_positional_parameter() {
    let _ = parse_single_file_ast(
        r#"
mutate |value ~Int|:
    value = value + 1
;

mutate(1 + 2)
"#,
    );
}

#[test]
fn accepts_fresh_rvalue_for_mutable_named_parameter() {
    let _ = parse_single_file_ast(
        r#"
mutate |value ~Int|:
    value = value + 1
;

mutate(value = 1 + 2)
"#,
    );
}

#[test]
fn accepts_fresh_template_for_mutable_parameter() {
    let _ = parse_single_file_ast(
        r#"
mutate |value ~String|:
    value = [:updated]
;

mutate([:content])
"#,
    );
}

#[test]
fn accepts_fresh_collection_for_mutable_parameter() {
    let _ = parse_single_file_ast(
        r#"
mutate |values ~{Int}|:
    ~values.push(4) catch:
    ;
;

mutate({1, 2, 3})
"#,
    );
}

#[test]
fn accepts_fresh_struct_constructor_for_mutable_parameter() {
    let _ = parse_single_file_ast(
        r#"
Item = |
    label String,
|

mutate |value ~Item|:
;

mutate(Item("x"))
"#,
    );
}

#[test]
fn rejects_tilde_on_immutable_place_argument() {
    assert_invalid_call_shape(
        r#"
mutate |value ~Int|:
    value = 5
;

x = 1
mutate(~x)
"#,
        |reason| {
            matches!(
                reason,
                InvalidCallShapeReason::MutableAccessOnImmutablePlace { .. }
            )
        },
    );
}

#[test]
fn rejects_tilde_on_non_place_argument_literal() {
    assert_invalid_call_shape(
        r#"
mutate |value ~Int|:
    value = 5
;

mutate(~12)
"#,
        |reason| {
            matches!(
                reason,
                InvalidCallShapeReason::MutableAccessOnNonPlace { .. }
            )
        },
    );
}

#[test]
fn rejects_immutable_place_without_tilde_for_mutable_parameter() {
    assert_invalid_call_shape(
        r#"
mutate |value ~Int|:
    value = 5
;

x = 1
mutate(x)
"#,
        |reason| {
            matches!(
                reason,
                InvalidCallShapeReason::ImmutablePlaceMutableAccessRequired { .. }
            )
        },
    );
}

#[test]
fn accepts_explicit_copy_as_fresh_mutable_argument() {
    let _ = parse_single_file_ast(
        r#"
mutate |value ~Int|:
    value = value + 1
;

source = 5
mutate(copy source)
"#,
    );
}

#[test]
fn rejects_tilde_on_explicit_copy_argument() {
    assert_invalid_call_shape(
        r#"
mutate |value ~Int|:
    value = 5
;

source = 5
mutate(~copy source)
"#,
        |reason| {
            matches!(
                reason,
                InvalidCallShapeReason::MutableAccessOnNonPlace { .. }
            )
        },
    );
}

#[test]
fn rejects_missing_tilde_for_mutable_named_parameter() {
    assert_invalid_call_shape(
        r#"
increment |value ~Int| -> Int:
    value = value + 1
    return value
;

x ~= 10
result = increment(value = x)
io.line([: [result]])
"#,
        |reason| matches!(reason, InvalidCallShapeReason::MutableAccessRequired { .. }),
    );
}

#[test]
fn duplicate_named_parameter_uses_canonical_diagnostic_text() {
    assert_invalid_call_shape(
        r#"
sum |a Int, b Int| -> Int:
    return a + b
;

sum(a = 1, a = 2)
"#,
        |reason| matches!(reason, InvalidCallShapeReason::DuplicateArgument { .. }),
    );
}

#[test]
fn unknown_named_parameter_lists_known_parameter_hint() {
    assert_invalid_call_shape(
        r#"
sum |a Int, b Int| -> Int:
    return a + b
;

sum(a = 1, typo = 2)
"#,
        |reason| matches!(reason, InvalidCallShapeReason::NamedArgumentNotFound { .. }),
    );
}

#[test]
fn mutable_marker_on_immutable_argument_uses_authored_marker_location() {
    // `mutate(~x)` places the authored `~` at column 8 and the value `x` at column 9.
    // The primary label must point at the marker, because the authored `~` is the source the
    // author must remove or repair.
    let diagnostic = parse_single_file_ast_diagnostic(
        r#"
mutate |value ~Int|:
    value = 5
;

x = 1
mutate(~x)
"#,
    );

    let DiagnosticPayload::InvalidCallShape { reason, .. } = &diagnostic.payload else {
        panic!(
            "expected InvalidCallShape diagnostic, got {:?}",
            diagnostic.payload
        );
    };
    assert!(
        matches!(
            reason,
            InvalidCallShapeReason::MutableAccessOnImmutablePlace { .. }
        ),
        "expected MutableAccessOnImmutablePlace, got {reason:?}"
    );

    let location = &diagnostic.primary_location;
    assert_eq!(
        location.start_pos.line_number, 6,
        "marker is on the call line"
    );
    assert_eq!(
        location.start_pos.char_column, 8,
        "marker `~` sits at column 8"
    );
    assert_ne!(
        location.start_pos.char_column, 9,
        "must not point at the value `x` at column 9"
    );
}

#[test]
fn mutable_marker_on_fresh_value_uses_authored_marker_location() {
    // `mutate(~12)` places the authored `~` at column 8 and the fresh literal at column 9.
    // The primary label must point at the marker, since the plain fresh value is valid and the
    // authored `~` is the mistake.
    let diagnostic = parse_single_file_ast_diagnostic(
        r#"
mutate |value ~Int|:
    value = 5
;

mutate(~12)
"#,
    );

    let DiagnosticPayload::InvalidCallShape { reason, .. } = &diagnostic.payload else {
        panic!(
            "expected InvalidCallShape diagnostic, got {:?}",
            diagnostic.payload
        );
    };
    assert!(
        matches!(
            reason,
            InvalidCallShapeReason::MutableAccessOnNonPlace { .. }
        ),
        "expected MutableAccessOnNonPlace, got {reason:?}"
    );

    let location = &diagnostic.primary_location;
    assert_eq!(
        location.start_pos.line_number, 5,
        "marker is on the call line"
    );
    assert_eq!(
        location.start_pos.char_column, 8,
        "marker `~` sits at column 8"
    );
    assert_ne!(
        location.start_pos.char_column, 9,
        "must not point at the fresh value at column 9"
    );
}

#[test]
fn unmarked_immutable_argument_uses_value_expression_location() {
    // `mutate(x)` has no authored `~`, so the value expression `x` is the call-site source the
    // author must change. The primary label must point at the value, at column 8.
    let diagnostic = parse_single_file_ast_diagnostic(
        r#"
mutate |value ~Int|:
    value = 5
;

x = 1
mutate(x)
"#,
    );

    let DiagnosticPayload::InvalidCallShape { reason, .. } = &diagnostic.payload else {
        panic!(
            "expected InvalidCallShape diagnostic, got {:?}",
            diagnostic.payload
        );
    };
    assert!(
        matches!(
            reason,
            InvalidCallShapeReason::ImmutablePlaceMutableAccessRequired { .. }
        ),
        "expected ImmutablePlaceMutableAccessRequired, got {reason:?}"
    );

    let location = &diagnostic.primary_location;
    assert_eq!(
        location.start_pos.line_number, 6,
        "value is on the call line"
    );
    assert_eq!(
        location.start_pos.char_column, 8,
        "value expression `x` sits at column 8"
    );
}
