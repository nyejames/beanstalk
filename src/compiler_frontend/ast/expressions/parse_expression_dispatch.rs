//! Expression token dispatch helpers.
//!
//! WHAT: routes one token at a time through expression-position parsing.
//! WHY: keeps delimiter/grammar ownership explicit while specialized helpers own detailed token families.

use super::error::ExpressionParseError;
use super::eval_expression::evaluate_expression;
use super::expression::{Expression, ExpressionKind, Operator};
use super::option_propagation::parse_option_propagation_suffix_for_expression;
use super::parse_expression::{
    ExpressionTrailingPolicy, create_expression_with_trailing_newline_policy,
};
use super::parse_expression_identifiers::parse_identifier_or_call;
use super::parse_expression_literals::{LiteralParseState, parse_literal_expression};
use super::parse_expression_places::{
    parse_copy_place_expression, parse_mutable_receiver_expression,
};
use super::parse_expression_templates::parse_template_expression;
use crate::ast_log;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::field_access::{ReceiverAccessMode, parse_postfix_chain};
use crate::compiler_frontend::ast::statements::fallible_handling::{
    fallible_catch_allowed_in_context, parse_fallible_handling_suffix_for_expression,
};
use crate::compiler_frontend::ast::statements::match_arm_boundaries::current_token_starts_match_arm_header;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::builtins::expression_parsing::{
    parse_builtin_cast_expression, parse_curly_literal_expression,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::trait_keyword_diagnostics::{
    reserved_trait_keyword_error, reserved_trait_keyword_or_dispatch_mismatch,
};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidBuiltinCallReason, InvalidControlFlowStatementReason,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::syntax_errors::expression_position::check_expression_common_mistake;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::compiler_frontend::type_coercion::parse_context::ExpectedType;
use crate::compiler_frontend::value_mode::ValueMode;

pub(super) enum ExpressionTokenStep {
    Continue,
    Advance,
    Break,
    Return(Box<Expression>),
}

pub(super) struct ExpressionDispatchState<'a> {
    pub(super) expected_type: &'a mut ExpectedType,
    pub(super) value_mode: &'a ValueMode,
    pub(super) consume_closing_parenthesis: bool,
    pub(super) allow_boundary_catch: bool,
    pub(super) allow_expected_result_evidence: bool,
    pub(super) expression: &'a mut Vec<AstNode>,
    pub(super) next_number_negative: &'a mut bool,
}

pub(super) fn push_expression_node(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
    expression: &mut Vec<AstNode>,
    allow_boundary_catch: bool,
    node: AstNode,
) -> Result<(), ExpressionParseError> {
    // ----------------------------
    //  Postfix field-access chain
    // ----------------------------
    // Postfix parsing happens after the primary node exists so fallible field-access chains bind to
    // the fully-built primary expression instead of only the leading identifier token.
    let node_after_postfix = if token_stream.index < token_stream.length
        && token_stream.current_token_kind() == &TokenKind::Dot
    {
        parse_postfix_chain(
            token_stream,
            node,
            ReceiverAccessMode::Shared,
            context,
            type_interner,
            string_table,
        )?
    } else {
        node
    };

    // ----------------------------
    //  Common mistake: `!=`
    // ----------------------------
    // Detect `!=` (Bang + Assign) before treating `!` as a result-handling suffix.
    if token_stream.index < token_stream.length
        && token_stream.current_token_kind() == &TokenKind::Bang
        && token_stream.peek_next_token() == Some(&TokenKind::Assign)
    {
        if let Some(error) = check_expression_common_mistake(token_stream, false) {
            return Err(error.into());
        }

        // Invariant: the condition above guarantees Bang+Assign, which
        // check_expression_common_mistake always matches. Reaching here is a compiler bug.
        return Err(CompilerError::compiler_error(
            "Bang+Assign pattern did not produce expected error",
        )
        .into());
    }

    // ----------------------------
    //  Fallible handling suffix
    // ----------------------------
    let node_after_fallible = if token_stream.index < token_stream.length
        && (token_stream.current_token_kind() == &TokenKind::Bang
            || token_stream.current_token_kind() == &TokenKind::Catch
            || (matches!(token_stream.current_token_kind(), TokenKind::Symbol(_))
                && token_stream.peek_next_token() == Some(&TokenKind::Bang)))
    {
        let handled = parse_fallible_handling_suffix_for_expression(
            token_stream,
            context,
            type_interner,
            node_after_postfix.get_expr_with_type_environment(type_interner.environment())?,
            true,
            allow_boundary_catch
                && expression.is_empty()
                && fallible_catch_allowed_in_context(context),
            string_table,
        )?;
        AstNode {
            kind: NodeKind::Rvalue(handled),
            location: token_stream.current_location(),
            scope: context.scope.clone(),
        }
    } else {
        node_after_postfix
    };

    // ----------------------------
    //  Option propagation suffix
    // ----------------------------
    let node_after_option_propagation = if token_stream.index < token_stream.length
        && token_stream.current_token_kind() == &TokenKind::QuestionMark
    {
        let propagated = parse_option_propagation_suffix_for_expression(
            token_stream,
            context,
            type_interner,
            node_after_fallible.get_expr_with_type_environment(type_interner.environment())?,
        )?;
        AstNode {
            kind: NodeKind::Rvalue(propagated),
            location: token_stream.current_location(),
            scope: context.scope.clone(),
        }
    } else {
        node_after_fallible
    };

    // ----------------------------
    //  Const record validation
    // ----------------------------
    // Const records are field-access-only. After postfix parsing and fallible
    // handling, reject any node that resolves to a const-record value in a
    // runtime context. The identifier-level check already caught bare names;
    // this catches field-access chains whose final step is itself a const record.
    if !context.kind.is_constant_context()
        && node_after_option_propagation
            .expression_is_const_record_value()
            .map_err(ExpressionParseError::from)?
    {
        let record_name = const_record_node_name(&node_after_option_propagation, string_table);
        return Err(CompilerDiagnostic::const_record_used_as_value(
            record_name,
            node_after_option_propagation.location.clone(),
        )
        .into());
    }

    expression.push(node_after_option_propagation);
    Ok(())
}

