pub(crate) mod build;
pub mod settings;

pub(crate) mod cli;
mod create_new_project;
mod dev_server;

pub(crate) mod compiler_tests {
    pub(crate) mod test_runner;
}

// New runtime and build system modules
pub(crate) mod runtime;
pub(crate) mod build_system {
    pub(crate) mod build_system;
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
            pub(crate) mod create_template_node;
            pub(crate) mod functions;
            pub(crate) mod imports;
            pub(crate) mod loops;
            pub(crate) mod structs;
            pub(crate) mod variables;
            pub(crate) mod collections;
            pub(crate) mod template;
        }
        pub(crate) mod builtin_methods;

        pub(crate) mod tokenizer {
            pub(crate) mod tokenizer;
            pub(crate) mod tokens;
            pub(crate) mod compiler_directives;

        }
    }
    pub(crate) mod optimizers {
        pub(crate) mod constant_folding;
    }

    pub(crate) mod module_dependencies;
    pub(crate) mod wir;

    pub(crate) mod borrow_checker {
        pub(crate) mod borrow_checker;
        pub(crate) mod extract;
    }

    mod html5_codegen {
        pub(crate) mod code_block_highlighting;
        pub(crate) mod dom_hooks;
        pub(crate) mod generate_html;
        pub(crate) mod html_styles;
    }

    #[allow(dead_code)]
    pub(crate) mod basic_utility_functions;
    pub(crate) mod compiler_dev_logging;
    pub(crate) mod compiler_errors;
    pub(crate) mod compiler_warnings;
    pub(crate) mod datatypes;
    pub(crate) mod traits;

    pub(crate) mod codegen {
        pub(crate) mod build_wasm;
        pub(crate) mod wasm_encoding;
        pub(crate) mod wat_to_wasm;
    }

    pub(crate) mod host_functions {
        pub(crate) mod registry;
        pub(crate) mod wasix_registry;
    }
}

use crate::compiler::codegen::build_wasm::new_wasm_module;
use crate::compiler::compiler_errors::{CompileError, CompilerMessages};
use crate::compiler::host_functions::registry::HostFunctionRegistry;
use crate::compiler::parsers::ast_nodes::AstNode;
use crate::compiler::parsers::tokenizer;
use crate::compiler::wir::build_wir::WIR;
use crate::settings::{Config, ProjectType};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

// Re-export types for the build system
use crate::compiler::module_dependencies::resolve_module_dependencies;
use crate::compiler::parsers::ast::Ast;
use crate::compiler::parsers::parse_file_headers::{parse_headers, Header};
pub(crate) use build::*;
use crate::compiler::compiler_warnings::CompilerWarning;
use crate::compiler::parsers::tokenizer::tokenizer::tokenize;
use crate::compiler::parsers::tokenizer::tokens::{FileTokens, TokenizeMode};

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
    DisableTimers,
}

pub struct Compiler<'a> {
    project_config: &'a Config,
    host_function_registry: HostFunctionRegistry,
}

impl<'a> Compiler<'a> {
    pub fn new(project_config: &'a Config, host_function_registry: HostFunctionRegistry) -> Self {
        Self {
            project_config,
            host_function_registry,
        }
    }

    /// -----------------------------
    ///          TOKENIZER
    /// -----------------------------
    pub fn source_to_tokens(
        &self,
        source_code: &str,
        module_path: &Path,
    ) -> Result<FileTokens, CompileError> {
        let tokenizer_mode = match self.project_config.project_type {
            ProjectType::Repl => TokenizeMode::TemplateHead,
            _ => TokenizeMode::Normal,
        };

        match tokenize(source_code, module_path, tokenizer_mode) {
            Ok(tokens) => Ok(tokens),
            Err(e) => Err(e.with_file_path(PathBuf::from(module_path))),
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
        &self,
        files: Vec<FileTokens>,
        warnings: &mut Vec<CompilerWarning>,
    ) -> Result<Vec<Header>, Vec<CompileError>> {
        parse_headers(files, &self.host_function_registry, warnings)
    }

    /// Every dependency needed for each file should be known before its headers are parsed.
    /// This is so structs that contain imported structs can know the shape of the imported structs first.
    /// ---------------------------
    ///     DEPENDENCY SORTING
    /// ---------------------------
    /// Now, as we parse the headers and combine the files,
    /// the types of each dependency will be known.
    /// This section answers the following question:
    /// - In what order must the headers be defined so that symbol resolution and type-checking of bodies can proceed deterministically?
    pub fn sort_headers(&self, headers: Vec<Header>) -> Result<Vec<Header>, Vec<CompileError>> {
        resolve_module_dependencies(headers)
    }

    /// -----------------------------
    ///         AST CREATION
    /// -----------------------------
    /// This assumes that the vec of FileTokens contains all dependencies for each file.
    /// The headers of each file will be parsed first, then each file will be combined into one module.
    /// The AST also provides a list of exports from the module.
    pub fn headers_to_ast(&self, module_tokens: Vec<Header>) -> Result<Ast, CompilerMessages> {
        Ast::new(module_tokens, &self.host_function_registry)
    }

    /// -----------------------------
    ///         WIR CREATION
    /// -----------------------------
    /// Lower to an IR for lifetime analysis and block level optimisations
    /// This IR maps well to WASM with integrated borrow checking
    pub fn ast_to_ir(&self, ast: Vec<AstNode>) -> Result<WIR, Vec<CompileError>> {
        // Use the new borrow checking pipeline
        compiler::wir::wir::borrow_check_pipeline(ast)
    }

    /// -----------------------
    ///        BACKEND
    ///    (Wasm Generation)
    /// -----------------------
    pub fn ir_to_wasm(wir: WIR) -> Result<Vec<u8>, CompileError> {
        new_wasm_module(wir)
    }
}
