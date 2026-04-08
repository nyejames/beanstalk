//! AST expression parsing and expression-list helpers.
//!
//! WHAT: parses token streams into typed AST expressions before evaluation and lowering.
//! WHY: expression parsing centralizes precedence, call parsing, and place-expression rules in one pass.

use super::eval_expression::evaluate_expression;
use crate::compiler_frontend::ast::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::call_argument::normalize_call_argument_values;
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, Operator,
};
use crate::compiler_frontend::ast::expressions::function_calls::parse_function_call;
use crate::compiler_frontend::ast::expressions::struct_instance::parse_struct_constructor_expression;
use crate::compiler_frontend::ast::field_access::{
    ReceiverAccessMode, parse_field_access_with_receiver_access, parse_postfix_chain,
};
use crate::compiler_frontend::ast::place_access::ast_node_is_place;
use crate::compiler_frontend::ast::receiver_methods::free_function_receiver_method_call_error;
use crate::compiler_frontend::ast::statements::choices::parse_choice_variant_value;
use crate::compiler_frontend::ast::statements::declarations::create_reference;
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnSlot,
};
use crate::compiler_frontend::ast::statements::result_handling::parse_result_handling_suffix_for_expression;
use crate::compiler_frontend::ast::templates::template::TemplateType;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::builtins::expression_parsing::{
    parse_builtin_cast_expression, parse_collection_expression,
};
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorMetaDataKey};
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::reserved_trait_syntax::{
    reserved_trait_keyword, reserved_trait_keyword_error,
};
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::token_scan::find_expression_end_index;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::compiler_frontend::traits::ContainsReferences;
use crate::compiler_frontend::type_coercion::parse_context::parse_expectation_for_target_type;
use crate::{
    ast_log, return_compiler_error, return_rule_error, return_syntax_error, return_type_error,
};

fn push_expression_node(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    string_table: &mut StringTable,
    expression: &mut Vec<AstNode>,
    node: AstNode,
) -> Result<(), CompilerError> {
    // Postfix parsing happens after the primary node exists so chains like `value.field ! fallback`
    // bind to the fully-built primary expression instead of only the leading identifier token.
    let node = if token_stream.index < token_stream.length
        && token_stream.current_token_kind() == &TokenKind::Dot
    {
        parse_postfix_chain(
            token_stream,
            node,
            ReceiverAccessMode::Shared,
            context,
            string_table,
        )?
    } else {
        node
    };

    let node = if token_stream.index < token_stream.length
        && (token_stream.current_token_kind() == &TokenKind::Bang
            || (matches!(token_stream.current_token_kind(), TokenKind::Symbol(_))
                && token_stream.peek_next_token() == Some(&TokenKind::Bang)))
    {
        let handled = parse_result_handling_suffix_for_expression(
            token_stream,
            context,
            node.get_expr()?,
            true,
            None,
            string_table,
        )?;
        AstNode {
            kind: NodeKind::Rvalue(handled),
            location: token_stream.current_location(),
            scope: context.scope.clone(),
        }
    } else {
        node
    };

    expression.push(node);
    Ok(())
}

