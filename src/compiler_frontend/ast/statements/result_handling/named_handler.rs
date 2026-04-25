//! Named result-handler parsing helpers.
//!
//! WHAT: parses `err! ... : ... ;` handler scopes and produces shared handler payload state.
//! WHY: call and expression result handling share identical named-handler syntax and validation,
//! so this module removes duplicated parser paths.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::function_body_to_ast;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::compiler_frontend::value_mode::ValueMode;
use crate::return_syntax_error;

use super::fallback::parse_named_handler_fallback;
use super::validation::{
    validate_named_result_handler_binding, validate_named_result_handler_conflict,
    validate_named_result_handler_value_requirement,
};

pub(super) struct NamedResultHandler {
    pub(super) error_name: StringId,
    pub(super) error_binding: InternedPath,
    pub(super) fallback: Option<Vec<Expression>>,
    pub(super) body: Vec<AstNode>,
}

pub(super) struct NamedResultHandlerSite<'a> {
    pub(super) success_result_types: &'a [DataType],
    pub(super) error_return_type: &'a DataType,
    pub(super) value_required: bool,
    pub(super) compilation_stage: &'a str,
    pub(super) scope_suggestion: &'a str,
    pub(super) bare_handler_suggestion: &'a str,
    pub(super) value_required_location: SourceLocation,
}

pub(super) fn parse_named_result_handler(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    site: NamedResultHandlerSite<'_>,
    warnings: Option<&mut Vec<CompilerWarning>>,
    string_table: &mut StringTable,
) -> Result<NamedResultHandler, CompilerError> {
    let TokenKind::Symbol(handler_name) = token_stream.current_token_kind().to_owned() else {
        return_syntax_error!(
            "Expected a named handler identifier before '!'.",
            token_stream.current_location(),
            {
                CompilationStage => site.compilation_stage,
                PrimarySuggestion => "Use syntax like 'err!: ... ;' to start a named handler",
            }
        );
    };

    let handler_name_location = token_stream.current_location();
    let handler_name_text = string_table.resolve(handler_name).to_owned();

    let mut local_handler_warnings: Vec<CompilerWarning> = Vec::new();
    let warnings = match warnings {
        Some(warnings) => warnings,
        None => &mut local_handler_warnings,
    };

    validate_named_result_handler_binding(
        &handler_name_text,
        handler_name_location.to_owned(),
        site.compilation_stage,
        warnings,
    )?;

    validate_named_result_handler_conflict(
        context,
        handler_name,
        &handler_name_text,
        handler_name_location.to_owned(),
        site.compilation_stage,
    )?;

    token_stream.advance();

    if token_stream.current_token_kind() != &TokenKind::Bang {
        return_syntax_error!(
            "Expected '!' after named handler identifier.",
            token_stream.current_location(),
            {
                CompilationStage => site.compilation_stage,
                PrimarySuggestion => "Add '!' after the handler name to start result handling",
                SuggestedInsertion => "!",
            }
        );
    }

    token_stream.advance();

    let handler_fallback = parse_named_handler_fallback(
        token_stream,
        context,
        site.success_result_types,
        string_table,
        &handler_name_text,
        site.compilation_stage,
        site.bare_handler_suggestion,
    )?;

    if token_stream.current_token_kind() != &TokenKind::Colon {
        return_syntax_error!(
            "Expected ':' to start the named handler scope.",
            token_stream.current_location(),
            {
                CompilationStage => site.compilation_stage,
                PrimarySuggestion => site.scope_suggestion,
                SuggestedInsertion => ":",
            }
        );
    }

    token_stream.advance();

    let mut handler_context = context.new_child_control_flow(ContextKind::Branch, string_table);
    let handler_error_id = handler_context.scope.append(handler_name);
    handler_context.add_var(Declaration {
        id: handler_error_id.to_owned(),
        value: Expression::no_value(
            token_stream.current_location(),
            site.error_return_type.to_owned(),
            ValueMode::ImmutableOwned,
        ),
    });

    let handler_body = function_body_to_ast(token_stream, handler_context, warnings, string_table)?;

    validate_named_result_handler_value_requirement(
        &handler_fallback,
        site.value_required,
        site.success_result_types,
        &handler_body,
        site.value_required_location,
        site.compilation_stage,
    )?;

    Ok(NamedResultHandler {
        error_name: handler_name,
        error_binding: handler_error_id,
        fallback: handler_fallback,
        body: handler_body,
    })
}
