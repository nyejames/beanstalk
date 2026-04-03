//! AST module construction and scope-context helpers.
//!
//! WHAT: combines per-file headers into one typed AST, resolves file-scoped imports, lowers
//! function/struct/const bodies, and synthesizes top-level template fragments.
//! WHY: this stage is where module-wide symbol identity and per-file visibility are enforced
//! together, so diagnostics must preserve the full shared `StringTable` context.

use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::function_body_to_ast::function_body_to_ast;
use crate::compiler_frontend::ast::import_bindings::{
    ConstantHeaderParseContext, FileImportBindings, parse_constant_header_declaration,
    resolve_file_import_bindings,
};
use crate::compiler_frontend::ast::receiver_methods::build_receiver_method_catalog;
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnSlot,
};
use crate::compiler_frontend::ast::statements::structs::create_struct_definition;
use crate::compiler_frontend::ast::templates::template_folding::TemplateFoldContext;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::top_level_templates::{
    collect_and_strip_comment_templates, synthesize_start_template_items,
};
use crate::compiler_frontend::ast::type_resolution::{
    ResolvedFunctionSignature, resolve_function_signature, resolve_struct_field_types,
    validate_no_recursive_runtime_structs,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::{DataType, Ownership, ReceiverKey};
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

pub(crate) use crate::compiler_frontend::ast::receiver_methods::{
    ReceiverMethodCatalog, ReceiverMethodEntry,
};

static CONTROL_FLOW_SCOPE_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[allow(dead_code)] // Used only in tests
/// Exported symbol metadata captured at AST construction time.
pub struct ModuleExport {
    pub id: StringId,
    pub signature: FunctionSignature,
}

/// Unified AST output for all source files in one compilation unit.
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

fn canonical_source_file_for_header(
    header: &Header,
    string_table: &mut StringTable,
) -> InternedPath {
    header
        .tokens
        .canonical_os_path
        .as_ref()
        .map(|canonical_path| InternedPath::from_path_buf(canonical_path, string_table))
        .unwrap_or_else(|| header.source_file.to_owned())
}

// ---------------------------------------------------------------------------
// AstBuildState: mutable accumulation state for AST construction
// ---------------------------------------------------------------------------

/// Mutable accumulation state for AST construction across all passes.
/// WHY: bundles the local maps that `Ast::new()` manages so each pass can be extracted into
/// a focused method without repeating large parameter lists.
struct AstBuildState<'a> {
    // Immutable configuration shared across passes.
    host_registry: &'a HostRegistry,
    style_directives: &'a StyleDirectiveRegistry,
    build_profile: FrontendBuildProfile,
    project_path_resolver: &'a Option<ProjectPathResolver>,
    path_format_config: &'a PathStringFormatConfig,

    // Mutable output state.
    ast: Vec<AstNode>,
    warnings: Vec<CompilerWarning>,
    declarations: Vec<Declaration>,
    module_constants: Vec<Declaration>,
    const_templates_by_path: FxHashMap<InternedPath, StringId>,
    rendered_path_usages: Rc<RefCell<Vec<RenderedPathUsage>>>,

    // Symbol registration tables (populated in pass 1).
    importable_symbol_exported: FxHashMap<InternedPath, bool>,
    file_imports_by_source: FxHashMap<InternedPath, Vec<FileImport>>,
    declared_paths_by_file: FxHashMap<InternedPath, FxHashSet<InternedPath>>,
    declared_names_by_file: FxHashMap<InternedPath, FxHashSet<StringId>>,
    module_file_paths: FxHashSet<InternedPath>,
    canonical_source_by_symbol_path: FxHashMap<InternedPath, InternedPath>,

    // Type resolution tables (populated in pass 2).
    resolved_struct_fields_by_path: FxHashMap<InternedPath, Vec<Declaration>>,
    struct_source_by_path: FxHashMap<InternedPath, InternedPath>,
    resolved_function_signatures_by_path: FxHashMap<InternedPath, ResolvedFunctionSignature>,
}

