//! Fallible suffix parsing implementation.
//!
//! WHAT: parses postfix `!` propagation and `catch` recovery suffixes for fallible calls and
//! expressions without exposing raw Result values as ordinary user data.
//!
//! WHY: fallible handling has dedicated control-flow rules (error-type compatibility, catch-body
//! value production, and boundary restrictions) that would make the general expression parser too
//! large and too coupled to function bodies.
//!
//! STAGE BOUNDARY: this is pure AST frontend parsing. Type checking of the resulting
//! `HandledFallibleFunctionCall` / `HandledFallibleHostFunctionCall` nodes happens later.

use crate::compiler_frontend::ast::ContextKind;
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::call_argument::CallArgument;
use crate::compiler_frontend::ast::expressions::error::ExpressionParseError;
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, FallibleHandling,
};
use crate::compiler_frontend::ast::statements::value_production::types::{
    ValueBlock, ValueCatchBlock,
};
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidResultHandlingReason, TypeMismatchContext,
};
use crate::compiler_frontend::datatypes::diagnostic_type_spelling;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::external_packages::ExternalFunctionId;
use crate::compiler_frontend::type_coercion::compatibility::is_postfix_error_compatible;

use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};

use super::catch_handler::{
    CatchFallibleHandler, CatchFallibleHandlerSite, parse_catch_fallible_handler_typed,
    parse_catch_without_error_binding_typed, parse_inline_catch_fallible_handler_typed,
    parse_inline_catch_without_error_binding_typed,
};
use super::success_types::fallible_success_type_ids;
use super::{EXPRESSION_STAGE, FUNCTION_CALL_STAGE};

pub(crate) struct HandledFallibleCall {
    pub(crate) name: InternedPath,
    pub(crate) args: Vec<CallArgument>,
    pub(crate) result_type_ids: Vec<TypeId>,
    pub(crate) call_location: SourceLocation,
}

pub(crate) struct FallibleCallSite {
    pub(crate) call: HandledFallibleCall,
    pub(crate) error_return_type_id: TypeId,
    pub(crate) value_required: bool,
    pub(crate) allow_boundary_catch: bool,
}

pub(crate) struct HandledFallibleHostCall {
    pub(crate) name: ExternalFunctionId,
    pub(crate) args: Vec<CallArgument>,
    pub(crate) result_type_ids: Vec<TypeId>,
    pub(crate) error_type_id: TypeId,
    pub(crate) call_location: SourceLocation,
}

pub(crate) struct FallibleHostCallSite {
    pub(crate) call: HandledFallibleHostCall,
    pub(crate) value_required: bool,
    pub(crate) allow_boundary_catch: bool,
}

struct FallibleHandlingSite<'a> {
    success_result_type_ids: &'a [TypeId],
    error_return_type_id: TypeId,
    value_required: bool,
    value_required_location: SourceLocation,
    compilation_stage: &'a str,
    allow_boundary_catch: bool,
}

impl HandledFallibleHostCall {
    pub(crate) fn into_ast_node(
        self,
        handling: FallibleHandling,
        ast_location: SourceLocation,
        scope: &InternedPath,
    ) -> AstNode {
        AstNode {
            kind: NodeKind::HandledFallibleHostFunctionCall {
                name: self.name,
                args: self.args,
                result_type_ids: self.result_type_ids,
                error_type_id: self.error_type_id,
                handling,
                location: self.call_location,
            },
            location: ast_location,
            scope: scope.clone(),
        }
    }
}

impl HandledFallibleCall {
    pub(crate) fn into_plain_ast_node(
        self,
        ast_location: SourceLocation,
        scope: &InternedPath,
    ) -> AstNode {
        AstNode {
            kind: NodeKind::FunctionCall {
                name: self.name,
                args: self.args,
                result_type_ids: self.result_type_ids,
                location: self.call_location,
            },
            location: ast_location,
            scope: scope.clone(),
        }
    }

    pub(crate) fn into_ast_node(
        self,
        handling: FallibleHandling,
        ast_location: SourceLocation,
        scope: &InternedPath,
    ) -> AstNode {
        AstNode {
            kind: NodeKind::HandledFallibleFunctionCall {
                name: self.name,
                args: self.args,
                result_type_ids: self.result_type_ids,
                handling,
                location: self.call_location,
            },
            location: ast_location,
            scope: scope.clone(),
        }
    }
}

