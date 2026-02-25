use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, Operator,
};
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::ast::function_body_to_ast::function_body_to_ast;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::traits::ContainsReferences;
use crate::{ast_log, return_syntax_error};

// Returns a ForLoop node or WhileLoop Node (or error if there's invalid syntax)
// TODO: Loop invariance analysis.
// I reckon this is possible to do at this stage through keeping a list of invariants.
// Pushing to a list of possible invariant calculations that could be moved to a header.
// This would require tracking whether a mutable var inside the loop that is used in an expression changes during any possible loop branch.
// If it does, then it can be left alone, otherwise it can be marked as invariant.
// Anything marked as invariant when parsing the AST to a lower IR can be hoisted up to the loop header.
pub fn create_loop(
    token_stream: &mut FileTokens,
    mut context: ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    ast_log!("Creating a Loop");

    // First check if the loop has a declaration or just an expression
    // If the first item is NOT a reference, then it is the item for the loop
    match token_stream.current_token_kind().to_owned() {
        TokenKind::Symbol(name, ..) => {
            // -----------------------------
            //          WHILE LOOP
            //  (existing variable found)
            // -----------------------------

            if let Some(arg) = context.get_reference(&name) {
                let mut data_type = arg.value.data_type.to_owned();
                let ownership = &arg.value.ownership;
                let condition = create_expression(
                    token_stream,
                    &context,
                    &mut data_type,
                    ownership,
                    false,
                    string_table,
                )?;

                // Make sure this condition is a boolean expression
                return match data_type {
                    DataType::Bool => {
                        // Make sure there is a colon after the condition
                        if token_stream.current_token_kind() != &TokenKind::Colon {
                            return_syntax_error!(
                                "A loop must have a colon after the condition",
                                token_stream.current_location().to_error_location(string_table),
                                {
                                    CompilationStage => "Loop Parsing",
                                    PrimarySuggestion => "Add ':' after the loop condition to open the loop body",
                                    SuggestedInsertion => ":",
                                }
                            );
                        }

                        token_stream.advance();
                        let scope = context.scope.clone();

                        // create while loop
                        Ok(AstNode {
                            kind: NodeKind::WhileLoop(
                                condition,
                                function_body_to_ast(
                                    token_stream,
                                    context,
                                    warnings,
                                    string_table,
                                )?,
                            ),
                            location: token_stream.current_location(),
                            scope,
                        })
                    }

                    _ => {
                        let type_str: &'static str =
                            Box::leak(data_type.to_string().into_boxed_str());
                        return_syntax_error!(
                            format!(
                                "A loop condition using an existing variable must be a boolean expression (true or false). Found a {} expression",
                                data_type.to_string()
                            ),
                            token_stream.current_location().to_error_location(string_table),
                            {
                                FoundType => type_str,
                                ExpectedType => "Bool",
                                CompilationStage => "Loop Parsing",
                                PrimarySuggestion => "Use a boolean expression for the while loop condition",
                            }
                        );
                    }
                };
            }

            // -----------------------------
            //          FOR LOOP
            //     (new variable found)
            // -----------------------------

            // TODO: might need to check for additional optional stuff like a type declaration or something here
            token_stream.advance();

            // Make sure there is an 'in' keyword after the variable
            if token_stream.current_token_kind() != &TokenKind::In {
                return_syntax_error!(
                    format!("A loop must have an 'in' keyword after the variable: {}", string_table.resolve(name)),
                    token_stream.current_location().to_error_location(string_table),
                    {
                        CompilationStage => "Loop Parsing",
                        PrimarySuggestion => "Add 'in' keyword after the loop variable",
                        SuggestedInsertion => "in",
                    }
                );
            }

            token_stream.advance();

            // TODO: need to check for mutable reference syntax
            // Is just defaulting to immutable reference for now
            let mut iterable_type = DataType::Inferred;
            let iterated_item = create_expression(
                token_stream,
                &context,
                &mut iterable_type,
                &Ownership::ImmutableReference,
                false,
                string_table,
            )?;

            if !is_range_iteration_expression(&iterated_item) {
                let type_str: &'static str = Box::leak(
                    iterated_item
                        .data_type
                        .display_with_table(string_table)
                        .into_boxed_str(),
                );

                return_syntax_error!(
                    format!(
                        "For-loop lowering currently supports only range iteration. Found '{}'",
                        iterated_item.data_type.display_with_table(string_table)
                    ),
                    token_stream.current_location().to_error_location(string_table),
                    {
                        FoundType => type_str,
                        ExpectedType => "Range",
                        CompilationStage => "Loop Parsing",
                        PrimarySuggestion => "Use a range expression like 'start to end' for now",
                        AlternativeSuggestion => "Collection/string for-loop lowering has not been implemented in this HIR phase yet",
                    }
                );
            }

            // For this phase we only accept ranges with numeric bounds so the loop binding has a
            // stable numeric type before HIR lowering.
            if !matches!(
                infer_range_binding_type(&iterated_item),
                Some(DataType::Int) | Some(DataType::Float)
            ) {
                return_syntax_error!(
                    "For-loop range bounds must currently be numeric (Int or Float)",
                    token_stream.current_location().to_error_location(string_table),
                    {
                        CompilationStage => "Loop Parsing",
                        PrimarySuggestion => "Use numeric range bounds such as '0 to 10'",
                    }
                );
            }

            // Make sure there is a colon
            if token_stream.current_token_kind() != &TokenKind::Colon {
                return_syntax_error!(
                    "A loop must have a colon after the condition",
                    token_stream.current_location().to_error_location(string_table),
                    {
                        CompilationStage => "Loop Parsing",
                        PrimarySuggestion => "Add ':' after the loop condition to open the loop body",
                        SuggestedInsertion => ":",
                    }
                );
            }

            token_stream.advance();

            let binding_type = infer_range_binding_type(&iterated_item).unwrap_or(DataType::Int);

            // The thing being iterated over
            let loop_arg = Declaration {
                id: context.scope.append(name),
                value: Expression::new(
                    iterated_item.kind.to_owned(),
                    token_stream.current_location(),
                    binding_type,
                    iterated_item.ownership.to_owned(),
                ),
            };

            context.declarations.push(loop_arg.to_owned());

            Ok(AstNode {
                scope: context.scope.to_owned(),
                kind: NodeKind::ForLoop(
                    Box::new(loop_arg),
                    iterated_item,
                    function_body_to_ast(token_stream, context, warnings, string_table)?,
                ),
                location: token_stream.current_location(),
            })
        }

        _ => {
            return_syntax_error!(
                "Loops must have a variable declaration or an expression",
                token_stream.current_location().to_error_location(string_table),
                {
                    CompilationStage => "Loop Parsing",
                    PrimarySuggestion => "Start the loop with a variable name for 'for' loops or a boolean expression for 'while' loops",
                }
            );
        }
    }
}

fn is_range_iteration_expression(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::Range(_, _) => true,
        ExpressionKind::Runtime(nodes) => nodes
            .iter()
            .any(|node| matches!(node.kind, NodeKind::Operator(Operator::Range))),
        _ => matches!(expression.data_type, DataType::Range),
    }
}

fn infer_range_binding_type(expression: &Expression) -> Option<DataType> {
    match &expression.kind {
        ExpressionKind::Range(start, end) => {
            if matches!(start.data_type, DataType::Float)
                || matches!(end.data_type, DataType::Float)
            {
                Some(DataType::Float)
            } else if matches!(start.data_type, DataType::Int | DataType::Bool)
                && matches!(end.data_type, DataType::Int | DataType::Bool)
            {
                Some(DataType::Int)
            } else {
                None
            }
        }

        ExpressionKind::Runtime(nodes) => {
            let range_uses_float = nodes.iter().any(|node| {
                matches!(
                    node.kind,
                    NodeKind::Rvalue(Expression {
                        kind: ExpressionKind::Float(_),
                        ..
                    })
                )
            });

            if range_uses_float {
                Some(DataType::Float)
            } else if is_range_iteration_expression(expression) {
                Some(DataType::Int)
            } else {
                None
            }
        }

        _ => None,
    }
}
