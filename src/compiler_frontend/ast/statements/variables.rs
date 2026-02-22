use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::field_access::parse_field_access;
use crate::compiler_frontend::ast::function_body_to_ast::function_body_to_ast;
use crate::compiler_frontend::ast::statements::functions::{
    FunctionSignature, parse_function_call,
};
use crate::compiler_frontend::ast::statements::structs::create_struct_definition;
use crate::compiler_frontend::ast::{
    ast_nodes::Var, expressions::parse_expression::create_expression,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::{ast_log, return_rule_error, return_syntax_error};

pub fn create_reference(
    token_stream: &mut FileTokens,
    reference_arg: &Var,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    // Move past the name
    token_stream.advance();

    match reference_arg.value.data_type {
        // Function Call
        DataType::Function(_, ref signature) => parse_function_call(
            token_stream,
            &reference_arg.id,
            context,
            signature,
            string_table,
        ),

        _ => {
            // This either becomes a reference or field access
            parse_field_access(token_stream, reference_arg, context, string_table)
        }
    }
}

pub fn new_var(
    token_stream: &mut FileTokens,
    id: StringId,
    context: &ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<Var, CompilerError> {
    // Move past the name
    token_stream.advance();

    let full_name = context.scope.to_owned().append(id);

    let mut ownership = Ownership::ImmutableOwned;

    if token_stream.current_token_kind() == &TokenKind::Mutable {
        token_stream.advance();
        ownership = Ownership::MutableOwned;
    };

    let mut data_type: DataType;

    match token_stream.current_token_kind() {
        // Go straight to the assignment
        TokenKind::Assign => {
            // Cringe Code
            // This whole function can be reworked to avoid this go_back() later.
            // For now, it's easy to read and parse this way while working on the specifics of the syntax
            token_stream.go_back();
            data_type = DataType::Inferred;
        }

        TokenKind::TypeParameterBracket => {
            let func_sig = FunctionSignature::new(token_stream, string_table, &full_name)?;
            let func_context = context.new_child_function(id, func_sig.to_owned(), string_table);

            // TODO: fast check for function without signature
            // let context = context.new_child_function(name, &[]);
            // return Ok(Arg {
            //     name: name.to_owned(),
            //     value: Expression::function_without_signature(
            //         new_ast(token_stream, context, false)?.ast,
            //         token_stream.current_location(),
            //     ),
            // });

            let function_body = function_body_to_ast(
                token_stream,
                func_context.to_owned(),
                warnings,
                string_table,
            )?;

            return Ok(Var {
                id: full_name,
                value: Expression::function(
                    None,
                    func_sig,
                    function_body,
                    token_stream.current_location(),
                ),
            });
        }

        // Has a type declaration
        TokenKind::DatatypeInt => data_type = DataType::Int,
        TokenKind::DatatypeFloat => data_type = DataType::Float,
        TokenKind::DatatypeBool => data_type = DataType::Bool,
        TokenKind::DatatypeString => data_type = DataType::String,

        // Collection Type Declaration
        TokenKind::OpenCurly => {
            token_stream.advance();

            // Check if there is a type inside the curly braces
            data_type = match token_stream.current_token_kind().to_datatype() {
                Some(data_type) => DataType::Collection(Box::new(data_type), ownership.to_owned()),
                _ => DataType::Collection(Box::new(DataType::Inferred), Ownership::MutableOwned),
            };

            // Make sure there is a closing curly brace
            if token_stream.current_token_kind() != &TokenKind::CloseCurly {
                return_syntax_error!(
                    "Missing closing curly brace for collection type declaration",
                    token_stream.current_location().to_error_location(string_table), {
                        CompilationStage => "Variable Declaration",
                        PrimarySuggestion => "Add '}' to close the collection type declaration",
                        SuggestedInsertion => "}",
                    }
                )
            }
        }

        TokenKind::Newline => {
            data_type = DataType::Inferred;
            // Ignore
        }

        TokenKind::Colon => {
            let struct_def = create_struct_definition(token_stream, string_table, &full_name)?;

            return Ok(Var {
                id: full_name,
                value: Expression::struct_definition(
                    struct_def,
                    token_stream.current_location(),
                    ownership,
                ),
            });
        }

        // SYNTAX ERRORS
        // Probably a missing reference or import
        TokenKind::Dot
        | TokenKind::AddAssign
        | TokenKind::SubtractAssign
        | TokenKind::DivideAssign
        | TokenKind::MultiplyAssign => {
            return_syntax_error!(
                format!(
                    "{} is undefined. Can't use {:?} after an undefined variable. Either define this variable first, import it or make sure its in scope.",
                    string_table.resolve(id),
                    token_stream.tokens[token_stream.index].kind
                ),
                token_stream.current_location().to_error_location(string_table), {
                    CompilationStage => "Variable Declaration",
                    PrimarySuggestion => "Make sure to import or define this variable before using it.",
                }
            )
        }

        // Other kinds of syntax errors
        _ => {
            return_syntax_error!(
                format!(
                    "Invalid token: {:?} after new variable declaration. Expect a type or assignment operator.",
                    token_stream.tokens[token_stream.index].kind
                ),
                token_stream.current_location().to_error_location(string_table), {
                    CompilationStage => "Variable Declaration",
                    PrimarySuggestion => "Use a type declaration (Int, String, etc.) or assignment operator '='",
                }
            )
        }
    };

    // Check for the assignment operator next
    // If this is parameters or a struct, then we can instead break out with a comma or struct close bracket
    token_stream.advance();

    match token_stream.current_token_kind() {
        TokenKind::Assign => {
            token_stream.advance();
        }

        // If end of statement, then it's unassigned.
        // For the time being, this is a syntax error.
        // When the compiler_frontend becomes more sophisticated,
        // it will be possible to statically ensure there is an assignment on all future branches.

        // Struct bracket should only be hit here in the context of the end of some parameters
        TokenKind::Comma
        | TokenKind::Eof
        | TokenKind::Newline
        | TokenKind::TypeParameterBracket => {
            let var_name = string_table.resolve(id);
            return_rule_error!(
                format!("Variable '{}' must be initialized with a value", var_name),
                token_stream.current_location().to_error_location(string_table), {
                    // VariableName => var_name,
                    CompilationStage => "Variable Declaration",
                    PrimarySuggestion => "Add '= value' after the variable declaration",
                }
            )
        }

        _ => {
            return_syntax_error!(
                format!(
                    "Unexpected Token: {:?}. Are you trying to reference a variable that doesn't exist yet?",
                    token_stream.current_token_kind()
                ),
                token_stream.current_location().to_error_location(string_table), {
                    CompilationStage => "Variable Declaration",
                    PrimarySuggestion => "Check that all referenced variables are declared before use",
                }
            )
        }
    }

    // The current token should be whatever is after the assignment operator

    // Check if this whole expression is nested in brackets.
    // This is just so we don't wastefully call create_expression recursively right away
    let parsed_expr = match token_stream.current_token_kind() {
        // Struct Definition
        // TokenKind::TypeParameterBracket => {
        //     // TODO
        // }
        TokenKind::OpenParenthesis => {
            token_stream.advance();
            create_expression(
                token_stream,
                context,
                &mut data_type,
                &ownership,
                true,
                string_table,
            )?
        }

        _ => create_expression(
            token_stream,
            context,
            &mut data_type,
            &ownership,
            false,
            string_table,
        )?,
    };

    ast_log!("Created new ", #ownership, " ", data_type);

    Ok(Var {
        id: full_name,
        value: parsed_expr,
    })
}
