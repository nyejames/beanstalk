// This function takes in a TokenContext from the tokenizer for every file in the module.
// It sorts each type of declaration in the top level scope into:
// - Functions
// - Structs
// - Choices (not yet implemented)
// - Constants
// - Globals (static mutable variables)
// - Implicit Main Function (any other logic in the top level scope implicitly becomes an init function)

// Everything at the top level of a file is visible to the whole module.
// Imports are already parsed in the tokenizer.

use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::ast_nodes::{Arg, AstNode, NodeKind};
use crate::compiler::parsers::build_ast::{AstBlock, ContextKind, ParserOutput, ScopeContext};
use crate::compiler::parsers::builtin_methods::get_builtin_methods;
use crate::compiler::parsers::expressions::mutation::handle_mutation;
use crate::compiler::parsers::expressions::parse_expression::create_multiple_expressions;
use crate::compiler::parsers::statements::branching::create_branch;
use crate::compiler::parsers::statements::functions::parse_function_call;
use crate::compiler::parsers::statements::loops::create_loop;
use crate::compiler::parsers::statements::structs::create_struct_definition;
use crate::compiler::parsers::statements::variables::new_arg;
use crate::compiler::parsers::tokens::{TokenContext, TokenKind, VarVisibility};
use crate::compiler::traits::ContainsReferences;
use crate::tokenizer::END_SCOPE_CHAR;
use crate::{ast_log, return_compiler_error, return_rule_error, return_syntax_error, settings};

pub struct Sections {
    body: TokenContext,
    context: ScopeContext,
}

// It provides the shape and signature of structs and functions for the AST parser to use for type checking.
pub fn parse_top_level_statements(
    token_stream: &mut TokenContext,
    mut context: ScopeContext,
    is_entry_point: bool,
) -> Result<ParserOutput, CompileError> {
    let mut ast: Vec<AstNode> =
        Vec::with_capacity(token_stream.length / settings::TOKEN_TO_NODE_RATIO);

    let mut public = Vec::new();
    let mut external_exports = Vec::new();

    // TODO: Start adding warnings where possible
    let warnings = Vec::new();

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
                            if let DataType::Function(required_arguments, returned_types) =
                                &arg.value.data_type
                            {
                                ast.push(parse_function_call(
                                    token_stream,
                                    name,
                                    &context,
                                    required_arguments,
                                    returned_types,
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

                    ast.push(parse_function_call(
                        token_stream,
                        name,
                        &context,
                        &host_func_call.parameters,
                        &converted_returns,
                    )?)
                } else {
                    // -----------------------------
                    //    NEW STRUCT DECLARATIONS
                    // -----------------------------
                    if let Some(TokenKind::Colon) = token_stream.peek_next_token() {
                        // Advance to the colon
                        token_stream.advance();

                        ast.push(AstNode {
                            kind: NodeKind::StructDefinition(
                                name.to_owned(),
                                // this skips the closing token
                                create_struct_definition(name, token_stream, &context)?,
                            ),
                            location: token_stream.current_location(),
                            scope: context.scope_name.to_owned(),
                        });
                        continue;
                    }

                    // -----------------------------
                    //   NEW VARIABLE DECLARATIONS
                    // -----------------------------
                    let arg = new_arg(token_stream, name, &context)?;

                    let visibility = match token_stream.previous_token() {
                        TokenKind::Export => {
                            external_exports.push(arg.to_owned());
                            VarVisibility::Exported
                        }
                        _ => VarVisibility::Private,
                    };

                    // If this at the top of the module, this is public
                    if context.kind == ContextKind::TopLevel {
                        public.push(arg.to_owned());
                    }

                    ast.push(AstNode {
                        kind: NodeKind::Declaration(
                            name.to_owned(),
                            arg.value.to_owned(),
                            visibility,
                        ),
                        location: token_stream.current_location(),
                        scope: context.scope_name.to_owned(),
                    });

                    context.add_var(arg);
                }
            }

            // Control Flow
            TokenKind::For => {
                token_stream.advance();

                ast.push(create_loop(
                    token_stream,
                    context.new_child_control_flow(ContextKind::Loop),
                )?);
            }

            TokenKind::If => {
                token_stream.advance();

                // This is extending as it might get folded into a vec of nodes
                ast.extend(create_branch(
                    token_stream,
                    &mut context.new_child_control_flow(ContextKind::Branch),
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
                    ContextKind::TopLevel => {
                        return_syntax_error!(
                            token_stream.current_location(),
                            "Unexpected scope close with '{END_SCOPE_CHAR}'. You have probably used too many '{END_SCOPE_CHAR}'\
                            as this scope close is in the global scope."
                        )
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
                // TODO: elaborate all the error cases where the next token is not a symbol
                // And tell the user you can only export newly declared functions or variables
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

    Ok(ParserOutput::new(
        AstBlock {
            ast,
            scope: context.scope_name,
            is_entry_point,
        },
        public,
        external_exports,
        warnings,
    ))
}

fn check_for_dot_access(
    token_stream: &mut TokenContext,
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
                DataType::Args(inner_args) => inner_args,
                DataType::Function(_, returned_args) => returned_args,
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
            if let DataType::Function(required_arguments, returned_types) = &access.value.data_type
            {
                ast.push(parse_function_call(
                    token_stream,
                    &name,
                    &context,
                    required_arguments,
                    returned_types,
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
