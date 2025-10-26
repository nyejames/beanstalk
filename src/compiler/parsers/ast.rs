use crate::compiler::compiler_errors::CompilerMessages;
use crate::compiler::compiler_warnings::CompilerWarning;
use crate::compiler::host_functions::registry::HostFunctionRegistry;
use crate::compiler::parsers::ast_nodes::{Arg, AstNode};
use crate::compiler::parsers::parse_file_headers::Header;
use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::settings;
use std::path::PathBuf;
use crate::compiler::parsers::build_ast::new_ast;

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

        // Each header needs to be wrapped in a struct instance.
        // It then exposes its

        for header in sorted_headers {
            let ast_context = ScopeContext::new(
                ContextKind::Function,
                header.path.to_owned(),
                &[],
            );

            let header_ast = new_ast()
        }

        Ast {
            nodes: ast,
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