/// Extracts a display name for a const-record diagnostic from an AST node.
///
/// WHAT: walks field-access chains back to the root identifier so the
/// diagnostic example points at the record, not an intermediate field.
fn const_record_node_name(node: &AstNode, string_table: &mut StringTable) -> StringId {
    match &node.kind {
        NodeKind::FieldAccess { base, .. } => const_record_node_name(base, string_table),

        NodeKind::VariableDeclaration(decl) => decl
            .id
            .name()
            .unwrap_or_else(|| string_table.intern("record")),

        NodeKind::Rvalue(expr) => match &expr.kind {
            ExpressionKind::Reference(path) => {
                path.name().unwrap_or_else(|| string_table.intern("record"))
            }
            _ => string_table.intern("record"),
        },

        _ => string_table.intern("record"),
    }
}

/// Pushes a unary operator node onto the expression stack when the current token
/// is `Negative` or `Not`. Returns `true` when an operator was consumed.
fn parse_unary_operator(
    token_stream: &FileTokens,
    context: &ScopeContext,
    expression: &mut Vec<AstNode>,
    next_number_negative: &mut bool,
) -> bool {
    match token_stream.current_token_kind() {
        TokenKind::Negative => {
            *next_number_negative = true;
            true
        }
        TokenKind::Not => {
            expression.push(AstNode {
                kind: NodeKind::Operator(Operator::Not),
                location: token_stream.current_location(),
                scope: context.scope.clone(),
            });
            true
        }
        _ => false,
    }
}

fn push_operator_node(
    expression: &mut Vec<AstNode>,
    context: &ScopeContext,
    location: SourceLocation,
    operator: Operator,
) {
    expression.push(AstNode {
        kind: NodeKind::Operator(operator),
        location,
        scope: context.scope.clone(),
    });
}

/// Convenience for the common match arm that pushes an operator and advances.
fn advance_with_operator(
    expression: &mut Vec<AstNode>,
    context: &ScopeContext,
    location: SourceLocation,
    operator: Operator,
) -> Result<ExpressionTokenStep, ExpressionParseError> {
    push_operator_node(expression, context, location, operator);
    Ok(ExpressionTokenStep::Advance)
}

