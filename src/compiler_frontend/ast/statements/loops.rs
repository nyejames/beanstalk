use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{
    AstNode, Declaration, ForLoopRange, NodeKind, RangeEndKind,
};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression_until;
use crate::compiler_frontend::ast::function_body_to_ast::function_body_to_ast;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::traits::ContainsReferences;
use crate::{ast_log, return_syntax_error};

pub fn create_loop(
    token_stream: &mut FileTokens,
    context: ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    ast_log!("Creating a Loop");

    // `loop <symbol> in ...` is the only iteration header shape for now.
    // Every other header is parsed as a boolean conditional loop.
    if let TokenKind::Symbol(name) = token_stream.current_token_kind().to_owned()
        && token_stream.peek_next_token() == Some(&TokenKind::In)
    {
        return create_iteration_loop(token_stream, context, warnings, string_table, name);
    }

    create_conditional_loop(token_stream, context, warnings, string_table)
}

fn create_conditional_loop(
    token_stream: &mut FileTokens,
    context: ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    let location = token_stream.current_location();
    let scope = context.scope.clone();

    let mut condition_type = DataType::Bool;
    let condition = create_expression_until(
        token_stream,
        &context,
        &mut condition_type,
        &Ownership::ImmutableOwned,
        &[TokenKind::Colon],
        string_table,
    )?;

    if !is_boolean_expression(&condition) {
        let found_type: &'static str = Box::leak(
            condition
                .data_type
                .display_with_table(string_table)
                .into_boxed_str(),
        );
        return_syntax_error!(
            format!(
                "Loop condition must be a boolean expression. Found '{}'",
                condition.data_type.display_with_table(string_table)
            ),
            token_stream.current_location().to_error_location(string_table),
            {
                FoundType => found_type,
                ExpectedType => "Bool",
                CompilationStage => "Loop Parsing",
                PrimarySuggestion => "Use a boolean expression after 'loop', e.g. loop is_ready():",
            }
        );
    }

    if token_stream.current_token_kind() != &TokenKind::Colon {
        return_syntax_error!(
            "A loop must have ':' after the loop header",
            token_stream.current_location().to_error_location(string_table),
            {
                CompilationStage => "Loop Parsing",
                PrimarySuggestion => "Add ':' after the loop condition",
                SuggestedInsertion => ":",
            }
        );
    }

    token_stream.advance();

    Ok(AstNode {
        kind: NodeKind::WhileLoop(
            condition,
            function_body_to_ast(token_stream, context, warnings, string_table)?,
        ),
        location,
        scope,
    })
}