pub(crate) fn parse_fallible_handling_suffix_for_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    expression: Expression,
    value_required: bool,
    allow_boundary_catch: bool,
    string_table: &mut StringTable,
) -> Result<Expression, ExpressionParseError> {
    let expression_type_id = expression.type_id;
    let type_environment = type_interner.environment();

    // Extract the error-return type from the expression's fallible carrier.
    // The success-slot type IDs are computed separately because they drive the
    // catch-handler's value-production target, not the propagation check.
    let Some((_success_type_ids, error_return_type_id)) =
        type_environment.fallible_carrier_slots(expression_type_id)
    else {
        return Err(CompilerDiagnostic::invalid_result_handling(
            InvalidResultHandlingReason::NotResultExpression,
            token_stream.current_location(),
        )
        .into());
    };

    let success_result_type_ids = fallible_success_type_ids(expression_type_id, type_environment);

    // Collapse single or zero success types into a concrete type for the
    // resulting handled expression. Multi-value successes become a tuple.
    let handled_type_id = match success_result_type_ids.as_slice() {
        [] => type_environment.builtins().none,
        [single] => *single,
        multiple => type_interner
            .environment_mut_for_derived_types()
            .intern_tuple(multiple.to_vec()),
    };

    let success_type_diagnostic_spelling =
        diagnostic_type_spelling(handled_type_id, type_interner.environment());

    if let Some(handling) = parse_fallible_handling_suffix(
        token_stream,
        context,
        type_interner,
        FallibleHandlingSite {
            success_result_type_ids: &success_result_type_ids,
            error_return_type_id,
            value_required,
            value_required_location: expression.location.clone(),
            compilation_stage: EXPRESSION_STAGE,
            allow_boundary_catch,
        },
        None,
        string_table,
    )? {
        let handled_expression = Expression::handled_result_with_type_id(
            expression,
            handling,
            handled_type_id,
            success_type_diagnostic_spelling,
            token_stream.current_location(),
        );

        return Ok(wrap_value_producing_catch_expression(
            handled_expression,
            success_result_type_ids,
            value_required,
            type_interner.environment(),
        ));
    }

    Ok(expression)
}

/// Returns whether `catch` handlers are syntactically permitted in the given scope context.
///
/// WHY: catch introduces a statement-like body block, so it is forbidden inside expression-only
/// contexts (conditions, templates, constants) where statements are not allowed.
pub(crate) fn fallible_catch_allowed_in_context(context: &ScopeContext) -> bool {
    !matches!(
        context.kind,
        ContextKind::Expression
            | ContextKind::Condition
            | ContextKind::Template
            | ContextKind::Constant
            | ContextKind::ConstantHeader
    )
}

fn parse_fallible_handling_suffix(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    site: FallibleHandlingSite<'_>,
    warnings: Option<&mut Vec<CompilerDiagnostic>>,
    string_table: &mut StringTable,
) -> Result<Option<FallibleHandling>, ExpressionParseError> {
    match token_stream.current_token_kind() {
        // `!` — propagate the error upward to the enclosing function's error slot.
        TokenKind::Bang => {
            parse_postfix_propagation(token_stream, context, site, type_interner.environment())
                .map(Some)
        }

        // `catch:` or `catch |err|:` — recover locally with a fallback or handler body.
        TokenKind::Catch => parse_catch_handling_suffix(
            token_stream,
            context,
            type_interner,
            site,
            warnings,
            string_table,
        )
        .map(Some),

        // Reject the old `symbol!` catch syntax that was removed from the language.
        TokenKind::Symbol(_) if token_stream.peek_next_token() == Some(&TokenKind::Bang) => {
            Err(CompilerDiagnostic::invalid_result_handling(
                InvalidResultHandlingReason::RemovedBangCatchHandlerSyntax,
                token_stream.current_location(),
            )
            .into())
        }

        // No fallible handling suffix present.
        _ => Ok(None),
    }
}

