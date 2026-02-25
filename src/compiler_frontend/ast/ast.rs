use crate::backends::function_registry::HostRegistry;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::function_body_to_ast::function_body_to_ast;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::statements::structs::create_struct_definition;
use crate::compiler_frontend::ast::templates::create_template_node::Template;
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
        const_template_count: usize,
        host_registry: &HostRegistry,
        string_table: &mut StringTable,
        entry_dir: InternedPath,
    ) -> Result<Ast, CompilerMessages> {
        // Each file will be combined into a single AST.
        let mut ast: Vec<AstNode> =
            Vec::with_capacity(sorted_headers.len() * settings::TOKEN_TO_NODE_RATIO);
        let external_exports: Vec<ModuleExport> = Vec::new();
        let mut warnings: Vec<CompilerWarning> = Vec::new();

        let mut const_template_tokens: Vec<(usize, FileTokens)> =
            Vec::with_capacity(const_template_count);

        // Collect all function signatures and struct definitions to register them in scope
        let declarations: Vec<Declaration> = Vec::new();
        for mut header in sorted_headers {
            match header.kind {
                HeaderKind::Function { signature } => {
                    // Function parameters should be available in the function body scope
                    let context = ScopeContext::new(
                        ContextKind::Function,
                        header.tokens.src_path.to_owned(),
                        &signature.parameters,
                        host_registry.clone(),
                        signature.returns.clone(),
                    );

                    let mut token_stream = header.tokens;

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

                    // Make the name from the header path.
                    // AST symbol IDs are stored as full InternedPath values and are unique
                    // module-wide, not only within a local scope.
                    ast.push(AstNode {
                        kind: NodeKind::Function(
                            token_stream.src_path,
                            signature.to_owned(),
                            body.to_owned(),
                        ),
                        location: header.name_location,
                        scope: context.scope.clone(), // Preserve the full path in the scope field
                    });
                }

                HeaderKind::StartFunction => {
                    let context = ScopeContext::new(
                        ContextKind::Module,
                        header.tokens.src_path.to_owned(),
                        &declarations,
                        host_registry.clone(),
                        vec![],
                    );

                    let mut token_stream = header.tokens;

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
                            InternedPath::from_single_str(TOP_LEVEL_TEMPLATE_NAME, string_table),
                            DataType::Template,
                            token_stream.current_location(),
                            Ownership::MutableOwned,
                        )]),
                        location: token_stream.current_location(),
                        scope: context.scope.clone(),
                    });

                    // Create an implicit "start" function that can be called by other modules
                    let full_name = token_stream
                        .src_path
                        .join_str(IMPLICIT_START_FUNC_NAME, string_table);

                    let main_signature = FunctionSignature {
                        parameters: vec![],
                        returns: vec![DataType::String],
                    };

                    ast.push(AstNode {
                        kind: NodeKind::Function(full_name, main_signature, body),
                        location: header.name_location,
                        scope: context.scope.clone(),
                    });
                }

                HeaderKind::Struct => {
                    let fields = match create_struct_definition(&mut header.tokens, string_table) {
                        Ok(f) => f,
                        Err(e) => {
                            return Err(CompilerMessages {
                                errors: vec![e],
                                warnings,
                            });
                        }
                    };

                    ast.push(AstNode {
                        kind: NodeKind::StructDefinition(header.tokens.src_path.to_owned(), fields), // Use the simple name for identifier
                        location: header.name_location,
                        scope: header.tokens.src_path, // Preserve the full path in the scope field
                    });
                }

                HeaderKind::Constant => {
                    // TODO: Implement constant handling
                    todo!()
                }

                HeaderKind::Choice => {
                    // TODO: Implement choice handling
                    todo!()
                }

                HeaderKind::ConstTemplate { file_order } => {
                    // This will then be provided to the build system separately from the main AST as a completely folded string
                    const_template_tokens.push((file_order, header.tokens));
                }
            }

            // TODO: create a function definition for these exported headers
            if header.exported {}
        }

        // Fold the top-level const templates into a single one and add it to the AST
        if !const_template_tokens.is_empty() {
            // Sort and concat all const templates
            const_template_tokens.sort_by_key(|(k, _)| *k);

            // Just grab the FileTokens, don't need the order any more
            let mut iter = const_template_tokens.into_iter();

            // Safe unwrap because already checked to be not empty above
            let (_, mut final_template) = iter.next().unwrap();

            // Shove all the tokens into the first FileTokens
            for (_, file_tokens) in iter {
                final_template.tokens.extend(file_tokens.tokens);
            }

            // Function parameters should be available in the function body scope
            let context = ScopeContext::new(
                ContextKind::Constant,
                final_template.src_path.to_owned(),
                &declarations,
                host_registry.clone(),
                vec![],
            );

            let template = match Template::new(&mut final_template, &context, None, string_table) {
                Ok(t) => t,
                Err(e) => {
                    return Err(CompilerMessages {
                        errors: vec![e],
                        warnings,
                    });
                }
            };

            let expr = Expression::template(template, Ownership::MutableOwned);

            ast.push(AstNode {
                // Source location for these things is an absolute mess.
                // Hopefully not possible to get errors after this point for these
                location: expr.location.to_owned(),
                kind: NodeKind::TopLevelTemplate(expr),
                scope: context.scope.clone(),
            });
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
    pub declarations: Vec<Declaration>,
    pub returns: Vec<DataType>,
    pub host_registry: HostRegistry,
    pub loop_depth: usize,
}
#[derive(PartialEq, Clone)]
pub enum ContextKind {
    Module, // The top-level scope of each file in the module
    Expression,
    Constant, // An expression that is enforced to be evaluated at compile time and can't contain non-constant references
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
        declarations: &[Declaration],
        host_registry: HostRegistry,
        returns: Vec<DataType>,
    ) -> ScopeContext {
        ScopeContext {
            kind,
            scope,
            declarations: declarations.to_owned(),
            returns,
            host_registry,
            loop_depth: 0,
        }
    }

    pub fn new_child_control_flow(&self, kind: ContextKind) -> ScopeContext {
        let mut new_context = self.to_owned();
        new_context.kind = kind;
        if matches!(new_context.kind, ContextKind::Loop) {
            new_context.loop_depth += 1;
        }

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
        new_context.scope = self.scope.append(id);
        new_context.loop_depth = 0;

        new_context.declarations = signature.parameters;

        new_context
    }

    pub fn new_child_expression(&self, returns: Vec<DataType>) -> ScopeContext {
        let mut new_context = self.to_owned();
        new_context.kind = ContextKind::Expression;
        new_context.returns = returns;
        new_context
    }

    // Can also be a cheeky struct or enum or something
    pub fn new_constant(scope: InternedPath) -> ScopeContext {
        ScopeContext {
            kind: ContextKind::Constant,
            scope,
            declarations: Vec::new(),
            returns: Vec::new(),
            host_registry: HostRegistry::default(),
            loop_depth: 0,
        }
    }

    pub fn add_var(&mut self, arg: Declaration) {
        self.declarations.push(arg);
    }

    pub fn is_inside_loop(&self) -> bool {
        self.loop_depth > 0
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
            loop_depth: $context.loop_depth,
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
            loop_depth: 0,
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
            loop_depth: 0,
        }
    }};
}
