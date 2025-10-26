use super::ast_nodes::NodeKind;
use crate::compiler::compiler_errors::{CompileError};
use crate::compiler::compiler_warnings::{CompilerWarning, WarningKind};
use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::ast_nodes::{Arg, AstNode};
use crate::compiler::parsers::builtin_methods::get_builtin_methods;
use crate::compiler::parsers::expressions::expression::{ExpressionKind};
use crate::compiler::parsers::expressions::mutation::handle_mutation;
use crate::compiler::parsers::expressions::parse_expression::create_multiple_expressions;

use crate::compiler::parsers::ast::{ContextKind, ScopeContext};
use crate::compiler::parsers::statements::branching::create_branch;
use crate::compiler::parsers::statements::functions::{FunctionSignature, parse_function_call};
use crate::compiler::parsers::statements::loops::create_loop;
use crate::compiler::parsers::statements::variables::new_arg;
use crate::compiler::parsers::tokens::{FileTokens, TokenKind};
use crate::compiler::traits::ContainsReferences;
use crate::tokenizer::END_SCOPE_CHAR;
use crate::{
    ast_log, return_compiler_error, return_rule_error, return_syntax_error, settings, timer_log,
};

pub fn new_ast(
    token_stream: &mut FileTokens,
    mut context: ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
) -> Result<Vec<AstNode>, CompileError> {
    let mut ast: Vec<AstNode> =
        Vec::with_capacity(token_stream.length / settings::TOKEN_TO_NODE_RATIO);

    while token_stream.index < token_stream.length {
        // This should be starting after the imports
        let current_token = token_stream.current_token_kind().to_owned();

        ast_log!("Parsing Token: {:?}", current_token);

        match current_token {
            TokenKind::ModuleStart(..) => {
                // Module start token is only used for naming; skip it.
                token_stream.advance();
            }

            // New Function or Variable declaration
            TokenKind::Symbol(ref name) => {
                // Check if this has already been declared (is a reference)
                if let Some(arg) = context.get_reference(name) {
                    // Then the associated mutation afterward.
                    // Or error if trying to mutate an immutable reference

                    // Move past the name
                    token_stream.advance();

                    check_for_dot_access(token_stream, arg, &context, &mut ast)?;

                    // Check what comes after the variable reference
                    match token_stream.current_token_kind() {

                        // ---------------------------
                        //          MUTATION
                        // ---------------------------
                        // Assignment operators
                        TokenKind::Assign
                        | TokenKind::AddAssign
                        | TokenKind::SubtractAssign
                        | TokenKind::MultiplyAssign
                        | TokenKind::DivideAssign
                        | TokenKind::ExponentAssign
                        | TokenKind::RootAssign => {
                            ast.push(handle_mutation(token_stream, arg, &context)?);
                        }

                        // Type declarations after variable reference - error (shadowing not supported)
                        TokenKind::DatatypeInt
                        | TokenKind::DatatypeFloat
                        | TokenKind::DatatypeBool
                        | TokenKind::DatatypeString

                        // Mutable token after variable reference - this is an error for reassignment
                        | TokenKind::Mutable => {
                            // Look ahead to see if this is ~= (mutable assignment)
                            if let Some(TokenKind::Assign) = token_stream.peek_next_token() {
                                // This is invalid: var ~= value where var already exists
                                // ~= should only be used for initial declarations, not reassignments
                                return_syntax_error!(
                                    token_stream.current_location(),
                                    "Invalid use of '~=' for reassignment. Variable '{}' is already declared. Use '=' to mutate it or create a new variable with a different name.",
                                    name
                                );
                            } else {
                                return_rule_error!(
                                    token_stream.current_location(),
                                    "Variable '{}' is already declared. Shadowing is not supported in Beanstalk. Use '=' to mutate its value or choose a different variable name",
                                    name
                                );
                            }
                        }

                        // ----------------------------
                        //        FUNCTION CALLS
                        // ----------------------------
                        TokenKind::OpenParenthesis => {
                            if let DataType::Function(signature) =
                                &arg.value.data_type
                            {
                                ast.push(parse_function_call(
                                    token_stream,
                                    name,
                                    &context,
                                    signature,
                                )?)
                            }
                        }

                        // At top level, a bare variable reference without assignment is a syntax error
                        _ => {
                            return_syntax_error!(
                                token_stream.current_location(),
                                "Unexpected token '{:?}' after variable reference '{}'. Expected assignment operator (=, +=, -=, etc.) for mutation",
                                token_stream.current_token_kind(),
                                name
                            );
                        }
                    }

                // ----------------------------
                //     HOST FUNCTION CALLS
                // ----------------------------
                } else if let Some(host_func_call) = context.host_registry.get_function(name) {
                    // Move past the name
                    token_stream.advance();

                    // Convert return types to Arg format
                    let converted_returns = host_func_call
                        .return_types
                        .iter()
                        .map(|x| x.to_arg())
                        .collect::<Vec<Arg>>();

                    let signature = FunctionSignature {
                        parameters: host_func_call.parameters.to_owned(),
                        returns: converted_returns.to_owned(),
                    };

                    ast.push(parse_function_call(
                        token_stream,
                        name,
                        &context,
                        &signature,
                    )?)
                } else {
                    let arg = new_arg(token_stream, name, &context, warnings)?;

                    // -----------------------------
                    //    NEW STRUCT DECLARATIONS
                    // -----------------------------
                    match arg.value.kind {
                        ExpressionKind::StructDefinition(ref params) => {
                            ast.push(AstNode {
                                kind: NodeKind::StructDefinition(
                                    name.to_owned(),
                                    params.to_owned(),
                                ),
                                location: token_stream.current_location(),
                                scope: context.scope_name.to_owned(),
                            });
                        }

                        // -----------------------------
                        //   NEW FUNCTION DECLARATION
                        // -----------------------------
                        ExpressionKind::Function(ref signature, ref body) => {
                            ast.push(AstNode {
                                kind: NodeKind::Function(
                                    name.to_owned(),
                                    signature.to_owned(),
                                    body.to_owned(),
                                ),
                                location: token_stream.current_location(),
                                scope: context.scope_name.to_owned(),
                            });
                        }

                        // -----------------------------
                        //   NEW VARIABLE DECLARATIONS
                        // -----------------------------
                        _ => {
                            ast.push(AstNode {
                                kind: NodeKind::VariableDeclaration(
                                    arg.to_owned(),
                                ),
                                location: token_stream.current_location(),
                                scope: context.scope_name.to_owned(),
                            });
                        }
                    }

                    context.add_var(arg);
                }
            }

            // Control Flow
            TokenKind::For => {
                token_stream.advance();

                ast.push(create_loop(
                    token_stream,
                    context.new_child_control_flow(ContextKind::Loop),
                    warnings,
                )?);
            }

            TokenKind::If => {
                token_stream.advance();

                // This is extending as it might get folded into a vec of nodes
                ast.extend(create_branch(
                    token_stream,
                    &mut context.new_child_control_flow(ContextKind::Branch),
                    warnings,
                )?);
            }

            TokenKind::Else => {
                // If we are inside an if / match statement, break out
                if context.kind == ContextKind::Branch {
                    break;
                } else {
                    return_rule_error!(
                        token_stream.current_location(),
                        "Unexpected use of 'else' keyword. You can only be used inside an if statement or match statement",
                    )
                }
            }

            // IGNORED TOKENS
            TokenKind::Newline | TokenKind::Empty => {
                // Skip standalone newlines / empty tokens
                token_stream.advance();
            }

            TokenKind::Return => {
                if !matches!(context.kind, ContextKind::Function) {
                    return_rule_error!(
                        token_stream.current_location(),
                        "Return statements can only be used inside functions",
                    )
                }

                token_stream.advance();

                let return_values = create_multiple_expressions(token_stream, &context, false)?;

                // if !return_value.is_pure() {
                //     *pure = false;
                // }

                ast.push(AstNode {
                    kind: NodeKind::Return(return_values),
                    location: token_stream.current_location(),
                    scope: context.scope_name.to_owned(),
                });
            }

            TokenKind::End => {
                // Check that this is a valid scope for a scope to close
                // Module scope should not have an 'end' anywhere
                match context.kind {
                    ContextKind::Expression => {
                        return_syntax_error!(
                            token_stream.current_location(),
                            "Unexpected scope close with '{END_SCOPE_CHAR}'. Expressions are not terminated like this.\
                            Surround the expression with brackets if you need it to be multi-line. This might just be a compiler bug."
                        );
                    }
                    ContextKind::Template => {
                        return_syntax_error!(
                            token_stream.current_location(),
                            "Unexpected use of '{END_SCOPE_CHAR}' inside a template. Templates are not closed with '{END_SCOPE_CHAR}'.\
                            If you are seeing this error, this might be a compiler bug instead."
                        )
                    }
                    _ => {
                        token_stream.advance();
                        break;
                    }
                }
            }

            TokenKind::Export => {
                // You can only export functions and variables at the top level
                // Push it as a warning
                warnings.push(CompilerWarning::new(
                    "You can only export functions and variables from the top level of a file",
                    token_stream.current_location(),
                    WarningKind::PointlessExport,
                    context.scope_name.to_owned(),
                ));
                token_stream.advance();
            }

            TokenKind::Eof => {
                break;
            }

            // Or stuff that hasn't been implemented yet
            _ => {
                return_compiler_error!(
                    "Token not recognised by AST parser when creating AST: {:?}",
                    &token_stream.current_token_kind()
                )
            }
        }
    }

    Ok(ast)
}

