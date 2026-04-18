//! Template expression parsing helpers.
//!
//! WHAT: parses template expressions and handles compile-time folding where applicable.
//! WHY: template expressions have distinct constant/runtime behavior and should not be buried in general token dispatch.

use super::expression::Expression;
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::templates::template::TemplateType;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::Ownership;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::{ast_log, return_rule_error};

pub(super) fn parse_template_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    consume_closing_parenthesis: bool,
    ownership: &Ownership,
    string_table: &mut StringTable,
) -> Result<Option<Expression>, CompilerError> {
    let template_context = context.new_template_parsing_context();
    let template = Template::new(token_stream, &template_context, vec![], string_table)?;

    match template.kind {
        TemplateType::StringFunction => {
            // In a constant context, return as Template rather than erroring immediately.
            // WHY: template slots may reference unresolved constants; the deferred constant
            // resolution loop retries after dependencies resolve. If the template is still
            // StringFunction after all constants are known, the loop emits a permanent
            // "not compile-time resolvable" error via parse_constant_header_declaration.
            if consume_closing_parenthesis
                && token_stream.current_token_kind() == &TokenKind::CloseParenthesis
            {
                token_stream.advance();
            }
            Ok(Some(Expression::template(template, ownership.to_owned())))
        }

        TemplateType::String => {
            if consume_closing_parenthesis
                && token_stream.current_token_kind() == &TokenKind::CloseParenthesis
            {
                token_stream.advance();
            }

            if !template.is_const_renderable_string() || template.has_unresolved_slots() {
                return Ok(Some(Expression::template(template, ownership.to_owned())));
            }

            ast_log!("Template is foldable now. Folding...");

            let mut fold_context = template_context
                .new_template_fold_context(string_table, "expression parsing template fold")?;
            let folded_string = template.fold_into_stringid(&mut fold_context)?;

            Ok(Some(Expression::string_slice(
                folded_string,
                token_stream.current_location(),
                ownership.get_owned(),
            )))
        }

        // Ignore comments
        TemplateType::Comment(_) => Ok(None),

        TemplateType::SlotInsert(_) => {
            if consume_closing_parenthesis
                && token_stream.current_token_kind() == &TokenKind::CloseParenthesis
            {
                token_stream.advance();
            }

            Ok(Some(Expression::template(template, ownership.to_owned())))
        }

        TemplateType::SlotDefinition(_) => {
            return_rule_error!(
                "'$slot' markers are only valid as direct nested templates inside template bodies.",
                token_stream.current_location(),
                {
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "Use '$slot' inside a template body where it defines a receiving slot",
                }
            )
        }
    }
}
