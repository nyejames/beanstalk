//! Mutable accumulation state for AST construction across all passes.
//!
//! WHAT: `AstBuildState` bundles all the maps that [`Ast::new`](crate::compiler_frontend::ast::Ast::new)
//! manages so each pass can be extracted into a focused method without repeating large parameter lists.
//!
//! WHY: one long-lived struct owns the pass-to-pass accumulation, the output vectors, and the
//! header-owned `ModuleSymbols` package. It is NOT a parser context — per-body scope growth is
//! owned by [`ScopeContext`](crate::compiler_frontend::ast::ScopeContext), which receives cloned
//! snapshots of `AstBuildState` data (e.g. `Rc<ReceiverMethodCatalog>`, declaration tables) for
//! each function/template body.
//!
//! ## Context boundary
//!
//! | Concern | Owner |
//! |---|---|
//! | Module-wide pass accumulation + output assembly | `AstBuildState` |
//! | Per-body parser state (locals, loops, type expectations) | `ScopeContext` |
//! | Input dependency bag for `Ast::new` | `AstBuildContext` |

use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::ast::Ast;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration};
use crate::compiler_frontend::ast::templates::top_level_templates::{
    collect_and_strip_comment_templates, collect_const_top_level_fragments,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::headers::module_symbols::ModuleSymbols;
use crate::compiler_frontend::headers::parse_file_headers::TopLevelConstFragment;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::paths::rendered_path_usage::RenderedPathUsage;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::projects::settings;
use crate::timer_log;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use crate::compiler_frontend::ast::type_resolution::ResolvedFunctionSignature;

pub(in crate::compiler_frontend::ast) struct AstBuildState<'a> {
    // Header-owned module symbol package from the header/dependency-sort phase.
    // Symbol-DB fields (importable_symbol_exported, file_imports_by_source, etc.)
    // live here and are accessed via self.module_symbols.xxx.
    pub(in crate::compiler_frontend::ast) module_symbols: ModuleSymbols,

    // Immutable configuration shared across passes.
    pub(in crate::compiler_frontend::ast) external_package_registry: &'a ExternalPackageRegistry,
    pub(in crate::compiler_frontend::ast) style_directives: &'a StyleDirectiveRegistry,
    pub(in crate::compiler_frontend::ast) build_profile: FrontendBuildProfile,
    pub(in crate::compiler_frontend::ast) project_path_resolver: &'a Option<ProjectPathResolver>,
    pub(in crate::compiler_frontend::ast) path_format_config: &'a PathStringFormatConfig,

    // Mutable output state.
    pub(in crate::compiler_frontend::ast) ast: Vec<AstNode>,
    pub(in crate::compiler_frontend::ast) warnings: Vec<CompilerWarning>,
    // Starts as dependency-sorted top-level declaration placeholders produced by
    // resolve_module_dependencies; grows with resolved constants and struct types
    // during AST type resolution.
    pub(in crate::compiler_frontend::ast) declarations: Vec<Declaration>,
    pub(in crate::compiler_frontend::ast) module_constants: Vec<Declaration>,
    pub(in crate::compiler_frontend::ast) const_templates_by_path:
        FxHashMap<InternedPath, StringId>,
    pub(in crate::compiler_frontend::ast) rendered_path_usages: Rc<RefCell<Vec<RenderedPathUsage>>>,

    // Builtin AST nodes seeded from the manifest; merged into output at finalization.
    pub(in crate::compiler_frontend::ast) builtin_struct_ast_nodes: Vec<AstNode>,

    // Type resolution tables (populated in passes 2–4).
    // Seeded with builtin struct data from the manifest; extended with user-defined types.
    pub(in crate::compiler_frontend::ast) resolved_struct_fields_by_path:
        FxHashMap<InternedPath, Vec<Declaration>>,
    pub(in crate::compiler_frontend::ast) struct_source_by_path:
        FxHashMap<InternedPath, InternedPath>,
    pub(in crate::compiler_frontend::ast) resolved_function_signatures_by_path:
        FxHashMap<InternedPath, ResolvedFunctionSignature>,
}