pub(super) fn dispatch_expression_token(
    token: TokenKind,
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    state: &mut ExpressionDispatchState<'_>,
    string_table: &mut StringTable,
) -> Result<ExpressionTokenStep, ExpressionParseError> {
    // This state machine is intentionally flat: each token either appends one AST node, advances
    // past a nested parse, or signals the caller that the surrounding grammar owns the delimiter.
    match token {
        // -------------------------------
        //  Delimiters and terminators
        // -------------------------------
        TokenKind::CloseCurly
        | TokenKind::Comma
        | TokenKind::Eof
        | TokenKind::TemplateClose
        | TokenKind::Arrow
        | TokenKind::StartTemplateBody
        | TokenKind::Colon
        | TokenKind::Else
        | TokenKind::End => dispatch_delimiter_token(token, token_stream, state, string_table),

        // -------------------------------
        //  Parentheses and grouping
        // -------------------------------
        TokenKind::CloseParenthesis => dispatch_close_parenthesis(token_stream, state),

        TokenKind::OpenParenthesis => {
            token_stream.advance();
            let value = create_expression_with_trailing_newline_policy(
                token_stream,
                context,
                type_interner,
                state.expected_type,
                state.value_mode,
                ExpressionTrailingPolicy {
                    consume_closing_parenthesis: true,
                    skip_trailing_newlines: true,
                    allow_boundary_catch: false,
                    allow_expected_result_evidence: state.allow_expected_result_evidence,
                },
                string_table,
            )?;

            push_expression_node(
                token_stream,
                context,
                type_interner,
                string_table,
                state.expression,
                state.allow_boundary_catch,
                AstNode {
                    kind: NodeKind::Rvalue(value),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                },
            )?;

            Ok(ExpressionTokenStep::Continue)
        }

        TokenKind::DatatypeInt | TokenKind::DatatypeFloat => {
            let cast_expression = parse_builtin_cast_expression(
                token_stream,
                context,
                type_interner,
                state.value_mode,
                string_table,
            )?;
            let cast_location = cast_expression.location.clone();

            push_expression_node(
                token_stream,
                context,
                type_interner,
                string_table,
                state.expression,
                state.allow_boundary_catch,
                AstNode {
                    kind: NodeKind::Rvalue(cast_expression),
                    location: cast_location,
                    scope: context.scope.clone(),
                },
            )?;

            Ok(ExpressionTokenStep::Continue)
        }

        TokenKind::OpenCurly => {
            parse_curly_literal_expression(
                token_stream,
                context,
                type_interner,
                state.expected_type,
                state.value_mode,
                state.expression,
                string_table,
            )?;
            Ok(ExpressionTokenStep::Advance)
        }

        TokenKind::Newline => dispatch_newline(token_stream, context, state),

        // -------------------------------
        //  Primary expressions
        // -------------------------------
        TokenKind::Symbol(..) | TokenKind::This => {
            parse_identifier_or_call(
                token_stream,
                context,
                type_interner,
                state.expression,
                state.allow_boundary_catch,
                state.allow_expected_result_evidence,
                string_table,
            )?;
            Ok(ExpressionTokenStep::Continue)
        }

        TokenKind::Mutable => {
            parse_mutable_receiver_expression(
                token_stream,
                context,
                type_interner,
                state.expression,
                state.allow_boundary_catch,
                string_table,
            )?;
            Ok(ExpressionTokenStep::Continue)
        }

        TokenKind::FloatLiteral(_)
        | TokenKind::IntLiteral(_)
        | TokenKind::StringSliceLiteral(_)
        | TokenKind::BoolLiteral(_)
        | TokenKind::CharLiteral(_)
        | TokenKind::NoneLiteral => {
            let mut literal_state = LiteralParseState {
                expected_type: state.expected_type,
                value_mode: state.value_mode,
                expression: state.expression,
                next_number_negative: state.next_number_negative,
                allow_boundary_catch: state.allow_boundary_catch,
            };
            parse_literal_expression(
                token_stream,
                context,
                type_interner,
                &mut literal_state,
                string_table,
            )?;
            Ok(ExpressionTokenStep::Continue)
        }

        TokenKind::TemplateHead => {
            if let Some(template_expression) = parse_template_expression(
                token_stream,
                context,
                type_interner,
                state.consume_closing_parenthesis,
                state.value_mode,
                string_table,
            )? {
                return Ok(ExpressionTokenStep::Return(Box::new(template_expression)));
            }

            Ok(ExpressionTokenStep::Advance)
        }

        TokenKind::Copy => {
            let copy_location = token_stream.current_location();
            token_stream.advance();

            let copied_place =
                parse_copy_place_expression(token_stream, context, type_interner, string_table)?;
            let copied_expression =
                copied_place.get_expr_with_type_environment(type_interner.environment())?;
            let copied_type = copied_expression.diagnostic_type;
            let copied_type_id = copied_expression.type_id;

            state.expression.push(AstNode {
                kind: NodeKind::Rvalue(Expression::copy_with_type_id(
                    copied_place,
                    copied_type,
                    copied_type_id,
                    copy_location.clone(),
                    state.value_mode.to_owned(),
                )),
                location: copy_location,
                scope: context.scope.clone(),
            });

            Ok(ExpressionTokenStep::Continue)
        }

        // -------------------------------
        //  Reserved / invalid tokens
        // -------------------------------
        TokenKind::If => Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::ValueBlockOutsideReceiver,
            token_stream.current_location(),
        )
        .into()),

        TokenKind::Assert => Err(CompilerDiagnostic::invalid_builtin_call(
            InvalidBuiltinCallReason::ExpressionPositionNotAllowed,
            Some(string_table.intern("assert")),
            token_stream.current_location(),
        )
        .into()),

        TokenKind::Must | TokenKind::TraitThis => {
            let keyword = reserved_trait_keyword_or_dispatch_mismatch(
                token_stream.current_token_kind(),
                token_stream.current_location(),
                "Expression Parsing",
                "expression parsing",
            )?;

            Err(reserved_trait_keyword_error(keyword, token_stream.current_location()).into())
        }

        TokenKind::Hash => {
            if token_stream.peek_next_token() != Some(&TokenKind::TemplateHead) {
                return Err(CompilerDiagnostic::unexpected_token(
                    TokenKind::Hash,
                    token_stream.current_location(),
                )
                .into());
            }

            Ok(ExpressionTokenStep::Advance)
        }

        TokenKind::Negative | TokenKind::Not => {
            let _ = parse_unary_operator(
                token_stream,
                context,
                state.expression,
                state.next_number_negative,
            );
            Ok(ExpressionTokenStep::Advance)
        }

        // -------------------------------
        //  Arithmetic operators
        // -------------------------------
        TokenKind::Add => advance_with_operator(
            state.expression,
            context,
            token_stream.current_location(),
            Operator::Add,
        ),
        TokenKind::Subtract => advance_with_operator(
            state.expression,
            context,
            token_stream.current_location(),
            Operator::Subtract,
        ),
        TokenKind::Multiply => advance_with_operator(
            state.expression,
            context,
            token_stream.current_location(),
            Operator::Multiply,
        ),
        TokenKind::Divide => advance_with_operator(
            state.expression,
            context,
            token_stream.current_location(),
            Operator::Divide,
        ),
        TokenKind::IntDivide => advance_with_operator(
            state.expression,
            context,
            token_stream.current_location(),
            Operator::IntDivide,
        ),
        TokenKind::Exponent => advance_with_operator(
            state.expression,
            context,
            token_stream.current_location(),
            Operator::Exponent,
        ),
        TokenKind::Modulus => advance_with_operator(
            state.expression,
            context,
            token_stream.current_location(),
            Operator::Modulus,
        ),

        // -------------------------------
        //  Comparison operators
        // -------------------------------
        TokenKind::Is => {
            dispatch_is_token(token_stream, context, type_interner, state, string_table)
        }

        TokenKind::LessThan => advance_with_operator(
            state.expression,
            context,
            token_stream.current_location(),
            Operator::LessThan,
        ),
        TokenKind::LessThanOrEqual => advance_with_operator(
            state.expression,
            context,
            token_stream.current_location(),
            Operator::LessThanOrEqual,
        ),
        TokenKind::GreaterThan => advance_with_operator(
            state.expression,
            context,
            token_stream.current_location(),
            Operator::GreaterThan,
        ),
        TokenKind::GreaterThanOrEqual => advance_with_operator(
            state.expression,
            context,
            token_stream.current_location(),
            Operator::GreaterThanOrEqual,
        ),
        TokenKind::And => advance_with_operator(
            state.expression,
            context,
            token_stream.current_location(),
            Operator::And,
        ),
        TokenKind::Or => advance_with_operator(
            state.expression,
            context,
            token_stream.current_location(),
            Operator::Or,
        ),

        TokenKind::ExclusiveRange => advance_with_operator(
            state.expression,
            context,
            token_stream.current_location(),
            Operator::Range,
        ),

        // -------------------------------
        //  Unexpected tokens
        // -------------------------------
        TokenKind::Wildcard => Err(CompilerDiagnostic::unexpected_token(
            TokenKind::Wildcard,
            token_stream.current_location(),
        )
        .into()),

        TokenKind::TypeParameterBracket => {
            if let Some(error) =
                check_expression_common_mistake(token_stream, state.expression.is_empty())
            {
                return Err(error.into());
            }

            Err(CompilerDiagnostic::unexpected_token(
                TokenKind::TypeParameterBracket,
                token_stream.current_location(),
            )
            .into())
        }

        TokenKind::AddAssign => Err(CompilerDiagnostic::unexpected_token(
            TokenKind::AddAssign,
            token_stream.current_location(),
        )
        .into()),

        _ => {
            if let Some(error) =
                check_expression_common_mistake(token_stream, state.expression.is_empty())
            {
                return Err(error.into());
            }

            Err(CompilerDiagnostic::unexpected_token(token, token_stream.current_location()).into())
        }
    }
}

