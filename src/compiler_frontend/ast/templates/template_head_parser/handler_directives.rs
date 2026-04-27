//! Handler-based style directive parsing.
//!
//! WHAT:
//! - Parses optional typed handler arguments.
//! - Normalizes argument values into `StyleDirectiveArgumentValue`.
//! - Applies handler effects and executes formatter factory callbacks.
//!
//! WHY:
//! - Project-owned directives and frontend handler directives share one execution
//!   contract, so this logic should be centralized and isolated from core directives.

use super::directive_args::parse_optional_parenthesized_expression;
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::style_directives::{
    StyleDirectiveArgumentType, StyleDirectiveArgumentValue, StyleDirectiveEffects,
    StyleDirectiveHandlerSpec,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation};
use crate::return_syntax_error;

#[derive(Clone)]
struct ParsedHandlerDirectiveArgument {
    value: Option<StyleDirectiveArgumentValue>,
    error_location: SourceLocation,
}

pub(super) fn apply_handler_style_directive(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    template: &mut Template,
    directive_name: &str,
    handler_spec: &StyleDirectiveHandlerSpec,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    let parsed_argument = parse_optional_handler_style_argument(
        token_stream,
        context,
        directive_name,
        handler_spec.argument_type,
        string_table,
    )?;

    apply_style_directive_effects(template, handler_spec.effects);

    if let Some(factory) = handler_spec.formatter_factory {
        // Frontend parsing/folding always executes formatter factories here. Directive
        // definition modules own the factory and formatter implementation details.
        let formatter = factory(parsed_argument.value.as_ref()).map_err(|message| {
            CompilerError::new_syntax_error(&message, parsed_argument.error_location)
        })?;
        template.apply_style_updates(|style| {
            style.formatter = formatter.clone();
        });
    }

    Ok(())
}

fn apply_style_directive_effects(template: &mut Template, effects: StyleDirectiveEffects) {
    // Effects mutate semantic template style state. Formatter identity is set
    // separately by the optional formatter factory output.
    template.apply_style_updates(|style| {
        if let Some(style_id) = effects.style_id {
            style.id = style_id;
        }
        if let Some(policy) = effects.body_whitespace_policy {
            style.body_whitespace_policy = policy;
        }
        if let Some(suppress_child_templates) = effects.suppress_child_templates {
            style.suppress_child_templates = suppress_child_templates;
        }
        if let Some(skip_parent_wrappers) = effects.skip_parent_child_wrappers {
            style.skip_parent_child_wrappers = skip_parent_wrappers;
        }
    });
}

fn parse_optional_handler_style_argument(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    directive_name: &str,
    argument_type: Option<StyleDirectiveArgumentType>,
    string_table: &mut StringTable,
) -> Result<ParsedHandlerDirectiveArgument, CompilerError> {
    let default_location = token_stream.current_location();

    let Some(expression) =
        parse_optional_parenthesized_expression(token_stream, context, string_table)?
    else {
        return Ok(ParsedHandlerDirectiveArgument {
            value: None,
            error_location: default_location,
        });
    };

    let Some(argument_type) = argument_type else {
        return_syntax_error!(
            format!("'${directive_name}' does not accept arguments."),
            default_location
        );
    };

    let argument_location = expression.location.clone();

    if !expression.is_compile_time_constant() {
        return_syntax_error!(
            format!("'${directive_name}(...)' requires a compile-time argument value."),
            argument_location,
            {
                PrimarySuggestion => "Use a literal or constant value that folds at compile time",
            }
        );
    }

    let normalized = normalize_provided_style_argument_value(
        expression,
        argument_type,
        directive_name,
        &argument_location,
        string_table,
    )?;

    Ok(ParsedHandlerDirectiveArgument {
        value: Some(normalized),
        error_location: argument_location,
    })
}

fn normalize_provided_style_argument_value(
    expression: Expression,
    argument_type: StyleDirectiveArgumentType,
    directive_name: &str,
    argument_location: &SourceLocation,
    string_table: &StringTable,
) -> Result<StyleDirectiveArgumentValue, CompilerError> {
    match argument_type {
        StyleDirectiveArgumentType::String => match expression.kind {
            ExpressionKind::StringSlice(text) => Ok(StyleDirectiveArgumentValue::String(
                string_table.resolve(text).to_owned(),
            )),
            _ => {
                return_syntax_error!(
                    format!("'${directive_name}(...)' expects a compile-time string argument."),
                    argument_location.clone()
                )
            }
        },
        StyleDirectiveArgumentType::Template => match expression.kind {
            ExpressionKind::Template(template) => Ok(StyleDirectiveArgumentValue::Template(
                Box::new(*template.to_owned()),
            )),
            _ => {
                return_syntax_error!(
                    format!("'${directive_name}(...)' expects a compile-time template argument."),
                    argument_location.clone()
                )
            }
        },
        StyleDirectiveArgumentType::Number => match expression.kind {
            ExpressionKind::Int(value) => Ok(StyleDirectiveArgumentValue::Number(value as f64)),
            ExpressionKind::Float(value) => Ok(StyleDirectiveArgumentValue::Number(value)),
            _ => {
                return_syntax_error!(
                    format!("'${directive_name}(...)' expects a compile-time numeric argument."),
                    argument_location.clone()
                )
            }
        },
        StyleDirectiveArgumentType::Bool => match expression.kind {
            ExpressionKind::Bool(value) => Ok(StyleDirectiveArgumentValue::Bool(value)),
            _ => {
                return_syntax_error!(
                    format!("'${directive_name}(...)' expects a compile-time bool argument."),
                    argument_location.clone()
                )
            }
        },
    }
}
