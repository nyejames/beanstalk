use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::function_body_to_ast::function_body_to_ast;
use crate::compiler_frontend::ast::import_bindings::{
    ConstantHeaderParseContext, parse_constant_header_declaration, resolve_file_import_bindings,
};
use crate::compiler_frontend::ast::statements::functions::{FunctionReturn, FunctionSignature};
use crate::compiler_frontend::ast::statements::structs::create_struct_definition;
use crate::compiler_frontend::ast::templates::create_template_node::Template;
use crate::compiler_frontend::ast::templates::template_folding::TemplateFoldContext;
use crate::compiler_frontend::ast::templates::top_level_templates::{
    collect_and_strip_comment_templates, synthesize_start_template_items,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::headers::parse_file_headers::{
    FileImport, Header, HeaderKind, TopLevelTemplateItem,
};
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::projects::settings::{self, IMPLICIT_START_FUNC_NAME};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

pub use crate::compiler_frontend::ast::templates::top_level_templates::{
    AstDocFragment, AstDocFragmentKind, AstStartTemplateItem,
};
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::paths::rendered_path_usage::RenderedPathUsage;
use crate::return_compiler_error;

static CONTROL_FLOW_SCOPE_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[allow(dead_code)] // Used only in tests
pub struct ModuleExport {
    pub id: StringId,
    pub signature: FunctionSignature,
}

pub struct Ast {
    pub nodes: Vec<AstNode>,
    pub module_constants: Vec<Declaration>,
    pub doc_fragments: Vec<AstDocFragment>,

    // The path to the original entry point file
    pub entry_path: InternedPath,

    // Exported out of the final compiled wasm module
    // Functions must use explicit 'export' syntax Token::Export to be exported
    // The only exception is the Main function, which is the start function of the entry point file
    #[allow(dead_code)] // Used only in tests
    pub external_exports: Vec<ModuleExport>,
    pub start_template_items: Vec<AstStartTemplateItem>,
    pub rendered_path_usages: Vec<RenderedPathUsage>,
    pub warnings: Vec<CompilerWarning>,
}

impl Ast {
    pub fn new(
        sorted_headers: Vec<Header>,
        top_level_template_items: Vec<TopLevelTemplateItem>,
        host_registry: &HostRegistry,
        style_directives: &StyleDirectiveRegistry,
        string_table: &mut StringTable,
        entry_dir: InternedPath,
        build_profile: FrontendBuildProfile,
        project_path_resolver: Option<ProjectPathResolver>,
        path_format_config: PathStringFormatConfig,
    ) -> Result<Ast, CompilerMessages> {
        // Each file will be combined into a single AST.
        let mut ast: Vec<AstNode> =
            Vec::with_capacity(sorted_headers.len() * settings::TOKEN_TO_NODE_RATIO);
        let external_exports: Vec<ModuleExport> = Vec::new();
        let mut warnings: Vec<CompilerWarning> = Vec::new();
        let mut const_templates_by_path: FxHashMap<InternedPath, StringId> = FxHashMap::default();
        let mut module_constants: Vec<Declaration> = Vec::new();
        let mut importable_symbol_exported: FxHashMap<InternedPath, bool> = FxHashMap::default();
        let mut file_imports_by_source: FxHashMap<InternedPath, Vec<FileImport>> =
            FxHashMap::default();
        let mut declared_paths_by_file: FxHashMap<InternedPath, FxHashSet<InternedPath>> =
            FxHashMap::default();
        let mut declared_names_by_file: FxHashMap<InternedPath, FxHashSet<StringId>> =
            FxHashMap::default();
        let mut module_file_paths: FxHashSet<InternedPath> = FxHashSet::default();
        let mut resolved_struct_fields_by_path: FxHashMap<InternedPath, Vec<Declaration>> =
            FxHashMap::default();
        let rendered_path_usages = Rc::new(RefCell::new(Vec::new()));
        let Some(project_path_resolver_for_folding) = project_path_resolver.as_ref() else {
            return Err(CompilerMessages {
                errors: vec![CompilerError::compiler_error(
                    "AST construction requires a project path resolver for template folding and path coercion.",
                )],
                warnings,
                string_table: Default::default(),
            });
        };

        // Collect every module declaration once.
        // WHY: Resolution stores fully qualified symbol paths.
        // Each file context later applies its own visibility filter instead of rebuilding declaration tables.
        let mut declarations: Vec<Declaration> = Vec::new();
        for header in &sorted_headers {
            module_file_paths.insert(header.source_file.to_owned());
            file_imports_by_source
                .entry(header.source_file.to_owned())
                .or_insert_with(|| header.file_imports.to_owned());

            match &header.kind {
                HeaderKind::Function { signature } => {
                    declarations.push(Declaration {
                        id: header.tokens.src_path.to_owned(),
                        value: Expression::new(
                            ExpressionKind::None,
                            header.name_location.to_owned(),
                            DataType::Function(Box::new(None), signature.to_owned()),
                            Ownership::ImmutableReference,
                        ),
                    });
                    importable_symbol_exported
                        .insert(header.tokens.src_path.to_owned(), header.exported);
                    declared_paths_by_file
                        .entry(header.source_file.to_owned())
                        .or_default()
                        .insert(header.tokens.src_path.to_owned());
                    if let Some(name) = header.tokens.src_path.name() {
                        declared_names_by_file
                            .entry(header.source_file.to_owned())
                            .or_default()
                            .insert(name);
                    }
                }
                HeaderKind::Struct { .. } => {
                    importable_symbol_exported
                        .insert(header.tokens.src_path.to_owned(), header.exported);
                    declared_paths_by_file
                        .entry(header.source_file.to_owned())
                        .or_default()
                        .insert(header.tokens.src_path.to_owned());
                    if let Some(name) = header.tokens.src_path.name() {
                        declared_names_by_file
                            .entry(header.source_file.to_owned())
                            .or_default()
                            .insert(name);
                    }
                }
                HeaderKind::StartFunction => {
                    let start_name = header
                        .source_file
                        .join_str(IMPLICIT_START_FUNC_NAME, string_table);
                    declarations.push(Declaration {
                        id: start_name.to_owned(),
                        value: Expression::new(
                            ExpressionKind::None,
                            header.name_location.to_owned(),
                            DataType::Function(
                                Box::new(None),
                                FunctionSignature {
                                    parameters: vec![],
                                    returns: vec![FunctionReturn::Value(DataType::StringSlice)],
                                },
                            ),
                            Ownership::ImmutableReference,
                        ),
                    });
                    declared_paths_by_file
                        .entry(header.source_file.to_owned())
                        .or_default()
                        .insert(start_name.to_owned());
                    if let Some(name) = start_name.name() {
                        declared_names_by_file
                            .entry(header.source_file.to_owned())
                            .or_default()
                            .insert(name);
                    }
                }
                HeaderKind::Constant { .. } => {
                    importable_symbol_exported
                        .insert(header.tokens.src_path.to_owned(), header.exported);
                    declared_paths_by_file
                        .entry(header.source_file.to_owned())
                        .or_default()
                        .insert(header.tokens.src_path.to_owned());
                    if let Some(name) = header.tokens.src_path.name() {
                        declared_names_by_file
                            .entry(header.source_file.to_owned())
                            .or_default()
                            .insert(name);
                    }
                }
                _ => {}
            }
        }

        // Build per-source-file import visibility and start-function aliases.
        // WHY: imports are file-scoped rules, but declarations are module-scoped identities.
        let file_import_bindings = match resolve_file_import_bindings(
            &file_imports_by_source,
            &module_file_paths,
            &importable_symbol_exported,
            &declared_paths_by_file,
            &declared_names_by_file,
            host_registry,
            string_table,
        ) {
            Ok(bindings) => bindings,
            Err(error) => {
                return Err(CompilerMessages {
                    errors: vec![error],
                    warnings,
                    string_table: Default::default(),
                });
            }
        };

        // Resolve constants and structs in dependency order with file-scoped visibility.
        // Struct defaults require constant-context parsing and import gates, so defaults can
        // consume constants deterministically.
        for header in &sorted_headers {
            let bindings = file_import_bindings
                .get(&header.source_file)
                .cloned()
                .unwrap_or_default();
            let source_file_scope = header
                .tokens
                .canonical_os_path
                .as_ref()
                .map(|canonical_path| InternedPath::from_path_buf(canonical_path, string_table))
                .unwrap_or_else(|| header.source_file.to_owned());

            match &header.kind {
                HeaderKind::Constant { .. } => {
                    let declaration = match parse_constant_header_declaration(
                        header,
                        ConstantHeaderParseContext {
                            declarations: &declarations,
                            visible_declaration_ids: &bindings.visible_symbol_paths,
                            start_import_aliases: &bindings.start_aliases,
                            host_registry,
                            style_directives,
                            project_path_resolver: project_path_resolver.clone(),
                            path_format_config: path_format_config.clone(),
                            build_profile,
                            warnings: &mut warnings,
                            rendered_path_usages: rendered_path_usages.clone(),
                            string_table,
                        },
                    ) {
                        Ok(declaration) => declaration,
                        Err(error) => {
                            return Err(CompilerMessages {
                                errors: vec![error],
                                warnings,
                                string_table: Default::default(),
                            });
                        }
                    };

                    declarations.push(declaration.clone());
                    module_constants.push(declaration);
                }
                HeaderKind::Struct { .. } => {
                    let context = ScopeContext::new(
                        ContextKind::Constant,
                        header.tokens.src_path.to_owned(),
                        &declarations,
                        host_registry.clone(),
                        vec![],
                    )
                    .with_style_directives(style_directives)
                    .with_build_profile(build_profile)
                    .with_visible_declarations(bindings.visible_symbol_paths.to_owned())
                    .with_start_import_aliases(bindings.start_aliases.to_owned())
                    .with_project_path_resolver(project_path_resolver.clone())
                    .with_path_format_config(path_format_config.clone())
                    .with_rendered_path_usage_sink(rendered_path_usages.clone())
                    .with_source_file_scope(source_file_scope);

                    let mut struct_tokens = header.tokens.to_owned();
                    let fields_result =
                        create_struct_definition(&mut struct_tokens, &context, string_table);
                    warnings.extend(context.take_emitted_warnings());

                    let fields = match fields_result {
                        Ok(fields) => fields,
                        Err(error) => {
                            return Err(CompilerMessages {
                                errors: vec![error],
                                warnings,
                                string_table: Default::default(),
                            });
                        }
                    };

                    resolved_struct_fields_by_path
                        .insert(header.tokens.src_path.to_owned(), fields.to_owned());

                    declarations.push(Declaration {
                        id: header.tokens.src_path.to_owned(),
                        value: Expression::new(
                            ExpressionKind::None,
                            header.name_location.to_owned(),
                            DataType::Struct(fields, Ownership::MutableOwned),
                            Ownership::ImmutableReference,
                        ),
                    });
                }
                _ => {}
            }
        }

        for header in sorted_headers {
            let bindings = file_import_bindings
                .get(&header.source_file)
                .cloned()
                .unwrap_or_default();
            let source_file_scope = header
                .tokens
                .canonical_os_path
                .as_ref()
                .map(|canonical_path| InternedPath::from_path_buf(canonical_path, string_table))
                .unwrap_or_else(|| header.source_file.to_owned());

            match header.kind {
                HeaderKind::Function { signature } => {
                    let mut function_declarations = declarations.to_owned();
                    function_declarations.extend(signature.parameters.to_owned());
                    let mut visible_declarations = bindings.visible_symbol_paths.to_owned();
                    for parameter in &signature.parameters {
                        visible_declarations.insert(parameter.id.to_owned());
                    }

                    // Function parameters should be available in the function body scope
                    let context = ScopeContext::new(
                        ContextKind::Function,
                        header.tokens.src_path.to_owned(),
                        &function_declarations,
                        host_registry.clone(),
                        signature.return_data_types(),
                    )
                    .with_style_directives(style_directives)
                    .with_build_profile(build_profile)
                    .with_visible_declarations(visible_declarations)
                    .with_start_import_aliases(bindings.start_aliases.to_owned())
                    .with_project_path_resolver(project_path_resolver.clone())
                    .with_path_format_config(path_format_config.clone())
                    .with_rendered_path_usage_sink(rendered_path_usages.clone())
                    .with_source_file_scope(source_file_scope.to_owned());

                    let mut token_stream = header.tokens;

                    let body_result = function_body_to_ast(
                        &mut token_stream,
                        context.to_owned(),
                        &mut warnings,
                        string_table,
                    );
                    warnings.extend(context.take_emitted_warnings());

                    let body = match body_result {
                        Ok(b) => b,
                        Err(e) => {
                            return Err(CompilerMessages {
                                errors: vec![e],
                                warnings,
                                string_table: Default::default(),
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
                    )
                    .with_style_directives(style_directives)
                    .with_build_profile(build_profile)
                    .with_visible_declarations(bindings.visible_symbol_paths.to_owned())
                    .with_start_import_aliases(bindings.start_aliases.to_owned())
                    .with_project_path_resolver(project_path_resolver.clone())
                    .with_path_format_config(path_format_config.clone())
                    .with_rendered_path_usage_sink(rendered_path_usages.clone())
                    .with_source_file_scope(source_file_scope.to_owned());

                    let mut token_stream = header.tokens;

                    let body_result = function_body_to_ast(
                        &mut token_stream,
                        context.to_owned(),
                        &mut warnings,
                        string_table,
                    );
                    warnings.extend(context.take_emitted_warnings());

                    let mut body = match body_result {
                        Ok(b) => b,
                        Err(e) => {
                            return Err(CompilerMessages {
                                errors: vec![e],
                                warnings,
                                string_table: Default::default(),
                            });
                        }
                    };

                    // Add the automatic return statement for the start function
                    let empty_string = string_table.get_or_intern(String::new());
                    body.push(AstNode {
                        kind: NodeKind::Return(vec![Expression::string_slice(
                            empty_string,
                            token_stream.current_location(),
                            Ownership::ImmutableOwned,
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
                        returns: vec![FunctionReturn::Value(DataType::StringSlice)],
                    };

                    ast.push(AstNode {
                        kind: NodeKind::Function(full_name, main_signature, body),
                        location: header.name_location,
                        scope: context.scope.clone(),
                    });
                }

                HeaderKind::Struct { .. } => {
                    let fields = match resolved_struct_fields_by_path
                        .get(&header.tokens.src_path)
                        .cloned()
                    {
                        Some(fields) => fields,
                        None => {
                            return Err(CompilerMessages {
                                errors: vec![CompilerError::compiler_error(
                                    "Struct fields were not resolved before AST emission.",
                                )],
                                warnings,
                                string_table: Default::default(),
                            });
                        }
                    };

                    ast.push(AstNode {
                        kind: NodeKind::StructDefinition(header.tokens.src_path.to_owned(), fields), // Use the simple name for identifier
                        location: header.name_location,
                        scope: header.tokens.src_path, // Preserve the full path in the scope field
                    });
                }

                HeaderKind::Constant { .. } => {
                    // Constant headers are parsed into declarations in the prepass above.
                }

                HeaderKind::Choice => {
                    return Err(CompilerMessages {
                        errors: vec![CompilerError::compiler_error(
                            "Choice headers should be rejected during header parsing before AST construction.",
                        )],
                        warnings,
                        string_table: Default::default(),
                    });
                }

                HeaderKind::ConstTemplate { .. } => {
                    let mut template_tokens = header.tokens;
                    let context = ScopeContext::new(
                        ContextKind::Constant,
                        template_tokens.src_path.to_owned(),
                        &declarations,
                        host_registry.clone(),
                        vec![],
                    )
                    .with_style_directives(style_directives)
                    .with_build_profile(build_profile)
                    .with_visible_declarations(bindings.visible_symbol_paths.to_owned())
                    .with_start_import_aliases(bindings.start_aliases.to_owned())
                    .with_project_path_resolver(project_path_resolver.clone())
                    .with_path_format_config(path_format_config.clone())
                    .with_rendered_path_usage_sink(rendered_path_usages.clone())
                    .with_source_file_scope(source_file_scope);

                    let template_result =
                        Template::new(&mut template_tokens, &context, vec![], string_table);
                    warnings.extend(context.take_emitted_warnings());
                    let template = match template_result {
                        Ok(template) => template,
                        Err(error) => {
                            return Err(CompilerMessages {
                                errors: vec![error],
                                warnings,
                                string_table: Default::default(),
                            });
                        }
                    };

                    match template.const_value_kind() {
                        // WHAT: top-level const templates can be direct strings or wrapper
                        // templates with optional, unfilled slots.
                        // WHY: unfilled slots are rendered as empty strings at compile time.
                        crate::compiler_frontend::ast::templates::template::TemplateConstValueKind::RenderableString
                        | crate::compiler_frontend::ast::templates::template::TemplateConstValueKind::WrapperTemplate => {}
                        crate::compiler_frontend::ast::templates::template::TemplateConstValueKind::SlotInsertHelper => {
                            return Err(CompilerMessages {
                                errors: vec![CompilerError::new_rule_error(
                                    "Top-level const templates cannot evaluate to '$insert(...)' helpers. Apply this insert while filling an immediate parent '$slot' template.",
                                    template.location,
                                )],
                                warnings,
                                string_table: Default::default(),
                            });
                        }
                        crate::compiler_frontend::ast::templates::template::TemplateConstValueKind::NonConst => {
                            return Err(CompilerMessages {
                                errors: vec![CompilerError::new_rule_error(
                                    "Top-level const templates must be fully foldable at compile time.",
                                    template.location,
                                )],
                                warnings,
                                string_table: Default::default(),
                            });
                        }
                    }

                    let mut fold_context = match context
                        .new_template_fold_context(string_table, "top-level const template folding")
                    {
                        Ok(context) => context,
                        Err(error) => {
                            return Err(CompilerMessages {
                                errors: vec![error],
                                warnings,
                                string_table: Default::default(),
                            });
                        }
                    };

                    let html = match template.fold_into_stringid(&mut fold_context) {
                        Ok(value) => value,
                        Err(error) => {
                            return Err(CompilerMessages {
                                errors: vec![error],
                                warnings,
                                string_table: Default::default(),
                            });
                        }
                    };

                    const_templates_by_path.insert(template_tokens.src_path, html);
                }
            }

            // TODO: create a function definition for these exported headers
            if header.exported {}
        }

        let doc_fragments = collect_and_strip_comment_templates(
            &mut ast,
            project_path_resolver_for_folding,
            &path_format_config,
            string_table,
        )
        .map_err(|error| CompilerMessages {
            errors: vec![error],
            warnings: warnings.clone(),
            string_table: Default::default(),
        })?;

        let start_template_items = synthesize_start_template_items(
            &mut ast,
            &entry_dir,
            &top_level_template_items,
            &const_templates_by_path,
            project_path_resolver_for_folding,
            &path_format_config,
            string_table,
        )
        .map_err(|error| CompilerMessages {
            errors: vec![error],
            warnings: warnings.clone(),
            string_table: Default::default(),
        })?;

        Ok(Ast {
            nodes: ast,
            module_constants,
            doc_fragments,
            entry_path: entry_dir,
            external_exports,
            start_template_items,
            rendered_path_usages: std::mem::take(&mut *rendered_path_usages.borrow_mut()),
            warnings,
        })
    }
}

#[derive(Clone)]
pub struct ScopeContext {
    pub kind: ContextKind,
    pub scope: InternedPath,
    // Full declaration table for path-identity lookup and type/context metadata.
    // This stays module-wide so resolution always uses stable unique IDs.
    pub declarations: Vec<Declaration>,
    // Optional file-local visibility gate over `declarations`.
    // When present, references must be in this set, which enforces import boundaries.
    pub visible_declaration_ids: Option<FxHashSet<InternedPath>>,
    // Bare file imports (`@path/to/file`) bind alias -> imported file start function path.
    pub start_import_aliases: FxHashMap<StringId, InternedPath>,
    pub expected_result_types: Vec<DataType>,
    pub host_registry: HostRegistry,
    pub style_directives: StyleDirectiveRegistry,
    pub loop_depth: usize,
    pub build_profile: FrontendBuildProfile,
    pub(crate) emitted_warnings: Rc<RefCell<Vec<CompilerWarning>>>,

    /// Project-aware path resolver for compile-time path validation.
    pub(crate) project_path_resolver: Option<ProjectPathResolver>,

    /// The real filesystem source file that this context originated from.
    /// For const templates, `scope` is a synthetic path like `#page.bst/#const_template0`,
    /// so this field carries the actual source file path for path resolution.
    pub(crate) source_file_scope: Option<InternedPath>,
    /// Path formatting config for `#origin`-aware path string coercion.
    pub(crate) path_format_config: PathStringFormatConfig,
    /// Shared rendered-path usage sink for builder-visible template/output facts.
    pub(crate) rendered_path_usages: Rc<RefCell<Vec<RenderedPathUsage>>>,
}
#[derive(PartialEq, Clone)]
pub enum ContextKind {
    Module, // The top-level scope of each file in the module
    Expression,
    Constant, // An expression that is enforced to be evaluated at compile time and can't contain non-constant references
    ConstantHeader, // Top-level exported constant declaration context (#name = ...)
    Function,
    Condition, // For loops and if statements
    Loop,
    Branch,
    Template,
}

impl ContextKind {
    pub fn is_constant_context(&self) -> bool {
        matches!(self, ContextKind::Constant | ContextKind::ConstantHeader)
    }

    pub fn allows_const_record_coercion(&self) -> bool {
        matches!(self, ContextKind::ConstantHeader)
    }
}

impl ScopeContext {
    pub fn new(
        kind: ContextKind,
        scope: InternedPath,
        declarations: &[Declaration],
        host_registry: HostRegistry,
        expected_result_types: Vec<DataType>,
    ) -> ScopeContext {
        ScopeContext {
            kind,
            scope,
            declarations: declarations.to_owned(),
            visible_declaration_ids: None,
            start_import_aliases: FxHashMap::default(),
            expected_result_types,
            host_registry,
            style_directives: StyleDirectiveRegistry::built_ins(),
            loop_depth: 0,
            build_profile: FrontendBuildProfile::Dev,
            emitted_warnings: Rc::new(RefCell::new(Vec::new())),
            project_path_resolver: None,
            source_file_scope: None,
            path_format_config: PathStringFormatConfig::default(),
            rendered_path_usages: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn new_child_control_flow(
        &self,
        kind: ContextKind,
        string_table: &mut StringTable,
    ) -> ScopeContext {
        let mut new_context = self.to_owned();
        new_context.kind = kind;
        if matches!(new_context.kind, ContextKind::Loop) {
            new_context.loop_depth += 1;
        }

        let scope_id = CONTROL_FLOW_SCOPE_COUNTER.fetch_add(1, Ordering::Relaxed);
        new_context.scope = self
            .scope
            .join_str(&format!("__scope_{}", scope_id), string_table);

        new_context
    }

    pub fn new_child_function(
        &self,
        id: StringId,
        signature: FunctionSignature,
        _string_table: &mut StringTable,
    ) -> ScopeContext {
        let mut new_context = self.to_owned();
        new_context.kind = ContextKind::Function;
        new_context.expected_result_types = signature.return_data_types();

        // Create a new scope path by joining the current scope with the function name
        new_context.scope = self.scope.append(id);
        new_context.loop_depth = 0;

        new_context.declarations = self.declarations.to_owned();
        new_context.declarations.extend(signature.parameters);

        new_context
    }

    pub fn new_child_expression(&self, expected_result_types: Vec<DataType>) -> ScopeContext {
        let mut new_context = self.to_owned();
        new_context.kind = ContextKind::Expression;
        new_context.expected_result_types = expected_result_types;
        new_context
    }

    /// Build the context used while parsing template expressions.
    ///
    /// Constant contexts stay constant so template-head captures can inline
    /// compile-time values. All other contexts parse templates as runtime-capable.
    pub fn new_template_parsing_context(&self) -> ScopeContext {
        let template_kind = if self.kind.is_constant_context() {
            self.kind.clone()
        } else {
            ContextKind::Template
        };

        ScopeContext {
            kind: template_kind,
            scope: self.scope.clone(),
            declarations: self.declarations.to_owned(),
            visible_declaration_ids: self.visible_declaration_ids.clone(),
            start_import_aliases: self.start_import_aliases.clone(),
            expected_result_types: vec![],
            host_registry: self.host_registry.clone(),
            style_directives: self.style_directives.clone(),
            loop_depth: self.loop_depth,
            build_profile: self.build_profile,
            emitted_warnings: self.emitted_warnings.clone(),
            project_path_resolver: self.project_path_resolver.clone(),
            source_file_scope: self.source_file_scope.clone(),
            path_format_config: self.path_format_config.clone(),
            rendered_path_usages: self.rendered_path_usages.clone(),
        }
    }

    /// Builds a constant child context that preserves project-aware folding/path state.
    ///
    /// WHAT: clones the parent visibility/declaration environment and forces
    ///       resolver + source file scope propagation into constant parsing paths.
    /// WHY: resolver-less constant contexts are invalid for template folding and
    ///      template-head path coercion.
    pub fn new_constant(scope: InternedPath, parent: &ScopeContext) -> ScopeContext {
        ScopeContext {
            kind: ContextKind::Constant,
            scope,
            declarations: parent.declarations.to_owned(),
            visible_declaration_ids: parent.visible_declaration_ids.clone(),
            start_import_aliases: parent.start_import_aliases.clone(),
            expected_result_types: Vec::new(),
            host_registry: parent.host_registry.clone(),
            style_directives: parent.style_directives.clone(),
            loop_depth: parent.loop_depth,
            build_profile: parent.build_profile,
            emitted_warnings: parent.emitted_warnings.clone(),
            project_path_resolver: parent.project_path_resolver.clone(),
            source_file_scope: parent.source_file_scope.clone(),
            path_format_config: parent.path_format_config.clone(),
            rendered_path_usages: parent.rendered_path_usages.clone(),
        }
    }

    pub(crate) fn required_project_path_resolver(
        &self,
        operation: &str,
    ) -> Result<&ProjectPathResolver, CompilerError> {
        let Some(resolver) = self.project_path_resolver.as_ref() else {
            return_compiler_error!(
                "Missing project path resolver during '{}'. Context scope: '{}'. This is a compiler setup bug.",
                operation,
                format!("{:?}", self.scope)
            );
        };
        Ok(resolver)
    }

    pub(crate) fn required_source_file_scope(
        &self,
        operation: &str,
    ) -> Result<&InternedPath, CompilerError> {
        let Some(source_scope) = self.source_file_scope.as_ref() else {
            return_compiler_error!(
                "Missing source file scope during '{}'. Context scope: '{}'. This is a compiler setup bug.",
                operation,
                format!("{:?}", self.scope)
            );
        };
        Ok(source_scope)
    }

    pub fn new_template_fold_context<'a>(
        &'a self,
        string_table: &'a mut StringTable,
        operation: &str,
    ) -> Result<TemplateFoldContext<'a>, CompilerError> {
        let resolver = self.required_project_path_resolver(operation)?;
        let source_file_scope = self.required_source_file_scope(operation)?;
        Ok(TemplateFoldContext {
            string_table,
            project_path_resolver: resolver,
            path_format_config: &self.path_format_config,
            source_file_scope,
        })
    }

    pub fn with_build_profile(mut self, profile: FrontendBuildProfile) -> ScopeContext {
        self.build_profile = profile;
        self
    }

    pub fn with_visible_declarations(mut self, visible: FxHashSet<InternedPath>) -> ScopeContext {
        // A context without this gate can resolve any declaration in the module.
        // File/start contexts set this to enforce import semantics.
        self.visible_declaration_ids = Some(visible);
        self
    }

    pub fn with_start_import_aliases(
        mut self,
        aliases: FxHashMap<StringId, InternedPath>,
    ) -> ScopeContext {
        self.start_import_aliases = aliases;
        self
    }

    pub fn with_style_directives(
        mut self,
        style_directives: &StyleDirectiveRegistry,
    ) -> ScopeContext {
        self.style_directives = style_directives.clone();
        self
    }

    pub(crate) fn with_project_path_resolver(
        mut self,
        resolver: Option<ProjectPathResolver>,
    ) -> ScopeContext {
        self.project_path_resolver = resolver;
        self
    }

    pub fn with_source_file_scope(mut self, source_file: InternedPath) -> ScopeContext {
        self.source_file_scope = Some(source_file);
        self
    }

    pub fn with_path_format_config(mut self, config: PathStringFormatConfig) -> ScopeContext {
        self.path_format_config = config;
        self
    }

    pub fn with_rendered_path_usage_sink(
        mut self,
        sink: Rc<RefCell<Vec<RenderedPathUsage>>>,
    ) -> ScopeContext {
        self.rendered_path_usages = sink;
        self
    }

    pub fn resolve_start_import(&self, name: &StringId) -> Option<&InternedPath> {
        self.start_import_aliases.get(name)
    }

    pub fn add_var(&mut self, arg: Declaration) {
        // Keep the declaration table and visibility gate in sync for locals declared in-body.
        // Otherwise, a newly declared local could exist in `declarations` but be invisible.
        if let Some(visible) = self.visible_declaration_ids.as_mut() {
            visible.insert(arg.id.to_owned());
        }
        self.declarations.push(arg);
    }

    pub fn is_inside_loop(&self) -> bool {
        self.loop_depth > 0
    }

    pub fn emit_warning(&self, warning: CompilerWarning) {
        self.emitted_warnings.borrow_mut().push(warning);
    }

    pub fn take_emitted_warnings(&self) -> Vec<CompilerWarning> {
        std::mem::take(&mut *self.emitted_warnings.borrow_mut())
    }

    pub fn record_rendered_path_usages(&self, usages: Vec<RenderedPathUsage>) {
        self.rendered_path_usages.borrow_mut().extend(usages);
    }

    pub fn take_rendered_path_usages(&self) -> Vec<RenderedPathUsage> {
        std::mem::take(&mut *self.rendered_path_usages.borrow_mut())
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
            kind: if $context.kind.is_constant_context() {
                $context.kind.clone()
            } else {
                ContextKind::Template
            },
            scope: $context.scope.clone(),
            declarations: $context.declarations.to_owned(),
            visible_declaration_ids: $context.visible_declaration_ids.clone(),
            start_import_aliases: $context.start_import_aliases.clone(),
            expected_result_types: vec![],
            host_registry: $context.host_registry.clone(),
            style_directives: $context.style_directives.clone(),
            loop_depth: $context.loop_depth,
            build_profile: $context.build_profile,
            emitted_warnings: $context.emitted_warnings.clone(),
            project_path_resolver: $context.project_path_resolver.clone(),
            source_file_scope: $context.source_file_scope.clone(),
            path_format_config: $context.path_format_config.clone(),
            rendered_path_usages: $context.rendered_path_usages.clone(),
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
            visible_declaration_ids: None,
            start_import_aliases: rustc_hash::FxHashMap::default(),
            expected_result_types: vec![],
            host_registry: $registry,
            style_directives:
                $crate::compiler_frontend::style_directives::StyleDirectiveRegistry::built_ins(),
            loop_depth: 0,
            build_profile: $crate::compiler_frontend::FrontendBuildProfile::Dev,
            emitted_warnings: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            project_path_resolver: None,
            source_file_scope: None,
            path_format_config: PathStringFormatConfig::default(),
            rendered_path_usages: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
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
            visible_declaration_ids: None,
            start_import_aliases: rustc_hash::FxHashMap::default(),
            expected_result_types: vec![], //Empty because conditions are always booleans
            host_registry: $registry,
            style_directives:
                $crate::compiler_frontend::style_directives::StyleDirectiveRegistry::built_ins(),
            loop_depth: 0,
            build_profile: $crate::compiler_frontend::FrontendBuildProfile::Dev,
            emitted_warnings: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            project_path_resolver: None,
            source_file_scope: None,
            path_format_config: PathStringFormatConfig::default(),
            rendered_path_usages: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
        }
    }};
}
