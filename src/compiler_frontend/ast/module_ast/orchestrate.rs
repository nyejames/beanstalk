//! AST construction orchestration and entry point.
//!
//! WHAT: Orchestrates all AST construction passes in sequence, from symbol registration
//! through finalization, then assembles the final `Ast` output. This is the entry point
//! for building a complete typed AST from sorted headers.
//!
//! WHY: Centralizes the pass sequence so the full compilation pipeline is readable in
//! one place without implementation details. Normalization logic is extracted into the
//! `finalization` submodule for better organization.
//!
//! ## Pass Sequence
//!
//! 1. **collect_declarations** — Register all symbols module-wide
//! 2. **resolve_import_bindings** — Build per-file visibility gates
//! 3. **resolve_types** — Resolve constants and struct field types
//! 4. **resolve_function_signatures** — Resolve function signatures
//! 5. **build_receiver_catalog** — Build receiver method catalog
//! 6. **emit_ast_nodes** — Lower function/template bodies
//! 7. **finalize** — Normalize templates and assemble output

use super::build_state::AstBuildState;
use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::templates::top_level_templates::{
    AstDocFragment, AstStartTemplateItem, collect_and_strip_comment_templates,
    synthesize_start_template_items,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::headers::parse_file_headers::{Header, TopLevelTemplateItem};
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::paths::rendered_path_usage::RenderedPathUsage;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;

/// Unified AST output for all source files in one compilation unit.
pub struct Ast {
    pub nodes: Vec<crate::compiler_frontend::ast::ast_nodes::AstNode>,
    pub module_constants: Vec<Declaration>,
    pub doc_fragments: Vec<AstDocFragment>,

    // The path to the original entry point file.
    pub entry_path: InternedPath,

    pub start_template_items: Vec<AstStartTemplateItem>,
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
    /// Pass 7: Assemble the final `Ast` from accumulated build state.
    ///
    /// WHAT: Strips doc-comment templates, synthesizes start-template items,
    /// normalizes all templates for HIR consumption, and assembles the final
    /// `Ast` output with all metadata.
    ///
    /// WHY: This is the final transformation before HIR lowering. Templates
    /// must be fully normalized (folded constants, render plans, complete
    /// metadata) so HIR receives semantically complete template inputs.
    pub(super) fn finalize(
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
            start_template_items,
            rendered_path_usages: std::mem::take(&mut *self.rendered_path_usages.borrow_mut()),
            warnings: self.warnings,
        })
    }
}

impl Ast {
    /// Constructs a complete typed AST from sorted headers.
    ///
    /// WHAT: Orchestrates all AST construction passes in sequence, from symbol
    /// registration through finalization, then assembles the final `Ast` output.
    ///
    /// WHY: Centralizes the pass sequence so the full compilation pipeline is
    /// readable in one place without implementation details.
    ///
    /// ## Pass Sequence
    ///
    /// 1. collect_declarations      — Register all symbols module-wide
    /// 2. resolve_import_bindings   — Build per-file visibility gates
    /// 3. resolve_types             — Resolve constants and struct field types
    /// 4. resolve_function_signatures — Resolve function signatures
    /// 5. build_receiver_catalog    — Build receiver method catalog
    /// 6. emit_ast_nodes            — Lower function/template bodies
    /// 7. finalize                  — Normalize templates and assemble output
    pub fn new(
        sorted_headers: Vec<Header>,
        top_level_template_items: Vec<TopLevelTemplateItem>,
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
        );

        state.collect_declarations(&sorted_headers, string_table)?;

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
