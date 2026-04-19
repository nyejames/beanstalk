//! Identifier-led expression parsing helpers.
//!
//! WHAT: parses identifier-led expression forms such as references, calls, constructors, and imported start aliases.
//! WHY: identifier tokens fan out into the largest number of semantic cases and need isolated handling.

use super::call_argument::normalize_call_arguments;
use super::expression::{Expression, ExpressionKind};
use super::function_calls::parse_function_call;
use super::parse_expression_dispatch::push_expression_node;
use super::struct_instance::parse_struct_constructor_expression;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::receiver_methods::free_function_receiver_method_call_error;
use crate::compiler_frontend::ast::statements::declarations::create_reference;
use crate::compiler_frontend::ast::templates::template::TemplateType;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::deferred_feature_diagnostics::deferred_feature_rule_error;
use crate::compiler_frontend::reserved_trait_syntax::{
    reserved_trait_keyword_error, reserved_trait_keyword_or_dispatch_mismatch,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::{return_compiler_error, return_rule_error};

pub(super) fn parse_identifier_or_call(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    expression: &mut Vec<AstNode>,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    // One identifier token can expand into several expression forms: a local/reference read,
    // struct construction, user-function call, host call, or imported start-function call.
    let TokenKind::Symbol(id) = token_stream.current_token_kind().to_owned() else {
        return Ok(());
    };

    let full_name = context.scope.to_owned().append(id);

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
            ownership: struct_ownership,
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
                struct_ownership,
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
                let choice_value = parse_choice_variant_value(token_stream, arg, string_table)?;
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
                        let func_call_expr = Expression::result_handled_function_call_with_arguments(
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
    // HOST FUNCTION CALL INSIDE EXPRESSION
    // ------------------------------------
    if let Some(host_func_def) = context.host_registry.get_function(string_table.resolve(id)) {
        if context.kind.is_constant_context() {
            return_rule_error!(
                format!(
                    "Constants cannot call host functions. '{}' is a runtime host call.",
                    string_table.resolve(id)
                ),
                token_stream.current_location(),
                {
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "Use only compile-time constant values inside constants and const templates",
                }
            );
        }

        // Convert return types to Arg format
        let signature = host_func_def.params_to_signature(string_table);

        // This is a function call - parse it using the function call parser
        let function_call_node = parse_function_call(
            token_stream,
            &full_name,
            context,
            &signature,
            true,
            None,
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

    let var_name = string_table.resolve(id).to_string();
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

/// Parse a `Choice::Variant` value expression for already-resolved choice declarations.
///
/// WHAT: resolves the variant name to a deterministic integer tag while preserving the
/// declaration-backed choice datatype on the resulting expression.
/// WHY: alpha choices are unit-variant-only today; this keeps expression parsing strict and
/// avoids rebuilding choice metadata from raw tokens.
fn parse_choice_variant_value(
    token_stream: &mut FileTokens,
    choice_declaration: &Declaration,
    string_table: &StringTable,
) -> Result<Expression, CompilerError> {
    let DataType::Choices {
        nominal_path,
        variants,
    } = &choice_declaration.value.data_type
    else {
        return_compiler_error!(
            "Choice variant parser was called with a non-choice declaration '{}'.",
            choice_declaration.id.to_portable_string(string_table)
        );
    };

    let choice_name = nominal_path.name_str(string_table).unwrap_or("<choice>");

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
        .position(|variant| variant.id == variant_name)
    else {
        let available_variants = variants
            .iter()
            .map(|variant| string_table.resolve(variant.id).to_owned())
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
