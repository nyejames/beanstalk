use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::functions::create_function_call_arguments;
use crate::compiler_frontend::builtins::BuiltinMethodKind;
use crate::compiler_frontend::builtins::error_type::{
    ERROR_HELPER_BUBBLE, ERROR_HELPER_PUSH_TRACE, ERROR_HELPER_WITH_LOCATION,
    is_builtin_error_data_type, resolve_builtin_error_location_type, resolve_builtin_error_type,
    resolve_builtin_stack_frame_type,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::host_functions::{
    COLLECTION_GET_HOST_NAME, COLLECTION_LENGTH_HOST_NAME, COLLECTION_PUSH_HOST_NAME,
    COLLECTION_REMOVE_HOST_NAME, ERROR_BUBBLE_HOST_NAME, ERROR_PUSH_TRACE_HOST_NAME,
    ERROR_WITH_LOCATION_HOST_NAME,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::reserved_trait_syntax::{
    reserved_trait_keyword, reserved_trait_keyword_error,
};
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::return_rule_error;

const COLLECTION_GET_NAME: &str = "get";
const COLLECTION_SET_NAME: &str = "set";
const COLLECTION_PUSH_NAME: &str = "push";
const COLLECTION_REMOVE_NAME: &str = "remove";
const COLLECTION_PULL_NAME: &str = "pull";
const COLLECTION_LENGTH_NAME: &str = "length";

const COLLECTION_BUILTIN_SET_PATH: &str = "__bs_collection_set";

#[derive(Clone, Copy, PartialEq, Eq)]
enum CollectionBuiltinMethod {
    Get,
    Set,
    Push,
    Remove,
    Length,
    PullDeprecated,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ErrorBuiltinMethod {
    WithLocation,
    PushTrace,
    Bubble,
}

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

fn is_assignment_operator(token: &TokenKind) -> bool {
    matches!(
        token,
        TokenKind::Assign
            | TokenKind::AddAssign
            | TokenKind::SubtractAssign
            | TokenKind::MultiplyAssign
            | TokenKind::DivideAssign
            | TokenKind::ExponentAssign
            | TokenKind::RootAssign
    )
}

pub(crate) fn ast_node_is_place(node: &AstNode) -> bool {
    match &node.kind {
        NodeKind::Rvalue(expr) => matches!(expr.kind, ExpressionKind::Reference(_)),
        NodeKind::FieldAccess { base, .. } => ast_node_is_place(base),
        NodeKind::MethodCall {
            receiver,
            builtin: Some(BuiltinMethodKind::CollectionGet),
            ..
        } => ast_node_is_place(receiver),
        _ => false,
    }
}

pub(crate) fn ast_node_is_mutable_place(node: &AstNode) -> bool {
    match &node.kind {
        NodeKind::Rvalue(expr) => {
            matches!(expr.kind, ExpressionKind::Reference(_)) && expr.ownership.is_mutable()
        }
        NodeKind::FieldAccess { base, .. } => ast_node_is_mutable_place(base),
        NodeKind::MethodCall {
            receiver,
            builtin: Some(BuiltinMethodKind::CollectionGet),
            ..
        } => ast_node_is_mutable_place(receiver),
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
        TokenKind::Must | TokenKind::TraitThis => {
            let keyword = reserved_trait_keyword(token_stream.current_token_kind())
                .expect("reserved trait token should map to a keyword");

            return Err(reserved_trait_keyword_error(
                keyword,
                token_stream.current_location(),
                "AST Construction",
                "Use a normal field or receiver method name until traits are implemented",
            ));
        }
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

fn collection_builtin_method_name(
    member_name: StringId,
    string_table: &StringTable,
) -> Option<CollectionBuiltinMethod> {
    match string_table.resolve(member_name) {
        COLLECTION_GET_NAME => Some(CollectionBuiltinMethod::Get),
        COLLECTION_SET_NAME => Some(CollectionBuiltinMethod::Set),
        COLLECTION_PUSH_NAME => Some(CollectionBuiltinMethod::Push),
        COLLECTION_REMOVE_NAME => Some(CollectionBuiltinMethod::Remove),
        COLLECTION_PULL_NAME => Some(CollectionBuiltinMethod::PullDeprecated),
        COLLECTION_LENGTH_NAME => Some(CollectionBuiltinMethod::Length),
        _ => None,
    }
}

fn collection_builtin_path(
    builtin: CollectionBuiltinMethod,
    string_table: &mut StringTable,
) -> InternedPath {
    let builtin_name = match builtin {
        CollectionBuiltinMethod::Get => COLLECTION_GET_HOST_NAME,
        CollectionBuiltinMethod::Set => COLLECTION_BUILTIN_SET_PATH,
        CollectionBuiltinMethod::Push => COLLECTION_PUSH_HOST_NAME,
        CollectionBuiltinMethod::Remove => COLLECTION_REMOVE_HOST_NAME,
        CollectionBuiltinMethod::Length => COLLECTION_LENGTH_HOST_NAME,
        CollectionBuiltinMethod::PullDeprecated => unreachable!(),
    };

    InternedPath::from_single_str(builtin_name, string_table)
}

fn collection_builtin_kind(builtin: CollectionBuiltinMethod) -> BuiltinMethodKind {
    match builtin {
        CollectionBuiltinMethod::Get => BuiltinMethodKind::CollectionGet,
        CollectionBuiltinMethod::Set => BuiltinMethodKind::CollectionSet,
        CollectionBuiltinMethod::Push => BuiltinMethodKind::CollectionPush,
        CollectionBuiltinMethod::Remove => BuiltinMethodKind::CollectionRemove,
        CollectionBuiltinMethod::Length => BuiltinMethodKind::CollectionLength,
        CollectionBuiltinMethod::PullDeprecated => unreachable!(),
    }
}

fn error_builtin_method_name(
    member_name: StringId,
    string_table: &StringTable,
) -> Option<ErrorBuiltinMethod> {
    match string_table.resolve(member_name) {
        ERROR_HELPER_WITH_LOCATION => Some(ErrorBuiltinMethod::WithLocation),
        ERROR_HELPER_PUSH_TRACE => Some(ErrorBuiltinMethod::PushTrace),
        ERROR_HELPER_BUBBLE => Some(ErrorBuiltinMethod::Bubble),
        _ => None,
    }
}

fn error_builtin_path(builtin: ErrorBuiltinMethod, string_table: &mut StringTable) -> InternedPath {
    let builtin_name = match builtin {
        ErrorBuiltinMethod::WithLocation => ERROR_WITH_LOCATION_HOST_NAME,
        ErrorBuiltinMethod::PushTrace => ERROR_PUSH_TRACE_HOST_NAME,
        ErrorBuiltinMethod::Bubble => ERROR_BUBBLE_HOST_NAME,
    };

    InternedPath::from_single_str(builtin_name, string_table)
}

fn error_builtin_kind(builtin: ErrorBuiltinMethod) -> BuiltinMethodKind {
    match builtin {
        ErrorBuiltinMethod::WithLocation => BuiltinMethodKind::ErrorWithLocation,
        ErrorBuiltinMethod::PushTrace => BuiltinMethodKind::ErrorPushTrace,
        ErrorBuiltinMethod::Bubble => BuiltinMethodKind::ErrorBubble,
    }
}

fn parse_builtin_method_args(
    token_stream: &mut FileTokens,
    expected_types: &[DataType],
    context: &ScopeContext,
    member_location: &SourceLocation,
    string_table: &mut StringTable,
) -> Result<Vec<Expression>, CompilerError> {
    if expected_types.is_empty() {
        if token_stream.current_token_kind() != &TokenKind::OpenParenthesis {
            return_rule_error!(
                "Builtin method call is missing '(' before the argument list.",
                token_stream.current_location(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Call the method with parentheses, for example '.length()'",
                }
            );
        }

        token_stream.advance();

        if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
            return_rule_error!(
                "This builtin method takes no arguments.",
                token_stream.current_location(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Remove the extra argument",
                }
            );
        }

        token_stream.advance();
        return Ok(Vec::new());
    }

    let signature_params = expected_types
        .iter()
        .enumerate()
        .map(|(index, expected_type)| Declaration {
            id: InternedPath::from_single_str(&format!("__builtin_arg_{index}"), string_table),
            value: Expression::no_value(
                member_location.to_owned(),
                expected_type.to_owned(),
                Ownership::ImmutableReference,
            ),
        })
        .collect::<Vec<_>>();

    create_function_call_arguments(token_stream, &signature_params, context, string_table)
}

fn parse_collection_builtin_member(
    token_stream: &mut FileTokens,
    current_node: AstNode,
    current_type: &DataType,
    member_name: StringId,
    member_location: SourceLocation,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Option<AstNode>, CompilerError> {
    let DataType::Collection(inner_type, _) = current_type else {
        return Ok(None);
    };

    let Some(builtin) = collection_builtin_method_name(member_name, string_table) else {
        return Ok(None);
    };

    if matches!(builtin, CollectionBuiltinMethod::PullDeprecated) {
        return_rule_error!(
            "Collection method 'pull(...)' was removed. Use 'remove(index)' instead.",
            member_location,
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Replace '.pull(index)' with '.remove(index)'",
            }
        );
    }

    if token_stream.peek_next_token() != Some(&TokenKind::OpenParenthesis) {
        return_rule_error!(
            format!(
                "Collection method '{}' must be called with parentheses.",
                string_table.resolve(member_name)
            ),
            member_location,
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Call this collection method with '(...)'",
            }
        );
    }

    if matches!(builtin, CollectionBuiltinMethod::Set) && !ast_node_is_mutable_place(&current_node)
    {
        return_rule_error!(
            "Collection 'set(index, value)' requires a mutable collection receiver.",
            member_location,
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Call 'set' on a mutable collection variable or mutable field path",
            }
        );
    }

    token_stream.advance();

    let (args, result_types) = match builtin {
        CollectionBuiltinMethod::Get => {
            let args = parse_builtin_method_args(
                token_stream,
                &[DataType::Int],
                context,
                &member_location,
                string_table,
            )?;
            let error_type = resolve_builtin_error_type(context, &member_location, string_table)?;
            let get_result_type = DataType::Result {
                ok: Box::new(inner_type.as_ref().to_owned()),
                err: Box::new(error_type),
            };
            (args, vec![get_result_type])
        }
        CollectionBuiltinMethod::Set => {
            let args = parse_builtin_method_args(
                token_stream,
                &[DataType::Int, inner_type.as_ref().to_owned()],
                context,
                &member_location,
                string_table,
            )?;
            (args, Vec::new())
        }
        CollectionBuiltinMethod::Push => {
            let args = parse_builtin_method_args(
                token_stream,
                &[inner_type.as_ref().to_owned()],
                context,
                &member_location,
                string_table,
            )?;
            (args, Vec::new())
        }
        CollectionBuiltinMethod::Remove => {
            let args = parse_builtin_method_args(
                token_stream,
                &[DataType::Int],
                context,
                &member_location,
                string_table,
            )?;
            (args, Vec::new())
        }
        CollectionBuiltinMethod::Length => {
            let args = parse_builtin_method_args(
                token_stream,
                &[],
                context,
                &member_location,
                string_table,
            )?;
            (args, vec![DataType::Int])
        }
        CollectionBuiltinMethod::PullDeprecated => unreachable!(),
    };

    if matches!(builtin, CollectionBuiltinMethod::Get)
        && token_stream.current_token_kind() != &TokenKind::Bang
        && !(matches!(token_stream.current_token_kind(), TokenKind::Symbol(_))
            && token_stream.peek_next_token() == Some(&TokenKind::Bang))
        && !is_assignment_operator(token_stream.current_token_kind())
    {
        return_rule_error!(
            "Calls to collection 'get(index)' must be explicitly handled with '!' syntax.",
            token_stream.current_location(),
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Use '.get(index)!' to handle/propagate errors, or assign through '.get(index) = value' for indexed writes",
            }
        );
    }

    Ok(Some(AstNode {
        kind: NodeKind::MethodCall {
            receiver: Box::new(current_node),
            method_path: collection_builtin_path(builtin, string_table),
            method: member_name,
            builtin: Some(collection_builtin_kind(builtin)),
            args,
            result_types,
            location: member_location.clone(),
        },
        scope: context.scope.to_owned(),
        location: member_location,
    }))
}

fn parse_error_builtin_member(
    token_stream: &mut FileTokens,
    current_node: AstNode,
    current_type: &DataType,
    member_name: StringId,
    member_location: SourceLocation,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Option<AstNode>, CompilerError> {
    if !is_builtin_error_data_type(current_type, string_table) {
        return Ok(None);
    }

    let Some(builtin) = error_builtin_method_name(member_name, string_table) else {
        return Ok(None);
    };

    if token_stream.peek_next_token() != Some(&TokenKind::OpenParenthesis) {
        return_rule_error!(
            format!(
                "Builtin error method '{}' must be called with parentheses.",
                string_table.resolve(member_name)
            ),
            member_location,
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Call this builtin error helper with '(...)'",
            }
        );
    }

    token_stream.advance();

    let error_type = resolve_builtin_error_type(context, &member_location, string_table)?;
    let (args, result_types) = match builtin {
        ErrorBuiltinMethod::WithLocation => {
            let location_type =
                resolve_builtin_error_location_type(context, &member_location, string_table)?;
            let args = parse_builtin_method_args(
                token_stream,
                &[location_type],
                context,
                &member_location,
                string_table,
            )?;
            (args, vec![error_type])
        }
        ErrorBuiltinMethod::PushTrace => {
            let frame_type =
                resolve_builtin_stack_frame_type(context, &member_location, string_table)?;
            let args = parse_builtin_method_args(
                token_stream,
                &[frame_type],
                context,
                &member_location,
                string_table,
            )?;
            (args, vec![error_type])
        }
        ErrorBuiltinMethod::Bubble => {
            let args = parse_builtin_method_args(
                token_stream,
                &[],
                context,
                &member_location,
                string_table,
            )?;
            (args, vec![error_type])
        }
    };

    Ok(Some(AstNode {
        kind: NodeKind::MethodCall {
            receiver: Box::new(current_node),
            method_path: error_builtin_path(builtin, string_table),
            method: member_name,
            builtin: Some(error_builtin_kind(builtin)),
            args,
            result_types,
            location: member_location.clone(),
        },
        scope: context.scope.to_owned(),
        location: member_location,
    }))
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

        if let Some(collection_builtin_call) = parse_collection_builtin_member(
            token_stream,
            current_node.to_owned(),
            &current_type,
            member_name,
            member_location.clone(),
            context,
            string_table,
        )? {
            current_node = collection_builtin_call;
            continue;
        }

        if let Some(error_builtin_call) = parse_error_builtin_member(
            token_stream,
            current_node.to_owned(),
            &current_type,
            member_name,
            member_location.clone(),
            context,
            string_table,
        )? {
            current_node = error_builtin_call;
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
                builtin: None,
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
