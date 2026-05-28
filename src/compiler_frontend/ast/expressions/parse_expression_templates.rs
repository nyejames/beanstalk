//! Template expression parsing helpers.
//!
//! WHAT: parses template expressions and handles compile-time folding where applicable.
//! WHY: template expressions have distinct constant/runtime behavior and should not be buried in general token dispatch.

use super::error::ExpressionParseError;
use super::expression::Expression;
use crate::ast_log;
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::templates::template::TemplateType;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, InvalidTemplateSlotReason};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::value_mode::ValueMode;

// WHAT: parses a template literal and optionally folds it to a string slice expression.
// WHY: templates appear in expression positions but have their own grammar (slots, definitions,
//      comments). This function decides whether the template stays a runtime value or can be
//      folded at compile time.
pub(super) fn parse_template_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    consume_closing_parenthesis: bool,
    value_mode: &ValueMode,
    string_table: &mut StringTable,
) -> Result<Option<Expression>, ExpressionParseError> {
    let template_context = context.new_template_parsing_context();
    let template = Template::new_with_type_interner(
        token_stream,
        &template_context,
        type_interner,
        vec![],
        string_table,
    )
    .map_err(ExpressionParseError::from)?;

    match template.kind {
        TemplateType::StringFunction => {
            // In a constant context, return as Template rather than erroring immediately.
            // WHY: constant dependency ordering resolves referenced constants before parsing;
            // if the template is still runtime-only here, constant-header validation emits the
            // permanent "not compile-time resolvable" error.
            maybe_consume_closing_parenthesis(token_stream, consume_closing_parenthesis);
            Ok(Some(Expression::template(template, value_mode.to_owned())))
        }

        TemplateType::String => {
            maybe_consume_closing_parenthesis(token_stream, consume_closing_parenthesis);

            if !template.is_const_renderable_string() || template.has_unresolved_slots() {
                return Ok(Some(Expression::template(template, value_mode.to_owned())));
            }

            ast_log!("Template is foldable now. Folding...");

            let mut fold_context = template_context
                .new_template_fold_context(string_table, "expression parsing template fold")?;
            let folded_string = template.fold_into_stringid(&mut fold_context)?;

            Ok(Some(Expression::string_slice(
                folded_string,
                token_stream.current_location(),
                value_mode.as_owned(),
            )))
        }

        // Comment templates do not produce an expression.
        TemplateType::Comment(_) => Ok(None),

        TemplateType::SlotInsert(_) => {
            maybe_consume_closing_parenthesis(token_stream, consume_closing_parenthesis);
            Ok(Some(Expression::template(template, value_mode.to_owned())))
        }

        TemplateType::SlotDefinition(_) => {
            let diagnostic = CompilerDiagnostic::invalid_template_slot(
                InvalidTemplateSlotReason::SlotDefinitionOutsideTemplateBody,
                None,
                token_stream.current_location(),
            );

            Err(diagnostic.into())
        }
    }
}

// Consume a trailing `)` when requested and one is present.
fn maybe_consume_closing_parenthesis(token_stream: &mut FileTokens, should_consume: bool) {
    if should_consume && token_stream.current_token_kind() == &TokenKind::CloseParenthesis {
        token_stream.advance();
    }
}
