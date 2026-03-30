use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::functions::create_function_call_arguments;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::return_rule_error;

fn reference_base_node(
    reference_arg: &Declaration,
    context: &ScopeContext,
    base_location: SourceLocation,
) -> AstNode {
    if context.kind.is_constant_context() {
        let mut inlined_expression = reference_arg.value.to_owned();
        inlined_expression.ownership = Ownership::ImmutableOwned;
        AstNode {
            kind: NodeKind::Rvalue(inlined_expression),
            location: base_location,
            scope: context.scope.clone(),
        }
    } else {
        AstNode {
            kind: NodeKind::Rvalue(Expression::reference(
                reference_arg.id.to_owned(),
                reference_arg.value.data_type.to_owned(),
                base_location.clone(),
                reference_arg.value.ownership.to_owned(),
            )),
            scope: context.scope.to_owned(),
            location: base_location,
        }
    }
}

fn current_node_type(node: &AstNode) -> Result<DataType, CompilerError> {
    Ok(node.get_expr()?.data_type)
}

pub(crate) fn ast_node_is_place(node: &AstNode) -> bool {
    match &node.kind {
        NodeKind::Rvalue(expr) => matches!(expr.kind, ExpressionKind::Reference(_)),
        NodeKind::FieldAccess { base, .. } => ast_node_is_place(base),
        _ => false,
    }
}

pub(crate) fn ast_node_is_mutable_place(node: &AstNode) -> bool {
    match &node.kind {
        NodeKind::Rvalue(expr) => {
            matches!(expr.kind, ExpressionKind::Reference(_)) && expr.ownership.is_mutable()
        }
        NodeKind::FieldAccess { base, .. } => ast_node_is_mutable_place(base),
        _ => false,
    }
}

fn parse_member_name(
    token_stream: &FileTokens,
    string_table: &mut StringTable,
) -> Result<StringId, CompilerError> {
    match token_stream.current_token_kind() {
        TokenKind::Symbol(id) => Ok(*id),
        TokenKind::IntLiteral(value) => Ok(string_table.get_or_intern(value.to_string())),
        _ => return_rule_error!(
            format!(
                "Expected property or method name after '.', found '{:?}'",
                token_stream.current_token_kind()
            ),
            token_stream.current_location(),
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Use a valid property or method name after the dot",
            }
        ),
    }
}

fn field_member(current_type: &DataType, field_id: StringId) -> Option<Declaration> {
    current_type.struct_fields().and_then(|fields| {
        fields
            .iter()
            .find(|field| field.id.name() == Some(field_id))
            .cloned()
    })
}

pub(crate) fn parse_postfix_chain(
    token_stream: &mut FileTokens,
    mut current_node: AstNode,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    // WHAT: parses chained postfix member access and receiver method calls (`a.b.c(...)`).
    // WHY: assignment parsing, expression parsing, and mutation all share the same postfix rules,
    //      so one parser keeps mutable-place checks and receiver lookup consistent.
    while token_stream.index < token_stream.length
        && token_stream.current_token_kind() == &TokenKind::Dot
    {
        token_stream.advance();

        if token_stream.index >= token_stream.length {
            let fallback_location = token_stream
                .tokens
                .last()
                .map(|token| token.location.clone())
                .unwrap_or_default();
            return_rule_error!(
                "Expected property or method name after '.', but reached the end of input.",
                fallback_location,
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Add a property or method name after the dot",
                }
            );
        }

        let member_name = parse_member_name(token_stream, string_table)?;
        let current_type = current_node_type(&current_node)?;
        let field_member = field_member(&current_type, member_name);
        let receiver_method = if current_type.is_const_record_struct() {
            current_type
                .struct_nominal_path()
                .map(|path| {
                    crate::compiler_frontend::datatypes::ReceiverKey::Struct(path.to_owned())
                })
                .as_ref()
                .and_then(|receiver| context.lookup_receiver_method(receiver, member_name))
        } else {
            current_type
                .receiver_key_from_type()
                .as_ref()
                .and_then(|receiver| context.lookup_receiver_method(receiver, member_name))
        };
        let member_location = token_stream.current_location();

        if let Some(field) = field_member {
            token_stream.advance();

            if token_stream.current_token_kind() == &TokenKind::OpenParenthesis {
                return_rule_error!(
                    format!(
                        "'{}' is a field, not a receiver method. Dot-call syntax is reserved for declared receiver methods.",
                        string_table.resolve(member_name)
                    ),
                    member_location,
                    {
                        CompilationStage => "AST Construction",
                        PrimarySuggestion => "Remove the parentheses to read the field, or declare a receiver method with this name instead",
                    }
                );
            }

            current_node =
                if context.kind.is_constant_context() && field.value.is_compile_time_constant() {
                    let mut inlined_expression = field.value;
                    inlined_expression.ownership = Ownership::ImmutableOwned;
                    AstNode {
                        kind: NodeKind::Rvalue(inlined_expression),
                        scope: context.scope.clone(),
                        location: member_location,
                    }
                } else {
                    AstNode {
                        kind: NodeKind::FieldAccess {
                            base: Box::new(current_node),
                            field: member_name,
                            data_type: field.value.data_type,
                            ownership: field.value.ownership,
                        },
                        scope: context.scope.to_owned(),
                        location: member_location,
                    }
                };
            continue;
        }

        let Some(method_entry) = receiver_method else {
            return_rule_error!(
                format!(
                    "Property or method '{}' not found for '{}'.",
                    string_table.resolve(member_name),
                    current_type.display_with_table(string_table)
                ),
                member_location,
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Check the available fields and receiver methods for this type",
                }
            );
        };

        if current_type.is_const_record_struct() {
            return_rule_error!(
                format!(
                    "Const struct records are data-only and do not support runtime method calls like '{}'.",
                    string_table.resolve(member_name)
                ),
                member_location,
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Call methods on a runtime struct value instead of a '#'-coerced const record",
                }
            );
        }

        if token_stream.peek_next_token() != Some(&TokenKind::OpenParenthesis) {
            return_rule_error!(
                format!(
                    "'{}' is a receiver method and must be called with parentheses.",
                    string_table.resolve(member_name)
                ),
                member_location,
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Call the method with 'value.method(...)'",
                }
            );
        }

        token_stream.advance();

        if method_entry.receiver_mutable && !ast_node_is_mutable_place(&current_node) {
            return_rule_error!(
                format!(
                    "Mutable receiver method '{}.{}(...)' requires a mutable place receiver.",
                    current_type.display_with_table(string_table),
                    string_table.resolve(member_name)
                ),
                member_location,
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Call this mutable method on a mutable variable or mutable field path, not on a temporary or immutable value",
                }
            );
        }

        let args = create_function_call_arguments(
            token_stream,
            &method_entry.signature.parameters[1..],
            context,
            string_table,
        )?;
        let result_types = method_entry.signature.return_data_types();

        current_node = AstNode {
            kind: NodeKind::MethodCall {
                receiver: Box::new(current_node),
                method_path: method_entry.function_path.to_owned(),
                method: member_name,
                args,
                result_types,
                location: member_location.clone(),
            },
            scope: context.scope.to_owned(),
            location: member_location,
        };
    }

    Ok(current_node)
}

pub fn parse_field_access(
    token_stream: &mut FileTokens,
    base_arg: &Declaration,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    let base_location = if token_stream.index > 0 {
        token_stream.tokens[token_stream.index - 1].location.clone()
    } else {
        token_stream.current_location()
    };

    parse_postfix_chain(
        token_stream,
        reference_base_node(base_arg, context, base_location),
        context,
        string_table,
    )
}
