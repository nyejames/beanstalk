//! Choice declaration/expression parsing helpers.
//!
//! WHAT: body-context choice expression parsing — `Choice::Variant` value construction.
//! Choice header shell parsing and the associated metadata types have moved to
//! `declaration_syntax::choice_shell` so the header stage can import them without going
//! through the AST module.
//!
//! WHY: keeping future choice/tagged-union expansion in one place instead of spreading
//! logic across header parsing and expression parsing modules.

use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::deferred_feature_diagnostics::deferred_feature_rule_error;
use crate::compiler_frontend::reserved_trait_syntax::{
    reserved_trait_keyword_error, reserved_trait_keyword_or_dispatch_mismatch,
};
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::{return_compiler_error, return_rule_error};

/// Parse `Choice::Variant` as a typed alpha choice value.
///
/// WHAT: resolves the variant against the declared choice and encodes the selected
/// variant as a deterministic internal tag index.
/// WHY: alpha reuses existing literal lowering (no new HIR expression variant) while
/// preserving full choice type identity on the expression.
pub(crate) fn parse_choice_variant_value(
    token_stream: &mut FileTokens,
    choice_declaration: &Declaration,
    string_table: &StringTable,
) -> Result<Expression, CompilerError> {
    let choice_name = choice_declaration
        .id
        .name_str(string_table)
        .unwrap_or("<choice>")
        .to_owned();

    let DataType::Choices(variants) = &choice_declaration.value.data_type else {
        return_compiler_error!(
            "Choice variant parser was called with a non-choice declaration '{}'.",
            choice_name
        );
    };

    token_stream.advance();
    if token_stream.current_token_kind() != &TokenKind::DoubleColon {
        return_compiler_error!(
            "Choice variant parser expected '::' after choice name '{}'.",
            choice_name
        );
    }

    token_stream.advance();
    token_stream.skip_newlines();

    let variant_location = token_stream.current_location();
    let variant_name = match token_stream.current_token_kind() {
        TokenKind::Symbol(name) => *name,
        TokenKind::Must | TokenKind::TraitThis => {
            let keyword = reserved_trait_keyword_or_dispatch_mismatch(
                token_stream.current_token_kind(),
                token_stream.current_location(),
                "Expression Parsing",
                "choice variant expression parsing",
            )?;

            return Err(reserved_trait_keyword_error(
                keyword,
                token_stream.current_location(),
                "Expression Parsing",
                "Use a normal choice variant name until traits are implemented",
            ));
        }
        _ => {
            return_rule_error!(
                format!("Expected a variant name after '{}::'.", choice_name),
                token_stream.current_location(),
                {
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "Use namespaced unit variant syntax like 'Choice::Variant'",
                }
            );
        }
    };

    let Some(variant_index) = variants
        .iter()
        .position(|variant| variant.id.name() == Some(variant_name))
    else {
        let available_variants = variants
            .iter()
            .filter_map(|variant| variant.id.name())
            .map(|name| string_table.resolve(name).to_owned())
            .collect::<Vec<_>>()
            .join(", ");

        return_rule_error!(
            format!(
                "Unknown variant '{}::{}'. Available variants: [{}].",
                choice_name,
                string_table.resolve(variant_name),
                available_variants
            ),
            variant_location,
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Use one of the declared variants for this choice",
            }
        );
    };

    token_stream.advance();
    if token_stream.current_token_kind() == &TokenKind::OpenParenthesis {
        return Err(deferred_feature_rule_error(
            format!(
                "Constructor-call syntax '{}::{}(...)' is deferred for Alpha.",
                choice_name,
                string_table.resolve(variant_name)
            ),
            token_stream.current_location(),
            "Expression Parsing",
            "Use unit variant values only for now: 'Choice::Variant'.",
        ));
    }

    Ok(Expression::new(
        ExpressionKind::Int(variant_index as i64),
        variant_location,
        choice_declaration.value.data_type.to_owned(),
        Ownership::ImmutableOwned,
    ))
}
