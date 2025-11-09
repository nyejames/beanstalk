use crate::compiler::compiler_errors::{CompileError, CompilerMessages};
use crate::compiler::compiler_warnings::CompilerWarning;
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::host_functions::registry::HostFunctionRegistry;
use crate::compiler::interned_path::InternedPath;
use crate::compiler::parsers::ast_nodes::{Arg, AstNode, NodeKind};
use crate::compiler::parsers::build_ast::function_body_to_ast;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::parse_file_headers::{Header, HeaderKind};
use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::compiler::parsers::tokenizer::tokens::FileTokens;
use crate::compiler::string_interning::{StringId, StringTable};
use crate::settings;
pub struct Ast {
    pub nodes: Vec<AstNode>,

    // The path to the original entry point file
    pub entry_path: InternedPath,

    // Exported out of the final compiled wasm module
    // Functions must use explicit 'export' syntax Token::Export to be exported
    pub external_exports: Vec<Arg>,
    pub warnings: Vec<CompilerWarning>,
}

impl Ast {
    pub fn new(
        sorted_headers: Vec<Header>,
        host_registry: &HostFunctionRegistry,
        string_table: &mut StringTable,
    ) -> Result<Ast, CompilerMessages> {
        // Each file will be combined into a single AST.
        let mut ast: Vec<AstNode> =
            Vec::with_capacity(sorted_headers.len() * settings::TOKEN_TO_NODE_RATIO);
        let mut external_exports: Vec<Arg> = Vec::new();
        let mut warnings: Vec<CompilerWarning> = Vec::new();
        let mut entry_path = None;

        // First pass: collect all function signatures to register them in scope
        let mut function_declarations: Vec<Arg> = Vec::new();
        for header in &sorted_headers {
            if let HeaderKind::Function(signature, _) = &header.kind {
                // Create an Arg representing this function for scope registration
                let interned_name = header.path.to_interned_string(string_table);
                let func_arg = Arg {
                    id: interned_name,
                    value: Expression {
                        kind: ExpressionKind::None,
                        data_type: DataType::Function(signature.clone()),
                        ownership: Ownership::ImmutableOwned,
                        location: header.name_location.clone(),
                    },
                };
                function_declarations.push(func_arg);
            }
        }

        for header in sorted_headers {
            match header.kind {
                HeaderKind::Function(signature, tokens) => {
                    // Function parameters should be available in the function body scope
                    let mut context = ScopeContext::new(
                        ContextKind::Function,
                        header.path.to_owned(),
                        &signature.parameters,
                        host_registry.clone(),
                        signature.returns.clone(),
                    );

                    let body = match function_body_to_ast(
                        &mut FileTokens::new(header.path.to_owned(), tokens),
                        context.to_owned(),
                        &mut warnings,
                        string_table,
                    ) {
                        Ok(b) => b,
                        Err(e) => {
                            return Err(CompilerMessages {
                                errors: vec![e],
                                warnings,
                            });
                        }
                    };

                    let interned_name = header.path.to_interned_string(string_table);
                    ast.push(AstNode {
                        kind: NodeKind::Function(
                            interned_name,
                            signature.to_owned(),
                            body.to_owned(),
                        ),
                        location: header.name_location,
                        scope: context.scope.clone(),
                    });
                }

                HeaderKind::EntryPoint(tokens) => {
                    let context = ScopeContext::new(
                        ContextKind::Module,
                        header.path.to_owned(),
                        &function_declarations,
                        host_registry.clone(),
                        Vec::new(),
                    );

                    let body = match function_body_to_ast(
                        &mut FileTokens::new(header.path.to_owned(), tokens),
                        context.to_owned(),
                        &mut warnings,
                        string_table,
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

                    let start_name = string_table.intern("_start");
                    ast.push(AstNode {
                        kind: NodeKind::Function(start_name, entry_signature, body),
                        location: header.name_location,
                        scope: context.scope.clone(),
                    });
                }

                HeaderKind::ImplicitMain(tokens) => {
                    let context = ScopeContext::new(
                        ContextKind::Module,
                        header.path.to_owned(),
                        &function_declarations,
                        host_registry.clone(),
                        Vec::new(),
                    );

                    let body = match function_body_to_ast(
                        &mut FileTokens::new(header.path.to_owned(), tokens),
                        context.to_owned(),
                        &mut warnings,
                        string_table,
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
                    let interned_name = header.path.to_interned_string(string_table);
                    let main_signature = FunctionSignature {
                        parameters: vec![],
                        returns: vec![],
                    };

                    ast.push(AstNode {
                        kind: NodeKind::Function(interned_name, main_signature, body),
                        location: header.name_location,
                        scope: context.scope.clone(),
                    });
                }

                HeaderKind::Struct(fields) => {
                    let interned_name = header.path.to_interned_string(string_table);
                    ast.push(AstNode {
                        kind: NodeKind::StructDefinition(interned_name, fields),
                        location: header.name_location,
                        scope: header.path.to_owned(),
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
    pub scope: InternedPath,
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
    pub fn new(
        kind: ContextKind,
        scope: InternedPath,
        declarations: &[Arg],
        host_registry: HostFunctionRegistry,
        returns: Vec<Arg>,
    ) -> ScopeContext {
        ScopeContext {
            kind,
            scope,
            declarations: declarations.to_owned(),
            returns,
            host_registry,
        }
    }

    pub fn new_child_control_flow(&self, kind: ContextKind) -> ScopeContext {
        let mut new_context = self.to_owned();
        new_context.kind = kind;

        // For now, add the lifetime ID to the scope.
        new_context
    }

    pub fn new_child_function(&self, id: StringId, signature: FunctionSignature) -> ScopeContext {
        let mut new_context = self.to_owned();
        new_context.kind = ContextKind::Function;
        new_context.returns = signature.returns.to_owned();

        // Create a new scope path by joining the current scope with the function name
        new_context.scope = self.scope.join_header(id);

        new_context.declarations = signature.parameters;

        new_context
    }

    pub fn new_child_expression(&self, returns: Vec<Arg>) -> ScopeContext {
        let mut new_context = self.to_owned();
        new_context.kind = ContextKind::Expression;
        new_context.returns = returns;
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
            scope: $context.scope.clone(),
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
    ($name:expr, $args:expr, $registry:expr, $string_table:expr) => {{
        let mut scope = InternedPath::new();
        scope.push_str($name, $string_table);
        ScopeContext {
            kind: ContextKind::Template,
            scope,
            declarations: $args,
            returns: vec![],
            host_registry: $registry,
        }
    }};
}

/// New Condition AstContext
///
/// name (for scope), args (declarations it can reference)
#[macro_export]
macro_rules! new_condition_context {
    ($name:expr, $args:expr, $registry:expr, $string_table:expr) => {{
        let mut scope = InternedPath::new();
        scope.push_str($name, $string_table);
        ScopeContext {
            kind: ContextKind::Condition,
            scope,
            declarations: $args,
            returns: vec![], //Empty because conditions are always booleans
            host_registry: $registry,
        }
    }};
}
