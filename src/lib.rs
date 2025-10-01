pub mod build;
pub mod settings;

pub mod cli;
mod create_new_project;
mod dev_server;
mod file_output;

pub mod compiler_tests;

// New runtime and build system modules
pub mod runtime;
pub mod build_system {
    pub mod build_system;
    pub mod core_build;
    pub mod embedded_project;
    pub mod html_project;
    pub mod jit;
    pub mod native_project;
    pub mod repl;
}

mod compiler {
    pub mod parsers {
        pub mod ast_nodes;
        pub mod build_ast;
        pub mod collections;
        pub mod markdown;
        pub mod expressions {
            pub mod eval_expression;
            pub mod expression;
            pub mod function_call_inline;
            pub mod mutation;
            pub mod parse_expression;
        }
        pub mod statements {
            pub mod branching;
            pub mod create_template_node;
            pub mod functions;
            pub mod loops;
            pub mod structs;
            pub mod variables;
        }
        pub mod builtin_methods;
        pub mod template;

        pub mod tokenizer;
        pub mod tokens;
    }
    pub mod optimizers {
        pub mod constant_folding;
        pub mod optimized_dataflow;
        pub mod place_interner;
        pub mod streamlined_diagnostics;
    }

    pub mod mir {
        pub mod arena;
        pub mod build_mir;
        pub mod cfg;
        pub mod check;
        pub mod counter;
        pub mod dataflow;
        pub mod diagnose;
        pub mod extract;
        pub mod liveness;
        pub mod mir;
        pub mod mir_nodes;
        pub mod place;
        pub mod unified_borrow_checker;
    }

    mod html5_codegen {
        pub mod code_block_highlighting;
        pub mod dom_hooks;
        pub mod generate_html;
        pub mod html_styles;
    }

    #[allow(dead_code)]
    pub mod basic_utility_functions;
    pub mod compiler_dev_logging;
    pub mod compiler_errors;
    pub mod compiler_warnings;
    pub mod datatypes;
    pub mod module_dependencies;
    pub mod traits;

    pub mod codegen {
        pub mod build_wasm;
        pub mod wasm_encoding;
        pub mod wat_to_wasm;
    }
}

use crate::compiler::codegen::build_wasm::new_wasm_module;
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::mir::build_mir::MIR;
use crate::compiler::parsers::ast_nodes::Arg;
use crate::compiler::parsers::build_ast::{
    AstBlock, ContextKind, ParserOutput, ScopeContext, new_ast,
};
use crate::compiler::parsers::tokenizer;
use crate::compiler::parsers::tokens::{TokenContext, TokenizeMode};
use crate::settings::{Config, ProjectType};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

// Re-export types for the build system
pub use build::*;

pub struct OutputModule {
    pub imports: HashSet<PathBuf>,
    pub source_path: PathBuf,
}

impl OutputModule {
    pub fn new(source_path: PathBuf, imports: HashSet<PathBuf>) -> Self {
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
}

impl<'a> Compiler<'a> {
    pub fn new(project_config: &'a Config) -> Self {
        Self { project_config }
    }

    /// -----------------------------
    ///          TOKENIZER
    /// -----------------------------
    /// At this stage,
    /// we are also collecting the list of imports for the module.
    /// This is so a dependency graph can start being built before the AST stage
    /// So modules are compiled to an AST in the correct order.
    /// Can be parallelised for all files in a project,
    /// as there is no need to check imports or types yet
    pub fn source_to_tokens(
        &self,
        source_code: &str,
        module_path: &Path,
    ) -> Result<TokenContext, CompileError> {
        let tokenizer_mode = match self.project_config.project_type {
            ProjectType::Repl => TokenizeMode::TemplateHead,
            _ => TokenizeMode::Normal,
        };

        match tokenizer::tokenize(source_code, module_path, tokenizer_mode) {
            Ok(tokens) => Ok(tokens),
            Err(e) => Err(e.with_file_path(PathBuf::from(module_path))),
        }
    }

    /// -----------------------------
    ///         AST CREATION
    /// -----------------------------
    /// This assumes the modules are in the right order for compiling
    /// Without any circular dependencies.
    /// All imports for a module must already be in public_declarations.
    /// So all the type-checking and folding can be performed correctly
    pub fn tokens_to_ast(
        &self,
        mut module_tokens: TokenContext,
        public_declarations: &[Arg],
    ) -> Result<ParserOutput, CompileError> {
        let ast_context = ScopeContext::new(
            ContextKind::Module,
            module_tokens.src_path.to_owned(),
            public_declarations,
        );

        let is_entry_point = self.project_config.entry_point == module_tokens.src_path;

        match new_ast(&mut module_tokens, ast_context, is_entry_point) {
            Ok(block) => Ok(block),
            Err(e) => Err(e.with_file_path(module_tokens.src_path.to_owned())),
        }
    }

    /// -----------------------------
    ///         MIR CREATION
    /// -----------------------------
    /// Lower to an IR for lifetime analysis and block level optimisations
    /// This IR maps well to WASM with integrated borrow checking
    pub fn ast_to_ir(&self, ast: AstBlock) -> Result<MIR, Vec<CompileError>> {
        // Use the new borrow checking pipeline
        compiler::mir::mir::borrow_check_pipeline(ast)
    }

    /// -----------------------
    ///        BACKEND
    ///    (Wasm Generation)
    /// -----------------------
    pub fn ir_to_wasm(mir: MIR) -> Result<Vec<u8>, CompileError> {
        new_wasm_module(mir)
    }
}