impl<'a> AstBuildState<'a> {
    pub(in crate::compiler_frontend::ast) fn new(
        external_package_registry: &'a ExternalPackageRegistry,
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
            external_package_registry,
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

    pub(in crate::compiler_frontend::ast) fn error_messages(
        &self,
        error: CompilerError,
        string_table: &crate::compiler_frontend::symbols::string_interning::StringTable,
    ) -> CompilerMessages {
        CompilerMessages::from_error_with_warnings(error, self.warnings.clone(), string_table)
    }

    /// Pass 7: Assemble the final [`Ast`] from the accumulated build state.
    ///
    /// WHAT: strips doc-comment templates, collects const top-level fragment values,
    /// normalizes all templates for HIR consumption, and assembles the final [`Ast`] output.
    ///
    /// WHY: this is the final transformation before HIR lowering. Templates must be fully
    /// normalized (folded constants, render plans, complete metadata) so HIR receives
    /// semantically complete template inputs.
    pub(in crate::compiler_frontend::ast) fn finalize(
        mut self,
        entry_dir: InternedPath,
        top_level_const_fragments: &[TopLevelConstFragment],
        string_table: &mut crate::compiler_frontend::symbols::string_interning::StringTable,
    ) -> Result<Ast, CompilerMessages> {
        let project_path_resolver = self.project_path_resolver.as_ref().ok_or_else(|| {
            self.error_messages(
                CompilerError::compiler_error(
                    "AST construction requires a project path resolver for template folding and path coercion.",
                ),
                string_table,
            )
        })?;

        let doc_fragments_start = Instant::now();
        let doc_fragments = collect_and_strip_comment_templates(
            &mut self.ast,
            project_path_resolver,
            self.path_format_config,
            string_table,
        )
        .map_err(|error| self.error_messages(error, string_table))?;
        timer_log!(
            doc_fragments_start,
            "AST/finalize/doc fragments collected in: "
        );
        let _ = doc_fragments_start;

        let const_fragments_start = Instant::now();
        let const_top_level_fragments = collect_const_top_level_fragments(
            top_level_const_fragments,
            &self.const_templates_by_path,
        )
        .map_err(|error| self.error_messages(error, string_table))?;
        timer_log!(
            const_fragments_start,
            "AST/finalize/const top-level fragments collected in: "
        );
        let _ = const_fragments_start;

        let ast_template_normalization_start = Instant::now();
        self.normalize_ast_templates_for_hir(project_path_resolver, string_table)
            .map_err(|error| self.error_messages(error, string_table))?;
        timer_log!(
            ast_template_normalization_start,
            "AST/finalize/AST templates normalized in: "
        );
        let _ = ast_template_normalization_start;

        let module_constant_normalization_start = Instant::now();
        let module_constants = self
            .normalize_module_constants_for_hir(project_path_resolver, string_table)
            .map_err(|error| self.error_messages(error, string_table))?;
        timer_log!(
            module_constant_normalization_start,
            "AST/finalize/module constants normalized in: "
        );
        let _ = module_constant_normalization_start;

        let builtin_merge_start = Instant::now();
        if !self.builtin_struct_ast_nodes.is_empty() {
            let mut ast_nodes = self.builtin_struct_ast_nodes;
            ast_nodes.extend(self.ast);
            self.ast = ast_nodes;
        }
        timer_log!(builtin_merge_start, "AST/finalize/builtin AST merge in: ");
        let _ = builtin_merge_start;

        Ok(Ast {
            nodes: self.ast,
            module_constants,
            doc_fragments,
            entry_path: entry_dir,
            const_top_level_fragments,
            rendered_path_usages: std::mem::take(&mut *self.rendered_path_usages.borrow_mut()),
            warnings: self.warnings,
        })
    }
}
