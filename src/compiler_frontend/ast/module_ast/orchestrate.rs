//! AST construction orchestration and entry point.
//!
//! WHAT: Orchestrates all AST construction passes in sequence, consuming a pre-built symbol
//! manifest through finalization, then assembles the final `Ast` output. This is the entry
//! point for building a complete typed AST from sorted headers.
//!
//! WHY: Centralizes the pass sequence so the full compilation pipeline is readable in
//! one place without implementation details. Normalization logic is extracted into the
//! `finalization` submodule for better organization.
//!
//! ## Pass Sequence
//!
//! 1. **resolve_import_bindings** — Build per-file visibility gates
//! 2. **resolve_types** — Resolve constants and struct field types
//! 3. **resolve_function_signatures** — Resolve function signatures
//! 4. **build_receiver_catalog** — Build receiver method catalog
//! 5. **emit_ast_nodes** — Lower function/template bodies
//! 6. **finalize** — Normalize templates and assemble output

use super::build_state::AstBuildState;
use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::templates::top_level_templates::{
    AstConstTopLevelFragment, AstDocFragment, collect_and_strip_comment_templates,
    collect_const_top_level_fragments,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::headers::parse_file_headers::{Header, TopLevelConstFragment};
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::paths::rendered_path_usage::RenderedPathUsage;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbol_manifest::SymbolManifest;

/// Unified AST output for all source files in one compilation unit.
pub struct Ast {
    pub nodes: Vec<crate::compiler_frontend::ast::ast_nodes::AstNode>,
    pub module_constants: Vec<Declaration>,
    pub doc_fragments: Vec<AstDocFragment>,

    // The path to the original entry point file.
    pub entry_path: InternedPath,

    /// Const top-level fragments with their runtime insertion indices.
    /// Builders merge these with the runtime fragment list returned by entry start().
    pub const_top_level_fragments: Vec<AstConstTopLevelFragment>,
    pub rendered_path_usages: Vec<RenderedPathUsage>,
    pub warnings: Vec<CompilerWarning>,
}

/// Shared dependencies/configuration required to build one module AST.
///
/// WHAT: groups the long-lived frontend services and per-build settings used across all AST passes.
/// WHY: `Ast::new` should describe its high-level inputs without a long parameter list.
pub struct AstBuildContext<'a> {
    pub host_registry: &'a HostRegistry,
    pub style_directives: &'a StyleDirectiveRegistry,
    pub string_table: &'a mut StringTable,
    pub entry_dir: InternedPath,
    pub build_profile: FrontendBuildProfile,
    pub project_path_resolver: Option<ProjectPathResolver>,
    pub path_format_config: PathStringFormatConfig,
}

impl<'a> AstBuildState<'a> {
    /// Pass 7: Assemble the final `Ast` from the accumulated build state.
    ///
    /// WHAT: strips doc-comment templates, collects const top-level fragment values,
    /// normalizes all templates for HIR consumption, and assembles the final `Ast` output.
    ///
    /// WHY: this is the final transformation before HIR lowering. Templates must be fully
    /// normalized (folded constants, render plans, complete metadata) so HIR receives
    /// semantically complete template inputs.
    pub(super) fn finalize(
        mut self,
        entry_dir: InternedPath,
        top_level_const_fragments: &[TopLevelConstFragment],
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

        let const_top_level_fragments = collect_const_top_level_fragments(
            top_level_const_fragments,
            &self.const_templates_by_path,
        )
        .map_err(|error| self.error_messages(error, string_table))?;

        self.normalize_ast_templates_for_hir(project_path_resolver, string_table)
            .map_err(|error| self.error_messages(error, string_table))?;

        let module_constants = self
            .normalize_module_constants_for_hir(project_path_resolver, string_table)
            .map_err(|error| self.error_messages(error, string_table))?;

        if !self.builtin_struct_ast_nodes.is_empty() {
            let mut ast_nodes = self.builtin_struct_ast_nodes;
            ast_nodes.extend(self.ast);
            self.ast = ast_nodes;
        }

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

impl Ast {
    /// Constructs a complete typed AST from sorted headers and a pre-built symbol manifest.
    ///
    /// WHAT: Orchestrates all AST construction passes in sequence, consuming the manifest
    /// through finalization, then assembles the final `Ast` output.
    ///
    /// WHY: Centralizes the pass sequence so the full compilation pipeline is
    /// readable in one place without implementation details. Symbol discovery is
    /// owned by the header/dependency stages and passed in via `manifest`.
    ///
    /// ## Pass Sequence
    ///
    /// 1. resolve_import_bindings   — Build per-file visibility gates
    /// 2. resolve_types             — Resolve constants and struct field types
    /// 3. resolve_function_signatures — Resolve function signatures
    /// 4. build_receiver_catalog    — Build receiver method catalog
    /// 5. emit_ast_nodes            — Lower function/template bodies
    /// 6. finalize                  — Normalize templates and assemble output
    pub fn new(
        sorted_headers: Vec<Header>,
        top_level_const_fragments: Vec<TopLevelConstFragment>,
        manifest: SymbolManifest,
        context: AstBuildContext<'_>,
    ) -> Result<Ast, CompilerMessages> {
        let AstBuildContext {
            host_registry,
            style_directives,
            string_table,
            entry_dir,
            build_profile,
            project_path_resolver,
            path_format_config,
        } = context;

        let mut state = AstBuildState::new(
            host_registry,
            style_directives,
            build_profile,
            &project_path_resolver,
            &path_format_config,
            sorted_headers.len(),
            manifest,
        );

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

        state.finalize(entry_dir, &top_level_const_fragments, string_table)
    }
}
