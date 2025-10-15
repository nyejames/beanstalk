use crate::tokenizer::END_SCOPE_CHAR;

use super::ast_nodes::NodeKind;
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::compiler_warnings::CompilerWarning;
use crate::compiler::datatypes::DataType;
use crate::compiler::host_functions::registry::HostFunctionRegistry;
use crate::compiler::parsers::ast_nodes::{Arg, AstNode};
use crate::compiler::parsers::builtin_methods::get_builtin_methods;
use crate::compiler::parsers::expressions::mutation::handle_mutation;
use crate::compiler::parsers::expressions::parse_expression::create_multiple_expressions;
use crate::compiler::parsers::statements::branching::create_branch;
use crate::compiler::parsers::statements::functions::parse_function_call;
use crate::compiler::parsers::statements::loops::create_loop;
use crate::compiler::parsers::statements::variables::new_arg;
use crate::compiler::parsers::tokens::{TokenContext, TokenKind, VarVisibility};
use crate::compiler::traits::ContainsReferences;
use crate::{ast_log, return_compiler_error, return_rule_error, return_syntax_error, settings};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct AstBlock {
    pub scope: PathBuf,
    pub ast: Vec<AstNode>, // Body
    pub is_entry_point: bool,
}
pub struct ParserOutput {
    pub ast: AstBlock,

    // Top level declarations in the module,
    // that can be seen by other Beanstalk files
    pub public: Vec<Arg>,

    // Exported out of the final compiled wasm module
    // Must use explicit 'export' syntax Token::Export
    pub external_exports: Vec<Arg>,
    pub warnings: Vec<CompilerWarning>,
}
impl ParserOutput {
    fn new(
        ast: AstBlock,
        public: Vec<Arg>,
        exports: Vec<Arg>,
        warnings: Vec<CompilerWarning>,
    ) -> ParserOutput {
        ParserOutput {
            ast,
            public,
            external_exports: exports,
            warnings,
        }
    }
}

#[derive(Clone)]
pub struct ScopeContext {
    pub kind: ContextKind,
    pub scope_name: PathBuf,
    pub declarations: Vec<Arg>,
    pub returns: Vec<Arg>,
    pub host_registry: HostFunctionRegistry,
}
#[derive(PartialEq, Clone)]
pub enum ContextKind {
    Module, // Global scope
    Expression,
    Function,
    Parameters, // Inside a function signature
    Condition,  // For loops and if statements
    Loop,
    Branch,
    Template,
}

impl ScopeContext {
    pub fn new(kind: ContextKind, scope: PathBuf, declarations: &[Arg]) -> ScopeContext {
        // Create a default registry - this will be replaced with the actual registry
        let host_registry = HostFunctionRegistry::new();

        ScopeContext {
            kind,
            scope_name: scope,
            declarations: declarations.to_owned(),
            returns: Vec::new(),
            host_registry,
        }
    }

    pub fn new_with_registry(
        kind: ContextKind,
        scope: PathBuf,
        declarations: &[Arg],
        host_registry: HostFunctionRegistry,
    ) -> ScopeContext {
        ScopeContext {
            kind,
            scope_name: scope,
            declarations: declarations.to_owned(),
            returns: Vec::new(),
            host_registry,
        }
    }

    pub fn new_child_control_flow(&self, kind: ContextKind) -> ScopeContext {
        let mut new_context = self.to_owned();
        new_context.kind = kind;

        // For now, add the lifetime ID to the scope.
        new_context
    }

    pub fn new_child_function(
        &self,
        name: &str,
        returns: &[Arg],
        arguments: Vec<Arg>,
    ) -> ScopeContext {
        let mut new_context = self.to_owned();
        new_context.kind = ContextKind::Function;
        new_context.returns = returns.to_owned();
        new_context.scope_name.push(name);
        new_context.declarations = arguments;

        new_context
    }

    pub fn new_parameters(&self) -> ScopeContext {
        let mut new_context = self.to_owned();
        new_context.kind = ContextKind::Parameters;
        new_context.scope_name.push("parameters");

        new_context
    }

    pub fn new_child_expression(&self, returns: Vec<Arg>) -> ScopeContext {
        let mut new_context = self.to_owned();
        new_context.kind = ContextKind::Expression;
        new_context.returns = returns;
        new_context.scope_name.push("expression");
        new_context
    }

    pub fn add_var(&mut self, arg: Arg) {
        self.declarations.push(arg);
    }
}

/// A new AstContext for scenes
///
/// Usage:
/// name (for the scope), args (declarations it can access)
#[macro_export]
macro_rules! new_template_context {
    ($context:expr) => {
        &ScopeContext {
            kind: ContextKind::Template,
            scope_name: $context.scope_name.to_owned(),
            declarations: $context.declarations.to_owned(),
            returns: vec![],
            host_registry: $context.host_registry.clone(),
        }
    };
}

