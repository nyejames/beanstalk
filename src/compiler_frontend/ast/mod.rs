//! AST stage — Stage 4 of the frontend pipeline.
//!
//! WHAT: constructs a typed AST from pre-sorted, pre-shaped top-level headers produced by
//! Stages 2–3 (header parsing and dependency sorting).
//!
//! WHY: separating executable-body parsing from header discovery keeps the pipeline
//! deterministic and lets AST focus on semantic resolution, type checking, and body
//! lowering without rediscovering top-level symbols.
//!
//! ## Ownership contract
//!
//! **Header parsing + dependency sorting own:**
//! - top-level declaration discovery
//! - top-level declaration shell parsing
//! - strict top-level dependency ordering
//! - appending implicit `start` header last (outside dependency graph)
//!
//! **AST owns:**
//! - lowering sorted header payloads (no top-level shell reparsing)
//! - resolving and validating top-level types/symbols
//! - parsing function bodies and other executable/body-local declarations
//! - template composition, compile-time folding, and runtime render-plan preparation
//!
//! ## Pipeline (6 passes)
//!
//! 1. `pass_import_bindings` — build per-file visibility gates from header import data
//! 2. `pass_type_resolution` — resolve constant values and struct field types
//! 3. `pass_function_signatures` — resolve function parameter/return types
//! 4. `build_receiver_catalog` — index receiver methods from resolved signatures
//! 5. `pass_emit_nodes` — lower function/template bodies into typed AST nodes
//! 6. `finalize` — normalize templates, assemble [`Ast`] output
//!
//! Entry point: [`Ast::new`].

// Internal AST implementation modules.
//
// `module_ast` contains the build state, pass methods, and scope-context helpers that
// implement the 6-pass pipeline. The rest of the AST surface is split by concern
// (expressions, statements, templates, field access, etc.).
pub(crate) mod ast_nodes;
mod import_bindings;
mod module_ast;
mod receiver_methods;
mod type_resolution;

pub(crate) mod expressions {
    pub(crate) mod call_argument;
    pub(crate) mod call_validation;
    pub(crate) mod eval_expression;
    pub(crate) mod expression;
    pub(crate) mod function_calls;
    pub(crate) mod mutation;
    pub(crate) mod parse_expression;
    pub(crate) mod parse_expression_dispatch;
    pub(crate) mod parse_expression_identifiers;
    pub(crate) mod parse_expression_literals;
    pub(crate) mod parse_expression_places;
    pub(crate) mod parse_expression_templates;
    pub(crate) mod struct_instance;
}

pub(crate) mod statements {
    pub(crate) mod body_dispatch;
    pub(crate) mod body_expr_stmt;
    pub(crate) mod body_return;
    pub(crate) mod body_symbol;
    pub(crate) mod branching;
    pub(crate) mod collections;
    pub(crate) mod condition_validation;
    pub(crate) mod declarations;
    pub(crate) mod functions;
    pub(crate) mod loops;
    pub(crate) mod multi_bind;
    pub(crate) mod result_handling;
}

mod field_access;
mod place_access;
pub(crate) mod templates;

// Public surface — minimal API consumed by later compiler stages.
//
// WHY: the AST module should expose one obvious entry surface. Internal helpers,
// pass implementations, and parser submodules stay private to `ast/`.
pub use module_ast::scope_context::{ContextKind, ScopeContext, TopLevelDeclarationIndex};
pub use templates::top_level_templates::AstDocFragment;
pub use templates::top_level_templates::AstDocFragmentKind;

