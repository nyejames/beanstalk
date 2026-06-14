//! Reactive template subscription parsing.
//!
//! WHAT:
//! - Parses the V1 `$(source)` head item grammar.
//! - Resolves the source identifier through the normal scope context.
//! - Attaches subscription metadata to the template content segment.
//!
//! WHY:
//! - Subscription syntax is intentionally narrower than expression interpolation. Keeping it
//!   isolated prevents general expression dependency tracking from leaking into templates.

#![allow(clippy::result_large_err)]

use super::head_expressions::push_template_head_reactive_subscription;
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionValueShape};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidTemplateStructureReason,
};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};

/// Parses and validates a `$(source)` template subscription.
///
/// The token stream enters on `TokenKind::Reactive` and exits on the token after the closing `)`.
pub(super) fn parse_reactive_subscription(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_environment: &TypeEnvironment,
    template: &mut Template,
    foldable: &mut bool,
) -> Result<(), CompilerDiagnostic> {
    let subscription_location = token_stream.current_location();

    token_stream.advance();
    if token_stream.current_token_kind() != &TokenKind::OpenParenthesis {
        return Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::ReactiveSubscriptionComplexExpression,
            subscription_location,
        ));
    }

    token_stream.advance();
    let source_name = match token_stream.current_token_kind() {
        TokenKind::CloseParenthesis => {
            return Err(CompilerDiagnostic::invalid_template_structure(
                InvalidTemplateStructureReason::ReactiveSubscriptionEmpty,
                token_stream.current_location(),
            ));
        }

        TokenKind::Symbol(source_name) => *source_name,

        _ => {
            return Err(CompilerDiagnostic::invalid_template_structure(
                InvalidTemplateStructureReason::ReactiveSubscriptionComplexExpression,
                token_stream.current_location(),
            ));
        }
    };

    let source_location = token_stream.current_location();

    token_stream.advance();
    if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
        let reason = if token_stream.current_token_kind() == &TokenKind::Comma {
            InvalidTemplateStructureReason::ReactiveSubscriptionMultipleSources
        } else {
            InvalidTemplateStructureReason::ReactiveSubscriptionComplexExpression
        };

        return Err(CompilerDiagnostic::invalid_template_structure(
            reason,
            token_stream.current_location(),
        ));
    }

    let Some(reference) = context.get_reference(&source_name) else {
        return Err(CompilerDiagnostic::unexpected_token(
            TokenKind::Symbol(source_name),
            source_location,
        ));
    };

    let Some(source) = reference.value.reactive_source.clone() else {
        return Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::ReactiveSubscriptionNonReactiveSource,
            source_location,
        ));
    };

    let mut expression = Expression::reference_with_type_id(
        reference.id.to_owned(),
        reference.value.diagnostic_type.to_owned(),
        reference.value.type_id,
        source_location,
        reference.value.value_mode.to_owned(),
        reference.value.const_record_state,
    )
    .with_reactive_source(source.clone());

    // Preserve explicit source shape (template, path, etc.) over the diagnostic-type
    // fallback used by the generic reference constructor.
    if reference.value.value_shape != ExpressionValueShape::Ordinary {
        expression.value_shape = reference.value.value_shape;
    }

    push_template_head_reactive_subscription(
        expression,
        source,
        context,
        type_environment,
        template,
        foldable,
        &subscription_location,
    )?;

    token_stream.advance();
    Ok(())
}
