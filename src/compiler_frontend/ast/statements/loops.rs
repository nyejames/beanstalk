use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind, Var};
use crate::compiler_frontend::ast::build_ast::function_body_to_ast;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::parsers::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::string_interning::StringTable;
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
                                token_stream.current_location().to_error_location(&string_table),
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
                            token_stream.current_location().to_error_location(&string_table),
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
                    token_stream.current_location().to_error_location(&string_table),
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

            // Make sure this type can be iterated over
            if !iterable_type.is_iterable() {
                let type_str: &'static str =
                    Box::leak(format!("{:?}", iterable_type).into_boxed_str());
                return_syntax_error!(
                    format!("The type {:?} is not iterable", iterable_type),
                    token_stream.current_location().to_error_location(&string_table),
                    {
                        FoundType => type_str,
                        CompilationStage => "Loop Parsing",
                        PrimarySuggestion => "Use an iterable type like Collection or Range in the for loop",
                    }
                );
            }

            // Make sure there is a colon
            if token_stream.current_token_kind() != &TokenKind::Colon {
                return_syntax_error!(
                    "A loop must have a colon after the condition",
                    token_stream.current_location().to_error_location(&string_table),
                    {
                        CompilationStage => "Loop Parsing",
                        PrimarySuggestion => "Add ':' after the loop condition to open the loop body",
                        SuggestedInsertion => ":",
                    }
                );
            }

            token_stream.advance();

            // The thing being iterated over
            let loop_arg = Var {
                id: name.to_owned(),
                value: Expression::new(
                    iterated_item.kind.to_owned(),
                    token_stream.current_location(),
                    iterated_item.data_type.to_owned(),
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
                token_stream.current_location().to_error_location(&string_table),
                {
                    CompilationStage => "Loop Parsing",
                    PrimarySuggestion => "Start the loop with a variable name for 'for' loops or a boolean expression for 'while' loops",
                }
            );
        }
    }
}