// Imports for the AST entry point and body-parsing helper.
use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::module_ast::build_state::AstBuildState;
use crate::compiler_frontend::ast::statements::body_dispatch::parse_function_body_statements;
use crate::compiler_frontend::ast::templates::top_level_templates::{
    AstConstTopLevelFragment, collect_and_strip_comment_templates,
    collect_const_top_level_fragments,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::headers::module_symbols::ModuleSymbols;
use crate::compiler_frontend::headers::parse_file_headers::{Header, TopLevelConstFragment};
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::paths::rendered_path_usage::RenderedPathUsage;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::FileTokens;
use crate::timer_log;
use std::time::Instant;

/// Unified AST output for all source files in one compilation unit.
///
/// WHAT: the fully resolved, typed AST produced by Stage 4, ready for HIR lowering.
/// WHY: one container keeps the pipeline contract explicit — HIR receives exactly this
/// struct and nothing else from the AST stage.
pub struct Ast {
    pub nodes: Vec<AstNode>,
    pub module_constants: Vec<crate::compiler_frontend::ast::ast_nodes::Declaration>,
    pub doc_fragments: Vec<AstDocFragment>,

    /// The path to the original entry point file.
    pub entry_path: InternedPath,

    /// Const top-level fragments with their runtime insertion indices.
    /// Builders merge these with the runtime fragment list returned by entry `start()`.
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

impl Ast {
    /// Constructs a complete typed AST from sorted headers and a pre-built symbol manifest.
    ///
    /// WHAT: Orchestrates all AST construction passes in sequence, consuming the manifest
    /// through finalization, then assembles the final [`Ast`] output.
    ///
    /// WHY: Centralizes the pass sequence so the full compilation pipeline is readable in
    /// one place without implementation details. Symbol discovery is owned by the header/
    /// dependency stages and passed in via `module_symbols`.
    pub fn new(
        sorted_headers: Vec<Header>,
        top_level_const_fragments: Vec<TopLevelConstFragment>,
        module_symbols: ModuleSymbols,
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
            module_symbols,
        );

        let import_bindings_start = Instant::now();
        let file_import_bindings = state.resolve_import_bindings(string_table)?;
        timer_log!(import_bindings_start, "AST/import bindings resolved in: ");
        let _ = import_bindings_start;

        let type_resolution_start = Instant::now();
        state.resolve_types(&sorted_headers, &file_import_bindings, string_table)?;
        timer_log!(type_resolution_start, "AST/type resolution completed in:");
        let _ = type_resolution_start;

        let function_signatures_start = Instant::now();
        state.resolve_function_signatures(&sorted_headers, &file_import_bindings, string_table)?;
        timer_log!(
            function_signatures_start,
            "AST/function signatures resolved in:"
        );
        let _ = function_signatures_start;

        let receiver_catalog_start = Instant::now();
        let receiver_methods = state.build_receiver_catalog(&sorted_headers, string_table)?;
        timer_log!(receiver_catalog_start, "AST/receiver catalog built in: ");
        let _ = receiver_catalog_start;

        let node_emission_start = Instant::now();
        state.emit_ast_nodes(
            sorted_headers,
            &file_import_bindings,
            &receiver_methods,
            string_table,
        )?;
        timer_log!(node_emission_start, "AST/node emission completed in: ");
        let _ = node_emission_start;

        let finalization_start = Instant::now();
        let ast = state.finalize(entry_dir, &top_level_const_fragments, string_table)?;
        timer_log!(finalization_start, "AST/finalization completed in: ");
        let _ = finalization_start;

        Ok(ast)
    }
}

impl<'a> AstBuildState<'a> {
    /// Pass 7: Assemble the final [`Ast`] from the accumulated build state.
    ///
    /// WHAT: strips doc-comment templates, collects const top-level fragment values,
    /// normalizes all templates for HIR consumption, and assembles the final [`Ast`] output.
    ///
    /// WHY: this is the final transformation before HIR lowering. Templates must be fully
    /// normalized (folded constants, render plans, complete metadata) so HIR receives
    /// semantically complete template inputs.
    fn finalize(
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

// WHAT: public(crate) entrypoint for function/start-function body parsing.
// WHY: callers should import one obvious `ast`-root function while detailed statement parsing
// lives in focused helper modules.
pub(crate) fn function_body_to_ast(
    token_stream: &mut FileTokens,
    context: ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<Vec<AstNode>, CompilerError> {
    parse_function_body_statements(token_stream, context, warnings, string_table)
}

#[cfg(test)]
#[path = "tests/parser_error_recovery_tests.rs"]
mod parser_error_recovery_tests;