fn parse_postfix_propagation(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    site: FallibleHandlingSite<'_>,
    type_environment: &TypeEnvironment,
) -> Result<FallibleHandling, ExpressionParseError> {
    token_stream.advance();

    // Reject the old `expr!fallback` inline-fallback syntax.
    if token_starts_removed_bang_fallback(token_stream.current_token_kind()) {
        return Err(CompilerDiagnostic::invalid_result_handling(
            InvalidResultHandlingReason::RemovedBangFallbackSyntax,
            token_stream.current_location(),
        )
        .into());
    }

    let Some(expected_error_type_id) = context.expected_error_type else {
        return Err(CompilerDiagnostic::invalid_result_handling(
            InvalidResultHandlingReason::FunctionHasNoErrorSlot,
            token_stream.current_location(),
        )
        .into());
    };

    if !is_postfix_error_compatible(
        expected_error_type_id,
        site.error_return_type_id,
        type_environment,
    ) {
        return Err(CompilerDiagnostic::type_mismatch(
            expected_error_type_id,
            site.error_return_type_id,
            TypeMismatchContext::ResultError,
            token_stream.current_location(),
        )
        .into());
    }

    Ok(FallibleHandling::Propagate)
}

/// Wraps value-producing catch recovery in the shared `ValueBlock` expression shape.
///
/// WHAT: only `catch` handlers become value blocks; postfix propagation stays an ordinary
/// handled fallible expression because it leaves the current function instead of recovering.
/// WHY: this keeps catch recovery on the same AST/HIR model as value `if` and match blocks
/// without changing statement-only catch behavior.
fn wrap_value_producing_catch_expression(
    handled_expression: Expression,
    result_type_ids: Vec<TypeId>,
    value_required: bool,
    type_environment: &TypeEnvironment,
) -> Expression {
    if !value_required
        || result_type_ids.is_empty()
        || !matches!(
            handled_expression.kind,
            ExpressionKind::HandledFallibleExpression {
                handling: FallibleHandling::Handler { .. },
                ..
            } | ExpressionKind::HandledFallibleFunctionCall {
                handling: FallibleHandling::Handler { .. },
                ..
            } | ExpressionKind::HandledFallibleHostFunctionCall {
                handling: FallibleHandling::Handler { .. },
                ..
            }
        )
    {
        return handled_expression;
    }

    let location = handled_expression.location.clone();
    let result_type_id = handled_expression.type_id;
    let diagnostic_type = diagnostic_type_spelling(result_type_id, type_environment);

    Expression::new(
        ExpressionKind::ValueBlock {
            block: Box::new(ValueBlock::Catch(ValueCatchBlock {
                handled_value: Box::new(handled_expression),
                location: location.clone(),
                result_type_ids,
            })),
        },
        location,
        result_type_id,
        diagnostic_type,
        crate::compiler_frontend::value_mode::ValueMode::ImmutableOwned,
    )
}

/// Returns true for token kinds that started the old `expr!fallback` inline-fallback syntax.
///
/// WHY: this prevents confusing parse errors when users write removed syntax such as
/// `result!"fallback"` or `result!{ fallback }`. A dedicated diagnostic is clearer than
/// falling through to unexpected-token.
fn token_starts_removed_bang_fallback(token: &TokenKind) -> bool {
    matches!(
        token,
        TokenKind::Symbol(_)
            | TokenKind::StringSliceLiteral(_)
            | TokenKind::RawStringLiteral(_)
            | TokenKind::IntLiteral(_)
            | TokenKind::FloatLiteral(_)
            | TokenKind::CharLiteral(_)
            | TokenKind::BoolLiteral(_)
            | TokenKind::NoneLiteral
            | TokenKind::OpenCurly
            | TokenKind::TemplateHead
    )
}