// -------------------------------
//  Delimiter and terminator tokens
// -------------------------------
fn dispatch_delimiter_token(
    token: TokenKind,
    token_stream: &mut FileTokens,
    state: &mut ExpressionDispatchState<'_>,
    string_table: &mut StringTable,
) -> Result<ExpressionTokenStep, ExpressionParseError> {
    if state.expression.is_empty() {
        match token {
            TokenKind::Comma => {
                return Err(CompilerDiagnostic::unexpected_token(
                    TokenKind::Comma,
                    token_stream.current_location(),
                )
                .into());
            }

            TokenKind::Arrow => {
                return Err(CompilerDiagnostic::unexpected_token(
                    TokenKind::Arrow,
                    token_stream.current_location(),
                )
                .into());
            }

            _ => {}
        }
    }

    if state.consume_closing_parenthesis {
        return Err(CompilerDiagnostic::missing_closing_delimiter(
            string_table.intern(")"),
            token_stream.current_location(),
        )
        .into());
    }

    Ok(ExpressionTokenStep::Break)
}

// -------------------------------
//  Close parenthesis
// -------------------------------
fn dispatch_close_parenthesis(
    token_stream: &mut FileTokens,
    state: &mut ExpressionDispatchState<'_>,
) -> Result<ExpressionTokenStep, ExpressionParseError> {
    if state.consume_closing_parenthesis {
        token_stream.advance();
    }

    if state.expression.is_empty() {
        return Err(CompilerDiagnostic::unexpected_token(
            TokenKind::CloseParenthesis,
            token_stream.current_location(),
        )
        .into());
    }

    Ok(ExpressionTokenStep::Break)
}

