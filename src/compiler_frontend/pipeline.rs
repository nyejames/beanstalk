//! Compiler frontend pipeline orchestration.
//!
//! WHAT: wires tokenization, header parsing, dependency sorting, AST/HIR construction, and borrow
//! validation into the stage flow described in the compiler design overview.

use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::analysis::borrow_checker::{
    BorrowCheckReport, check_borrows as run_borrow_checker,
};
use crate::compiler_frontend::ast::{Ast, AstBuildContext};
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::headers::parse_file_headers::{
    HeaderParseOptions, Headers, parse_headers,
};
use crate::compiler_frontend::hir::hir_builder::lower_module;
use crate::compiler_frontend::hir::module::HirModule;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::module_dependencies::{SortedHeaders, resolve_module_dependencies};
use crate::compiler_frontend::paths::path_format::{OutputPathStyle, PathStringFormatConfig};
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::identity::SourceFileTable;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::newline_handling::NewlineMode;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenizeMode};
use crate::projects::settings::Config;
use std::path::{Path, PathBuf};

pub struct CompilerFrontend {
    pub(crate) external_package_registry: ExternalPackageRegistry,
    pub(crate) style_directives: StyleDirectiveRegistry,
    pub(crate) string_table: StringTable,
    pub(crate) project_path_resolver: Option<ProjectPathResolver>,
    pub(crate) path_format_config: PathStringFormatConfig,
    pub(crate) source_files: SourceFileTable,
    pub(crate) newline_mode: NewlineMode,
}

impl CompilerFrontend {
    pub(crate) fn new(
        project_config: &Config,
        string_table: StringTable,
        style_directives: StyleDirectiveRegistry,
        external_package_registry: ExternalPackageRegistry,
        project_path_resolver: Option<ProjectPathResolver>,
        newline_mode: NewlineMode,
    ) -> Self {
        let origin = project_config
            .settings
            .get("origin")
            .cloned()
            .unwrap_or_else(|| String::from("/"));
        let path_format_config = PathStringFormatConfig {
            origin,
            output_style: OutputPathStyle::Portable,
        };

        Self {
            external_package_registry,
            style_directives,
            string_table,
            project_path_resolver,
            path_format_config,
            source_files: SourceFileTable::empty(),
            newline_mode,
        }
    }

    /// Attach per-module file identities built during Stage 0.
    pub fn set_source_files(&mut self, source_files: SourceFileTable) {
        self.source_files = source_files;
    }

    /// -----------------------------
    /// TOKENIZER
    /// -----------------------------
    pub fn source_to_tokens(
        &mut self,
        source_code: &str,
        module_path: &PathBuf,
        tokenizer_mode: TokenizeMode,
    ) -> Result<FileTokens, CompilerError> {
        let (logical_path, file_id, canonical_os_path) = match self
            .source_files
            .get_by_canonical_path(module_path.as_path())
        {
            Some(identity) => (
                identity.logical_path.clone(),
                Some(identity.file_id),
                Some(identity.canonical_os_path.clone()),
            ),
            None => (
                InternedPath::from_path_buf(module_path, &mut self.string_table),
                None,
                Some(module_path.to_owned()),
            ),
        };

        let mut tokens = tokenize(
            source_code,
            &logical_path,
            tokenizer_mode,
            self.newline_mode,
            &self.style_directives,
            &mut self.string_table,
            file_id,
        )?;
        tokens.canonical_os_path = canonical_os_path;
        Ok(tokens)
    }

    /// ---------------------------
    /// HEADER PARSING
    /// ---------------------------
    pub fn tokens_to_headers(
        &mut self,
        files: Vec<FileTokens>,
        warnings: &mut Vec<CompilerWarning>,
        entry_file_path: &Path,
    ) -> Result<Headers, Vec<CompilerError>> {
        let entry_file_id = self
            .source_files
            .get_by_canonical_path(entry_file_path)
            .map(|identity| identity.file_id);

        parse_headers(
            files,
            &self.external_package_registry,
            warnings,
            entry_file_path,
            HeaderParseOptions {
                entry_file_id,
                project_path_resolver: self.project_path_resolver.clone(),
                path_format_config: self.path_format_config.clone(),
                style_directives: self.style_directives.clone(),
            },
            &mut self.string_table,
        )
    }

    /// ---------------------------
    /// DEPENDENCY SORTING
    /// ---------------------------
    pub fn sort_headers(&mut self, headers: Headers) -> Result<SortedHeaders, Vec<CompilerError>> {
        resolve_module_dependencies(headers, &mut self.string_table)
    }

    /// -----------------------------
    /// AST CREATION
    /// -----------------------------
    pub fn headers_to_ast(
        &mut self,
        sorted: SortedHeaders,
        entry_file_path: &Path,
        build_profile: FrontendBuildProfile,
    ) -> Result<Ast, CompilerMessages> {
        let SortedHeaders {
            headers,
            top_level_const_fragments,
            entry_runtime_fragment_count: _,
            module_symbols,
        } = sorted;

        let interned_entry_dir = self
            .source_files
            .get_by_canonical_path(entry_file_path)
            .map_or_else(
                || InternedPath::from_path_buf(entry_file_path, &mut self.string_table),
                |identity| identity.logical_path.clone(),
            );

        Ast::new(
            headers,
            top_level_const_fragments,
            module_symbols,
            AstBuildContext {
                external_package_registry: &self.external_package_registry,
                style_directives: &self.style_directives,
                string_table: &mut self.string_table,
                entry_dir: interned_entry_dir,
                build_profile,
                project_path_resolver: self.project_path_resolver.clone(),
                path_format_config: self.path_format_config.clone(),
            },
        )
    }

    /// -----------------------------
    /// HIR GENERATION
    /// -----------------------------
    pub fn generate_hir(&mut self, ast: Ast) -> Result<HirModule, CompilerMessages> {
        let hir_module =
            lower_module(ast, &mut self.string_table, self.path_format_config.clone())?;
        Ok(hir_module)
    }

    // ------------------------------
    //  BORROW CHECKING AND ANALYSIS
    // ------------------------------
    pub fn check_borrows(
        &self,
        hir_module: &HirModule,
    ) -> Result<BorrowCheckReport, CompilerMessages> {
        match run_borrow_checker(
            hir_module,
            &self.external_package_registry,
            &self.string_table,
        ) {
            Ok(report) => Ok(report),
            Err(error) => Err(CompilerMessages::from_error_ref(error, &self.string_table)),
        }
    }

    // Planned: dedicated last-use analysis pass (memory-management-design.md §Last-Use Analysis).
    // pub fn last_use_analysis(&self, hir_module: &HirModule) -> LastUseReport

    // Planned: static dispatch analysis (memory-management-design.md §Unified ABI).
    // pub fn static_dispatch_analysis(&self, hir_module: &HirModule) -> StaticDispatchReport

    // Planned: inlining analysis.
    // pub fn static_dispatch_analysis(&self, hir_module: &HirModule) -> StaticDispatchReport
}