impl<'a> AstBuildState<'a> {
    fn new(
        host_registry: &'a HostRegistry,
        style_directives: &'a StyleDirectiveRegistry,
        build_profile: FrontendBuildProfile,
        project_path_resolver: &'a Option<ProjectPathResolver>,
        path_format_config: &'a PathStringFormatConfig,
        header_count: usize,
    ) -> Self {
        Self {
            host_registry,
            style_directives,
            build_profile,
            project_path_resolver,
            path_format_config,
            ast: Vec::with_capacity(header_count * settings::TOKEN_TO_NODE_RATIO),
            warnings: Vec::new(),
            declarations: Vec::new(),
            module_constants: Vec::new(),
            const_templates_by_path: FxHashMap::default(),
            rendered_path_usages: Rc::new(RefCell::new(Vec::new())),
            importable_symbol_exported: FxHashMap::default(),
            file_imports_by_source: FxHashMap::default(),
            declared_paths_by_file: FxHashMap::default(),
            declared_names_by_file: FxHashMap::default(),
            module_file_paths: FxHashSet::default(),
            canonical_source_by_symbol_path: FxHashMap::default(),
            resolved_struct_fields_by_path: FxHashMap::default(),
            struct_source_by_path: FxHashMap::default(),
            resolved_function_signatures_by_path: FxHashMap::default(),
        }
    }

    fn error_messages(
        &self,
        error: CompilerError,
        string_table: &StringTable,
    ) -> CompilerMessages {
        CompilerMessages::from_error_with_warnings(error, self.warnings.clone(), string_table)
    }

    /// Registers a symbol into the module-wide declared-path and declared-name tables.
    /// When `exported` is `Some`, also records the symbol's export visibility for import gates.
    /// WHY: this pattern was repeated for every importable header variant (Function, Struct,
    /// Constant, StartFunction). Centralising it prevents a missed insert from silently
    /// breaking visibility.
    fn register_declared_symbol(
        &mut self,
        symbol_path: &InternedPath,
        source_file: &InternedPath,
        exported: Option<bool>,
    ) {
        if let Some(is_exported) = exported {
            self.importable_symbol_exported
                .insert(symbol_path.to_owned(), is_exported);
        }
        self.declared_paths_by_file
            .entry(source_file.to_owned())
            .or_default()
            .insert(symbol_path.to_owned());
        if let Some(name) = symbol_path.name() {
            self.declared_names_by_file
                .entry(source_file.to_owned())
                .or_default()
                .insert(name);
        }
    }

    /// Pass 1: Collect every module declaration once.
    /// WHY: resolution stores fully qualified symbol paths.
    /// Each file context later applies its own visibility filter instead of rebuilding
    /// declaration tables.
    fn collect_declarations(
        &mut self,
        sorted_headers: &[Header],
        string_table: &mut StringTable,
    ) {
        for header in sorted_headers {
            self.module_file_paths
                .insert(header.source_file.to_owned());
            self.canonical_source_by_symbol_path.insert(
                header.tokens.src_path.to_owned(),
                canonical_source_file_for_header(header, string_table),
            );
            self.file_imports_by_source
                .entry(header.source_file.to_owned())
                .or_insert_with(|| header.file_imports.to_owned());

            match &header.kind {
                HeaderKind::Function { signature } => {
                    self.declarations.push(Declaration {
                        id: header.tokens.src_path.to_owned(),
                        value: Expression::new(
                            ExpressionKind::NoValue,
                            header.name_location.to_owned(),
                            DataType::Function(Box::new(None), signature.to_owned()),
                            Ownership::ImmutableReference,
                        ),
                    });
                    self.register_declared_symbol(
                        &header.tokens.src_path,
                        &header.source_file,
                        Some(header.exported),
                    );
                }
                HeaderKind::Struct { .. } => {
                    self.register_declared_symbol(
                        &header.tokens.src_path,
                        &header.source_file,
                        Some(header.exported),
                    );
                }
                HeaderKind::StartFunction => {
                    let start_name = header
                        .source_file
                        .join_str(IMPLICIT_START_FUNC_NAME, string_table);
                    self.declarations.push(Declaration {
                        id: start_name.to_owned(),
                        value: Expression::new(
                            ExpressionKind::NoValue,
                            header.name_location.to_owned(),
                            DataType::Function(
                                Box::new(None),
                                FunctionSignature {
                                    parameters: vec![],
                                    returns: vec![ReturnSlot::success(FunctionReturn::Value(
                                        DataType::StringSlice,
                                    ))],
                                },
                            ),
                            Ownership::ImmutableReference,
                        ),
                    });
                    self.register_declared_symbol(&start_name, &header.source_file, None);
                }
                HeaderKind::Constant { .. } => {
                    self.register_declared_symbol(
                        &header.tokens.src_path,
                        &header.source_file,
                        Some(header.exported),
                    );
                }
                _ => {}
            }
        }
    }

