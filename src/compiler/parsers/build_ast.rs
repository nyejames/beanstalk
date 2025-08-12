use crate::compiler::compiler_errors::ErrorType;
use crate::compiler::parsers::tokens::TextLocation;
use crate::tokenizer::END_SCOPE_CHAR;
#[allow(unused_imports)]
use colour::{blue_ln, green_ln, red_ln};

use super::ast_nodes::NodeKind;
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::compiler_warnings::CompilerWarning;
use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::ast_nodes::{Arg, AstNode};
use crate::compiler::parsers::builtin_methods::get_builtin_methods;
use crate::compiler::parsers::expressions::expression::Expression;
use crate::compiler::parsers::expressions::parse_expression::{
    create_args_from_types, create_multiple_expressions,
};
use crate::compiler::parsers::statements::branching::create_branch;
use crate::compiler::parsers::statements::functions::parse_function_call;
use crate::compiler::parsers::statements::loops::create_loop;
use crate::compiler::parsers::statements::variables::new_arg;
use crate::compiler::parsers::tokenizer::PRINT_KEYWORD;
use crate::compiler::parsers::tokens::{TokenContext, TokenKind, VarVisibility};
use crate::compiler::traits::ContainsReferences;
use crate::{ast_log, return_compiler_error, return_rule_error, return_syntax_error, settings};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct AstBlock {
    pub scope: PathBuf,
    pub ast: Vec<AstNode>, // Body
    pub is_entry_point: bool,
}
pub struct ParserOutput {
    pub ast: AstBlock,
    pub exports: Vec<Arg>,
    pub warnings: Vec<CompilerWarning>,
}
impl ParserOutput {
    fn new(ast: AstBlock, exports: Vec<Arg>, warnings: Vec<CompilerWarning>) -> ParserOutput {
        ParserOutput {
            ast,
            exports,
            warnings,
        }
    }
}

#[derive(Clone)]
pub struct ScopeContext {
    pub kind: ContextKind,
    pub scope_name: PathBuf,
    pub declarations: Vec<Arg>,
    pub returns: Vec<DataType>,
}
#[derive(PartialEq, Clone)]
pub enum ContextKind {
    Module, // Global scope
    Expression,
    Function,
    Condition, // For loops and if statements
    Loop,
    Branch,
    Template,
    Config,
}

impl ScopeContext {
    pub fn new(kind: ContextKind, scope: PathBuf, declarations: &[Arg]) -> ScopeContext {
        ScopeContext {
            kind,
            scope_name: scope,
            declarations: declarations.to_owned(),
            returns: Vec::new(),
        }
    }

    pub fn new_child_control_flow(&self, kind: ContextKind) -> ScopeContext {
        let mut new_context = self.to_owned();
        new_context.kind = kind;

        // For now, add the lifetime ID to the scope.
        new_context
    }

    pub fn new_child_function(&self, name: &str, returns: &[DataType]) -> ScopeContext {
        let mut new_context = self.to_owned();
        new_context.kind = ContextKind::Function;
        new_context.returns = returns.to_owned();
        new_context.scope_name.push(name);
        new_context
    }

    pub fn new_child_expression(&self, returns: Vec<DataType>) -> ScopeContext {
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
        }
    };
}

/// New Config AstContext
///
/// name (for scope), args (declarations it can reference)
#[macro_export]
macro_rules! new_config_context {
    ($name:expr, $args:expr) => {
        ScopeContext {
            kind: ContextKind::Template,
            scope_name: PathBuf::from($name),
            declarations: $args,
            returns: vec![],
        }
    };
}

/// New Condition AstContext
///
/// name (for scope), args (declarations it can reference)
#[macro_export]
macro_rules! new_condition_context {
    ($name:expr, $args:expr) => {
        ScopeContext {
            kind: ContextKind::Condition,
            scope_name: PathBuf::from($name),
            declarations: $args,
            returns: vec![], //Empty because conditions are always booleans
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

    // TODO: All top level declarations are exports
    let mut exports = Vec::new();

    // TODO: Start adding warnings where possible
    let mut warnings = Vec::new();

    // let start_pos = token_stream.current_location(); // Store start position for potential nodes

    while token_stream.index < token_stream.length {
        // This should be starting after the imports
        let current_token = token_stream.current_token_kind().to_owned();

        ast_log!("Parsing Token: {:?}", current_token);

        match current_token {
            TokenKind::Comment => {
                // Comments are ignored during AST creation.
                token_stream.advance();
            }

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
                if let Some(arg) = context.find_reference(name) {
                    // Then the associated mutation afterward.
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
                                DataType::Function(_, returned_args) => {
                                    &create_args_from_types(&returned_args)
                                }
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

                    ast.push(AstNode {
                        kind: NodeKind::Expression(arg.value.to_owned()),
                        scope: context.scope_name.to_owned(),
                        location: token_stream.current_location(),
                    });

                // NEW VARIABLE DECLARATION
                } else {
                    let mut visibility = VarVisibility::Private;
                    let arg = new_arg(token_stream, name, &context)?;

                    if visibility == VarVisibility::Public {
                        exports.push(arg.to_owned());
                    }

                    context.add_var(arg.to_owned());

                    ast.push(AstNode {
                        kind: NodeKind::Declaration(
                            name.to_owned(),
                            arg.value.to_owned(),
                            visibility.to_owned(),
                        ),
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

                ast.push(create_branch(
                    token_stream,
                    &mut context.new_child_control_flow(ContextKind::Branch),
                )?);
            }
            TokenKind::Else => {
                // This will break out if statements, but must be inside the if_statement context
                // Preserves the else token for the branching parser to know there is an else case rather than just an end to the scope
                // TODO: how do we handle this for match blocks?
            }

            // IGNORED TOKENS
            TokenKind::Newline | TokenKind::Empty => {
                // Skip standalone newlines / empty tokens
                token_stream.advance();
            }

            TokenKind::Print => {
                // Move past the print keyword
                token_stream.advance();

                ast.push(parse_function_call(
                    token_stream,
                    PRINT_KEYWORD,
                    &context.new_child_function(PRINT_KEYWORD, &[]),
                    // Print does not return anything
                    &[Arg {
                        name: String::new(),
                        value: Expression::string(String::new(), token_stream.current_location()),
                    }],
                    &[],
                )?);
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
                // Check that this is a valid scope for an explicit 'end' to be used
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
                        break;
                    }
                }
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
        exports,
        warnings,
    ))
}
