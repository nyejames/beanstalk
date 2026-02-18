pub(crate) mod ast;
pub(crate) mod headers;
pub(crate) mod tokenizer;
pub(crate) mod optimizers {
    pub(crate) mod constant_folding;
}

pub(crate) mod module_dependencies;

pub(crate) mod basic_utility_functions;

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
pub(crate) mod traits;

pub(crate) mod host_functions {}

pub(crate) mod hir;

pub(crate) mod borrow_checker;

use crate::backends::function_registry::HostRegistry;
use crate::compiler_frontend::ast::ast::Ast;
use crate::compiler_frontend::borrow_checker::{BorrowCheckOutcome, BorrowChecker};
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::headers::parse_file_headers::{Header, parse_headers};
use crate::compiler_frontend::hir::build_hir::HirBuilderContext;
use crate::compiler_frontend::hir::hir_nodes::HirModule;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::module_dependencies::resolve_module_dependencies;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokenizer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenizeMode};
use crate::projects::settings::Config;
use std::collections::HashSet;
use std::path::PathBuf;

pub struct OutputModule {
    pub(crate) imports: HashSet<PathBuf>,
    pub(crate) source_path: PathBuf,
}

impl OutputModule {
    pub(crate) fn new(source_path: PathBuf, imports: HashSet<PathBuf>) -> Self {
        OutputModule {
            imports,
            source_path,
        }
    }
}

/// Flags change the behavior of the core compiler_frontend pipeline.
/// These are a future-proof way of extending the behavior of a build system or the core pipeline
/// For the built-in CLI these are added as cli flags, but builders can decide how to choose flags
#[derive(PartialEq, Debug, Clone)]
pub enum Flag {
    Release, // Dev mode is default
    DisableWarnings,
    DisableTimers,
}

pub struct CompilerFrontend<'a> {
    pub(crate) project_config: &'a Config,
    pub(crate) host_function_registry: HostRegistry,
    pub(crate) string_table: StringTable,
}

/// Special reserved name for top-level templates
pub const TOP_LEVEL_TEMPLATE_NAME: &str = "#template";

impl<'a> CompilerFrontend<'a> {
    pub(crate) fn new(project_config: &'a Config, mut string_table: StringTable) -> Self {
        // Create a builtin host function registry with print and other host functions
        let host_function_registry = HostRegistry::new(&mut string_table);

        Self {
            project_config,
            host_function_registry,
            string_table,
        }
    }

    /// -----------------------------
    ///          TOKENIZER
    /// -----------------------------
    pub fn source_to_tokens(
        &mut self,
        source_code: &str,
        module_path: &PathBuf,
        tokenizer_mode: TokenizeMode,
    ) -> Result<FileTokens, CompilerError> {
        let interned_path = &InternedPath::from_path_buf(module_path, &mut self.string_table);

        match tokenize(
            source_code,
            interned_path,
            tokenizer_mode,
            &mut self.string_table,
        ) {
            Ok(tokens) => Ok(tokens),
            Err(e) => Err(e.with_file_path(module_path.to_owned())),
        }
    }

    /// ---------------------------
    ///       HEADER PARSING
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
    ) -> Result<Vec<Header>, Vec<CompilerError>> {
        parse_headers(
            files,
            &self.host_function_registry,
            warnings,
            &self.project_config.entry_dir,
            &mut self.string_table,
        )
    }

    /// ---------------------------
    ///     DEPENDENCY SORTING
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
    ///         AST CREATION
    /// -----------------------------
    /// This assumes that the vec of FileTokens contains all dependencies for each file.
    /// The headers of each file will be parsed first, then each file will be combined into one module.
    /// The AST also provides a list of exports from the module.
    pub fn headers_to_ast(&mut self, module_tokens: Vec<Header>) -> Result<Ast, CompilerMessages> {
        let interned_entry_dir =
            InternedPath::from_path_buf(&self.project_config.entry_dir, &mut self.string_table);

        Ast::new(
            module_tokens,
            &self.host_function_registry,
            &mut self.string_table,
            interned_entry_dir,
        )
    }

    /// -----------------------------
    ///         HIR GENERATION
    /// -----------------------------
    /// Generate HIR from AST nodes, linearizing expressions and creating
    /// a place-based representation suitable for borrow checking analysis.
    pub fn generate_hir(&mut self, ast: Ast) -> Result<HirModule, CompilerMessages> {
        let ctx = HirBuilderContext::new(&mut self.string_table);
        let hir_module = ctx.build_hir_module(ast)?;

        // Display HIR if the show_hir feature is enabled
        #[cfg(feature = "show_hir")]
        {
            println!("{}", hir_module.debug_string(&self.string_table));
        }

        Ok(hir_module)
    }

    /// -----------------------------
    ///        BORROW CHECKING
    /// -----------------------------
    /// Perform borrow checking on HIR nodes to validate memory safety
    /// and ownership rules according to Beanstalk's reference semantics.
    pub fn check_borrows(
        &self,
        hir_module: &HirModule,
    ) -> Result<BorrowCheckOutcome, CompilerMessages> {
        let mut checker = BorrowChecker::new();
        checker.check_module(hir_module, &self.string_table)
    }
}