fn check_for_dot_access(
    token_stream: &mut FileTokens,
    arg: &Arg,
    context: &ScopeContext,
    ast: &mut Vec<AstNode>,
) -> Result<(), CompileError> {
    // Name of variable, with any accesses added to the path
    let mut scope = context.scope_name.to_owned();

    // We will need to keep pushing nodes if there are accesses after method calls
    while token_stream.current_token_kind() == &TokenKind::Dot {
        // Move past the dot
        token_stream.advance();

        // Currently, there is no just integer access.
        // Only properties or methods are accessed on structs and collections.
        // Collections have a .get() method for accessing elements, no [] syntax.
        if let TokenKind::Symbol(name, ..) = token_stream.current_token_kind().to_owned() {
            let members = match &arg.value.data_type {
                DataType::Parameters(inner_args) => inner_args,
                DataType::Function(sig) => &sig.returns,
                _ => &get_builtin_methods(&arg.value.data_type),
            };

            // Nothing to access error
            if members.is_empty() {
                return_rule_error!(
                    token_stream.current_location(),
                    "'{}' has No methods or properties to access 😞",
                    name
                )
            }

            // No access with that name exists error
            let access = match members.iter().find(|member| member.name == *name) {
                Some(access) => access,
                None => return_rule_error!(
                    token_stream.current_location(),
                    "Can't find property or method '{}' inside '{}'",
                    name,
                    arg.name
                ),
            };

            // Add the name to the scope
            scope.push(&access.name);

            // Move past the name
            token_stream.advance();

            // ----------------------------
            //        METHOD CALLS
            // ----------------------------
            if let DataType::Function(signature) = &access.value.data_type {
                ast.push(parse_function_call(
                    token_stream,
                    &name,
                    context,
                    signature,
                )?)
            }
        } else {
            return_rule_error!(
                token_stream.current_location(),
                "Expected the name of a property or method after the dot (accessing a member of the variable such as a method or property). Found '{:?}' instead.",
                token_stream.current_token_kind()
            )
        }
    }

    Ok(())
}
