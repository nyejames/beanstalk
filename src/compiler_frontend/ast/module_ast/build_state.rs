//! Mutable accumulation state for AST construction across all passes.
//!
//! WHAT: `AstBuildState` bundles all the maps that `Ast::new()` manages so each pass can be
//! extracted into a focused method without repeating large parameter lists.

use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringId;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::projects::settings;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::rc::Rc;

use crate::compiler_frontend::ast::type_resolution::ResolvedFunctionSignature;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::paths::rendered_path_usage::RenderedPathUsage;

pub(super) struct AstBuildState<'a> {
    // Immutable configuration shared across passes.
    pub(super) host_registry: &'a HostRegistry,
    pub(super) style_directives: &'a StyleDirectiveRegistry,
    pub(super) build_profile: FrontendBuildProfile,
    pub(super) project_path_resolver: &'a Option<ProjectPathResolver>,
    pub(super) path_format_config: &'a PathStringFormatConfig,

    // Mutable output state.
    pub(super) ast: Vec<AstNode>,
    pub(super) warnings: Vec<CompilerWarning>,
    pub(super) declarations: Vec<Declaration>,
    pub(super) module_constants: Vec<Declaration>,
    pub(super) const_templates_by_path: FxHashMap<InternedPath, StringId>,
    pub(super) rendered_path_usages: Rc<RefCell<Vec<RenderedPathUsage>>>,

    // Symbol registration tables (populated in pass 1).
    pub(super) importable_symbol_exported: FxHashMap<InternedPath, bool>,
    pub(super) file_imports_by_source: FxHashMap<
        InternedPath,
        Vec<crate::compiler_frontend::headers::parse_file_headers::FileImport>,
    >,
    pub(super) declared_paths_by_file: FxHashMap<InternedPath, FxHashSet<InternedPath>>,
    pub(super) declared_names_by_file: FxHashMap<InternedPath, FxHashSet<StringId>>,
    pub(super) module_file_paths: FxHashSet<InternedPath>,
    pub(super) canonical_source_by_symbol_path: FxHashMap<InternedPath, InternedPath>,
    pub(super) builtin_visible_symbol_paths: FxHashSet<InternedPath>,
    pub(super) builtin_struct_ast_nodes: Vec<AstNode>,

    // Type resolution tables (populated in passes 2–4).
    pub(super) resolved_struct_fields_by_path: FxHashMap<InternedPath, Vec<Declaration>>,
    pub(super) struct_source_by_path: FxHashMap<InternedPath, InternedPath>,
    pub(super) resolved_function_signatures_by_path:
        FxHashMap<InternedPath, ResolvedFunctionSignature>,
}

impl<'a> AstBuildState<'a> {
    pub(super) fn new(
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
            builtin_visible_symbol_paths: FxHashSet::default(),
            builtin_struct_ast_nodes: Vec::new(),
            resolved_struct_fields_by_path: FxHashMap::default(),
            struct_source_by_path: FxHashMap::default(),
            resolved_function_signatures_by_path: FxHashMap::default(),
        }
    }

    pub(super) fn error_messages(
        &self,
        error: CompilerError,
        string_table: &crate::compiler_frontend::string_interning::StringTable,
    ) -> CompilerMessages {
        CompilerMessages::from_error_with_warnings(error, self.warnings.clone(), string_table)
    }

    /// Registers a symbol into the module-wide declared-path and declared-name tables.
    /// When `exported` is `Some`, also records the symbol's export visibility for import gates.
    /// WHY: this pattern was repeated for every importable header variant (Function, Struct,
    /// Constant, StartFunction). Centralising it prevents a missed insert from silently
    /// breaking visibility.
    pub(super) fn register_declared_symbol(
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
}