    /// Build per-source-file import visibility and start-function aliases.
    /// WHY: imports are file-scoped rules, but declarations are module-scoped identities.
    fn resolve_import_bindings(
        &self,
        string_table: &mut StringTable,
    ) -> Result<FxHashMap<InternedPath, FileImportBindings>, CompilerMessages> {
        resolve_file_import_bindings(
            &self.file_imports_by_source,
            &self.module_file_paths,
            &self.importable_symbol_exported,
            &self.declared_paths_by_file,
            &self.declared_names_by_file,
            self.host_registry,
            string_table,
        )
        .map_err(|error| self.error_messages(error, string_table))
    }

    /// Pass 2: Resolve constants and struct field types in dependency order.
    /// WHY: struct defaults require constant-context parsing and import gates, so defaults
    /// can consume constants deterministically.
    fn resolve_types(
        &mut self,
        sorted_headers: &[Header],
        file_import_bindings: &FxHashMap<InternedPath, FileImportBindings>,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        for header in sorted_headers {
            let bindings = file_import_bindings
                .get(&header.source_file)
                .cloned()
                .unwrap_or_default();
            let source_file_scope = canonical_source_file_for_header(header, string_table);

            match &header.kind {
                HeaderKind::Constant { .. } => {
                    let declaration = parse_constant_header_declaration(
                        header,
                        ConstantHeaderParseContext {
                            declarations: &self.declarations,
                            visible_declaration_ids: &bindings.visible_symbol_paths,
                            start_import_aliases: &bindings.start_aliases,
                            host_registry: self.host_registry,
                            style_directives: self.style_directives,
                            project_path_resolver: self.project_path_resolver.clone(),
                            path_format_config: self.path_format_config.clone(),
                            build_profile: self.build_profile,
                            warnings: &mut self.warnings,
                            rendered_path_usages: self.rendered_path_usages.clone(),
                            string_table,
                        },
                    )
                    .map_err(|error| self.error_messages(error, string_table))?;

                    self.declarations.push(declaration.clone());
                    self.module_constants.push(declaration);
                }
                HeaderKind::Struct { .. } => {
                    let context = ScopeContext::new(
                        ContextKind::Constant,
                        header.tokens.src_path.to_owned(),
                        &self.declarations,
                        self.host_registry.clone(),
                        vec![],
                    )
                    .with_style_directives(self.style_directives)
                    .with_build_profile(self.build_profile)
                    .with_visible_declarations(bindings.visible_symbol_paths.to_owned())
                    .with_start_import_aliases(bindings.start_aliases.to_owned())
                    .with_project_path_resolver(self.project_path_resolver.clone())
                    .with_path_format_config(self.path_format_config.clone())
                    .with_rendered_path_usage_sink(self.rendered_path_usages.clone())
                    .with_source_file_scope(source_file_scope.to_owned());

                    let mut struct_tokens = header.tokens.to_owned();
                    let fields_result =
                        create_struct_definition(&mut struct_tokens, &context, string_table);
                    self.warnings.extend(context.take_emitted_warnings());

                    let parsed_fields = fields_result
                        .map_err(|error| self.error_messages(error, string_table))?;

                    let fields = resolve_struct_field_types(
                        &header.tokens.src_path,
                        &parsed_fields,
                        &self.declarations,
                        Some(&bindings.visible_symbol_paths),
                        string_table,
                    )
                    .map_err(|error| self.error_messages(error, string_table))?;

                    self.resolved_struct_fields_by_path
                        .insert(header.tokens.src_path.to_owned(), fields.to_owned());
                    self.struct_source_by_path.insert(
                        header.tokens.src_path.to_owned(),
                        source_file_scope.to_owned(),
                    );

                    self.declarations.push(Declaration {
                        id: header.tokens.src_path.to_owned(),
                        value: Expression::new(
                            ExpressionKind::NoValue,
                            header.name_location.to_owned(),
                            DataType::runtime_struct(
                                header.tokens.src_path.to_owned(),
                                fields,
                                Ownership::MutableOwned,
                            ),
                            Ownership::ImmutableReference,
                        ),
                    });
                }
                _ => {}
            }
        }

        validate_no_recursive_runtime_structs(&self.resolved_struct_fields_by_path, string_table)
            .map_err(|error| self.error_messages(error, string_table))
    }

