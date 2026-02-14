use super::ast_nodes::NodeKind;
use crate::compiler::compiler_errors::CompilerError;
use crate::compiler::compiler_warnings::{CompilerWarning, WarningKind};
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::parsers::ast_nodes::AstNode;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::expressions::mutation::handle_mutation;
use crate::compiler::parsers::expressions::parse_expression::create_multiple_expressions;

use crate::compiler::parsers::ast::{ContextKind, ScopeContext};
use crate::compiler::parsers::statements::branching::create_branch;
use crate::compiler::parsers::statements::create_template_node::Template;
use crate::compiler::parsers::statements::functions::parse_function_call;
use crate::compiler::parsers::statements::loops::create_loop;
use crate::compiler::parsers::statements::variables::new_var;
use crate::compiler::parsers::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler::string_interning::StringTable;
use crate::compiler::traits::ContainsReferences;
use crate::{ast_log, return_compiler_error, return_rule_error, return_syntax_error, settings};

pub fn function_body_to_ast(
    token_stream: &mut FileTokens,
    mut context: ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<Vec<AstNode>, CompilerError> {
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

            TokenKind::Symbol(id) => {
                // Check if this has already been declared (is a reference)
                if let Some(arg) = context.get_reference(&id) {
                    // Then the associated mutation afterward.
                    // Or error if trying to mutate an immutable reference

                    // Move past the name
                    token_stream.advance();

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
                            // So this seems to be the only case where we have a reference as an L-value.
                            // I think this means field access ONLY happens here if it happens at this stage,
                            // expression parsing will need to do its own thing separately

                            ast.push(handle_mutation(token_stream, &arg, &context, string_table)?);
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
                                let var_name_static: &'static str = Box::leak(string_table.resolve(id).to_string().into_boxed_str());
                                return_syntax_error!(
                                    format!("Invalid use of '~=' for reassignment. Variable '{}' is already declared. Use '=' to mutate it or create a new variable with a different name.", string_table.resolve(id)),
                                    token_stream.current_location().to_error_location(string_table), {
                                        VariableName => var_name_static,
                                        CompilationStage => "AST Construction",
                                        PrimarySuggestion => "Use '=' to mutate the existing variable instead of '~='",
                                    }
                                );
                            } else {
                                let var_name_static: &'static str = Box::leak(string_table.resolve(id).to_string().into_boxed_str());
                                return_rule_error!(
                                    format!("Variable '{}' is already declared. Shadowing is not supported in Beanstalk. Use '=' to mutate its value or choose a different variable name", string_table.resolve(id)),
                                    token_stream.current_location().to_error_location(string_table), {
                                        VariableName => var_name_static,
                                        CompilationStage => "AST Construction",
                                        PrimarySuggestion => "Use '=' to mutate the existing variable or choose a different name",
                                    }
                                );
                            }
                        }

                        // ----------------------------
                        //       FUNCTION CALLS
                        // ----------------------------
                        TokenKind::OpenParenthesis => {
                            if let DataType::Function(receiver, signature) =
                                &arg.value.data_type
                            {
                                // If this is a method, this should be an error
                                // As methods can only be called from their receivers
                                if receiver.is_some() {
                                    return_rule_error!(
                                        "This only exists as a method, not a standalone function. Method calls can only be made on the reciever of a function",
                                        token_stream.current_location().to_error_location(string_table), {
                                            CompilationStage => "AST Construction",
                                            PrimarySuggestion => "Call this method from an instance of its reciever, or define this as its own function",
                                        }
                                    )
                                }

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
                            let var_name_static: &'static str = Box::leak(string_table.resolve(id).to_string().into_boxed_str());
                            return_syntax_error!(
                                format!("Unexpected token '{:?}' after variable reference '{}'. Expected assignment operator (=, +=, -=, etc.) for mutation", token_stream.current_token_kind(), string_table.resolve(id)),
                                token_stream.current_location().to_error_location(string_table), {
                                    VariableName => var_name_static,
                                    CompilationStage => "AST Construction",
                                    PrimarySuggestion => "Add an assignment operator like '=' or '+=' after the variable",
                                }
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

                // -----------------------------------------
                //   New Function or Variable declaration
                // -----------------------------------------
                } else {
                    let arg = new_var(token_stream, id, &context, warnings, string_table)?;

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
                        "Unexpected use of 'else' keyword. It can only be used inside an if statement or match statement",
                        token_stream.current_location().to_error_location(&string_table), {
                            CompilationStage => "AST Construction",
                            PrimarySuggestion => "Remove the 'else' or place it inside an if/match statement",
                        }
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
                        "Return statements can only be used inside functions",
                        token_stream
                            .current_location()
                            .to_error_location(&string_table),
                        {}
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
                            "Unexpected scope close. Expressions are not terminated like this.
                            Surround the expression with brackets if you need it to be multi-line. This might just be a compiler bug.",
                            token_stream.current_location().to_error_location(&string_table), {

                            }
                        );
                    }
                    ContextKind::Template => {
                        return_syntax_error!(
                            "Unexpected use of ';' inside a template. Templates are not closed with ';'.
                            If you are seeing this error, this might be a compiler bug instead.",
                            token_stream.current_location().to_error_location(&string_table), {

                            }
                        )
                    }
                    _ => {
                        // Consume the end token
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
                    token_stream
                        .current_location()
                        .to_error_location(&string_table),
                    WarningKind::PointlessExport,
                    context.scope.to_path_buf(&string_table),
                ));
                token_stream.advance();
            }

            // String template at the top level of a function.
            TokenKind::TemplateHead | TokenKind::TopLevelTemplate => {
                // If this isn't the top level of the module, this should be an error
                // Only top level scope can have top level templates
                if context.kind != ContextKind::Module {
                    return_rule_error!(
                        "Templates can only be used like this at the top level. Not inside the body of a function",
                        token_stream
                            .current_location()
                            .to_error_location(&string_table),
                        {}
                    )
                }

                let template = Template::new(token_stream, &context, None, string_table)?;
                let expr = Expression::template(template, Ownership::MutableOwned);

                ast.push(AstNode {
                    kind: NodeKind::TopLevelTemplate(expr),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                })
            }

            TokenKind::Eof => {
                break;
            }

            // Or stuff that hasn't been implemented yet
            _ => {
                return_compiler_error!(format!(
                    "Unexpected token found in the body of a function. Could be unimplemented. Token: {:?}",
                    token_stream.current_token_kind()
                ))
            }
        }
    }

    Ok(ast)
}

