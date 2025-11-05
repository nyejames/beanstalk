use super::ast_nodes::NodeKind;
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::compiler_warnings::{CompilerWarning, WarningKind};
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::parsers::ast_nodes::{Arg, AstNode};
use crate::compiler::parsers::builtin_methods::get_builtin_methods;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::expressions::mutation::handle_mutation;
use crate::compiler::parsers::expressions::parse_expression::create_multiple_expressions;
use crate::tokenizer::tokenizer::END_SCOPE_CHAR;

use crate::compiler::parsers::ast::{ContextKind, ScopeContext};
use crate::compiler::parsers::statements::branching::create_branch;
use crate::compiler::parsers::statements::create_template_node::Template;
use crate::compiler::parsers::statements::functions::parse_function_call;
use crate::compiler::parsers::statements::loops::create_loop;
use crate::compiler::parsers::statements::variables::new_arg;
use crate::compiler::parsers::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler::string_interning::StringTable;
use crate::compiler::traits::ContainsReferences;
use crate::{ast_log, return_compiler_error, return_rule_error, return_syntax_error, settings};

pub fn function_body_to_ast(
    token_stream: &mut FileTokens,
    mut context: ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
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
            TokenKind::Symbol(id) => {
                // Check if this has already been declared (is a reference)
                if let Some(arg) = context.get_reference(&id) {
                    // Then the associated mutation afterward.
                    // Or error if trying to mutate an immutable reference

                    // Move past the name
                    token_stream.advance();

                    check_for_dot_access(token_stream, arg, &context, &mut ast, string_table)?;

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
                            ast.push(handle_mutation(token_stream, arg, &context, string_table)?);
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
                                    string_table.resolve(id)
                                );
                            } else {
                                return_rule_error!(
                                    token_stream.current_location(),
                                    "Variable '{}' is already declared. Shadowing is not supported in Beanstalk. Use '=' to mutate its value or choose a different variable name",
                                    string_table.resolve(id)
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
                                    &id,
                                    &context,
                                    signature,
                                    string_table,
                                )?)
                            }
                        }

                        // At top level, a bare variable reference without assignment is a syntax error
                        _ => {
                            return_syntax_error!(
                                token_stream.current_location(),
                                "Unexpected token '{:?}' after variable reference '{}'. Expected assignment operator (=, +=, -=, etc.) for mutation",
                                token_stream.current_token_kind(),
                                &id
                            );
                        }
                    }

                // ----------------------------
                //     HOST FUNCTION CALLS
                // ----------------------------
                } else if let Some(host_func_call) = context.host_registry.get_function(&id) {
                    // Move past the name
                    token_stream.advance();

                    // Convert return types to Arg format
                    let signature = host_func_call.params_to_signature(string_table);

                    ast.push(parse_function_call(
                        token_stream,
                        &id,
                        &context,
                        &signature,
                        string_table,
                    )?)
                } else {
                    let arg = new_arg(token_stream, id, &context, warnings, string_table)?;

                    // -----------------------------
                    //    NEW STRUCT DECLARATIONS
                    // -----------------------------
                    match arg.value.kind {
                        ExpressionKind::StructDefinition(ref params) => {
                            ast.push(AstNode {
                                kind: NodeKind::StructDefinition(id, params.to_owned()),
                                location: token_stream.current_location(),
                                scope: context.scope.clone(),
                            });
                        }

                        // -----------------------------
                        //   NEW FUNCTION DECLARATION
                        // -----------------------------
                        ExpressionKind::Function(ref signature, ref body) => {
                            ast.push(AstNode {
                                kind: NodeKind::Function(id, signature.to_owned(), body.to_owned()),
                                location: token_stream.current_location(),
                                scope: context.scope.clone(),
                            });
                        }

                        // -----------------------------
                        //   NEW VARIABLE DECLARATIONS
                        // -----------------------------
                        _ => {
                            ast.push(AstNode {
                                kind: NodeKind::VariableDeclaration(arg.to_owned()),
                                location: token_stream.current_location(),
                                scope: context.scope.clone(),
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
                    string_table,
                )?);
            }

            TokenKind::If => {
                token_stream.advance();

                // This is extending as it might get folded into a vec of nodes
                ast.extend(create_branch(
                    token_stream,
                    &mut context.new_child_control_flow(ContextKind::Branch),
                    warnings,
                    string_table,
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

                let return_values =
                    create_multiple_expressions(token_stream, &context, false, string_table)?;

                // if !return_value.is_pure() {
                //     *pure = false;
                // }

                ast.push(AstNode {
                    kind: NodeKind::Return(return_values),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
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
                    context.scope.clone(),
                ));
                token_stream.advance();
            }

            // String template as an expression without being assigned.
            // This is the primary way to produce output in Beanstalk.
            // Top-level templates automatically output to a host-defined output mechanism.
            TokenKind::TemplateHead | TokenKind::ParentTemplate => {
                let template = Template::new(token_stream, &context, None, string_table)?;
                let expr = Expression::template(template, Ownership::MutableOwned);
                let template_output_name = string_table.intern("template_output");
                let beanstalk_io_module = string_table.intern("beanstalk_io");
                ast.push(AstNode {
                    kind: NodeKind::HostFunctionCall(
                        template_output_name,
                        Vec::from([expr]),
                        Vec::new(),
                        beanstalk_io_module,
                        template_output_name,
                        token_stream.current_location(),
                    ),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                })
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
    string_table: &mut StringTable,
) -> Result<(), CompileError> {
    // Name of variable, with any accesses added to the path
    let mut scope = context.scope.clone();

    // We will need to keep pushing nodes if there are accesses after method calls
    while token_stream.current_token_kind() == &TokenKind::Dot {
        // Move past the dot
        token_stream.advance();

        // Currently, there is no just integer access.
        // Only properties or methods are accessed on structs and collections.
        // Collections have a .get() method for accessing elements, no [] syntax.
        if let TokenKind::Symbol(id) = token_stream.current_token_kind().to_owned() {
            let members = match &arg.value.data_type {
                DataType::Parameters(inner_args) => inner_args,
                DataType::Function(sig) => &sig.returns,
                _ => &get_builtin_methods(&arg.value.data_type, string_table),
            };

            // Nothing to access error
            if members.is_empty() {
                return_rule_error!(
                    token_stream.current_location(),
                    "'{}' has No methods or properties to access 😞",
                    &id
                )
            }

            // No access with that name exists error
            let access = match members.iter().find(|member| member.id == id) {
                Some(access) => access,
                None => return_rule_error!(
                    token_stream.current_location(),
                    "Can't find property or method '{}' inside '{}'",
                    string_table.resolve(id),
                    string_table.resolve(arg.id)
                ),
            };

            // Add the name to the scope
            scope.push(access.id);

            // Move past the name
            token_stream.advance();

            // ----------------------------
            //        METHOD CALLS
            // ----------------------------
            if let DataType::Function(signature) = &access.value.data_type {
                ast.push(parse_function_call(
                    token_stream,
                    &id,
                    context,
                    signature,
                    string_table,
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