fn parse_identifier_or_call(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    expression: &mut Vec<AstNode>,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    // One identifier token can expand into several expression forms: a local/reference read,
    // struct construction, user-function call, host call, or imported start-function call.
    let TokenKind::Symbol(id, ..) = token_stream.current_token_kind().to_owned() else {
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
            if matches!(&arg.value.data_type, DataType::Choices(_)) {
                // Keep choice construction semantics centralized in `statements::choices` so
                // parse-expression only routes the `Choice::Variant` shape.
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

        if context.kind.is_constant_context() && !arg.value.is_compile_time_constant() {
            return_rule_error!(
                format!(
                    "Constants can only reference other constants. '{}' resolves to a non-constant value.",
                    string_table.resolve(id)
                ),
                token_stream.current_location(),
                {
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
                        let func_call_expr = Expression::function_call(
                            name,
                            normalize_call_argument_values(&args),
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
                            normalize_call_argument_values(&args),
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

    if let Some(start_target) = context.resolve_start_import(&id) {
        token_stream.advance();

        match token_stream.current_token_kind() {
            TokenKind::OpenParenthesis => {
                let function_call_node = parse_function_call(
                    token_stream,
                    start_target,
                    context,
                    &FunctionSignature {
                        parameters: vec![],
                        returns: vec![ReturnSlot::success(FunctionReturn::Value(
                            DataType::StringSlice,
                        ))],
                    },
                    true,
                    None,
                    string_table,
                )?;
                let function_call_location = function_call_node.location.to_owned();

                match function_call_node.kind {
                    NodeKind::FunctionCall {
                        name,
                        args,
                        result_types,
                        location,
                    } => {
                        push_expression_node(
                            token_stream,
                            context,
                            string_table,
                            expression,
                            AstNode {
                                kind: NodeKind::Rvalue(Expression::function_call(
                                    name,
                                    normalize_call_argument_values(&args),
                                    result_types,
                                    location,
                                )),
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
                        push_expression_node(
                            token_stream,
                            context,
                            string_table,
                            expression,
                            AstNode {
                                kind: NodeKind::Rvalue(Expression::result_handled_function_call(
                                    name,
                                    normalize_call_argument_values(&args),
                                    result_types,
                                    handling,
                                    location,
                                )),
                                location: function_call_location.to_owned(),
                                scope: context.scope.clone(),
                            },
                        )?;
                        return Ok(());
                    }
                    _ => {}
                }

                return_compiler_error!(
                    "Expected a function call node for imported file start alias"
                );
            }

            TokenKind::Dot => {
                return_rule_error!(
                    format!(
                        "Imported file '{}' is callable only as '{}()'. File-struct member access is no longer supported.",
                        string_table.resolve(id),
                        string_table.resolve(id),
                    ),
                    token_stream.current_location(),
                    {
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => "Import exports directly with '@path/to/file/symbol' or '@path/to/file {a, b}'",
                    }
                );
            }

            _ => {
                return_rule_error!(
                    format!(
                        "Imported file '{}' can only be used as a callable start import ('{}()').",
                        string_table.resolve(id),
                        string_table.resolve(id),
                    ),
                    token_stream.current_location(),
                    {
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => "Call the file start function with 'file()' or import specific exports directly",
                    }
                );
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
            let func_call_expr = Expression::host_function_call(
                host_function_id,
                normalize_call_argument_values(&args),
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

fn parse_mutable_receiver_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    expression: &mut Vec<AstNode>,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    let marker_location = token_stream.current_location();
    token_stream.advance();

    let TokenKind::Symbol(id) = token_stream.current_token_kind().to_owned() else {
        return_rule_error!(
            "Mutable receiver marker '~' must be followed by a receiver symbol.",
            marker_location,
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Use receiver-call syntax like '~value.method(...)'",
            }
        );
    };

    let Some(reference_arg) = context.get_reference(&id) else {
        return_rule_error!(
            format!(
                "Undefined variable '{}'. Mutable receiver calls require a declared receiver place.",
                string_table.resolve(id)
            ),
            token_stream.current_location(),
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Declare this receiver variable before using '~receiver.method(...)'",
            }
        );
    };

    if token_stream.peek_next_token() != Some(&TokenKind::Dot) {
        return_rule_error!(
            "Mutable receiver marker '~' is only valid for receiver method calls like '~value.method(...)'.",
            marker_location,
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Apply '~' directly to a receiver method call",
            }
        );
    }

    token_stream.advance();
    let receiver_node = parse_field_access_with_receiver_access(
        token_stream,
        reference_arg,
        context,
        ReceiverAccessMode::Mutable,
        string_table,
    )?;
    push_expression_node(
        token_stream,
        context,
        string_table,
        expression,
        receiver_node,
    )
}

fn parse_literal_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    expected_type: &DataType,
    ownership: &Ownership,
    expression: &mut Vec<AstNode>,
    next_number_negative: &mut bool,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    match token_stream.current_token_kind().to_owned() {
        TokenKind::FloatLiteral(mut float) => {
            if *next_number_negative {
                float = -float;
                *next_number_negative = false;
            }

            let location = token_stream.current_location();
            let float_expr = Expression::float(float, location.to_owned(), ownership.to_owned());
            token_stream.advance();
            push_expression_node(
                token_stream,
                context,
                string_table,
                expression,
                AstNode {
                    kind: NodeKind::Rvalue(float_expr),
                    location,
                    scope: context.scope.clone(),
                },
            )?;
            Ok(())
        }

        TokenKind::IntLiteral(mut int) => {
            if *next_number_negative {
                *next_number_negative = false;
                int = -int;
            };

            let location = token_stream.current_location();
            let int_expr = Expression::int(int, location.to_owned(), ownership.to_owned());
            token_stream.advance();
            push_expression_node(
                token_stream,
                context,
                string_table,
                expression,
                AstNode {
                    kind: NodeKind::Rvalue(int_expr),
                    scope: context.scope.clone(),
                    location,
                },
            )?;
            Ok(())
        }

        TokenKind::StringSliceLiteral(string) => {
            let location = token_stream.current_location();
            let string_expr =
                Expression::string_slice(string, location.to_owned(), ownership.to_owned());
            token_stream.advance();
            push_expression_node(
                token_stream,
                context,
                string_table,
                expression,
                AstNode {
                    kind: NodeKind::Rvalue(string_expr),
                    scope: context.scope.clone(),
                    location,
                },
            )?;
            Ok(())
        }

        TokenKind::BoolLiteral(value) => {
            let location = token_stream.current_location();
            let bool_expr = Expression::bool(value, location.to_owned(), ownership.to_owned());
            token_stream.advance();
            push_expression_node(
                token_stream,
                context,
                string_table,
                expression,
                AstNode {
                    kind: NodeKind::Rvalue(bool_expr),
                    location,
                    scope: context.scope.clone(),
                },
            )?;
            Ok(())
        }

        TokenKind::CharLiteral(value) => {
            let location = token_stream.current_location();
            let char_expr = Expression::char(value, location.to_owned(), ownership.to_owned());
            token_stream.advance();
            push_expression_node(
                token_stream,
                context,
                string_table,
                expression,
                AstNode {
                    kind: NodeKind::Rvalue(char_expr),
                    location,
                    scope: context.scope.clone(),
                },
            )?;
            Ok(())
        }

        TokenKind::NoneLiteral => {
            let inner_type = if let DataType::Option(inner_type) = expected_type {
                inner_type.as_ref().to_owned()
            } else if token_stream.index > 0
                && matches!(
                    token_stream.previous_token(),
                    TokenKind::Is | TokenKind::Not
                )
            {
                // Comparisons like `value is none` infer the option shape from the
                // left-hand side expression during evaluation.
                DataType::Inferred
            } else {
                return_rule_error!(
                    "The 'none' literal requires an explicit optional type context",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => "Use 'none' only where a concrete optional type is expected (for example 'String?')",
                    }
                );
            };

            let location = token_stream.current_location();
            // Propagate the binding's ownership so that `name ~String? = none`
            // produces a mutable binding. Other literals (int, float, string)
            // already receive ownership from the same parameter; none must too.
            let mut none_expr = Expression::option_none(inner_type, location.clone());
            none_expr.ownership = ownership.to_owned();
            token_stream.advance();
            push_expression_node(
                token_stream,
                context,
                string_table,
                expression,
                AstNode {
                    kind: NodeKind::Rvalue(none_expr),
                    location,
                    scope: context.scope.clone(),
                },
            )?;
            Ok(())
        }

        _ => Ok(()),
    }
}

fn parse_template_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    consume_closing_parenthesis: bool,
    ownership: &Ownership,
    string_table: &mut StringTable,
) -> Result<Option<Expression>, CompilerError> {
    let template_context = context.new_template_parsing_context();
    let template = Template::new(token_stream, &template_context, vec![], string_table)?;

    match template.kind {
        TemplateType::StringFunction => {
            if context.kind.is_constant_context() {
                return_rule_error!(
                    "Constants and const templates require compile-time template folding. This template is runtime.",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => "Remove runtime values from this template so it can fold at compile time",
                    }
                );
            }

            if consume_closing_parenthesis
                && token_stream.current_token_kind() == &TokenKind::CloseParenthesis
            {
                token_stream.advance();
            }
            Ok(Some(Expression::template(template, ownership.to_owned())))
        }

        TemplateType::String => {
            if consume_closing_parenthesis
                && token_stream.current_token_kind() == &TokenKind::CloseParenthesis
            {
                token_stream.advance();
            }

            if !template.is_const_renderable_string() || template.has_unresolved_slots() {
                return Ok(Some(Expression::template(template, ownership.to_owned())));
            }

            ast_log!("Template is foldable now. Folding...");

            let mut fold_context = template_context
                .new_template_fold_context(string_table, "expression parsing template fold")?;
            let folded_string = template.fold_into_stringid(&mut fold_context)?;

            Ok(Some(Expression::string_slice(
                folded_string,
                token_stream.current_location(),
                ownership.get_owned(),
            )))
        }

        // Ignore comments
        TemplateType::Comment(_) => Ok(None),

        TemplateType::SlotInsert(_) => {
            if consume_closing_parenthesis
                && token_stream.current_token_kind() == &TokenKind::CloseParenthesis
            {
                token_stream.advance();
            }

            Ok(Some(Expression::template(template, ownership.to_owned())))
        }

        TemplateType::SlotDefinition(_) => {
            return_rule_error!(
                "'$slot' markers are only valid as direct nested templates inside template bodies.",
                token_stream.current_location(),
                {
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "Use '$slot' inside a template body where it defines a receiving slot",
                }
            )
        }
    }
}

fn parse_unary_operator(
    token_stream: &FileTokens,
    context: &ScopeContext,
    expression: &mut Vec<AstNode>,
    next_number_negative: &mut bool,
) -> bool {
    match token_stream.current_token_kind() {
        TokenKind::Negative => {
            *next_number_negative = true;
            true
        }
        TokenKind::Not => {
            expression.push(AstNode {
                kind: NodeKind::Operator(Operator::Not),
                location: token_stream.current_location(),
                scope: context.scope.clone(),
            });
            true
        }
        _ => false,
    }
}

enum ExpressionTokenStep {
    Continue,
    Advance,
    Break,
    Return(Box<Expression>),
}

struct ExpressionDispatchState<'a> {
    data_type: &'a mut DataType,
    ownership: &'a Ownership,
    consume_closing_parenthesis: bool,
    expression: &'a mut Vec<AstNode>,
    next_number_negative: &'a mut bool,
}

fn push_operator_node(
    expression: &mut Vec<AstNode>,
    context: &ScopeContext,
    location: SourceLocation,
    operator: Operator,
) {
    expression.push(AstNode {
        kind: NodeKind::Operator(operator),
        location,
        scope: context.scope.clone(),
    });
}

fn dispatch_expression_token(
    token: TokenKind,
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    state: &mut ExpressionDispatchState<'_>,
    string_table: &mut StringTable,
) -> Result<ExpressionTokenStep, CompilerError> {
    // This state machine is intentionally flat: each token either appends one AST node, advances
    // past a nested parse, or signals the caller that the surrounding grammar owns the delimiter.
    match token {
        TokenKind::CloseCurly
        | TokenKind::Comma
        | TokenKind::Eof
        | TokenKind::TemplateClose
        | TokenKind::Arrow
        | TokenKind::StartTemplateBody
        | TokenKind::Colon
        | TokenKind::End => {
            if state.expression.is_empty() {
                match token {
                    TokenKind::Comma => {
                        let mut error = CompilerError::new_syntax_error(
                            "Unexpected ',' in expression. Commas separate list items, function arguments, or return declarations.",
                            token_stream.current_location(),
                        );
                        error.new_metadata_entry(
                            ErrorMetaDataKey::CompilationStage,
                            String::from("Expression Parsing"),
                        );
                        error.new_metadata_entry(
                            ErrorMetaDataKey::PrimarySuggestion,
                            String::from("Add a value before ',' or remove the comma"),
                        );
                        return Err(error);
                    }

                    TokenKind::Arrow => {
                        let mut error = CompilerError::new_syntax_error(
                            "Unexpected '->' in expression. Arrow syntax is only valid in function signatures.",
                            token_stream.current_location(),
                        );
                        error.new_metadata_entry(
                            ErrorMetaDataKey::CompilationStage,
                            String::from("Expression Parsing"),
                        );
                        error.new_metadata_entry(
                            ErrorMetaDataKey::PrimarySuggestion,
                            String::from(
                                "Use '->' only in function signatures like '|args| -> Type:'",
                            ),
                        );
                        return Err(error);
                    }

                    _ => {}
                }
            }

            if state.consume_closing_parenthesis {
                return_syntax_error!(
                    format!("Unexpected token: '{:?}'. Seems to be missing a closing parenthesis at the end of this expression.", token),
                    token_stream.current_location(),
                    {
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => "Add a closing parenthesis ')' at the end of the expression",
                        SuggestedInsertion => ")",
                    }
                )
            }

            Ok(ExpressionTokenStep::Break)
        }

        TokenKind::CloseParenthesis => {
            if state.consume_closing_parenthesis {
                token_stream.advance();
            }

            if state.expression.is_empty() {
                return_syntax_error!(
                    "Empty expression found. Expected a value, variable, or expression.",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => "Add a value, variable reference, or expression inside the parentheses",
                    }
                );
            }

            Ok(ExpressionTokenStep::Break)
        }

        TokenKind::OpenParenthesis => {
            token_stream.advance();
            let value = create_expression(
                token_stream,
                context,
                state.data_type,
                state.ownership,
                true,
                string_table,
            )?;

            push_expression_node(
                token_stream,
                context,
                string_table,
                state.expression,
                AstNode {
                    kind: NodeKind::Rvalue(value),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                },
            )?;

            Ok(ExpressionTokenStep::Continue)
        }

        TokenKind::DatatypeInt | TokenKind::DatatypeFloat => {
            let cast_expression = parse_builtin_cast_expression(
                token_stream,
                context,
                state.ownership,
                string_table,
            )?;
            let cast_location = cast_expression.location.clone();

            push_expression_node(
                token_stream,
                context,
                string_table,
                state.expression,
                AstNode {
                    kind: NodeKind::Rvalue(cast_expression),
                    location: cast_location,
                    scope: context.scope.clone(),
                },
            )?;

            Ok(ExpressionTokenStep::Continue)
        }

        TokenKind::OpenCurly => {
            parse_collection_expression(
                token_stream,
                context,
                state.data_type,
                state.ownership,
                state.expression,
                string_table,
            )?;
            Ok(ExpressionTokenStep::Advance)
        }

        TokenKind::Newline => {
            let previous_token = if token_stream.index == 0 {
                &TokenKind::Newline
            } else {
                token_stream.previous_token()
            };
            if state.consume_closing_parenthesis
                || (previous_token.continues_expression()
                    && !matches!(previous_token, TokenKind::End))
            {
                token_stream.skip_newlines();
                return Ok(ExpressionTokenStep::Continue);
            }

            while let Some(next) = token_stream.peek_next_token() {
                if next.continues_expression() {
                    token_stream.skip_newlines();
                    continue;
                }
                break;
            }

            ast_log!("Breaking out of expression with newline");
            Ok(ExpressionTokenStep::Break)
        }

        TokenKind::Symbol(..) => {
            parse_identifier_or_call(token_stream, context, state.expression, string_table)?;
            Ok(ExpressionTokenStep::Continue)
        }

        TokenKind::Mutable => {
            parse_mutable_receiver_expression(
                token_stream,
                context,
                state.expression,
                string_table,
            )?;
            Ok(ExpressionTokenStep::Continue)
        }

        TokenKind::FloatLiteral(_)
        | TokenKind::IntLiteral(_)
        | TokenKind::StringSliceLiteral(_)
        | TokenKind::BoolLiteral(_)
        | TokenKind::CharLiteral(_)
        | TokenKind::NoneLiteral => {
            parse_literal_expression(
                token_stream,
                context,
                state.data_type,
                state.ownership,
                state.expression,
                state.next_number_negative,
                string_table,
            )?;
            Ok(ExpressionTokenStep::Continue)
        }

        TokenKind::TemplateHead => {
            if let Some(template_expression) = parse_template_expression(
                token_stream,
                context,
                state.consume_closing_parenthesis,
                state.ownership,
                string_table,
            )? {
                return Ok(ExpressionTokenStep::Return(Box::new(template_expression)));
            }

            Ok(ExpressionTokenStep::Advance)
        }

        TokenKind::Copy => {
            let copy_location = token_stream.current_location();
            token_stream.advance();

            let copied_place = parse_copy_place_expression(token_stream, context, string_table)?;
            let copied_type = copied_place.get_expr()?.data_type;

            state.expression.push(AstNode {
                kind: NodeKind::Rvalue(Expression::copy(
                    copied_place,
                    copied_type,
                    copy_location.clone(),
                    state.ownership.to_owned(),
                )),
                location: copy_location,
                scope: context.scope.clone(),
            });

            Ok(ExpressionTokenStep::Continue)
        }

        TokenKind::Must | TokenKind::TraitThis => {
            let keyword = reserved_trait_keyword(token_stream.current_token_kind())
                .expect("reserved trait token should map to a keyword");

            Err(reserved_trait_keyword_error(
                keyword,
                token_stream.current_location(),
                "Expression Parsing",
                "Use a normal expression element until traits are implemented",
            ))
        }

        TokenKind::Hash => {
            if token_stream.peek_next_token() != Some(&TokenKind::TemplateHead) {
                return_type_error!(
                    "Unexpected '#' in expression. '#' is only valid before a template head.",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => "Remove '#' or place it directly before a template expression",
                    }
                );
            }

            Ok(ExpressionTokenStep::Advance)
        }

        TokenKind::Negative | TokenKind::Not => {
            let _ = parse_unary_operator(
                token_stream,
                context,
                state.expression,
                state.next_number_negative,
            );
            Ok(ExpressionTokenStep::Advance)
        }

        TokenKind::In => {
            token_stream.advance();
            let mut range_type = DataType::Range;
            let reference_ownership = state.ownership.get_reference();
            let value = evaluate_expression(
                context,
                std::mem::take(state.expression),
                &mut range_type,
                &reference_ownership,
                string_table,
            )?;
            Ok(ExpressionTokenStep::Return(Box::new(value)))
        }

        TokenKind::Add => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::Add,
            );
            Ok(ExpressionTokenStep::Advance)
        }
        TokenKind::Subtract => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::Subtract,
            );
            Ok(ExpressionTokenStep::Advance)
        }
        TokenKind::Multiply => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::Multiply,
            );
            Ok(ExpressionTokenStep::Advance)
        }
        TokenKind::Divide => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::Divide,
            );
            Ok(ExpressionTokenStep::Advance)
        }
        TokenKind::Exponent => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::Exponent,
            );
            Ok(ExpressionTokenStep::Advance)
        }
        TokenKind::Modulus => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::Modulus,
            );
            Ok(ExpressionTokenStep::Advance)
        }

        TokenKind::Is => match token_stream.peek_next_token() {
            Some(TokenKind::Not) => {
                token_stream.advance();
                push_operator_node(
                    state.expression,
                    context,
                    token_stream.current_location(),
                    Operator::NotEqual,
                );
                Ok(ExpressionTokenStep::Advance)
            }

            Some(TokenKind::Colon) => {
                if state.expression.len() > 1 {
                    return_type_error!(
                        format!(
                            "Match statements can only have one value to match against. Found: {}",
                            state.expression.len()
                        ),
                        token_stream.current_location(),
                        {
                            CompilationStage => "Expression Parsing",
                            PrimarySuggestion => "Simplify the expression to a single value before the 'is:' match",
                        }
                    )
                }

                let value = evaluate_expression(
                    context,
                    std::mem::take(state.expression),
                    state.data_type,
                    state.ownership,
                    string_table,
                )?;
                Ok(ExpressionTokenStep::Return(Box::new(value)))
            }

            _ => {
                push_operator_node(
                    state.expression,
                    context,
                    token_stream.current_location(),
                    Operator::Equality,
                );
                Ok(ExpressionTokenStep::Advance)
            }
        },

        TokenKind::LessThan => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::LessThan,
            );
            Ok(ExpressionTokenStep::Advance)
        }
        TokenKind::LessThanOrEqual => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::LessThanOrEqual,
            );
            Ok(ExpressionTokenStep::Advance)
        }
        TokenKind::GreaterThan => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::GreaterThan,
            );
            Ok(ExpressionTokenStep::Advance)
        }
        TokenKind::GreaterThanOrEqual => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::GreaterThanOrEqual,
            );
            Ok(ExpressionTokenStep::Advance)
        }
        TokenKind::And => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::And,
            );
            Ok(ExpressionTokenStep::Advance)
        }
        TokenKind::Or => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::Or,
            );
            Ok(ExpressionTokenStep::Advance)
        }
        TokenKind::ExclusiveRange => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::Range,
            );
            Ok(ExpressionTokenStep::Advance)
        }

        TokenKind::Wildcard => {
            let mut error = CompilerError::new_syntax_error(
                "Unexpected wildcard '_' in expression. Wildcards are only valid in supported pattern positions.",
                token_stream.current_location(),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::CompilationStage,
                String::from("Expression Parsing"),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::PrimarySuggestion,
                String::from(
                    "Use a concrete value/expression here, or use 'else:' for default match arms",
                ),
            );
            Err(error)
        }

        TokenKind::TypeParameterBracket => {
            let mut error = CompilerError::new_syntax_error(
                "Unexpected '|' in expression. This token is only valid in function signatures and struct definitions.",
                token_stream.current_location(),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::CompilationStage,
                String::from("Expression Parsing"),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::PrimarySuggestion,
                String::from("Remove the stray '|' or move it into a declaration signature"),
            );
            Err(error)
        }

        TokenKind::AddAssign => Ok(ExpressionTokenStep::Advance),

        _ => {
            return_syntax_error!(
                format!("Invalid token used in expression: '{:?}'", token),
                token_stream.current_location(),
                {
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "Remove or replace this token with a valid expression element",
                }
            )
        }
    }
}

