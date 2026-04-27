//! Identifier-led expression parsing helpers.
//!
//! WHAT: parses identifier-led expression forms such as references, calls, constructors, and imported start aliases.
//! WHY: identifier tokens fan out into the largest number of semantic cases and need isolated handling.

use super::call_argument::normalize_call_arguments;
use super::choice_constructor::parse_choice_construct;
use super::expression::{Expression, ExpressionKind};
use super::function_calls::{parse_external_function_call, parse_function_call};
use super::parse_expression_dispatch::push_expression_node;
use super::struct_instance::parse_struct_constructor_expression;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::receiver_methods::free_function_receiver_method_call_error;
use crate::compiler_frontend::ast::statements::declarations::create_reference;
use crate::compiler_frontend::ast::templates::template::TemplateType;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
// choice_constructor module handles choice construct parsing; no deferred feature wrapper needed.

use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::compiler_frontend::value_mode::ValueMode;
use crate::return_rule_error;

pub(super) fn parse_identifier_or_call(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    expression: &mut Vec<AstNode>,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    // Fast path for reserved receiver keyword `this`.
    if token_stream.current_token_kind() == &TokenKind::This {
        return parse_this_reference(token_stream, context, expression, string_table);
    }

    // One identifier token can expand into several expression forms: a local/reference read,
    // struct construction, user-function call, host call, or imported start-function call.
    let TokenKind::Symbol(id) = token_stream.current_token_kind().to_owned() else {
        return Ok(());
    };

    if let Some(arg) = context.get_reference(&id) {
        if let ExpressionKind::Template(template_value) = &arg.value.kind
            && matches!(template_value.kind, TemplateType::SlotInsert(_))
            && !matches!(
                context.kind,
                ContextKind::Template | ContextKind::Constant | ContextKind::ConstantHeader
            )
        {
            return_rule_error!(
                "'$insert(...)' helpers can only be used while filling an immediate parent template that defines matching '$slot' targets.",
                token_stream.current_location(),
                {
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "Use this '$insert(...)' helper inside a template invocation that has a slot-bearing parent in the head chain",
                }
            );
        }

        if let DataType::Struct {
            nominal_path,
            fields,
            ..
        } = &arg.value.data_type
            && token_stream.peek_next_token() == Some(&TokenKind::OpenParenthesis)
        {
            // Struct constructors are parsed before constant-reference checks.
            // This keeps `#x = MyStruct(...)` on the constructor path so const
            // record coercion can validate field values instead of rejecting the
            // struct symbol itself as a non-constant reference.
            let struct_instance = parse_struct_constructor_expression(
                token_stream,
                nominal_path,
                id,
                fields,
                &arg.value.value_mode,
                context,
                string_table,
            )?;

            push_expression_node(
                token_stream,
                context,
                string_table,
                expression,
                AstNode {
                    kind: NodeKind::Rvalue(struct_instance),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                },
            )?;

            return Ok(());
        }

        if token_stream.peek_next_token() == Some(&TokenKind::DoubleColon) {
            if matches!(&arg.value.data_type, DataType::Choices { .. }) {
                let choice_value =
                    parse_choice_construct(token_stream, arg, context, string_table)?;
                let choice_location = choice_value.location.to_owned();

                push_expression_node(
                    token_stream,
                    context,
                    string_table,
                    expression,
                    AstNode {
                        kind: NodeKind::Rvalue(choice_value),
                        location: choice_location,
                        scope: context.scope.clone(),
                    },
                )?;

                return Ok(());
            }

            return_rule_error!(
                format!(
                    "'{}' is not a choice declaration. Only choices support namespaced variant construction with '::'.",
                    string_table.resolve(id)
                ),
                token_stream.current_location(),
                {
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "Use 'Choice::Variant' only when the left symbol is a declared choice type",
                }
            );
        }

        if context.kind.is_constant_context()
            && !arg.value.is_compile_time_constant()
            && !arg.is_unresolved_constant_placeholder()
        {
            let variable_name = string_table.resolve(id).to_owned();
            return_rule_error!(
                format!(
                    "Constants can only reference other constants. '{}' resolves to a non-constant value.",
                    variable_name
                ),
                token_stream.current_location(),
                {
                    VariableName => variable_name,
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "Only reference constants in constant declarations and const templates",
                }
            );
        }

        match &arg.value.data_type {
            DataType::Function(_, signature) => {
                // Advance past the function name to position at the opening parenthesis
                token_stream.advance();

                // This is a function call - parse it using the function call parser
                let function_call_node = parse_function_call(
                    token_stream,
                    &arg.id,
                    context,
                    signature,
                    true,
                    None,
                    string_table,
                )?;
                let function_call_location = function_call_node.location.to_owned();

                // -------------------------------
                // FUNCTION CALL INSIDE EXPRESSION
                // -------------------------------
                match function_call_node.kind {
                    NodeKind::FunctionCall {
                        name,
                        args,
                        result_types,
                        location,
                    } => {
                        let func_call_expr = Expression::function_call_with_arguments(
                            name,
                            normalize_call_arguments(&args),
                            result_types,
                            location,
                        );

                        push_expression_node(
                            token_stream,
                            context,
                            string_table,
                            expression,
                            AstNode {
                                kind: NodeKind::Rvalue(func_call_expr),
                                location: function_call_location.to_owned(),
                                scope: context.scope.clone(),
                            },
                        )?;

                        return Ok(());
                    }
                    NodeKind::ResultHandledFunctionCall {
                        name,
                        args,
                        result_types,
                        handling,
                        location,
                    } => {
                        let func_call_expr = Expression::result_handled_function_call(
                            name,
                            normalize_call_arguments(&args),
                            result_types,
                            handling,
                            location,
                        );

                        push_expression_node(
                            token_stream,
                            context,
                            string_table,
                            expression,
                            AstNode {
                                kind: NodeKind::Rvalue(func_call_expr),
                                location: function_call_location.to_owned(),
                                scope: context.scope.clone(),
                            },
                        )?;

                        return Ok(());
                    }
                    _ => {}
                }
            }

            DataType::Struct { .. } => {
                // Fall through to normal reference behaviour for non-constructor uses.
                let reference_node = create_reference(token_stream, arg, context, string_table)?;
                push_expression_node(
                    token_stream,
                    context,
                    string_table,
                    expression,
                    reference_node,
                )?;
                return Ok(());
            }

            // --------------------------
            // VARIABLE INSIDE EXPRESSION
            // --------------------------
            _ => {
                let reference_node = create_reference(token_stream, arg, context, string_table)?;
                push_expression_node(
                    token_stream,
                    context,
                    string_table,
                    expression,
                    reference_node,
                )?;
                return Ok(()); // Will have moved onto the next token already
            }
        }
    }

    // ------------------------------------
    // EXTERNAL CONSTANT INSIDE EXPRESSION
    // ------------------------------------
    if let Some((_const_id, const_def)) = context.lookup_visible_external_constant(id) {
        token_stream.advance();
        let location = token_stream.current_location();
        let value_mode = ValueMode::ImmutableOwned;
        let const_expr = match const_def.value {
            crate::compiler_frontend::external_packages::ExternalConstantValue::Float(value) => {
                Expression::float(value, location, value_mode)
            }
            crate::compiler_frontend::external_packages::ExternalConstantValue::Int(value) => {
                Expression::int(value, location, value_mode)
            }
            crate::compiler_frontend::external_packages::ExternalConstantValue::StringSlice(
                value,
            ) => {
                let string_id = string_table.intern(value);
                Expression::string_slice(string_id, location, value_mode)
            }
            crate::compiler_frontend::external_packages::ExternalConstantValue::Bool(value) => {
                Expression::bool(value, location, value_mode)
            }
        };
        push_expression_node(
            token_stream,
            context,
            string_table,
            expression,
            AstNode {
                kind: NodeKind::Rvalue(const_expr),
                location: SourceLocation::default(),
                scope: context.scope.clone(),
            },
        )?;
        return Ok(());
    }

    // ------------------------------------
    // HOST FUNCTION CALL INSIDE EXPRESSION
    // ------------------------------------
    if let Some((func_id, host_func_def)) = context.lookup_visible_external_function(id) {
        if context.kind.is_constant_context() {
            return_rule_error!(
                format!(
                    "Constants cannot call external functions. '{}' is a runtime external call.",
                    string_table.resolve(id)
                ),
                token_stream.current_location(),
                {
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "Use only compile-time constant values inside constants and const templates",
                }
            );
        }

        // External calls parse from metadata directly; do not synthesize fake parameter declarations.
        token_stream.advance();

        let function_call_node = parse_external_function_call(
            token_stream,
            func_id,
            host_func_def,
            context,
            string_table,
        )?;

        if let NodeKind::HostFunctionCall {
            name: host_function_id,
            args,
            result_types,
            location,
        } = function_call_node.kind
        {
            let func_call_expr = Expression::host_function_call_with_arguments(
                host_function_id,
                normalize_call_arguments(&args),
                result_types,
                location,
            );

            push_expression_node(
                token_stream,
                context,
                string_table,
                expression,
                AstNode {
                    kind: NodeKind::Rvalue(func_call_expr),
                    location: SourceLocation::default(),
                    scope: context.scope.clone(),
                },
            )?;

            return Ok(());
        }
    }

    if token_stream.peek_next_token() == Some(&TokenKind::OpenParenthesis)
        && let Some(method_entry) = context.lookup_visible_receiver_method_by_name(id)
    {
        return Err(free_function_receiver_method_call_error(
            id,
            method_entry,
            token_stream.current_location(),
            "Expression Parsing",
            string_table,
        ));
    }

    // External types cannot be constructed with struct literal syntax.
    if token_stream.peek_next_token() == Some(&TokenKind::OpenParenthesis)
        && context.lookup_visible_external_type(id).is_some()
    {
        return_rule_error!(
            format!(
                "Cannot construct external type '{}' with a struct literal. External types are opaque and can only be obtained from external function calls.",
                string_table.resolve(id)
            ),
            token_stream.current_location(),
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Use an external function that returns this type instead",
            }
        );
    }

    let var_name = string_table.resolve(id).to_string();
    if context.is_visible_type_alias_name(id) {
        return_rule_error!(
            format!("`{}` is a type alias and cannot be used as a value.", var_name),
            token_stream.current_location(),
            {
                VariableName => var_name,
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Use the type alias only in type annotations, not in expressions",
            }
        );
    }
    return_rule_error!(
        format!(
            "Undefined variable '{}'. Variable must be declared before use.",
            var_name
        ),
        token_stream.current_location(),
        {
            VariableName => var_name,
            CompilationStage => "Expression Parsing",
            PrimarySuggestion => "Declare the variable before using it in this expression",
        }
    )
}

/// Parse a `this` reference inside a receiver method body.
///
/// WHAT: validates that `this` is in scope (i.e. the current function declared a receiver)
/// and emits a reference node identical to a normal local read.
/// WHY: `this` is a reserved keyword token, not an ordinary identifier, so it needs its own
/// parse path, but semantically it behaves like any other parameter reference.
fn parse_this_reference(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    expression: &mut Vec<AstNode>,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    let this_id = string_table.intern("this");

    let Some(receiver_declaration) = context.get_reference(&this_id) else {
        return_rule_error!(
            "'this' can only be used inside the body of a receiver method.",
            token_stream.current_location(),
            {
                VariableName => "this",
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Declare a receiver method with 'this' as the first parameter, or use a normal variable name",
            }
        );
    };

    let reference_node =
        create_reference(token_stream, receiver_declaration, context, string_table)?;

    push_expression_node(
        token_stream,
        context,
        string_table,
        expression,
        reference_node,
    )
}