    /// Resolve function signatures after struct declarations are available.
    /// WHY: late resolution lets signatures use named struct types and receiver syntax
    /// without adding a second nominal-type system just for headers.
    fn resolve_function_signatures(
        &mut self,
        sorted_headers: &[Header],
        file_import_bindings: &FxHashMap<InternedPath, FileImportBindings>,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        for header in sorted_headers {
            let HeaderKind::Function { signature } = &header.kind else {
                continue;
            };

            let bindings = file_import_bindings
                .get(&header.source_file)
                .cloned()
                .unwrap_or_default();
            let resolved_signature = resolve_function_signature(
                &header.tokens.src_path,
                signature,
                &self.declarations,
                Some(&bindings.visible_symbol_paths),
                string_table,
            )
            .map_err(|error| self.error_messages(error, string_table))?;

            let Some(function_declaration) = self
                .declarations
                .iter_mut()
                .find(|declaration| declaration.id == header.tokens.src_path)
            else {
                return Err(self.error_messages(
                    CompilerError::compiler_error(
                        "Function declaration was not registered before AST signature resolution.",
                    ),
                    string_table,
                ));
            };

            function_declaration.value.data_type = DataType::Function(
                Box::new(resolved_signature.receiver.to_owned()),
                resolved_signature.signature.to_owned(),
            );
            self.resolved_function_signatures_by_path
                .insert(header.tokens.src_path.to_owned(), resolved_signature);
        }
        Ok(())
    }

    /// Build the receiver method catalog from resolved function signatures.
    fn build_receiver_catalog(
        &self,
        sorted_headers: &[Header],
        string_table: &mut StringTable,
    ) -> Result<Rc<ReceiverMethodCatalog>, CompilerMessages> {
        build_receiver_method_catalog(
            sorted_headers,
            &self.resolved_function_signatures_by_path,
            &self.resolved_struct_fields_by_path,
            &self.struct_source_by_path,
            &self.canonical_source_by_symbol_path,
            string_table,
        )
        .map(Rc::new)
        .map_err(|error| self.error_messages(error, string_table))
    }