// WHAT: parses a comma-separated expression list against already-known expected result types.
// WHY: function calls and multi-return contexts must preserve arity and per-slot type
//      expectations while still sharing the normal expression parser.
pub fn create_multiple_expressions(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    consume_closing_parenthesis: bool,
    string_table: &mut StringTable,
) -> Result<Vec<Expression>, CompilerError> {
    let mut expressions: Vec<Expression> = Vec::new();
    for (type_index, expected_type) in context.expected_result_types.iter().enumerate() {
        // Pass Inferred for concrete scalar/composite types so that eval_expression stays
        // strict (Exact context); callers own their own coercion or validation after this
        // call returns. Pass the expected type through only for Option variants so that
        // `none` literals can resolve their inner type from the surrounding context.
        let mut expr_type = parse_expectation_for_target_type(expected_type);
        let expression = create_expression_with_trailing_newline_policy(
            token_stream,
            context,
            &mut expr_type,
            &Ownership::ImmutableOwned,
            false,
            consume_closing_parenthesis,
            string_table,
        )?;

        expressions.push(expression);

        // Newlines are expression terminators almost everywhere else. Only normalize
        // them here when we're inside a parenthesized list so multiline calls like
        // `io(\n value\n)` leave us positioned on the comma or `)`.
        if consume_closing_parenthesis && token_stream.current_token_kind() == &TokenKind::Newline {
            token_stream.skip_newlines();
        }

        if type_index + 1 < context.expected_result_types.len() {
            if token_stream.current_token_kind() != &TokenKind::Comma {
                return_type_error!(
                    format!(
                        "Too few arguments provided. Expected: {}. Provided: {}.",
                        context.expected_result_types.len(),
                        expressions.len()
                    ),
                    token_stream.current_location(),
                    {
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => "Add missing arguments to match the expected count",
                    }
                )
            }

            token_stream.advance();
        }
    }

    if consume_closing_parenthesis {
        if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
            return_syntax_error!(
                format!(
                    "Expected closing parenthesis after arguments, found '{:?}'",
                    token_stream.current_token_kind()
                ),
                token_stream.current_location(),
                {
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "Add ')' after the final argument",
                    SuggestedInsertion => ")",
                }
            );
        }

        token_stream.advance();
    }

    Ok(expressions)
}