fn parse_catch_handling_suffix(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    site: FallibleHandlingSite<'_>,
    warnings: Option<&mut Vec<CompilerDiagnostic>>,
    string_table: &mut StringTable,
) -> Result<FallibleHandling, ExpressionParseError> {
    if !site.allow_boundary_catch {
        return Err(CompilerDiagnostic::invalid_result_handling(
            InvalidResultHandlingReason::CatchOutsideBoundary,
            token_stream.current_location(),
        )
        .into());
    }

    token_stream.advance();
    let catch_context = context.activate_pending_catch_assignment_targets();

    match token_stream.current_token_kind() {
        // `catch then ...` — inline value-producing fallback with no error binding.
        TokenKind::Then => parse_inline_catch_without_error_binding(
            token_stream,
            &catch_context,
            type_interner,
            site,
            string_table,
        ),

        // `catch:` — no error binding, just a fallback body.
        TokenKind::Colon => parse_catch_without_error_binding(
            token_stream,
            &catch_context,
            type_interner,
            site,
            warnings,
            string_table,
        ),

        // `catch |err|:` or `catch |err| then ...` — bind the error value before recovery.
        TokenKind::TypeParameterBracket => parse_catch_handler(
            token_stream,
            &catch_context,
            type_interner,
            site,
            warnings,
            string_table,
        ),

        _ => Err(CompilerDiagnostic::invalid_result_handling(
            InvalidResultHandlingReason::ExpectedCatchBlockOrHandler,
            token_stream.current_location(),
        )
        .into()),
    }
}

/// Delegates to `parse_inline_catch_without_error_binding_typed` for `catch then ...`.
fn parse_inline_catch_without_error_binding(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    site: FallibleHandlingSite<'_>,
    string_table: &mut StringTable,
) -> Result<FallibleHandling, ExpressionParseError> {
    let CatchFallibleHandler { error, body } = parse_inline_catch_without_error_binding_typed(
        token_stream,
        context,
        type_interner,
        CatchFallibleHandlerSite {
            success_result_type_ids: site.success_result_type_ids,
            error_return_type_id: site.error_return_type_id,
            value_required: site.value_required,
            compilation_stage: site.compilation_stage,
            value_required_location: site.value_required_location,
        },
        string_table,
    )?;

    Ok(FallibleHandling::Handler { error, body })
}

/// Delegates to `parse_catch_without_error_binding_typed` for `catch:` (no error binding).
fn parse_catch_without_error_binding(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    site: FallibleHandlingSite<'_>,
    warnings: Option<&mut Vec<CompilerDiagnostic>>,
    string_table: &mut StringTable,
) -> Result<FallibleHandling, ExpressionParseError> {
    let CatchFallibleHandler { error, body } = parse_catch_without_error_binding_typed(
        token_stream,
        context,
        type_interner,
        CatchFallibleHandlerSite {
            success_result_type_ids: site.success_result_type_ids,
            error_return_type_id: site.error_return_type_id,
            value_required: site.value_required,
            compilation_stage: site.compilation_stage,
            value_required_location: site.value_required_location,
        },
        warnings,
        string_table,
    )?;

    Ok(FallibleHandling::Handler { error, body })
}

/// Delegates to the block or inline binding parser for `catch |err|`.
fn parse_catch_handler(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    site: FallibleHandlingSite<'_>,
    warnings: Option<&mut Vec<CompilerDiagnostic>>,
    string_table: &mut StringTable,
) -> Result<FallibleHandling, ExpressionParseError> {
    let handler_site = CatchFallibleHandlerSite {
        success_result_type_ids: site.success_result_type_ids,
        error_return_type_id: site.error_return_type_id,
        value_required: site.value_required,
        compilation_stage: site.compilation_stage,
        value_required_location: site.value_required_location,
    };

    let CatchFallibleHandler { error, body } = if next_catch_binding_is_inline(token_stream) {
        parse_inline_catch_fallible_handler_typed(
            token_stream,
            context,
            type_interner,
            handler_site,
            warnings,
            string_table,
        )?
    } else {
        parse_catch_fallible_handler_typed(
            token_stream,
            context,
            type_interner,
            handler_site,
            warnings,
            string_table,
        )?
    };

    Ok(FallibleHandling::Handler { error, body })
}

fn next_catch_binding_is_inline(token_stream: &FileTokens) -> bool {
    let mut index = token_stream.index + 1;

    while index < token_stream.length {
        match &token_stream.tokens[index].kind {
            TokenKind::TypeParameterBracket => {
                return token_stream
                    .tokens
                    .iter()
                    .skip(index + 1)
                    .find(|token| token.kind != TokenKind::Newline)
                    .is_some_and(|token| token.kind == TokenKind::Then);
            }

            TokenKind::Newline | TokenKind::End | TokenKind::Eof => return false,

            _ => index += 1,
        }
    }

    false
}

