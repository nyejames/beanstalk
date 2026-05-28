//! Literal expression parsing regression tests.
//!
//! WHAT: validates parsing of int, float, string, char, bool, and template literals plus
//!       malformed-literal diagnostics.
//! WHY: literals are the simplest expressions but span many token kinds; targeted coverage
//!      prevents silent changes to literal type inference.

use super::*;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext, TopLevelDeclarationTable};
use crate::compiler_frontend::compiler_messages::render::{DiagnosticRenderContext, terminal};
use crate::compiler_frontend::compiler_messages::{
    CompileTimeEvaluationErrorReason, DiagnosticKind, DiagnosticPayload, RuleDiagnosticKind,
};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::compiler_frontend::type_coercion::compatibility::TypeCompatibilityCache;
use crate::compiler_frontend::type_coercion::parse_context::ExpectedType;
use crate::compiler_frontend::value_mode::ValueMode;
use std::rc::Rc;

#[test]
fn parse_literal_expression_rejects_int_negation_overflow() {
    let mut string_table = StringTable::new();
    let scope = InternedPath::from_single_str("test.bst", &mut string_table);
    let context = ScopeContext::new(
        ContextKind::Expression,
        scope.clone(),
        Rc::new(TopLevelDeclarationTable::new(vec![])),
        ExternalPackageRegistry::new(),
        vec![],
    );

    let tokens = vec![
        Token::new(TokenKind::IntLiteral(i64::MIN), SourceLocation::default()),
        Token::new(TokenKind::Eof, SourceLocation::default()),
    ];
    let mut token_stream = FileTokens::new(scope, tokens);
    let mut expression = Vec::new();
    let mut next_number_negative = true;
    let mut type_environment = TypeEnvironment::new();
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);
    let expected_type = ExpectedType::Infer;
    let value_mode = ValueMode::ImmutableOwned;
    let mut literal_state = LiteralParseState {
        expected_type: &expected_type,
        value_mode: &value_mode,
        expression: &mut expression,
        next_number_negative: &mut next_number_negative,
        allow_boundary_catch: true,
    };

    let error = parse_literal_expression(
        &mut token_stream,
        &context,
        &mut type_interner,
        &mut literal_state,
        &mut string_table,
    )
    .expect_err("negating Int::MIN should return a rule error");

    let diagnostic = error
        .diagnostic()
        .expect("literal overflow should remain a typed diagnostic");
    assert_eq!(
        diagnostic.kind,
        DiagnosticKind::Rule(RuleDiagnosticKind::CompileTimeEvaluationError)
    );
    assert_eq!(diagnostic.kind.code(), "BST-RULE-0053");
    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::CompileTimeEvaluationError {
            reason: CompileTimeEvaluationErrorReason::IntegerOverflow,
            operation: _
        }
    ));

    let render_context = DiagnosticRenderContext::new(&string_table);
    let guidance = terminal::format_payload_guidance(&diagnostic.payload, render_context);
    assert!(
        guidance
            .iter()
            .any(|line| line
                .contains("Compile-time integer overflow while evaluating this expression")),
        "{}",
        guidance.join("\n")
    );
}
