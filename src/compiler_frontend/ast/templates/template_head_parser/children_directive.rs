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

#![allow(clippy::result_large_err)]
use super::directive_args::parse_required_parenthesized_expression;
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::template::TemplateType;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidTemplateDirectiveReason,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::FileTokens;
use crate::compiler_frontend::value_mode::ValueMode;

/// Parses the `$children(template_or_string)` directive which specifies a
/// wrapper template to apply around all direct child templates in the body.
pub(super) fn parse_children_style_directive(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    template: &mut Template,
    string_table: &mut StringTable,
) -> Result<(), CompilerDiagnostic> {
    let directive_argument = parse_required_parenthesized_expression(
        token_stream,
        context,
        type_interner,
        string_table,
    )?;
    let argument_location = directive_argument.location.clone();

    if !directive_argument.is_compile_time_constant() {
        return Err(CompilerDiagnostic::invalid_template_directive(
            Some(string_table.intern("children")),
            InvalidTemplateDirectiveReason::InvalidArgument,
            argument_location,
        ));
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
                return Err(CompilerDiagnostic::invalid_template_directive(
                    Some(string_table.intern("children")),
                    InvalidTemplateDirectiveReason::InvalidArgument,
                    argument_location,
                ));
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
            return Err(CompilerDiagnostic::invalid_template_directive(
                Some(string_table.intern("children")),
                InvalidTemplateDirectiveReason::InvalidArgument,
                argument_location,
            ));
        }
    };

    template.style.child_templates.push(normalized);
    Ok(())
}
