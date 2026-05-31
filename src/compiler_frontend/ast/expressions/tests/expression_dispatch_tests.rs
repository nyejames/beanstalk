//! Expression token dispatch tests.
//!
//! WHAT: validates `dispatch_expression_token` behaviour for tokens that are valid in some
//!       expression contexts but invalid in others (for example Hash before TemplateHead).
//! WHY: the dispatcher is the single expression-entry gate; targeted tests prevent regressions
//!      in how edge-case tokens are rejected or advanced.

use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::ast::expressions::parse_expression_dispatch::{
    ExpressionDispatchState, dispatch_expression_token,
};
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext, TopLevelDeclarationTable};
use crate::compiler_frontend::compiler_messages::DiagnosticPayload;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::{
    FileTokens, SourceLocation, Token, TokenKind, TokenizeMode,
};
use crate::compiler_frontend::type_coercion::compatibility::TypeCompatibilityCache;
use crate::compiler_frontend::type_coercion::parse_context::ExpectedType;
use crate::compiler_frontend::value_mode::ValueMode;
use std::rc::Rc;

fn test_scope(string_table: &mut StringTable) -> (InternedPath, ScopeContext) {
    let scope = InternedPath::from_single_str("test.bst", string_table);
    let context = ScopeContext::new(
        ContextKind::Expression,
        scope.clone(),
        Rc::new(TopLevelDeclarationTable::new(vec![])),
        ExternalPackageRegistry::new(),
        vec![],
    );
    (scope, context)
}

fn token(kind: TokenKind, scope: &InternedPath) -> Token {
    Token::new(
        kind,
        SourceLocation::new(scope.clone(), Default::default(), Default::default()),
    )
}

#[test]
fn hash_in_expression_position_rejected() {
    let mut string_table = StringTable::default();
    let (scope, context) = test_scope(&mut string_table);
    let tokens = vec![
        token(TokenKind::IntLiteral(1), &scope),
        token(TokenKind::Hash, &scope),
        token(TokenKind::IntLiteral(2), &scope),
        token(TokenKind::Eof, &scope),
    ];
    let mut stream = FileTokens::new(scope.clone(), tokens);
    let mut expression = vec![];
    let mut expected_type = ExpectedType::Infer;
    let mut next_number_negative = false;
    let mut state = ExpressionDispatchState {
        expected_type: &mut expected_type,
        value_mode: &ValueMode::ImmutableOwned,
        consume_closing_parenthesis: false,
        allow_boundary_catch: true,
        allow_expected_result_evidence: true,
        expression: &mut expression,
        next_number_negative: &mut next_number_negative,
    };
    let mut type_environment = TypeEnvironment::new();
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);

    // First token: IntLiteral(1)
    let result = dispatch_expression_token(
        TokenKind::IntLiteral(1),
        &mut stream,
        &context,
        &mut type_interner,
        &mut state,
        &mut string_table,
    );
    assert!(result.is_ok());
    stream.advance();

    // Second token: Hash — should error because next token is IntLiteral, not TemplateHead
    let result = dispatch_expression_token(
        TokenKind::Hash,
        &mut stream,
        &context,
        &mut type_interner,
        &mut state,
        &mut string_table,
    );
    assert!(
        result.is_err(),
        "Expected Hash in expression position to be rejected"
    );
}

#[test]
fn hash_before_template_head_allowed() {
    let mut string_table = StringTable::default();
    let (scope, context) = test_scope(&mut string_table);
    let tokens = vec![
        token(TokenKind::Hash, &scope),
        token(TokenKind::TemplateHead, &scope),
        token(TokenKind::Eof, &scope),
    ];
    let mut stream = FileTokens::new(scope.clone(), tokens);
    let mut expression = vec![];
    let mut expected_type = ExpectedType::Infer;
    let mut next_number_negative = false;
    let mut state = ExpressionDispatchState {
        expected_type: &mut expected_type,
        value_mode: &ValueMode::ImmutableOwned,
        consume_closing_parenthesis: false,
        allow_boundary_catch: true,
        allow_expected_result_evidence: true,
        expression: &mut expression,
        next_number_negative: &mut next_number_negative,
    };
    let mut type_environment = TypeEnvironment::new();
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);

    let result = dispatch_expression_token(
        TokenKind::Hash,
        &mut stream,
        &context,
        &mut type_interner,
        &mut state,
        &mut string_table,
    );
    assert!(
        result.is_ok(),
        "Expected Hash before TemplateHead to advance"
    );
}

#[test]
fn hash_from_tokenized_source_rejected() {
    let mut string_table = StringTable::default();
    let source = "result = 1 # 2";
    let file_path = InternedPath::from_single_str("test.bst", &mut string_table);
    let file_tokens = tokenize(
        source,
        &file_path,
        TokenizeMode::Normal,
        &crate::compiler_frontend::style_directives::StyleDirectiveRegistry::built_ins(),
        &mut string_table,
        None,
    )
    .unwrap();

    // Find the tokens after "result = "
    let mut index = 0;
    while index < file_tokens.length {
        if matches!(file_tokens.tokens[index].kind, TokenKind::Assign) {
            index += 1;
            break;
        }
        index += 1;
    }

    // Slice from after Assign to end
    let expr_tokens: Vec<Token> = file_tokens.tokens[index..].to_vec();
    let scope = InternedPath::from_single_str("test.bst", &mut string_table);
    let mut stream = FileTokens::new(scope.clone(), expr_tokens);

    let context = ScopeContext::new(
        ContextKind::Expression,
        scope.clone(),
        Rc::new(TopLevelDeclarationTable::new(vec![])),
        ExternalPackageRegistry::new(),
        vec![],
    );

    let mut type_environment = TypeEnvironment::new();
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);
    let mut expected_type = ExpectedType::Infer;

    let result = create_expression(
        &mut stream,
        &context,
        &mut type_interner,
        &mut expected_type,
        &ValueMode::ImmutableOwned,
        false,
        &mut string_table,
    );

    assert!(
        result.is_err(),
        "Expected expression with stray # to fail, but got: ok"
    );
}

use crate::compiler_frontend::tests::test_support::parse_single_file_ast_diagnostic;

#[test]
fn full_frontend_stray_hash_error() {
    let source = "result = 1 # 2";
    let diagnostic = parse_single_file_ast_diagnostic(source);

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::UnexpectedToken { .. }
    ));
}