// WHAT: parses one expression and evaluates the AST fragment into a typed expression node.
// WHY: expression parsing is the choke point where token structure, place rules, and expected
//      type information meet before later lowering stages.
pub fn create_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    data_type: &mut DataType,
    ownership: &Ownership,
    consume_closing_parenthesis: bool,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    create_expression_with_trailing_newline_policy(
        token_stream,
        context,
        data_type,
        ownership,
        consume_closing_parenthesis,
        true,
        string_table,
    )
}

// WHAT: shared expression parser entry with configurable trailing-newline behavior.
// WHY: callers parsing comma-separated lists outside parentheses (for example
//      fallback/return lists) must preserve line boundaries between statements.
pub(crate) fn create_expression_with_trailing_newline_policy(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    data_type: &mut DataType,
    ownership: &Ownership,
    consume_closing_parenthesis: bool,
    skip_trailing_newlines: bool,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    let mut expression: Vec<AstNode> = Vec::new();

    ast_log!(
        "Parsing ",
        #ownership,
        data_type.display_with_table(string_table),
        " Expression"
    );

    // Build the flat infix AST fragment first. `evaluate_expression` is the stage that turns
    // this fragment into precedence-ordered RPN, resolves the final type, and folds constants.
    let mut next_number_negative = false;
    while token_stream.index < token_stream.length {
        let token = token_stream.current_token_kind().to_owned();
        ast_log!("Parsing expression: ", #token);
        let mut dispatch_state = ExpressionDispatchState {
            data_type,
            ownership,
            consume_closing_parenthesis,
            expression: &mut expression,
            next_number_negative: &mut next_number_negative,
        };
        match dispatch_expression_token(
            token,
            token_stream,
            context,
            &mut dispatch_state,
            string_table,
        )? {
            ExpressionTokenStep::Continue => continue,
            ExpressionTokenStep::Advance => token_stream.advance(),
            ExpressionTokenStep::Break => break,
            ExpressionTokenStep::Return(value) => return Ok(*value),
        }
    }

    if skip_trailing_newlines {
        token_stream.skip_newlines();
    }

    evaluate_expression(context, expression, data_type, ownership, string_table)
}

fn parse_copy_place_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    // `copy` only accepts places because the backend clones the current stored value, not an
    // arbitrary temporary expression result.
    match token_stream.current_token_kind() {
        TokenKind::OpenParenthesis => {
            let open_location = token_stream.current_location();
            token_stream.advance();

            let place = parse_copy_place_expression(token_stream, context, string_table)?;
            if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
                return_syntax_error!(
                    "Expected ')' after copy operand",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => "Wrap only a single place expression in parentheses after 'copy'",
                    }
                );
            }

            token_stream.advance();
            Ok(AstNode {
                location: open_location,
                ..place
            })
        }

        TokenKind::Symbol(symbol) => {
            let Some(reference_arg) = context.get_reference(symbol) else {
                return_rule_error!(
                    format!(
                        "Undefined variable '{}'. Explicit copies require a declared place.",
                        string_table.resolve(*symbol)
                    ),
                    token_stream.current_location(),
                    {
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => "Declare the variable before using 'copy'",
                    }
                );
            };

            match &reference_arg.value.data_type {
                DataType::Function(_, _) => {
                    return_rule_error!(
                        "The 'copy' keyword only accepts places, not function values or calls",
                        token_stream.current_location(),
                        {
                            CompilationStage => "Expression Parsing",
                            PrimarySuggestion => "Copy a variable or field, not a function symbol",
                        }
                    );
                }

                _ => {
                    let place =
                        create_reference(token_stream, reference_arg, context, string_table)?;
                    if !ast_node_is_place(&place) {
                        return_rule_error!(
                            "The 'copy' keyword only accepts a place expression",
                            token_stream.current_location(),
                            {
                                CompilationStage => "Expression Parsing",
                                PrimarySuggestion => "Use 'copy' before a variable or field access such as 'copy value' or 'copy user.name'",
                            }
                        );
                    }

                    Ok(place)
                }
            }
        }

        TokenKind::Must | TokenKind::TraitThis => {
            let keyword = reserved_trait_keyword(token_stream.current_token_kind())
                .expect("reserved trait token should map to a keyword");

            Err(reserved_trait_keyword_error(
                keyword,
                token_stream.current_location(),
                "Expression Parsing",
                "Use a normal place expression until traits are implemented",
            ))
        }

        _ => {
            return_syntax_error!(
                "The 'copy' keyword only accepts a place expression",
                token_stream.current_location(),
                {
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "Use 'copy' before a variable or field access such as 'copy value' or 'copy user.name'",
                }
            )
        }
    }
}