    /// Pass 3: Emit AST nodes for each header kind (functions, structs, templates).
    fn emit_ast_nodes(
        &mut self,
        sorted_headers: Vec<Header>,
        file_import_bindings: &FxHashMap<InternedPath, FileImportBindings>,
        receiver_methods: &Rc<ReceiverMethodCatalog>,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        for header in sorted_headers {
            let bindings = file_import_bindings
                .get(&header.source_file)
                .cloned()
                .unwrap_or_default();
            let source_file_scope = canonical_source_file_for_header(&header, string_table);

            match header.kind {
                HeaderKind::Function { signature: _ } => {
                    let Some(resolved_signature) = self
                        .resolved_function_signatures_by_path
                        .get(&header.tokens.src_path)
                        .cloned()
                    else {
                        return Err(self.error_messages(
                            CompilerError::compiler_error(
                                "Function signature was not resolved before AST emission.",
                            ),
                            string_table,
                        ));
                    };

                    let mut function_declarations = self.declarations.to_owned();
                    function_declarations
                        .extend(resolved_signature.signature.parameters.to_owned());
                    let mut visible_declarations = bindings.visible_symbol_paths.to_owned();
                    for parameter in &resolved_signature.signature.parameters {
                        visible_declarations.insert(parameter.id.to_owned());
                    }

                    // Function parameters should be available in the function body scope
                    let mut context = ScopeContext::new(
                        ContextKind::Function,
                        header.tokens.src_path.to_owned(),
                        &function_declarations,
                        self.host_registry.clone(),
                        resolved_signature.signature.return_data_types(),
                    )
                    .with_style_directives(self.style_directives)
                    .with_build_profile(self.build_profile)
                    .with_visible_declarations(visible_declarations)
                    .with_start_import_aliases(bindings.start_aliases.to_owned())
                    .with_project_path_resolver(self.project_path_resolver.clone())
                    .with_path_format_config(self.path_format_config.clone())
                    .with_rendered_path_usage_sink(self.rendered_path_usages.clone())
                    .with_receiver_methods(receiver_methods.clone())
                    .with_source_file_scope(source_file_scope.to_owned());
                    context.expected_error_type = resolved_signature
                        .signature
                        .error_return()
                        .map(|ret| ret.data_type().to_owned());

                    let mut token_stream = header.tokens;

                    let body_result = function_body_to_ast(
                        &mut token_stream,
                        context.to_owned(),
                        &mut self.warnings,
                        string_table,
                    );
                    self.warnings.extend(context.take_emitted_warnings());

                    let body = body_result
                        .map_err(|error| self.error_messages(error, string_table))?;

                    // AST symbol IDs are stored as full InternedPath values and are unique
                    // module-wide, not only within a local scope.
                    self.ast.push(AstNode {
                        kind: NodeKind::Function(
                            token_stream.src_path,
                            resolved_signature.signature,
                            body.to_owned(),
                        ),
                        location: header.name_location,
                        scope: context.scope.clone(),
                    });
                }

                HeaderKind::StartFunction => {
                    let context = ScopeContext::new(
                        ContextKind::Module,
                        header.tokens.src_path.to_owned(),
                        &self.declarations,
                        self.host_registry.clone(),
                        vec![],
                    )
                    .with_style_directives(self.style_directives)
                    .with_build_profile(self.build_profile)
                    .with_visible_declarations(bindings.visible_symbol_paths.to_owned())
                    .with_start_import_aliases(bindings.start_aliases.to_owned())
                    .with_project_path_resolver(self.project_path_resolver.clone())
                    .with_path_format_config(self.path_format_config.clone())
                    .with_rendered_path_usage_sink(self.rendered_path_usages.clone())
                    .with_receiver_methods(receiver_methods.clone())
                    .with_source_file_scope(source_file_scope.to_owned());

                    let mut token_stream = header.tokens;

                    let body_result = function_body_to_ast(
                        &mut token_stream,
                        context.to_owned(),
                        &mut self.warnings,
                        string_table,
                    );
                    self.warnings.extend(context.take_emitted_warnings());

                    let mut body = body_result
                        .map_err(|error| self.error_messages(error, string_table))?;

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
                        returns: vec![ReturnSlot::success(FunctionReturn::Value(
                            DataType::StringSlice,
                        ))],
                    };

                    self.ast.push(AstNode {
                        kind: NodeKind::Function(full_name, main_signature, body),
                        location: header.name_location,
                        scope: context.scope.clone(),
                    });
                }

                HeaderKind::Struct { .. } => {
                    let fields = self
                        .resolved_struct_fields_by_path
                        .get(&header.tokens.src_path)
                        .cloned()
                        .ok_or_else(|| {
                            self.error_messages(
                                CompilerError::compiler_error(
                                    "Struct fields were not resolved before AST emission.",
                                ),
                                string_table,
                            )
                        })?;

                    self.ast.push(AstNode {
                        kind: NodeKind::StructDefinition(header.tokens.src_path.to_owned(), fields),
                        location: header.name_location,
                        scope: header.tokens.src_path,
                    });
                }

                HeaderKind::Constant { .. } => {
                    // Constant headers are parsed into declarations in the prepass above.
                }

                HeaderKind::Choice => {
                    return Err(self.error_messages(
                        CompilerError::compiler_error(
                            "Choice headers should be rejected during header parsing before AST construction.",
                        ),
                        string_table,
                    ));
                }

                HeaderKind::ConstTemplate { .. } => {
                    let mut template_tokens = header.tokens;
                    let context = ScopeContext::new(
                        ContextKind::Constant,
                        template_tokens.src_path.to_owned(),
                        &self.declarations,
                        self.host_registry.clone(),
                        vec![],
                    )
                    .with_style_directives(self.style_directives)
                    .with_build_profile(self.build_profile)
                    .with_visible_declarations(bindings.visible_symbol_paths.to_owned())
                    .with_start_import_aliases(bindings.start_aliases.to_owned())
                    .with_project_path_resolver(self.project_path_resolver.clone())
                    .with_path_format_config(self.path_format_config.clone())
                    .with_rendered_path_usage_sink(self.rendered_path_usages.clone())
                    .with_source_file_scope(source_file_scope);

                    let template_result =
                        Template::new(&mut template_tokens, &context, vec![], string_table);
                    self.warnings.extend(context.take_emitted_warnings());
                    let template = template_result
                        .map_err(|error| self.error_messages(error, string_table))?;

                    match template.const_value_kind() {
                        // WHAT: top-level const templates can be direct strings or wrapper
                        // templates with optional, unfilled slots.
                        // WHY: unfilled slots are rendered as empty strings at compile time.
                        crate::compiler_frontend::ast::templates::template::TemplateConstValueKind::RenderableString
                        | crate::compiler_frontend::ast::templates::template::TemplateConstValueKind::WrapperTemplate => {}
                        crate::compiler_frontend::ast::templates::template::TemplateConstValueKind::SlotInsertHelper => {
                            return Err(self.error_messages(
                                CompilerError::new_rule_error(
                                    "Top-level const templates cannot evaluate to '$insert(...)' helpers. Apply this insert while filling an immediate parent '$slot' template.",
                                    template.location,
                                ),
                                string_table,
                            ));
                        }
                        crate::compiler_frontend::ast::templates::template::TemplateConstValueKind::NonConst => {
                            return Err(self.error_messages(
                                CompilerError::new_rule_error(
                                    "Top-level const templates must be fully foldable at compile time.",
                                    template.location,
                                ),
                                string_table,
                            ));
                        }
                    }

                    let mut fold_context = match context
                        .new_template_fold_context(string_table, "top-level const template folding")
                    {
                        Ok(ctx) => ctx,
                        Err(error) => {
                            return Err(self.error_messages(error, string_table));
                        }
                    };

                    let html = match template.fold_into_stringid(&mut fold_context) {
                        Ok(value) => value,
                        Err(error) => {
                            return Err(self.error_messages(error, string_table));
                        }
                    };

                    self.const_templates_by_path
                        .insert(template_tokens.src_path, html);
                }
            }
        }
        Ok(())
    }

    /// Assemble the final `Ast` from accumulated build state.
    fn finalize(
        mut self,
        entry_dir: InternedPath,
        top_level_template_items: &[TopLevelTemplateItem],
        string_table: &mut StringTable,
    ) -> Result<Ast, CompilerMessages> {
        let project_path_resolver = self.project_path_resolver.as_ref().ok_or_else(|| {
            self.error_messages(
                CompilerError::compiler_error(
                    "AST construction requires a project path resolver for template folding and path coercion.",
                ),
                string_table,
            )
        })?;

        let doc_fragments = collect_and_strip_comment_templates(
            &mut self.ast,
            project_path_resolver,
            self.path_format_config,
            string_table,
        )
        .map_err(|error| self.error_messages(error, string_table))?;

        let start_template_items = synthesize_start_template_items(
            &mut self.ast,
            &entry_dir,
            top_level_template_items,
            &self.const_templates_by_path,
            project_path_resolver,
            self.path_format_config,
            string_table,
        )
        .map_err(|error| self.error_messages(error, string_table))?;

        Ok(Ast {
            nodes: self.ast,
            module_constants: self.module_constants,
            doc_fragments,
            entry_path: entry_dir,
            external_exports: Vec::new(),
            start_template_items,
            rendered_path_usages: std::mem::take(&mut *self.rendered_path_usages.borrow_mut()),
            warnings: self.warnings,
        })
    }
}

