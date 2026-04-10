//! Compiler frontend pipeline orchestration.
//!
//! WHAT: wires tokenization, header parsing, dependency sorting, AST/HIR construction, and borrow
//! validation into the stage flow described in the compiler design overview.

pub(crate) mod ast;
pub(crate) mod headers;
pub(crate) mod source_location;
pub(crate) mod style_directives;
pub(crate) mod tokenizer;
pub(crate) mod optimizers {
    pub(crate) mod constant_folding;
}

pub(crate) mod module_dependencies;

pub(crate) mod basic_utility_functions;
pub(crate) mod builtins;
pub(crate) mod deferred_feature_diagnostics;
pub(crate) mod identifier_policy;
pub(crate) mod reserved_trait_syntax;

pub(crate) mod compiler_messages {
    pub(crate) mod compiler_dev_logging;
    pub(crate) mod compiler_errors;
    pub(crate) mod compiler_warnings;
    pub(crate) mod display_messages;
}
pub(crate) use compiler_messages::compiler_errors;
pub(crate) use compiler_messages::compiler_warnings;
pub(crate) use compiler_messages::display_messages;
pub(crate) mod datatypes;
pub(crate) mod interned_path;
pub(crate) mod string_interning;
pub(crate) mod token_scan;
pub(crate) mod traits;
pub(crate) mod type_coercion;
pub(crate) mod type_syntax;

pub(crate) mod host_functions;

pub(crate) mod hir;

pub(crate) mod analysis;
pub(crate) mod identity;
pub(crate) mod paths;
#[cfg(test)]
pub(crate) mod test_support;

use crate::compiler_frontend::analysis::borrow_checker::{
    BorrowCheckReport, check_borrows as run_borrow_checker,
};
use crate::compiler_frontend::ast::ast::{Ast, AstBuildContext};
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::headers::parse_file_headers::{
    Header, HeaderParseOptions, Headers, TopLevelTemplateItem, parse_headers_with_path_resolver,
};
use crate::compiler_frontend::hir::hir_builder::lower_module;
use crate::compiler_frontend::hir::hir_nodes::HirModule;
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::identity::SourceFileTable;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::module_dependencies::resolve_module_dependencies;
use crate::compiler_frontend::paths::path_format::{OutputPathStyle, PathStringFormatConfig};
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::newline_handling::NewlineMode;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenizeMode};
use crate::projects::settings::Config;
use std::path::{Path, PathBuf};