// --------------------------
//  Catch handler call parsing
// --------------------------

pub(crate) fn parse_fallible_handling_suffix_for_call(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    handler_call: FallibleCallSite,
    warnings: Option<&mut Vec<CompilerDiagnostic>>,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<AstNode, ExpressionParseError> {
    let FallibleCallSite {
        call,
        error_return_type_id,
        value_required,
        allow_boundary_catch,
    } = handler_call;

    let handling = parse_fallible_handling_suffix(
        token_stream,
        context,
        type_interner,
        FallibleHandlingSite {
            success_result_type_ids: &call.result_type_ids,
            error_return_type_id,
            value_required,
            value_required_location: call.call_location.clone(),
            compilation_stage: FUNCTION_CALL_STAGE,
            allow_boundary_catch,
        },
        warnings,
        string_table,
    )?;

    let Some(handling) = handling else {
        return Err(CompilerDiagnostic::invalid_result_handling(
            InvalidResultHandlingReason::ExpectedCatchBlockOrHandler,
            token_stream.current_location(),
        )
        .into());
    };

    if value_required && matches!(handling, FallibleHandling::Handler { .. }) {
        let call_location = call.call_location.clone();
        let result_type_ids = call.result_type_ids.clone();
        let handled_expression = Expression::handled_fallible_function_call_with_typed_arguments(
            call.name,
            call.args,
            result_type_ids.clone(),
            handling,
            type_interner.environment_mut_for_derived_types(),
            call_location,
        );
        let catch_expression = wrap_value_producing_catch_expression(
            handled_expression,
            result_type_ids,
            true,
            type_interner.environment(),
        );

        return Ok(AstNode {
            kind: NodeKind::Rvalue(catch_expression),
            location: token_stream.current_location(),
            scope: context.scope.clone(),
        });
    }

    Ok(call.into_ast_node(handling, token_stream.current_location(), &context.scope))
}

pub(crate) fn parse_fallible_handling_suffix_for_host_call(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    handler_call: FallibleHostCallSite,
    warnings: Option<&mut Vec<CompilerDiagnostic>>,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<AstNode, ExpressionParseError> {
    let FallibleHostCallSite {
        call,
        value_required,
        allow_boundary_catch,
    } = handler_call;

    let handling = parse_fallible_handling_suffix(
        token_stream,
        context,
        type_interner,
        FallibleHandlingSite {
            success_result_type_ids: &call.result_type_ids,
            error_return_type_id: call.error_type_id,
            value_required,
            value_required_location: call.call_location.clone(),
            compilation_stage: FUNCTION_CALL_STAGE,
            allow_boundary_catch,
        },
        warnings,
        string_table,
    )?;

    let Some(handling) = handling else {
        return Err(CompilerDiagnostic::invalid_result_handling(
            InvalidResultHandlingReason::ExpectedCatchBlockOrHandler,
            token_stream.current_location(),
        )
        .into());
    };

    if value_required && matches!(handling, FallibleHandling::Handler { .. }) {
        let call_location = call.call_location.clone();
        let result_type_ids = call.result_type_ids.clone();
        let handled_expression = Expression::handled_fallible_host_function_call_with_typed_arguments(
            crate::compiler_frontend::ast::expressions::expression::HandledFallibleHostFunctionCallInput {
                id: call.name,
                args: call.args,
                result_type_ids: result_type_ids.clone(),
                error_type_id: call.error_type_id,
                handling,
                location: call_location,
            },
            type_interner.environment_mut_for_derived_types(),
        );
        let catch_expression = wrap_value_producing_catch_expression(
            handled_expression,
            result_type_ids,
            true,
            type_interner.environment(),
        );

        return Ok(AstNode {
            kind: NodeKind::Rvalue(catch_expression),
            location: token_stream.current_location(),
            scope: context.scope.clone(),
        });
    }

    Ok(call.into_ast_node(handling, token_stream.current_location(), &context.scope))
}
