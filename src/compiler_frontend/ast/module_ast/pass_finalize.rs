//! Pass 7: finalization and the top-level `Ast::new` orchestrator.
//!
//! WHAT: assembles the final `Ast` output from build state — strips doc-comment templates,
//! synthesizes start-template items, prepends builtin structs, then runs all passes in order.
//! WHY: keeping the orchestrator here makes the full pass sequence readable in one place.

use super::build_state::AstBuildState;
use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
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
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;

#[allow(dead_code)] // Used only in tests
/// Exported symbol metadata captured at AST construction time.
pub struct ModuleExport {
    pub id: StringId,
    pub signature: FunctionSignature,
}

/// Unified AST output for all source files in one compilation unit.
pub struct Ast {
    pub nodes: Vec<crate::compiler_frontend::ast::ast_nodes::AstNode>,
    pub module_constants: Vec<Declaration>,
    pub doc_fragments: Vec<AstDocFragment>,

    // The path to the original entry point file.
    pub entry_path: InternedPath,

    // Exported out of the final compiled wasm module.
    // Functions must use explicit 'export' syntax Token::Export to be exported.
    // The only exception is the Main function, which is the start function of the entry point file.
    #[allow(dead_code)] // Used only in tests
    pub external_exports: Vec<ModuleExport>,
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

        if !self.builtin_struct_ast_nodes.is_empty() {
            let mut ast_nodes = self.builtin_struct_ast_nodes;
            ast_nodes.extend(self.ast);
            self.ast = ast_nodes;
        }

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

impl Ast {
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
