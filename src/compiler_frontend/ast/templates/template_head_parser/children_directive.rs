//! `$children(..)` directive parsing and normalization.
//!
//! WHAT:
//! - Parses `$children(template_or_string)` arguments.
//! - Validates compile-time restrictions and accepted argument types.
//! - Normalizes string arguments into wrapper templates.
//!
//! WHY:
//! - `$children(..)` has directive-specific compile-time behavior that should stay
//!   isolated from generic style-handler logic.

use super::directive_args::parse_required_parenthesized_expression;
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::template::TemplateType;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::FileTokens;
use crate::compiler_frontend::value_mode::ValueMode;
use crate::return_syntax_error;

/// Parses the `$children(template_or_string)` directive which specifies a
/// wrapper template to apply around all direct child templates in the body.
pub(super) fn parse_children_style_directive(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    template: &mut Template,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    let directive_argument =
        parse_required_parenthesized_expression(token_stream, context, string_table)?;
    let argument_location = directive_argument.location.clone();

    if !directive_argument.is_compile_time_constant() {
        return_syntax_error!(
            "The '$children(..)' directive only accepts compile-time values.",
            argument_location,
            {
                PrimarySuggestion => "Use a template literal, string literal, or constant reference that folds at compile time",
            }
        );
    }

    let normalized = match directive_argument.kind {
        ExpressionKind::Template(child_template) => {
            if matches!(
                child_template.kind,
                TemplateType::StringFunction
                    | TemplateType::SlotDefinition(_)
                    | TemplateType::SlotInsert(_)
                    | TemplateType::Comment(_)
            ) {
                return_syntax_error!(
                    "The '$children(..)' directive only accepts compile-time template/string values.",
                    argument_location
                );
            }

            child_template.as_ref().to_owned()
        }

        ExpressionKind::StringSlice(value) => {
            let mut wrapper = Template::empty();
            wrapper.kind = TemplateType::String;
            wrapper.location = argument_location.to_owned();
            wrapper.content.add(Expression::string_slice(
                value,
                argument_location,
                ValueMode::ImmutableOwned,
            ));
            wrapper.resync_composition_metadata();
            wrapper
        }

        _ => {
            return_syntax_error!(
                "The '$children(..)' directive only accepts template or string arguments.",
                argument_location
            )
        }
    };

    template.style.child_templates.push(normalized);
    Ok(())
}
