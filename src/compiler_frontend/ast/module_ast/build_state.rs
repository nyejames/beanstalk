//! Mutable accumulation state for AST construction across all passes.
//!
//! WHAT: `AstBuildState` bundles all the maps that `Ast::new()` manages so each pass can be
//! extracted into a focused method without repeating large parameter lists.

use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::headers::module_symbols::ModuleSymbols;
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::projects::settings;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::rc::Rc;

use crate::compiler_frontend::ast::type_resolution::ResolvedFunctionSignature;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::paths::rendered_path_usage::RenderedPathUsage;

pub(super) struct AstBuildState<'a> {
    // Header-owned module symbol package from the header/dependency-sort phase.
    // Symbol-DB fields (importable_symbol_exported, file_imports_by_source, etc.)
    // live here and are accessed via self.module_symbols.xxx.
    pub(super) module_symbols: ModuleSymbols,

    // Immutable configuration shared across passes.
    pub(super) host_registry: &'a HostRegistry,
    pub(super) style_directives: &'a StyleDirectiveRegistry,
    pub(super) build_profile: FrontendBuildProfile,
    pub(super) project_path_resolver: &'a Option<ProjectPathResolver>,
    pub(super) path_format_config: &'a PathStringFormatConfig,

    // Mutable output state.
    pub(super) ast: Vec<AstNode>,
    pub(super) warnings: Vec<CompilerWarning>,
    // Starts as manifest declaration stubs; grows with resolved constants and struct types
    // in passes 3–4. Separate from manifest because it is mutated during AST construction.
    pub(super) declarations: Vec<Declaration>,
    pub(super) module_constants: Vec<Declaration>,
    pub(super) const_templates_by_path: FxHashMap<InternedPath, StringId>,
    pub(super) rendered_path_usages: Rc<RefCell<Vec<RenderedPathUsage>>>,

    // Builtin AST nodes seeded from the manifest; merged into output at finalization.
    pub(super) builtin_struct_ast_nodes: Vec<AstNode>,

    // Type resolution tables (populated in passes 2–4).
    // Seeded with builtin struct data from the manifest; extended with user-defined types.
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
        mut module_symbols: ModuleSymbols,
    ) -> Self {
        // Extract the fields that AstBuildState mutates during passes so the module_symbols
        // package can be stored whole for its read-only symbol-DB fields.
        let declarations = std::mem::take(&mut module_symbols.declarations);
        let builtin_struct_ast_nodes = std::mem::take(&mut module_symbols.builtin_struct_ast_nodes);
        let resolved_struct_fields_by_path =
            std::mem::take(&mut module_symbols.resolved_struct_fields_by_path);
        let struct_source_by_path = std::mem::take(&mut module_symbols.struct_source_by_path);

        Self {
            module_symbols,
            host_registry,
            style_directives,
            build_profile,
            project_path_resolver,
            path_format_config,
            ast: Vec::with_capacity(header_count * settings::TOKEN_TO_NODE_RATIO),
            warnings: Vec::new(),
            declarations,
            module_constants: Vec::new(),
            const_templates_by_path: FxHashMap::default(),
            rendered_path_usages: Rc::new(RefCell::new(Vec::new())),
            builtin_struct_ast_nodes,
            resolved_struct_fields_by_path,
            struct_source_by_path,
            resolved_function_signatures_by_path: FxHashMap::default(),
        }
    }

    pub(super) fn error_messages(
        &self,
        error: CompilerError,
        string_table: &crate::compiler_frontend::symbols::string_interning::StringTable,
    ) -> CompilerMessages {
        CompilerMessages::from_error_with_warnings(error, self.warnings.clone(), string_table)
    }
}
