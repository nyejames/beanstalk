pub(crate) mod build;
pub mod cli;
mod create_new_project;
mod dev_server;
pub mod settings;

pub(crate) mod compiler_tests {
    #[cfg(test)]
    pub(crate) mod control_flow_linearizer_tests;
    #[cfg(test)]
    pub(crate) mod expression_linearizer_tests;
    #[cfg(test)]
    pub(crate) mod hir_builder_tests;
    pub(crate) mod test_runner;
    #[cfg(test)]
    pub(crate) mod variable_manager_tests;
    #[cfg(test)]
    pub(crate) mod wasm_codegen_tests;
    #[cfg(test)]
    pub(crate) mod wasm_integration_tests;
}

// New runtime and build system modules
pub(crate) mod runtime;
pub(crate) mod build_system {
    pub(crate) mod core_build;
    pub(crate) mod embedded_project;
    pub(crate) mod html_project;
    pub(crate) mod jit;
    pub(crate) mod native_project;
    pub(crate) mod repl;
}

mod compiler {
    pub(crate) mod parsers {
        pub(crate) mod ast;
        pub(crate) mod ast_nodes;
        pub(crate) mod build_ast;

        pub(crate) mod parse_file_headers;
        // pub(crate) mod markdown; // Commented out to silence unused warnings - will be used by frontend later
        pub(crate) mod expressions {
            pub(crate) mod eval_expression;
            pub(crate) mod expression;
            pub(crate) mod function_call_inline;
            pub(crate) mod mutation;
            pub(crate) mod parse_expression;
        }
        pub(crate) mod statements {
            pub(crate) mod branching;
            pub(crate) mod collections;
            pub(crate) mod create_template_node;
            pub(crate) mod functions;
            pub(crate) mod imports;
            pub(crate) mod loops;
            pub(crate) mod structs;
            pub(crate) mod template;
            pub(crate) mod variables;
        }
        pub(crate) mod builtin_methods;
        pub(crate) mod field_access;

        pub(crate) mod tokenizer {
            pub(crate) mod compiler_directives;
            pub(crate) mod tokenizer;
            pub(crate) mod tokens;
        }
    }
    pub(crate) mod optimizers {
        pub(crate) mod constant_folding;
    }

    pub(crate) mod module_dependencies;

    mod html5_codegen {
        pub(crate) mod code_block_highlighting;
        pub(crate) mod dom_hooks;
        pub(crate) mod generate_html;
        pub(crate) mod html_styles;
        // pub(crate) mod js_parser;
        // pub(crate) mod web_parser;
    }

    #[allow(dead_code)]
    pub(crate) mod basic_utility_functions;

    pub(crate) mod compiler_messages {
        pub(crate) mod compiler_dev_logging;
        pub(crate) mod compiler_errors;
        pub(crate) mod compiler_warnings;
    }
    // Temporary re-exports to preserve old import paths after moving modules
    // into `compiler_messages`. This minimizes churn across the codebase.
    pub(crate) use compiler_messages::compiler_dev_logging;
    pub(crate) use compiler_messages::compiler_errors;
    pub(crate) use compiler_messages::compiler_warnings;
    pub(crate) mod datatypes;
    pub(crate) mod interned_path;
    pub(crate) mod string_interning;
    pub(crate) mod traits;

    pub(crate) mod host_functions {
        pub(crate) mod registry;
    }

    pub(crate) mod hir {
        pub(crate) mod build_hir;
        pub(crate) mod control_flow_linearizer;
        pub(crate) mod display_hir;
        pub(crate) mod expression_linearizer;
        pub(crate) mod nodes;
        pub(crate) mod variable_manager;
    }

    pub(crate) mod lir {
        pub(crate) mod nodes;
    }

    pub(crate) mod codegen {
        pub(crate) mod wasm;
    }
}

use crate::compiler::host_functions::registry::HostFunctionRegistry;
use crate::compiler::string_interning::StringTable;
use crate::settings::{Config, ProjectType};
use std::collections::HashSet;
use std::path::PathBuf;

// Re-export types for the build system
use crate::compiler::codegen::wasm::encode::encode_wasm;
use crate::compiler::compiler_messages::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler::compiler_messages::compiler_warnings::CompilerWarning;
use crate::compiler::hir::build_hir::HirBuilderContext;
use crate::compiler::hir::nodes::{HirModule, HirNode};
use crate::compiler::interned_path::InternedPath;
use crate::compiler::lir::nodes::LirModule;
use crate::compiler::module_dependencies::resolve_module_dependencies;
use crate::compiler::parsers::ast::Ast;
use crate::compiler::parsers::parse_file_headers::{Header, parse_headers};
use crate::compiler::parsers::tokenizer::tokenizer::tokenize;
use crate::compiler::parsers::tokenizer::tokens::{FileTokens, TokenizeMode};
pub(crate) use build::*;

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

#[derive(PartialEq, Debug)]
pub enum Flag {
    ShowAst,
    DisableWarnings,
    ShowWarnings, // The default behaviour for tests is to hide warnings, so this enables them in those cases
    DisableTimers,
    Verbose, // TODO: Prints out absolutely everything
}

pub struct Compiler<'a> {
    project_config: &'a Config,
    host_function_registry: HostFunctionRegistry,
    string_table: StringTable,
}

impl<'a> Compiler<'a> {
    pub fn new(
        project_config: &'a Config,
        host_function_registry: HostFunctionRegistry,
        string_table: StringTable,
    ) -> Self {
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
    ) -> Result<FileTokens, CompilerError> {
        let tokenizer_mode = match self.project_config.project_type {
            ProjectType::Repl => TokenizeMode::TemplateHead,
            _ => TokenizeMode::Normal,
        };

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
            &self.project_config.entry_point,
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
        // Pass string table to AST construction for string interning during AST building
        Ast::new(
            module_tokens,
            &self.host_function_registry,
            &mut self.string_table,
        )
    }

    /// -----------------------------
    ///         HIR GENERATION
    /// -----------------------------
    /// Generate HIR from AST nodes, linearizing expressions and creating
    /// a place-based representation suitable for borrow checking analysis.
    pub fn generate_hir(&mut self, ast: Ast) -> Result<HirModule, CompilerMessages> {
        let ctx = HirBuilderContext::new(&mut self.string_table);
        ctx.build_hir_module(ast)
    }

    // TODO
    /// -----------------------------
    ///        BORROW CHECKING
    /// -----------------------------
    /// Perform borrow checking on HIR nodes to validate memory safety
    /// and ownership rules according to Beanstalk's reference semantics.
    // pub fn check_borrows(&mut self, hir_nodes: &mut Vec<HirNode>) -> Result<(), CompilerError> {
    //     // Perform borrow checking analysis
    //     check_borrows(hir_nodes, &mut self.string_table)
    // }

    // TODO
    /// -----------------------------
    ///         LIR GENERATION
    /// -----------------------------
    /// Generate LIR from HIR nodes.
    /// LIR is a representation designed for lowering to Was
    // pub fn generate_lir(&mut self, hir_nodes: &[HirNode]) -> Result<LirModule, CompilerError> {
    //     lower_to_lir(hir_nodes, &self.string_table)
    // }

    // TODO
    /// -----------------------------
    ///         Wasm Codegen
    /// -----------------------------
    /// Lower to wasm bytes from the lir
    pub fn generate_wasm(&self, lir: &LirModule) -> Result<Vec<u8>, CompilerError> {
        encode_wasm(lir)
    }
}