/// Flags change the behavior of the core `compiler_frontend` pipeline.
/// These are a future-proof way of extending the behavior of a build system or the core pipeline.
/// For the built-in CLI these are added as CLI flags, but builders can decide how to choose flags.
#[derive(PartialEq, Debug, Clone)]
pub enum Flag {
    Release, // Dev mode is default
    DisableWarnings,
    DisableTimers,
    HtmlWasm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrontendBuildProfile {
    Dev,
    Release,
}

pub struct CompilerFrontend {
    pub(crate) host_function_registry: HostRegistry,
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
        project_path_resolver: Option<ProjectPathResolver>,
        newline_mode: NewlineMode,
    ) -> Self {
        // Create a builtin host function registry with print and other host functions
        let host_function_registry = HostRegistry::new();

        // Build path formatting config from project settings.
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
            host_function_registry,
            style_directives,
            string_table,
            project_path_resolver,
            path_format_config,
            source_files: SourceFileTable::empty(),
            newline_mode,
        }
    }

    /// Attach per-module file identities built during Stage 0.
    ///
    /// WHAT: stores canonical/logical path mapping plus deterministic `FileId`s.
    /// WHY: downstream frontend stages should not reconstruct identity from path text.
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
    /// First, each file will be parsed into separate headers
    /// so every symbol they use is known before parsing their bodies.
    /// This section answers the following questions:
    /// - What has been imported from other files?
    /// - What symbols (functions, structs, consts, types, imports) exist in this file?
    /// - What types and shapes do those symbols have?
    /// - What imports do headers actually depend on?
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

        parse_headers_with_path_resolver(
            files,
            &self.host_function_registry,
            warnings,
            entry_file_path,
            HeaderParseOptions {
                entry_file_id,
                project_path_resolver: self.project_path_resolver.clone(),
                path_format_config: self.path_format_config.clone(),
            },
            &mut self.string_table,
        )
    }

    /// ---------------------------
    /// DEPENDENCY SORTING
    /// ---------------------------
    /// Now, as we parse the headers and combine the files,
    /// the types of each dependency will be known.
    /// Every dependency needed for each file should be known before its headers are parsed.
    /// This is so structs that contain imported structs can know the shape of the imports first.
    /// This section answers the following question:
    /// - In what order must the headers be defined so that symbol resolution and type-checking of bodies can proceed deterministically?
    pub fn sort_headers(
        &mut self,
        headers: Vec<Header>,
    ) -> Result<Vec<Header>, Vec<CompilerError>> {
        resolve_module_dependencies(headers, &mut self.string_table)
    }

    /// -----------------------------
    /// AST CREATION
    /// -----------------------------
    /// This assumes that the `Vec<FileTokens>` contains all dependencies for each file.
    /// The headers of each file will be parsed first, then each file will be combined into one module.
    /// The AST also provides a list of exports from the module.
    pub fn headers_to_ast(
        &mut self,
        headers: Vec<Header>,
        top_level_template_items: Vec<TopLevelTemplateItem>,
        entry_file_path: &Path,
        build_profile: FrontendBuildProfile,
    ) -> Result<Ast, CompilerMessages> {
        let interned_entry_dir = self
            .source_files
            .get_by_canonical_path(entry_file_path)
            .map_or_else(
                || InternedPath::from_path_buf(entry_file_path, &mut self.string_table),
                |identity| identity.logical_path.clone(),
            );

        Ast::new(
            headers,
            top_level_template_items,
            AstBuildContext {
                host_registry: &self.host_function_registry,
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
    /// Generate HIR from AST nodes, making control flow explicit and preserving typed
    /// expression trees where appropriate for backend-facing semantic lowering.
    ///
    /// HIR does not own general template parsing/folding. A project path resolver is still
    /// required here for a narrow transitional constant-template fallback during lowering.
    pub fn generate_hir(&mut self, ast: Ast) -> Result<HirModule, CompilerMessages> {
        let Some(project_path_resolver) = self.project_path_resolver.clone() else {
            return Err(CompilerMessages::from_error_ref(
                CompilerError::compiler_error(
                    "HIR generation requires a project path resolver for transitional constant-template lowering.",
                ),
                &self.string_table,
            ));
        };
        let hir_module = lower_module(
            ast,
            &mut self.string_table,
            self.path_format_config.clone(),
            project_path_resolver,
        )?;
        Ok(hir_module)
    }

    // ------------------------------
    //  BORROW CHECKING AND ANALYSIS
    // ------------------------------
    // Borrow validation runs after HIR construction. The borrow checker enforces
    // language rules that must be consistent across all backends.
    pub fn check_borrows(
        &self,
        hir_module: &HirModule,
    ) -> Result<BorrowCheckReport, CompilerMessages> {
        match run_borrow_checker(hir_module, &self.host_function_registry, &self.string_table) {
            Ok(report) => Ok(report),
            Err(error) => Err(CompilerMessages::from_error_ref(error, &self.string_table)),
        }
    }

    // Planned: dedicated last-use analysis pass (memory-management design "Last-Use Analysis").
    // Provides a list of places that possible_drops can be inserted for heap managed values
    // pub fn last_use_analysis()

    // Other phases (might be wrapped into the previous phase)
    // - Determine which functions can be statically dispatched with guaranteed drops or no drops at all
    // - Determine inlining opportunities
}
