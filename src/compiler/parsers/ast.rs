use crate::compiler::compiler_errors::{CompileError, CompilerMessages};
use crate::compiler::compiler_warnings::CompilerWarning;
use crate::compiler::host_functions::registry::HostFunctionRegistry;
use crate::compiler::parsers::ast_nodes::{Arg, AstNode, NodeKind};
use crate::compiler::parsers::build_ast::function_body_to_ast;
use crate::compiler::parsers::parse_file_headers::{Header, HeaderKind};
use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::compiler::parsers::tokenizer::tokens::FileTokens;
use crate::settings;
use std::path::PathBuf;

pub struct Ast {
    pub nodes: Vec<AstNode>,

    // The path to the original entry point file
    pub entry_path: PathBuf,

    // Exported out of the final compiled wasm module
    // Functions must use explicit 'export' syntax Token::Export to be exported
    pub external_exports: Vec<Arg>,
    pub warnings: Vec<CompilerWarning>,
}

impl Ast {
    pub fn new(
        sorted_headers: Vec<Header>,
        host_registry: &HostFunctionRegistry,
    ) -> Result<Ast, CompilerMessages> {
        // Each file will be combined into a single AST.
        let mut ast: Vec<AstNode> =
            Vec::with_capacity(sorted_headers.len() * settings::TOKEN_TO_NODE_RATIO);
        let mut external_exports: Vec<Arg> = Vec::new();
        let mut warnings: Vec<CompilerWarning> = Vec::new();
        let mut entry_path = None;

        for header in sorted_headers {
            match header.kind {
                HeaderKind::Function(signature, tokens) => {
                    let context = ScopeContext::new_with_registry(
                        ContextKind::Function,
                        header.path.to_owned(),
                        &[],
                        host_registry.clone(),
                    );

                    let body = match function_body_to_ast(
                        &mut FileTokens::new(header.path.to_owned(), tokens),
                        context.to_owned(),
                        &mut warnings,
                    ) {
                        Ok(b) => b,
                        Err(e) => {
                            return Err(CompilerMessages {
                                errors: vec![e],
                                warnings,
                            });
                        }
                    };

                    ast.push(AstNode {
                        kind: NodeKind::Function(
                            header.name.to_owned(),
                            signature.to_owned(),
                            body.to_owned(),
                        ),
                        location: header.name_location,
                        scope: context.scope_name,
                    });
                }

                HeaderKind::EntryPoint(tokens) => {
                    let context = ScopeContext::new_with_registry(
                        ContextKind::Module,
                        header.path.to_owned(),
                        &[],
                        host_registry.clone(),
                    );

                    let body = match function_body_to_ast(
                        &mut FileTokens::new(header.path.to_owned(), tokens),
                        context.to_owned(),
                        &mut warnings,
                    ) {
                        Ok(b) => b,
                        Err(e) => {
                            return Err(CompilerMessages {
                                errors: vec![e],
                                warnings,
                            });
                        }
                    };

                    // Create an entry point function that will be exported as the start function
                    let entry_signature = FunctionSignature {
                        parameters: vec![],
                        returns: vec![],
                    };

                    entry_path = Some(header.path.to_owned());

                    ast.push(AstNode {
                        kind: NodeKind::Function("_start".to_string(), entry_signature, body),
                        location: header.name_location,
                        scope: context.scope_name,
                    });
                }

                HeaderKind::ImplicitMain(tokens) => {
                    let context = ScopeContext::new_with_registry(
                        ContextKind::Module,
                        header.path.to_owned(),
                        &[],
                        host_registry.clone(),
                    );

                    let body = match function_body_to_ast(
                        &mut FileTokens::new(header.path.to_owned(), tokens),
                        context.to_owned(),
                        &mut warnings,
                    ) {
                        Ok(b) => b,
                        Err(e) => {
                            return Err(CompilerMessages {
                                errors: vec![e],
                                warnings,
                            });
                        }
                    };

                    // Create an implicit main function that can be called by other modules
                    let function_name = format!(
                        "_implicit_main_{}",
                        header
                            .path
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                    );
                    let main_signature = FunctionSignature {
                        parameters: vec![],
                        returns: vec![],
                    };

                    ast.push(AstNode {
                        kind: NodeKind::Function(function_name, main_signature, body),
                        location: header.name_location,
                        scope: context.scope_name,
                    });
                }

                HeaderKind::Struct(fields) => {
                    ast.push(AstNode {
                        kind: NodeKind::StructDefinition(header.name, fields),
                        location: header.name_location,
                        scope: header.path,
                    });
                }

                HeaderKind::Constant(_arg) => {
                    // TODO: Implement constant handling
                }

                HeaderKind::Choice => {
                    // TODO: Implement choice handling
                }
            }

            // TODO: create an function definition for these exported headers
            if header.exported {}
        }

        match entry_path {
            None => Err(CompilerMessages {
                warnings,
                errors: vec![CompileError::compiler_error(
                    "No entry point found. The compiler should always create an entry point.",
                )],
            }),
            Some(path) => Ok(Ast {
                nodes: ast,
                entry_path: path,
                external_exports,
                warnings,
            }),
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
    Module, // The top-level scope of each file in the module
    Expression,
    Function,
    Condition, // For loops and if statements
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

    pub fn new_child_function(&self, name: &str, signature: FunctionSignature) -> ScopeContext {
        let mut new_context = self.to_owned();
        new_context.kind = ContextKind::Function;
        new_context.returns = signature.returns.to_owned();
        new_context.scope_name.push(name);

        new_context.declarations = signature.parameters;

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
