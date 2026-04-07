use crate::compiler_frontend::ast::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::ast::expressions::call_argument::CallAccessMode;
use crate::compiler_frontend::ast::expressions::function_calls::parse_call_arguments;
use crate::compiler_frontend::ast::test_support::parse_single_file_ast_error;
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::newline_handling::NewlineMode;
use crate::compiler_frontend::tokenizer::tokens::{TokenKind, TokenizeMode};

fn parse_args(
    source: &str,
) -> Vec<crate::compiler_frontend::ast::expressions::call_argument::CallArgument> {
    let mut string_table = StringTable::new();
    let file_path = InternedPath::from_single_str("#page.bst", &mut string_table);
    let mut tokens = tokenize(
        source,
        &file_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
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
        &[],
        HostRegistry::new(),
        vec![],
    );
    parse_call_arguments(&mut tokens, &context, &mut string_table)
        .expect("call arguments should parse")
}

fn parse_args_error(source: &str) -> crate::compiler_frontend::compiler_errors::CompilerError {
    let mut string_table = StringTable::new();
    let file_path = InternedPath::from_single_str("#page.bst", &mut string_table);
    let mut tokens = tokenize(
        source,
        &file_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
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
        &[],
        HostRegistry::new(),
        vec![],
    );
    parse_call_arguments(&mut tokens, &context, &mut string_table)
        .expect_err("call arguments should fail")
}

// ── Parser-level tests (syntax / parse_call_arguments) ───────────────────────

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
    let error = parse_single_file_ast_error(
        r#"
take |value ~Int|:
;

value ~= 1
take(~value = value)
"#,
    );
    assert!(
        error
            .msg
            .contains("Mutable marker '~' is only allowed on the value side of a named argument")
    );
}

#[test]
fn rejects_positional_after_named() {
    let error = parse_single_file_ast_error(
        r#"
sum |a Int, b Int| -> Int:
    return a + b
;

sum(a = 1, 2)
"#,
    );
    assert!(
        error.msg.contains("positional arguments after named")
            || error.msg.contains("does not allow positional")
    );
}

#[test]
fn rejects_duplicate_named_target() {
    let error = parse_single_file_ast_error(
        r#"
sum |a Int, b Int| -> Int:
    return a + b
;

sum(a = 1, a = 2)
"#,
    );
    assert!(error.msg.contains("more than once") || error.msg.contains("Parameter 'a'"));
}

#[test]
fn rejects_unknown_named_parameter() {
    let error = parse_single_file_ast_error(
        r#"
sum |a Int, b Int| -> Int:
    return a + b
;

sum(a = 1, unknown = 2)
"#,
    );
    assert!(error.msg.contains("no parameter named 'unknown'"));
}

#[test]
fn rejects_missing_required_parameter() {
    let error = parse_single_file_ast_error(
        r#"
sum |a Int, b Int| -> Int:
    return a + b
;

sum(a = 1)
"#,
    );
    assert!(error.msg.contains("Missing required argument") || error.msg.contains("parameter 'b'"));
}

#[test]
fn rejects_tilde_on_left_side_of_named_arg() {
    // ~name = value is explicitly rejected at the parse level
    let error = parse_args_error("take(~value = 1)");
    assert!(
        error
            .msg
            .contains("Mutable marker '~' is only allowed on the value side")
    );
}
