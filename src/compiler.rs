use std::collections::HashSet;
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::parsers::build_ast::{AstBlock, ContextKind, ScopeContext, new_ast};
use crate::compiler::parsers::tokenizer;
use crate::compiler::wasm_codegen::wasm_emitter::WasmModule;
use crate::settings::Config;
use crate::Flag;
use std::path::{Path, PathBuf};
use crate::compiler::parsers::ast_nodes::Arg;
use crate::compiler::parsers::tokens::TokenContext;

#[allow(dead_code)]
pub mod basic_utility_functions;

pub mod compiler_dev_logging;
pub mod compiler_errors;
pub mod datatypes;
mod module_dependencies;
pub mod release_optimizers;
pub mod traits;

pub mod parsers {
    pub mod ast_nodes;
    pub mod build_ast;
    pub mod collections;
    mod create_template_node;
    pub mod markdown;
    pub mod expressions {
        pub mod constant_folding;
        pub mod eval_expression;
        pub mod expression;
        pub mod function_call_inline;
        pub mod parse_expression;
    }
    pub mod statements {
        pub mod functions;
        pub mod loops;
        pub mod structs;
    }
    pub mod builtin_methods;
    pub mod template;

    pub mod tokenizer;
    pub mod tokens;
    pub mod variables;
}
mod html5_codegen {
    pub mod code_block_highlighting;
    pub mod dom_hooks;
    pub mod generate_html;
    pub mod html_styles;
    pub mod js_parser;
    pub mod web_parser;
}

pub mod wasm_codegen {
    pub mod wasm_emitter;
    pub mod wasm_memory;
    pub mod wat_to_wasm;
}

pub struct OutputModule {
    pub imports: HashSet<PathBuf>,
    pub source_path: PathBuf,
    pub wasm: WasmModule,
}

impl OutputModule {
    pub fn new(source_path: PathBuf, imports: HashSet<PathBuf>) -> Self {
        OutputModule {
            imports,
            source_path,
            wasm: WasmModule::new(),
        }
    }
}

pub struct Compiler<'a> {
    project_config: &'a Config,
    flags: &'a [Flag],
}

impl<'a> Compiler<'a> {
    pub fn new(project_config: &'a Config, flags: &'a [Flag]) -> Self {
        Self {
            project_config,
            flags,
        }
    }

    /// -----------------------------
    ///          TOKENIZING
    /// -----------------------------
    /// At this stage,
    /// we are also collecting the list of imports for the module.
    /// This is so a dependency graph can start being built before the AST stage
    /// So modules are compiled to an AST in the correct order.
    /// Can be parallelized for all files in a project,
    /// as there is no need to check imports or types yet
    pub fn source_to_tokens(
        &self,
        source_code: &str,
        module_path: &Path,
    ) -> Result<TokenContext, CompileError> {
        match tokenizer::tokenize(source_code, module_path) {
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
    pub fn tokens_to_ast(&self, module_tokens: &mut TokenContext, module_path: PathBuf, public_declarations: &[Arg]) -> Result<AstBlock, CompileError> {
        let ast_context = ScopeContext::new(ContextKind::Module, module_path.to_owned(), public_declarations.to_owned());
        match new_ast(module_tokens, ast_context) {

            Ok(block) => Ok(block),

            Err(e) => {
                Err(e.with_file_path(PathBuf::from(module_path)))
            }
        }
    }
}
