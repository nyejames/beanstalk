//! Compiler frontend pipeline orchestration.
//!
//! WHAT: wires tokenization, header parsing, dependency sorting, AST/HIR construction, and borrow
//! validation into the stage flow described in the compiler design overview.

use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::analysis::borrow_checker::{
    BorrowCheckReport, check_borrows as run_borrow_checker,
};
use crate::compiler_frontend::ast::{Ast, AstBuildContext, AstBuildInput};
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, DiagnosticBag};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::headers::parse_file_headers::{
    FileFrontendPrepareError, FileFrontendPrepareOutput, HeaderParseOptions, Headers,
    parse_file_headers_with_table,
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
}

/// Shared immutable inputs used while one source file is prepared against a local string table.
///
/// WHAT: collects the frontend-owned registries and entry-file identity needed by tokenization and
/// header parsing.
/// WHY: parallel file preparation passes this context by shared reference to Rayon workers without
/// giving them mutable access to the module-global string table.
pub(crate) struct FrontendFilePrepareContext<'a> {
    pub(crate) source_files: &'a SourceFileTable,
    pub(crate) style_directives: &'a StyleDirectiveRegistry,
    pub(crate) external_package_registry: &'a ExternalPackageRegistry,
    pub(crate) entry_file_path: &'a Path,
    pub(crate) options: &'a HeaderParseOptions,
}

/// Per-file source inputs and numbering offsets for local frontend preparation.
///
/// WHAT: keeps the source text/path and synthetic-fragment offsets together for one worker item.
/// WHY: grouping these inputs keeps the preparation API explicit without a broad argument list.
pub(crate) struct FrontendFilePrepareInput<'a> {
    pub(crate) source_code: &'a str,
    pub(crate) source_path: &'a PathBuf,
    pub(crate) const_template_offset: usize,
    pub(crate) runtime_fragment_offset: usize,
}

