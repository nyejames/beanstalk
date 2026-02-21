use crate::backends::function_registry::HostRegistry;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind, Var};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::function_body_to_ast::function_body_to_ast;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::FileTokens;
use crate::projects::settings::{self, IMPLICIT_START_FUNC_NAME, TOP_LEVEL_TEMPLATE_NAME};

pub struct ModuleExport {
    pub id: StringId,
    pub signature: FunctionSignature,
}
pub struct Ast {
    pub nodes: Vec<AstNode>,

    // The path to the original entry point file
    pub entry_path: InternedPath,

    // Exported out of the final compiled wasm module
    // Functions must use explicit 'export' syntax Token::Export to be exported
    // The only exception is the Main function, which is the start function of the entry point file
    pub external_exports: Vec<ModuleExport>,
    pub warnings: Vec<CompilerWarning>,
}

impl Ast {
    pub fn new(
        sorted_headers: Vec<Header>,
        host_registry: &HostRegistry,
        string_table: &mut StringTable,
        entry_dir: InternedPath,
    ) -> Result<Ast, CompilerMessages> {
        // Each file will be combined into a single AST.
        let mut ast: Vec<AstNode> =
            Vec::with_capacity(sorted_headers.len() * settings::TOKEN_TO_NODE_RATIO);
        let external_exports: Vec<ModuleExport> = Vec::new();
        let mut warnings: Vec<CompilerWarning> = Vec::new();

        // Collect all function signatures and struct definitions to register them in scope
        let declarations: Vec<Var> = Vec::new();
        for header in sorted_headers {
            match header.kind {
                HeaderKind::Function {
                    signature,
                    body: tokens,
                } => {
                    // Function parameters should be available in the function body scope
                    let context = ScopeContext::new(
                        ContextKind::Function,
                        header.path.to_owned(),
                        &signature.parameters,
                        host_registry.clone(),
                        signature.returns.clone(),
                    );

                    let mut token_stream = FileTokens::new(header.path.to_owned(), tokens);

                    let body = match function_body_to_ast(
                        &mut token_stream,
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

                    // Make name from header path
                    // This ensures unique namespaced function names
                    // ALL symbols become full paths in the AST converted to interned strings
                    let unique_name = header.path.extract_header_name(string_table);
                    ast.push(AstNode {
                        kind: NodeKind::Function(
                            unique_name,
                            signature.to_owned(),
                            body.to_owned(),
                        ),
                        location: header.name_location,
                        scope: context.scope.clone(), // Preserve the full path in the scope field
                    });
                }

                HeaderKind::StartFunction(body) => {
                    let context = ScopeContext::new(
                        ContextKind::Module,
                        header.path.to_owned(),
                        &declarations,
                        host_registry.clone(),
                        vec![],
                    );

                    let mut token_stream = FileTokens::new(header.path.to_owned(), body);

                    let mut body = match function_body_to_ast(
                        &mut token_stream,
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

                    // Add the automatic return statement for the start function
                    body.push(AstNode {
                        kind: NodeKind::Return(vec![Expression::reference(
                            string_table.intern(TOP_LEVEL_TEMPLATE_NAME),
                            DataType::Template,
                            token_stream.current_location(),
                            Ownership::MutableOwned,
                        )]),
                        location: token_stream.current_location(),
                        scope: context.scope.clone(),
                    });

                    // Create an implicit "start" function that can be called by other modules
                    let interned_name = header
                        .path
                        .join_str(IMPLICIT_START_FUNC_NAME, string_table)
                        .extract_header_name(string_table);

                    let main_signature = FunctionSignature {
                        parameters: vec![],
                        returns: vec![DataType::String],
                    };

                    ast.push(AstNode {
                        kind: NodeKind::Function(interned_name, main_signature, body),
                        location: header.name_location,
                        scope: context.scope.clone(),
                    });
                }

                HeaderKind::Struct(fields) => {
                    // Create name for AST node identifier from the path
                    let simple_name = header.path.extract_header_name(string_table);

                    ast.push(AstNode {
                        kind: NodeKind::StructDefinition(simple_name, fields), // Use the simple name for identifier
                        location: header.name_location,
                        scope: header.path.to_owned(), // Preserve the full path in scope field
                    });
                }

                HeaderKind::Constant(_arg) => {
                    // TODO: Implement constant handling
                }

                HeaderKind::Choice(_args) => {
                    // TODO: Implement choice handling
                }
            }

            // TODO: create a function definition for these exported headers
            if header.exported {}
        }

        Ok(Ast {
            nodes: ast,
            entry_path: entry_dir,
            external_exports,
            warnings,
        })
    }
}

#[derive(Clone)]
pub struct ScopeContext {
    pub kind: ContextKind,
    pub scope: InternedPath,
    pub declarations: Vec<Var>,
    pub returns: Vec<DataType>,
    pub host_registry: HostRegistry,
}
#[derive(PartialEq, Clone)]
pub enum ContextKind {
    Module, // The top-level scope of each file in the module
    Expression,
    Constant, // An expression that is enforced to be evaluated at compile time and can't contain non constant reference s
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
        declarations: &[Var],
        host_registry: HostRegistry,
        returns: Vec<DataType>,
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

    pub fn new_child_function(
        &self,
        id: StringId,
        signature: FunctionSignature,
        string_table: &mut StringTable,
    ) -> ScopeContext {
        let mut new_context = self.to_owned();
        new_context.kind = ContextKind::Function;
        new_context.returns = signature.returns.to_owned();

        // Create a new scope path by joining the current scope with the function name
        new_context.scope = self.scope.join_header(id, string_table);

        new_context.declarations = signature.parameters;

        new_context
    }

    pub fn new_child_expression(&self, returns: Vec<DataType>) -> ScopeContext {
        let mut new_context = self.to_owned();
        new_context.kind = ContextKind::Expression;
        new_context.returns = returns;
        new_context
    }

    pub fn new_constant(scope: InternedPath) -> ScopeContext {
        ScopeContext {
            kind: ContextKind::Constant,
            scope,
            declarations: Vec::new(),
            returns: Vec::new(),
            host_registry: HostRegistry::default(),
        }
    }

    pub fn add_var(&mut self, arg: Var) {
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