// WHAT: parses an expression from a bounded token slice without consuming the stop token.
// WHY: some parent parsers need normal expression semantics while reserving a delimiter for the
//      surrounding grammar layer to inspect and consume itself.
pub(crate) fn create_expression_until(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    data_type: &mut DataType,
    ownership: &Ownership,
    stop_tokens: &[TokenKind],
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    if stop_tokens.is_empty() {
        return create_expression(
            token_stream,
            context,
            data_type,
            ownership,
            false,
            string_table,
        );
    }

    let start_index = token_stream.index;
    let end_index = find_expression_end_index(&token_stream.tokens, start_index, stop_tokens);

    if end_index >= token_stream.length {
        let expected_tokens = stop_tokens
            .iter()
            .map(|token| format!("{token:?}"))
            .collect::<Vec<_>>()
            .join(", ");

        return_syntax_error!(
            format!(
                "Expected one of [{}] to end this expression, but reached end of file",
                expected_tokens
            ),
            token_stream.current_location(),
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Complete the expression and add the required delimiter token",
            }
        )
    }

    if end_index == start_index {
        return_syntax_error!(
            "Expected an expression before this delimiter",
            token_stream.tokens[end_index]
                .location.clone()
                ,
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Add a valid expression before this token",
            }
        )
    }

    if !stop_tokens
        .iter()
        .any(|stop| token_stream.tokens[end_index].kind == *stop)
    {
        let expected_tokens = stop_tokens
            .iter()
            .map(|token| format!("{token:?}"))
            .collect::<Vec<_>>()
            .join(", ");

        return_syntax_error!(
            format!(
                "Expected one of [{}] to end this expression",
                expected_tokens
            ),
            token_stream.tokens[end_index]
                .location.clone()
                ,
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Add the required delimiter token after this expression",
            }
        )
    }

    let mut expression_tokens = token_stream.tokens[start_index..end_index].to_vec();
    expression_tokens.push(Token::new(
        TokenKind::Eof,
        token_stream.tokens[end_index].location.clone(),
    ));

    let mut scoped_stream = FileTokens::new(token_stream.src_path.clone(), expression_tokens);
    let expression = create_expression(
        &mut scoped_stream,
        context,
        data_type,
        ownership,
        false,
        string_table,
    )?;

    token_stream.index = end_index;
    Ok(expression)
}
