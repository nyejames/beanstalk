use crate::compiler::compiler_errors::CompileError;
use crate::compiler::compiler_errors::ErrorType;
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::parsers::ast_nodes::{Arg, AstNode, NodeKind};
use crate::compiler::parsers::build_ast::{ScopeContext, new_ast};
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::expressions::parse_expression::create_expression;
use crate::compiler::parsers::tokens::{TokenContext, TokenKind};
use crate::compiler::traits::ContainsReferences;
use crate::{ast_log, return_syntax_error};

// Returns a ForLoop node or WhileLoop Node (or error if there's invalid syntax)
// TODO: Loop invariance analysis.
// I reckon this is possible to do at this stage through keeping a list of invariants.
// Pushing to a list of possible invariant calculations that could be moved to a header.
// This would require tracking whether a mutable var inside the loop that is used in an expression changes during any possible loop branch.
// If it does, then it can be left alone, otherwise it can be marked as invariant.
// Anything marked as invariant when parsing the AST to a lower IR can be hoisted up to the loop header.
pub fn create_loop(
    token_stream: &mut TokenContext,
    mut context: ScopeContext,
) -> Result<AstNode, CompileError> {
    ast_log!("Creating a Loop");

    // First check if the loop has a declaration or just an expression
    // If the first item is NOT a reference, then it is the item for the loop
    match token_stream.current_token_kind().to_owned() {
        TokenKind::Symbol(name, ..) => {
            // -----------------------------
            //          WHILE LOOP
            //  (existing variable found)
            // -----------------------------

            if let Some(arg) = context.find_reference(&name) {
                let mut data_type = arg.value.data_type.to_owned();
                let condition = create_expression(token_stream, &context, &mut data_type, false)?;

                // Make sure this condition is a boolean expression
                return match data_type {
                    DataType::Bool(..) => {
                        // Make sure there is a colon after the condition
                        if token_stream.current_token_kind() != &TokenKind::Colon {
                            return_syntax_error!(
                                token_stream.current_location(),
                                "A loop must have a colon after the condition",
                            );
                        }

                        token_stream.advance();
                        let scope = context.scope_name.clone();

                        // create while loop
                        Ok(AstNode {
                            kind: NodeKind::WhileLoop(
                                condition,
                                new_ast(token_stream, context, false)?.ast,
                            ),
                            location: token_stream.current_location(),
                            scope,
                        })
                    }

                    _ => {
                        return_syntax_error!(
                            token_stream.current_location(),
                            "A loop condition using an existing variable must be a boolean expression (true or false). Found a {} expression",
                            data_type.to_string()
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
                    token_stream.current_location(),
                    "A loop must have an 'in' keyword after the variable",
                );
            }

            token_stream.advance();

            // TODO: need to check for mutable reference syntax
            let mut iterable_type = DataType::Inferred(Ownership::ImmutableReference);
            let iterated_item =
                create_expression(token_stream, &context, &mut iterable_type, false)?;

            // Make sure this type can be iterated over
            if !iterable_type.is_iterable() {
                return_syntax_error!(
                    token_stream.current_location(),
                    "The type {:?} is not iterable",
                    iterable_type
                );
            }

            // Make sure there is a colon
            if token_stream.current_token_kind() != &TokenKind::Colon {
                return_syntax_error!(
                    token_stream.current_location(),
                    "A loop must have a colon after the condition",
                );
            }

            token_stream.advance();

            let loop_arg = Arg {
                name: name.to_owned(),
                value: Expression::new(
                    iterated_item.kind.to_owned(),
                    token_stream.current_location(),
                    iterated_item.data_type.to_owned(),
                    context.lifetime,
                ),
            };

            context.declarations.push(loop_arg.to_owned());

            Ok(AstNode {
                scope: context.scope_name.to_owned(),
                kind: NodeKind::ForLoop(
                    Box::new(loop_arg),
                    iterated_item,
                    new_ast(token_stream, context, false)?.ast,
                ),
                location: token_stream.current_location(),
            })
        }

        _ => {
            return_syntax_error!(
                token_stream.current_location(),
                "Loops must have a variable declaration or an expression",
            );
        }
    }
}