// -------------------------------
//  Newline
// -------------------------------
fn dispatch_newline(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    state: &mut ExpressionDispatchState<'_>,
) -> Result<ExpressionTokenStep, ExpressionParseError> {
    // When at the very start of the stream, treat the virtual previous token as a newline
    // so expression continuation logic does not fire.
    let previous_token = if token_stream.index == 0 {
        &TokenKind::Newline
    } else {
        token_stream.previous_token()
    };
    if state.consume_closing_parenthesis
        || (previous_token.continues_expression() && !matches!(previous_token, TokenKind::End))
    {
        token_stream.skip_newlines();
        return Ok(ExpressionTokenStep::Continue);
    }

    // ----------------------------
    //  Lookahead for continuation
    // ----------------------------
    // Look ahead past newlines to find the next meaningful token.
    // If that token continues the expression, skip newlines and keep parsing.
    let saved_index = token_stream.index;
    token_stream.skip_newlines();
    if context.kind == ContextKind::MatchArm
        && current_token_starts_match_arm_header(token_stream).is_some()
    {
        token_stream.index = saved_index;
        return Ok(ExpressionTokenStep::Break);
    }

    if token_stream.index < token_stream.length
        && token_stream.current_token_kind().continues_expression()
    {
        return Ok(ExpressionTokenStep::Continue);
    }
    token_stream.index = saved_index;

    ast_log!("Breaking out of expression with newline");
    Ok(ExpressionTokenStep::Break)
}

// -------------------------------
//  Is operator
// -------------------------------
fn dispatch_is_token(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    state: &mut ExpressionDispatchState<'_>,
    string_table: &mut StringTable,
) -> Result<ExpressionTokenStep, ExpressionParseError> {
    match token_stream.peek_next_token() {
        // `is not` → inequality operator.
        Some(TokenKind::Not) => {
            token_stream.advance();
            advance_with_operator(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::NotEqual,
            )
        }

        // `is:` → type guard in a match arm. The left-hand side must be a single expression.
        Some(TokenKind::Colon) => {
            if state.expression.len() > 1 {
                return Err(CompilerDiagnostic::unexpected_token(
                    TokenKind::Colon,
                    token_stream.current_location(),
                )
                .into());
            }

            let value = evaluate_expression(
                context,
                std::mem::take(state.expression),
                type_interner,
                state.expected_type,
                state.value_mode,
                string_table,
            )?;
            Ok(ExpressionTokenStep::Return(Box::new(value)))
        }

        // Bare `is` → equality operator.
        _ => advance_with_operator(
            state.expression,
            context,
            token_stream.current_location(),
            Operator::Equality,
        ),
    }
}

#[cfg(test)]
#[path = "tests/expression_dispatch_tests.rs"]
mod expression_dispatch_tests;
