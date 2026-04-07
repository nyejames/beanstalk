use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::signatures::parse_parameters;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::FileTokens;
use crate::return_rule_error;

pub fn create_struct_definition(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Vec<Declaration>, CompilerError> {
    // Should start at the parameter bracket
    // Need to skip it,
    token_stream.advance();

    let arguments = parse_parameters(token_stream, &mut true, string_table, true, context)?;

    // Skip the Parameters token
    token_stream.advance();

    validate_struct_default_values(&arguments, string_table)?;

    Ok(arguments)
}

fn validate_struct_default_values(
    fields: &[Declaration],
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    for field in fields {
        if matches!(field.value.kind, ExpressionKind::NoValue) {
            continue;
        }

        if !field.value.is_compile_time_constant() {
            let field_name = field.id.name_str(string_table).unwrap_or("<field>");
            return_rule_error!(
                format!(
                    "Struct field '{}' default value must fold to a single compile-time value.",
                    field_name
                ),
                field.value.location.clone(), {
                    CompilationStage => "Struct/Parameter Parsing",
                    PrimarySuggestion => "Use only compile-time constants and constant expressions in struct default values",
                }
            );
        }
    }

    Ok(())
}

#[cfg(test)]
#[path = "tests/struct_parsing_tests.rs"]
mod struct_parsing_tests;