/// New Config AstContext
///
/// name (for scope), args (declarations it can reference)
#[macro_export]
macro_rules! new_config_context {
    ($name:expr, $args:expr, $registry:expr) => {
        ScopeContext {
            kind: ContextKind::Template,
            scope_name: PathBuf::from($name),
            declarations: $args,
            returns: vec![],
            host_registry: $registry,
        }
    };
}

/// New Condition AstContext
///
/// name (for scope), args (declarations it can reference)
#[macro_export]
macro_rules! new_condition_context {
    ($name:expr, $args:expr, $registry:expr) => {
        ScopeContext {
            kind: ContextKind::Condition,
            scope_name: PathBuf::from($name),
            declarations: $args,
            returns: vec![], //Empty because conditions are always booleans
            host_registry: $registry,
        }
    };
}

// This is a new scope
pub fn new_ast(
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

    // let start_pos = token_stream.current_location(); // Store start position for potential nodes

    while token_stream.index < token_stream.length {
        // This should be starting after the imports
        let current_token = token_stream.current_token_kind().to_owned();

        ast_log!("Parsing Token: {:?}", current_token);

        match current_token {
            // Template literals
            // TokenKind::TemplateHead | TokenKind::ParentTemplate => {
            //     // Add the default core HTML styles as the initially unlocked styles
            //     // let mut unlocked_styles = HashMap::from(get_html_styles());
            //
            //     if !matches!(context.kind, ContextKind::Module) {
            //         return_rule_error!(
            //             token_stream.current_location(),
            //             "Template literals can only be used at the top level of a module. \n
            //             This is because they are handled differently by the compiler depending on the type of project",
            //         )
            //     }
            //
            //     let template = new_template(
            //         token_stream,
            //         &context,
            //         &mut HashMap::new(),
            //         &mut Style::default(),
            //     )?;
            //
            //     match template.kind {
            //         TemplateType::StringTemplate => {
            //             ast.push(AstNode {
            //                 kind: NodeKind::Expression(Expression::template(
            //                     template,
            //                     context.id,
            //                 )),
            //                 scope: context.scope_name.to_owned(),
            //                 location: token_stream.current_location(),
            //             });
            //         }
            //         TemplateType::Slot => {
            //             return_rule_error!(
            //                 token_stream.current_location(),
            //                 "Slots can only be used inside child templates. Slot templates must have a parent template.",
            //             )
            //         }
            //         _ => {}
            //     }
            // }
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

                    // Name of variable, with any accesses added to the path
                    let mut scope = context.scope_name.to_owned();

                    // We will need to keep pushing nodes if there are accesses after method calls
                    while token_stream.current_token_kind() == &TokenKind::Dot {
                        // Move past the dot
                        token_stream.advance();

                        // Currently, there is no just integer access.
                        // Only properties or methods are accessed on structs and collections.
                        // Collections have a .get() method for accessing elements, no [] syntax.

                        if let TokenKind::Symbol(name, ..) =
                            token_stream.current_token_kind().to_owned()
                        {
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
                            if let DataType::Function(required_arguments, returned_types) =
                                &access.value.data_type
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

                        // Mutable token after variable reference - this is invalid (shadowing attempt)
                        | TokenKind::Mutable => {
                            // Look ahead to see if this is ~= (an invalid reassignment attempt)
                            if let Some(TokenKind::Assign) = token_stream.peek_next_token() {
                                return_rule_error!(
                                    token_stream.current_location(),
                                    "Cannot use '~=' for reassignment of variable '{}'. Use '~=' only for initial mutable variable declarations. To mutate this variable, use '=' instead",
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
                    let converted_returns = host_func_call.return_types
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

                // -----------------------------
                //   NEW VARIABLE DECLARATIONS
                // -----------------------------
                } else {
                    let arg = new_arg(token_stream, name, &context)?;

                    let visibility = match token_stream.previous_token() {
                        TokenKind::Export => {
                            external_exports.push(arg.to_owned());
                            VarVisibility::Exported
                        }
                        _ => VarVisibility::Private,
                    };

                    // If this at the top of the module, this is public
                    if context.kind == ContextKind::Module {
                        public.push(arg.to_owned());
                    }

                    context.add_var(arg.to_owned());

                    ast.push(AstNode {
                        kind: NodeKind::Declaration(name.to_owned(), arg.value, visibility),
                        location: token_stream.current_location(),
                        scope: context.scope_name.to_owned(),
                    });
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
                // If we are inside an if / match statement, bre
                if context.kind == ContextKind::Branch {
                    break;
                } else {
                    return_rule_error!(
                        token_stream.current_location(),
                        "Unexpected token '{:?}'. 'else' can only be used inside an if statement or match statement",
                        token_stream.current_token_kind()
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
                    ContextKind::Module => {
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
