//! Compiler frontend pipeline orchestration.
//!
//! WHAT: wires tokenization, header parsing, dependency sorting, AST/HIR construction, and borrow
//! validation into the stage flow described in the compiler design overview.

use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::analysis::borrow_checker::{
    BorrowCheckReport, check_borrows as run_borrow_checker,
};
use crate::compiler_frontend::arena::FrontendArenaCapacityEstimate;
use crate::compiler_frontend::ast::{Ast, AstBuildContext, AstBuildInput};
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, DiagnosticBag};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::headers::beandown_prepare::prepare_beandown_file;
use crate::compiler_frontend::headers::parse_file_headers::{
    FileFrontendPrepareError, FileFrontendPrepareOutput, HeaderParseOptions, Headers,
    parse_file_headers_with_table,
};
use crate::compiler_frontend::headers::plain_markdown_prepare::{
    PlainMarkdownPrepareInput, prepare_plain_markdown_file,
};
use crate::compiler_frontend::hir::hir_builder::lower_module;
use crate::compiler_frontend::hir::module::HirModule;
use crate::compiler_frontend::module_dependencies::{SortedHeaders, resolve_module_dependencies};
use crate::compiler_frontend::paths::path_format::{OutputPathStyle, PathStringFormatConfig};
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::identity::{FileId, SourceFileTable};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenizerEntryMode};
use crate::libraries::SourceFileKind;
use crate::projects::settings::Config;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct CompilerFrontend {
    pub(crate) external_package_registry: Arc<ExternalPackageRegistry>,
    pub(crate) style_directives: StyleDirectiveRegistry,
    pub(crate) string_table: StringTable,
    pub(crate) project_path_resolver: Option<ProjectPathResolver>,
    pub(crate) path_format_config: PathStringFormatConfig,
    pub(crate) template_const_loop_iteration_limit: usize,
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
    pub(crate) source_kind: SourceFileKind,
    pub(crate) const_template_offset: usize,
    pub(crate) runtime_fragment_offset: usize,
}

/// Stable identity facts for one source file as seen by the frontend.
///
/// WHAT: bundles the interned logical path, explicit file ID, and canonical OS path that
///       tokenization and non-tokenized preparation both need.
/// WHY: keeps source-identity lookup in one place so Markdown preparation can reuse the same
///      identity path as tokenized files without duplicating the `SourceFileTable` fallback logic.
struct FrontendSourceFileIdentity {
    logical_path: InternedPath,
    file_id: Option<FileId>,
    canonical_os_path: Option<PathBuf>,
}

/// Look up frontend identity for a source path.
///
/// WHAT: returns the logical interned path, stable file ID, and canonical OS path for one file.
/// WHY: tokenized Beanstalk/Beandown files and non-tokenized Markdown files must share the same
///      source identity so downstream stages treat them as ordinary module members.
fn source_file_identity(
    source_files: &SourceFileTable,
    source_path: &PathBuf,
    string_table: &mut StringTable,
) -> FrontendSourceFileIdentity {
    match source_files.get_by_canonical_path(source_path.as_path()) {
        Some(identity) => FrontendSourceFileIdentity {
            logical_path: identity.logical_path.clone(),
            file_id: Some(identity.file_id),
            canonical_os_path: Some(identity.canonical_os_path.clone()),
        },
        None => FrontendSourceFileIdentity {
            logical_path: InternedPath::from_path_buf(source_path, string_table),
            file_id: None,
            canonical_os_path: Some(source_path.to_owned()),
        },
    }
}

impl CompilerFrontend {
    pub(crate) fn new(
        project_config: &Config,
        string_table: StringTable,
        style_directives: StyleDirectiveRegistry,
        external_package_registry: Arc<ExternalPackageRegistry>,
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
            template_const_loop_iteration_limit: project_config.template_const_loop_iteration_limit,
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
        tokenizer_entry_mode: TokenizerEntryMode,
        string_table: &mut StringTable,
    ) -> Result<FileTokens, CompilerDiagnostic> {
        let identity = source_file_identity(source_files, module_path, string_table);

        let mut tokens = tokenize(
            source_code,
            &identity.logical_path,
            tokenizer_entry_mode,
            style_directives,
            string_table,
            identity.file_id,
        )?;
        tokens.canonical_os_path = identity.canonical_os_path;
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
        match input.source_kind {
            SourceFileKind::PlainMarkdown => {
                let identity = source_file_identity(
                    context.source_files,
                    input.source_path,
                    local_string_table,
                );
                Ok(prepare_plain_markdown_file(
                    PlainMarkdownPrepareInput {
                        source_code: input.source_code,
                        source_file: identity.logical_path,
                        file_id: identity.file_id,
                        canonical_os_path: identity.canonical_os_path,
                    },
                    local_string_table,
                ))
            }
            SourceFileKind::Beanstalk | SourceFileKind::Beandown => {
                let tokenizer_entry_mode =
                    match TokenizerEntryMode::for_source_file_kind(input.source_kind) {
                        Some(mode) => mode,
                        None => unreachable!("Beanstalk and Beandown have tokenizer entry modes"),
                    };

                let tokenization = Self::tokenize_source(
                    context.source_files,
                    context.style_directives,
                    input.source_code,
                    input.source_path,
                    tokenizer_entry_mode,
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

                if input.source_kind == SourceFileKind::Beanstalk {
                    parse_file_headers_with_table(
                        &mut file_tokens,
                        context.entry_file_path,
                        context.options,
                        context.external_package_registry,
                        local_string_table,
                        input.const_template_offset,
                        input.runtime_fragment_offset,
                    )
                } else {
                    Ok(prepare_beandown_file(file_tokens, local_string_table))
                }
            }
        }
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
        capacity_estimate: FrontendArenaCapacityEstimate,
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
                external_package_registry: Arc::clone(&self.external_package_registry),
                style_directives: &self.style_directives,
                string_table: &mut self.string_table,
                entry_dir: interned_entry_file,
                build_profile,
                project_path_resolver: self.project_path_resolver.clone(),
                path_format_config: self.path_format_config.clone(),
                template_const_loop_iteration_limit: self.template_const_loop_iteration_limit,
                capacity_estimate,
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
            self.external_package_registry.as_ref(),
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