// ---------------------------------------------------------------------------
// Ast::new – thin orchestrator over AstBuildState
// ---------------------------------------------------------------------------

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
        let mut state = AstBuildState::new(
            host_registry,
            style_directives,
            build_profile,
            &project_path_resolver,
            &path_format_config,
            sorted_headers.len(),
        );

        state.collect_declarations(&sorted_headers, string_table);

        let file_import_bindings = state.resolve_import_bindings(string_table)?;

        state.resolve_types(&sorted_headers, &file_import_bindings, string_table)?;

        state.resolve_function_signatures(&sorted_headers, &file_import_bindings, string_table)?;

        let receiver_methods = state.build_receiver_catalog(&sorted_headers, string_table)?;

        state.emit_ast_nodes(
            sorted_headers,
            &file_import_bindings,
            &receiver_methods,
            string_table,
        )?;

        state.finalize(entry_dir, &top_level_template_items, string_table)
    }
}

// ---------------------------------------------------------------------------
// ScopeContext – shared parser/lowering context for one active AST scope
// ---------------------------------------------------------------------------

#[derive(Clone)]
/// Shared parser/lowering context for one active AST scope.
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
    pub expected_error_type: Option<DataType>,
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
    pub(crate) receiver_methods: Rc<ReceiverMethodCatalog>,
}
#[derive(PartialEq, Clone)]
/// High-level scope categories used by parser/lowering rules.
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
            expected_error_type: None,
            host_registry,
            style_directives: StyleDirectiveRegistry::built_ins(),
            loop_depth: 0,
            build_profile: FrontendBuildProfile::Dev,
            emitted_warnings: Rc::new(RefCell::new(Vec::new())),
            project_path_resolver: None,
            source_file_scope: None,
            path_format_config: PathStringFormatConfig::default(),
            rendered_path_usages: Rc::new(RefCell::new(Vec::new())),
            receiver_methods: Rc::new(ReceiverMethodCatalog::default()),
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
        new_context.expected_error_type = signature.error_return().map(|ret| ret.data_type().to_owned());

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
            expected_error_type: self.expected_error_type.clone(),
            host_registry: self.host_registry.clone(),
            style_directives: self.style_directives.clone(),
            loop_depth: self.loop_depth,
            build_profile: self.build_profile,
            emitted_warnings: self.emitted_warnings.clone(),
            project_path_resolver: self.project_path_resolver.clone(),
            source_file_scope: self.source_file_scope.clone(),
            path_format_config: self.path_format_config.clone(),
            rendered_path_usages: self.rendered_path_usages.clone(),
            receiver_methods: self.receiver_methods.clone(),
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
            expected_error_type: parent.expected_error_type.clone(),
            host_registry: parent.host_registry.clone(),
            style_directives: parent.style_directives.clone(),
            loop_depth: parent.loop_depth,
            build_profile: parent.build_profile,
            emitted_warnings: parent.emitted_warnings.clone(),
            project_path_resolver: parent.project_path_resolver.clone(),
            source_file_scope: parent.source_file_scope.clone(),
            path_format_config: parent.path_format_config.clone(),
            rendered_path_usages: parent.rendered_path_usages.clone(),
            receiver_methods: parent.receiver_methods.clone(),
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

    pub fn new_template_fold_context<'b>(
        &'b self,
        string_table: &'b mut StringTable,
        operation: &str,
    ) -> Result<TemplateFoldContext<'b>, CompilerError> {
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

    pub(crate) fn with_receiver_methods(
        mut self,
        receiver_methods: Rc<ReceiverMethodCatalog>,
    ) -> ScopeContext {
        self.receiver_methods = receiver_methods;
        self
    }

    pub fn resolve_start_import(&self, name: &StringId) -> Option<&InternedPath> {
        self.start_import_aliases.get(name)
    }

    pub(crate) fn lookup_receiver_method(
        &self,
        receiver: &ReceiverKey,
        method_name: StringId,
    ) -> Option<&ReceiverMethodEntry> {
        let entry = self
            .receiver_methods
            .by_receiver_and_name
            .get(&(receiver.to_owned(), method_name))?;

        let current_source_file = self.source_file_scope.as_ref()?;
        if &entry.source_file == current_source_file || entry.exported {
            Some(entry)
        } else {
            None
        }
    }

    pub(crate) fn lookup_visible_receiver_method_by_name(
        &self,
        method_name: StringId,
    ) -> Option<&ReceiverMethodEntry> {
        let current_source_file = self.source_file_scope.as_ref()?;
        let entries = self.receiver_methods.by_method_name.get(&method_name)?;

        entries
            .iter()
            .find(|entry| &entry.source_file == current_source_file)
            .or_else(|| entries.iter().find(|entry| entry.exported))
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
            receiver_methods: $context.receiver_methods.clone(),
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
            receiver_methods: std::rc::Rc::new(
                $crate::compiler_frontend::ast::ast::ReceiverMethodCatalog::default(),
            ),
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
            receiver_methods: std::rc::Rc::new(
                $crate::compiler_frontend::ast::ast::ReceiverMethodCatalog::default(),
            ),
        }
    }};
}

#[cfg(test)]
#[path = "tests/module_ast_receiver_method_tests.rs"]
mod module_ast_receiver_method_tests;
