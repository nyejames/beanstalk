use crate::compiler_frontend::parsers::expressions::expression::Expression;
use crate::{
    compiler_frontend::{
        compiler_errors::CompilerError,
        datatypes::DataType,
        parsers::{
            ast::ScopeContext,
            ast_nodes::{AstNode, NodeKind, Var},
            statements::functions::parse_function_call,
            tokenizer::tokens::{FileTokens, TokenKind},
        },
        string_interning::StringTable,
        traits::ContainsReferences,
    },
    return_rule_error,
};

pub fn parse_field_access(
    token_stream: &mut FileTokens,
    base_arg: &Var,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    // Start with the base variable
    let mut current_node = AstNode {
        kind: NodeKind::Rvalue(Expression::reference(base_arg)),
        scope: base_arg.value.location.scope.to_owned(),
        location: base_arg.value.location.to_owned(),
    };

    // let built_in_methods = get_builtin_methods(current_type, string_table);

    let mut current_type = base_arg.value.data_type.clone();

    // Process each dot access in sequence
    while token_stream.current_token_kind() == &TokenKind::Dot {
        token_stream.advance();

        // Get the field/method name or index
        let field_id = match token_stream.current_token_kind() {
            TokenKind::Symbol(id) => *id,
            TokenKind::IntLiteral(val) => string_table.get_or_intern(val.to_string()),
            _ => return_rule_error!(
                format!(
                    "Expected property or method name after '.', found '{:?}'",
                    token_stream.current_token_kind()
                ),
                token_stream.current_location().to_error_location(string_table), {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Use a valid property or method name after the dot"
                }
            ),
        };

        // Get the base members
        let mut members = match &current_type {
            DataType::Struct(inner_args, ..) => inner_args.to_owned(),

            // TODO: Function returns
            // Needs to convert each return into a var that can be accessed
            // This will be done by giving each type a number to specify which return it is
            // DataType::Function(_, sig) => {}

            // Other types may have methods implemented on them
            _ => Vec::new(),
        };

        // Lookup methods implemented by this type and add them to the members list
        if let Some(arg) = context.get_reference(&field_id) {
            if let DataType::Function(receiver, ..) = &arg.value.data_type {
                if let Some(receiver_type) = receiver.as_ref() {
                    if *receiver_type == current_type {
                        members.push(arg.clone());
                    }
                }
            }
        }

        if members.is_empty() {
            return_rule_error!(
                format!(
                    "'{:?}' has no methods or properties to access",
                    current_type
                ),
                token_stream.current_location().to_error_location(string_table), {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "This type doesn't support property or method access"
                }
            );
        }

        // Find the accessed member
        let member = match members.iter().find(|m| m.id == field_id) {
            Some(member) => member.clone(),
            None => {
                return_rule_error!(
                    format!(
                        "Property or method '{}' not found",
                        string_table.resolve(field_id)
                    ),
                    token_stream.current_location().to_error_location(string_table), {
                        CompilationStage => "AST Construction",
                        PrimarySuggestion => "Check the available methods and properties for this type"
                    }
                )
            }
        };

        let field_location = token_stream.current_location();
        token_stream.advance();

        // Decide if this is a method call or field access
        current_node = if let DataType::Function(_, signature) = &member.value.data_type {
            // It's a method call
            parse_function_call(token_stream, &member.id, context, &signature, string_table)?
        } else {
            // It's a property access.
            AstNode {
                kind: NodeKind::FieldAccess {
                    base: Box::new(current_node),
                    field: field_id,
                    data_type: member.value.data_type.to_owned(),
                    ownership: member.value.ownership.to_owned(),
                },
                scope: context.scope.to_owned(),
                location: field_location,
            }
        };

        // Update current type for next iteration
        current_type = member.value.data_type;
    }

    Ok(current_node)
}