fn create_iteration_loop(
    token_stream: &mut FileTokens,
    mut context: ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
    binder_name: crate::compiler_frontend::string_interning::StringId,
) -> Result<AstNode, CompilerError> {
    let location = token_stream.current_location();

    if context.get_reference(&binder_name).is_some() {
        return_syntax_error!(
            format!(
                "Loop binder '{}' is already declared in this scope",
                string_table.resolve(binder_name)
            ),
            token_stream.current_location().to_error_location(string_table),
            {
                CompilationStage => "Loop Parsing",
                PrimarySuggestion => "Use a new binder name for the loop item",
            }
        );
    }

    token_stream.advance();
    if token_stream.current_token_kind() != &TokenKind::In {
        return_syntax_error!(
            "Iteration loops must include 'in' after the binder name",
            token_stream.current_location().to_error_location(string_table),
            {
                CompilationStage => "Loop Parsing",
                PrimarySuggestion => "Use syntax like: loop i in 0 to 10:",
                SuggestedInsertion => "in",
            }
        );
    }

    token_stream.advance();

    // Parse header as: `start (to|upto) end [by step] :`
    let mut start_type = DataType::Inferred;
    let start = create_expression_until(
        token_stream,
        &context,
        &mut start_type,
        &Ownership::ImmutableReference,
        &[
            TokenKind::ExclusiveRange,
            TokenKind::InclusiveRange,
            TokenKind::Colon,
        ],
        string_table,
    )?;

    let end_kind = match token_stream.current_token_kind() {
        TokenKind::ExclusiveRange => RangeEndKind::Exclusive,
        TokenKind::InclusiveRange => RangeEndKind::Inclusive,
        TokenKind::Colon => {
            return_syntax_error!(
                "Collection iteration is not implemented yet; use a range loop with 'to' or 'upto'",
                token_stream.current_location().to_error_location(string_table),
                {
                    CompilationStage => "Loop Parsing",
                    PrimarySuggestion => "Use range syntax like: loop i in 0 to 10:",
                }
            );
        }
        _ => {
            return_syntax_error!(
                "Range loops must include 'to' or 'upto' between bounds",
                token_stream.current_location().to_error_location(string_table),
                {
                    CompilationStage => "Loop Parsing",
                    PrimarySuggestion => "Use syntax like: loop i in start to end:",
                    AlternativeSuggestion => "Use 'upto' for inclusive end bounds",
                }
            );
        }
    };

    token_stream.advance();

    let mut end_type = DataType::Inferred;
    let end = create_expression_until(
        token_stream,
        &context,
        &mut end_type,
        &Ownership::ImmutableReference,
        &[TokenKind::By, TokenKind::Colon],
        string_table,
    )?;

    let step = if token_stream.current_token_kind() == &TokenKind::By {
        token_stream.advance();

        let mut step_type = DataType::Inferred;
        Some(create_expression_until(
            token_stream,
            &context,
            &mut step_type,
            &Ownership::ImmutableReference,
            &[TokenKind::Colon],
            string_table,
        )?)
    } else {
        None
    };

    let start_numeric = numeric_type_for_expression(&start).ok_or_else(|| {
        CompilerError::new_syntax_error(
            "Range start must be numeric (Int or Float)",
            token_stream
                .current_location()
                .to_error_location(string_table),
        )
    })?;
    let end_numeric = numeric_type_for_expression(&end).ok_or_else(|| {
        CompilerError::new_syntax_error(
            "Range end must be numeric (Int or Float)",
            token_stream
                .current_location()
                .to_error_location(string_table),
        )
    })?;

    let step_numeric = if let Some(step_expr) = &step {
        Some(numeric_type_for_expression(step_expr).ok_or_else(|| {
            CompilerError::new_syntax_error(
                "Range step must be numeric (Int or Float)",
                token_stream
                    .current_location()
                    .to_error_location(string_table),
            )
        })?)
    } else {
        None
    };

    let uses_float = matches!(start_numeric, DataType::Float)
        || matches!(end_numeric, DataType::Float)
        || matches!(step_numeric, Some(DataType::Float));

    // Float ranges require explicit step to avoid accidental non-terminating loops.
    if uses_float && step.is_none() {
        return_syntax_error!(
            "Float ranges require an explicit 'by' step",
            token_stream.current_location().to_error_location(string_table),
            {
                CompilationStage => "Loop Parsing",
                PrimarySuggestion => "Add an explicit step, e.g. loop t in 0.0 to 1.0 by 0.1:",
            }
        );
    }

    if let Some(step_expr) = &step
        && is_zero_numeric_literal(step_expr)
    {
        return_syntax_error!(
            "Range step cannot be zero",
            token_stream.current_location().to_error_location(string_table),
            {
                CompilationStage => "Loop Parsing",
                PrimarySuggestion => "Use a non-zero step value after 'by'",
            }
        );
    }

    if token_stream.current_token_kind() != &TokenKind::Colon {
        return_syntax_error!(
            "A loop must have ':' after the loop header",
            token_stream.current_location().to_error_location(string_table),
            {
                CompilationStage => "Loop Parsing",
                PrimarySuggestion => "Add ':' after the loop header",
                SuggestedInsertion => ":",
            }
        );
    }

    let binding_type = if uses_float {
        DataType::Float
    } else {
        DataType::Int
    };

    let loop_binding = Declaration {
        id: context.scope.append(binder_name),
        value: Expression::new(
            ExpressionKind::None,
            location.clone(),
            binding_type,
            Ownership::ImmutableOwned,
        ),
    };
    context.add_var(loop_binding.to_owned());

    token_stream.advance();

    Ok(AstNode {
        scope: context.scope.to_owned(),
        kind: NodeKind::ForLoop(
            Box::new(loop_binding),
            ForLoopRange {
                start,
                end,
                end_kind,
                step,
            },
            function_body_to_ast(token_stream, context, warnings, string_table)?,
        ),
        location,
    })
}

fn is_boolean_expression(expression: &Expression) -> bool {
    match &expression.data_type {
        DataType::Bool => true,
        DataType::Reference(inner) => matches!(inner.as_ref(), DataType::Bool),
        _ => false,
    }
}

fn numeric_type_for_expression(expression: &Expression) -> Option<DataType> {
    numeric_type_from_datatype(&expression.data_type)
}

fn numeric_type_from_datatype(data_type: &DataType) -> Option<DataType> {
    match data_type {
        DataType::Int => Some(DataType::Int),
        DataType::Float => Some(DataType::Float),
        DataType::Reference(inner) => numeric_type_from_datatype(inner),
        _ => None,
    }
}

fn is_zero_numeric_literal(expression: &Expression) -> bool {
    match expression.kind {
        ExpressionKind::Int(value) => value == 0,
        ExpressionKind::Float(value) => value == 0.0,
        _ => false,
    }
}