impl CompilerFrontend {
    pub(crate) fn new(
        project_config: &Config,
        string_table: StringTable,
        style_directives: StyleDirectiveRegistry,
        external_package_registry: ExternalPackageRegistry,
        project_path_resolver: Option<ProjectPathResolver>,
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
        }
    }

    /// Attach per-module file identities built during Stage 0.
    pub fn set_source_files(&mut self, source_files: SourceFileTable) {
        self.source_files = source_files;
    }

    // -----------------------------
    //  TOKENIZER
    // -----------------------------
    // Test-only stage entrypoint for suites that intentionally exercise tokenization before
    // header preparation. Production module compilation uses parallel local-table preparation.
    #[cfg(test)]
    #[allow(clippy::result_large_err)]
    pub(crate) fn source_to_tokens(
        &mut self,
        source_code: &str,
        module_path: &PathBuf,
        tokenizer_mode: TokenizeMode,
    ) -> Result<FileTokens, CompilerDiagnostic> {
        let source_files = &self.source_files;
        let style_directives = &self.style_directives;
        let string_table = &mut self.string_table;

        Self::tokenize_source(
            source_files,
            style_directives,
            source_code,
            module_path,
            tokenizer_mode,
            string_table,
        )
    }

    /// Tokenize source text against an explicitly supplied string table.
    ///
    /// WHAT: resolves source file identity and runs tokenization without assuming ownership of the
    ///       string table. This allows per-file tokenization against local string-table forks.
    /// WHY: parallel and fork-based frontend preparation need to tokenize independently before
    ///      merging deltas back into the module/global table.
    #[allow(clippy::result_large_err)]
    pub(crate) fn tokenize_source(
        source_files: &SourceFileTable,
        style_directives: &StyleDirectiveRegistry,
        source_code: &str,
        module_path: &PathBuf,
        tokenizer_mode: TokenizeMode,
        string_table: &mut StringTable,
    ) -> Result<FileTokens, CompilerDiagnostic> {
        let (logical_path, file_id, canonical_os_path) =
            match source_files.get_by_canonical_path(module_path.as_path()) {
                Some(identity) => (
                    identity.logical_path.clone(),
                    Some(identity.file_id),
                    Some(identity.canonical_os_path.clone()),
                ),
                None => (
                    InternedPath::from_path_buf(module_path, string_table),
                    None,
                    Some(module_path.to_owned()),
                ),
            };

        let mut tokens = tokenize(
            source_code,
            &logical_path,
            tokenizer_mode,
            style_directives,
            string_table,
            file_id,
        )?;
        tokens.canonical_os_path = canonical_os_path;
        Ok(tokens)
    }

    /// Tokenize and header-parse one source file against a caller-provided local string table.
    ///
    /// WHAT: this is the core per-file preparation logic without merge/remap, so callers can run
    ///       tokenization and header parsing in parallel before deterministically merging results.
    /// WHY: parallel frontend preparation needs each worker to own its local table without shared
    ///      mutable access to the module-global table.
    pub(crate) fn prepare_file_frontend_local(
        context: &FrontendFilePrepareContext<'_>,
        input: FrontendFilePrepareInput<'_>,
        local_string_table: &mut StringTable,
    ) -> Result<FileFrontendPrepareOutput, FileFrontendPrepareError> {
        let tokenization = Self::tokenize_source(
            context.source_files,
            context.style_directives,
            input.source_code,
            input.source_path,
            TokenizeMode::Normal,
            local_string_table,
        );

        let mut file_tokens = match tokenization {
            Ok(tokens) => tokens,
            Err(diagnostic) => {
                return Err(FileFrontendPrepareError {
                    warnings: Vec::new(),
                    diagnostic: Box::new(diagnostic),
                });
            }
        };

        parse_file_headers_with_table(
            &mut file_tokens,
            context.entry_file_path,
            context.options,
            context.external_package_registry,
            local_string_table,
            input.const_template_offset,
            input.runtime_fragment_offset,
        )
    }

    // ---------------------------
    //  DEPENDENCY SORTING
    // ---------------------------
    pub fn sort_headers(&mut self, headers: Headers) -> Result<SortedHeaders, DiagnosticBag> {
        resolve_module_dependencies(headers, &mut self.string_table)
    }

    // -----------------------------
    //  AST CREATION
    // -----------------------------
    pub fn headers_to_ast(
        &mut self,
        sorted: SortedHeaders,
        entry_file_path: &Path,
        build_profile: FrontendBuildProfile,
    ) -> Result<Ast, CompilerMessages> {
        let interned_entry_file = self
            .source_files
            .get_by_canonical_path(entry_file_path)
            .map_or_else(
                || InternedPath::from_path_buf(entry_file_path, &mut self.string_table),
                |identity| identity.logical_path.clone(),
            );

        Ast::new(
            AstBuildInput {
                headers: sorted.headers,
                module_symbols: sorted.module_symbols,
                import_environment: sorted.import_environment,
                top_level_const_fragments: sorted.top_level_const_fragments,
            },
            AstBuildContext {
                external_package_registry: &self.external_package_registry,
                style_directives: &self.style_directives,
                string_table: &mut self.string_table,
                entry_dir: interned_entry_file,
                build_profile,
                project_path_resolver: self.project_path_resolver.clone(),
                path_format_config: self.path_format_config.clone(),
            },
        )
    }

    // -----------------------------
    //  HIR GENERATION
    // -----------------------------
    pub fn generate_hir(
        &mut self,
        ast: Ast,
    ) -> Result<(HirModule, TypeEnvironment), CompilerMessages> {
        lower_module(ast, &mut self.string_table, self.path_format_config.clone())
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
            Err(error) => match error.into_diagnostic_or_infrastructure() {
                Ok(diagnostic) => Err(CompilerMessages::from_diagnostic_ref(
                    diagnostic,
                    &self.string_table,
                )),
                Err(error) => Err(CompilerMessages::from_error_ref(error, &self.string_table)),
            },
        }
    }
}