// fn check_for_dot_access(
//     token_stream: &mut FileTokens,
//     arg: &Arg,
//     context: &ScopeContext,
//     ast: &mut Vec<AstNode>,
//     string_table: &mut StringTable,
// ) -> Result<(), CompilerError> {
//     // Name of variable, with any accesses added to the path
//     let mut scope = context.scope.clone();
//
//     // We will need to keep pushing nodes if there are accesses after method calls
//     while token_stream.current_token_kind() == &TokenKind::Dot {
//         // Move past the dot
//         token_stream.advance();
//
//         // Currently, there is no just integer access.
//         // Only properties or methods are accessed on structs and collections.
//         // Collections have a .get() method for accessing elements, no [] syntax.
//         if let TokenKind::Symbol(id) = token_stream.current_token_kind().to_owned() {
//             let members = match &arg.value.data_type {
//                 DataType::Parameters(inner_args) => inner_args,
//                 DataType::Function(_, sig) => &sig.returns,
//                 _ => &get_builtin_methods(&arg.value.data_type, string_table),
//             };
//
//             // Nothing to access error
//             if members.is_empty() {
//                 let var_name_static: &'static str =
//                     Box::leak(string_table.resolve(id).to_string().into_boxed_str());
//                 return_rule_error!(
//                     format!("'{}' has no methods or properties to access 😞", string_table.resolve(id)),
//                     token_stream.current_location().to_error_location(&string_table), {
//                         VariableName => var_name_static,
//                         CompilationStage => "AST Construction",
//                         PrimarySuggestion => "This type doesn't support property or method access",
//                     }
//                 )
//             }
//
//             // No access with that name exists error
//             let access = match members.iter().find(|member| member.id == id) {
//                 Some(access) => access,
//                 None => {
//                     let property_name_static: &'static str =
//                         Box::leak(string_table.resolve(id).to_string().into_boxed_str());
//                     let var_name_static: &'static str =
//                         Box::leak(string_table.resolve(arg.id).to_string().into_boxed_str());
//                     return_rule_error!(
//                         format!("Can't find property or method '{}' inside '{}'", string_table.resolve(id), string_table.resolve(arg.id)),
//                         token_stream.current_location().to_error_location(&string_table), {
//                             VariableName => property_name_static,
//                             CompilationStage => "AST Construction",
//                             PrimarySuggestion => "Check the available methods and properties for this type",
//                         }
//                     )
//                 }
//             };
//
//             // Add the name to the scope
//             scope.push(access.id);
//
//             // Move past the name
//             token_stream.advance();
//
//             // ----------------------------
//             //        METHOD CALLS
//             // ----------------------------
//             if let DataType::Function(_, signature) = &access.value.data_type {
//                 ast.push(parse_function_call(
//                     token_stream,
//                     &id,
//                     context,
//                     signature,
//                     string_table,
//                 )?)
//             }
//         } else {
//             return_rule_error!(
//                 format!("Expected the name of a property or method after the dot (accessing a member of the variable such as a method or property). Found '{:?}' instead.", token_stream.current_token_kind()),
//                 token_stream.current_location().to_error_location(&string_table), {
//                     CompilationStage => "AST Construction",
//                     PrimarySuggestion => "Use a valid property or method name after the dot",
//                 }
//             )
//         }
//     }
//
//     Ok(())
// }
