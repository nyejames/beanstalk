// Core build functionality shared across all project types
//
// Contains the common compilation pipeline steps that are used by all project builders:
// - Tokenization
// - AST generation
// - WIR generation
// - WASM generation

use crate::compiler::codegen::build_wasm::new_wasm_module;
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::module_dependencies::resolve_module_dependencies;
use crate::compiler::parsers::ast_nodes::Arg;
use crate::compiler::parsers::build_ast::AstBlock;
use crate::settings::{Config, EXPORTS_CAPACITY};
use crate::{Compiler, Flag, InputModule, timer_log};
use colour::green_ln;
use rayon::prelude::*;
use std::time::Instant;

/// External function import required by the compiled WASM
#[derive(Debug, Clone)]
pub struct ExternalImport {
    /// Module name (e.g., "env", "beanstalk_io", "host")
    pub module: String,
    /// Function name
    pub function: String,
    /// Function signature for validation
    pub signature: FunctionSignature,
    /// Whether this is a built-in compiler function or user-defined import
    pub import_type: ImportType,
}

/// Function signature for external imports
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    /// Parameter types
    pub params: Vec<WasmType>,
    /// Return types
    pub returns: Vec<WasmType>,
}

/// Type of external import
#[derive(Debug, Clone)]
pub enum ImportType {
    /// Built-in compiler library function (IO, memory management, etc.)
    BuiltIn(BuiltInFunction),
    /// User-defined external function from host environment
    External,
}

/// Built-in compiler functions that the runtime must provide
#[derive(Debug, Clone)]
pub enum BuiltInFunction {
    /// IO operations
    Print,
    ReadInput,
    WriteFile,
    ReadFile,
    /// Memory management
    Malloc,
    Free,
    /// Environment access
    GetEnv,
    SetEnv,
    /// System operations
    Exit,
}

/// WASM value types for function signatures
#[derive(Debug, Clone)]
pub enum WasmType {
    I32,
    I64,
    F32,
    F64,
}

/// Core compilation result containing WASM and required imports
pub struct CompilationResult {
    pub wasm_bytes: Vec<u8>,
    pub required_imports: Vec<ExternalImport>,
    pub exported_functions: Vec<String>,
}

/// Perform the core compilation pipeline shared by all project types
pub fn compile_modules(
    modules: Vec<InputModule>,
    config: &Config,
    flags: &[Flag],
) -> Result<CompilationResult, Vec<CompileError>> {
    let time = Instant::now();
    let compiler = Compiler::new(config);

    // ----------------------------------
    //         Token generation
    // ----------------------------------
    let project_tokens: Vec<Result<crate::compiler::parsers::tokens::TokenContext, CompileError>> =
        modules
            .par_iter()
            .map(|module| compiler.source_to_tokens(&module.source_code, &module.source_path))
            .collect();
    timer_log!(time, "Tokenized in: ");

    // ----------------------------------
    //      Dependency resolution
    // ----------------------------------
    // TODO: Imports are known at this stage,
    // so should be creating the vec of required imports here.
    // Imports from host environment or from other modules should be separated and treated differently.
    let time = Instant::now();
    let sorted_modules = resolve_module_dependencies(project_tokens)?;
    timer_log!(time, "Dependency graph created in: ");

    // ----------------------------------
    //          AST generation
    // ----------------------------------
    let time = Instant::now();
    let mut exported_declarations: Vec<Arg> = Vec::with_capacity(EXPORTS_CAPACITY);
    let mut errors: Vec<CompileError> = Vec::new();
    let mut ast_blocks: Vec<AstBlock> = Vec::with_capacity(sorted_modules.len());

    for module in sorted_modules {
        match compiler.tokens_to_ast(module, &exported_declarations) {
            Ok(parser_output) => {
                exported_declarations.extend(parser_output.public);
                ast_blocks.push(parser_output.ast);
            }
            Err(e) => {
                errors.push(e);
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    if !flags.contains(&Flag::DisableTimers) {
        print!("AST created in: ");
        green_ln!("{:?}", time.elapsed());
    }

    // ----------------------------------
    //       Link ASTs into module
    // ----------------------------------
    let mut combined_module: Vec<crate::compiler::parsers::ast_nodes::AstNode> = Vec::new();
    for block in &ast_blocks {
        combined_module.extend(block.ast.clone());
    }

    // ----------------------------------
    //          WIR generation
    // ----------------------------------
    let wir = match compiler.ast_to_ir(AstBlock {
        ast: combined_module,
        is_entry_point: true,
        scope: config.entry_point.to_owned(),
    }) {
        Ok(wir) => {
            if !flags.contains(&Flag::DisableTimers) {
                print!("Wasm Intermediate Representation generated in: ");
                green_ln!("{:?}", time.elapsed());
            }
            wir
        }
        Err(e) => return Err(e),
    };

    // ----------------------------------
    //          WASM generation
    // ----------------------------------
    let wasm_bytes = match new_wasm_module(wir) {
        Ok(w) => w,
        Err(e) => return Err(vec![e]),
    };

    if !flags.contains(&Flag::DisableTimers) {
        print!("WASM generated in: ");
        green_ln!("{:?}", time.elapsed());
    }

    // -----------------------------------
    //      Extract required imports
    // -----------------------------------
    let required_imports = extract_required_imports(&exported_declarations);
    let exported_functions = extract_exported_functions(&exported_declarations);

    Ok(CompilationResult {
        wasm_bytes,
        required_imports,
        exported_functions,
    })
}

/// Extract required external imports from the compilation
fn extract_required_imports(exported_declarations: &[Arg]) -> Vec<ExternalImport> {
    let mut imports = Vec::new();

    // Add standard IO imports that are always required
    imports.extend(get_standard_io_imports());

    // TODO: Scan the MIR/AST for user-defined external function calls
    // This will be implemented when we add support for importing external functions

    imports
}

/// Get the standard IO imports that are built into the compiler
fn get_standard_io_imports() -> Vec<ExternalImport> {
    vec![
        ExternalImport {
            module: "beanstalk_io".to_string(),
            function: "print".to_string(),
            signature: FunctionSignature {
                params: vec![WasmType::I32, WasmType::I32], // ptr, len
                returns: vec![],
            },
            import_type: ImportType::BuiltIn(BuiltInFunction::Print),
        },
        ExternalImport {
            module: "beanstalk_io".to_string(),
            function: "read_input".to_string(),
            signature: FunctionSignature {
                params: vec![WasmType::I32],  // buffer ptr
                returns: vec![WasmType::I32], // bytes read
            },
            import_type: ImportType::BuiltIn(BuiltInFunction::ReadInput),
        },
        ExternalImport {
            module: "beanstalk_io".to_string(),
            function: "write_file".to_string(),
            signature: FunctionSignature {
                params: vec![WasmType::I32, WasmType::I32, WasmType::I32, WasmType::I32], // path_ptr, path_len, content_ptr, content_len
                returns: vec![WasmType::I32], // success/error code
            },
            import_type: ImportType::BuiltIn(BuiltInFunction::WriteFile),
        },
        ExternalImport {
            module: "beanstalk_io".to_string(),
            function: "read_file".to_string(),
            signature: FunctionSignature {
                params: vec![WasmType::I32, WasmType::I32, WasmType::I32], // path_ptr, path_len, buffer_ptr
                returns: vec![WasmType::I32], // bytes read or error code
            },
            import_type: ImportType::BuiltIn(BuiltInFunction::ReadFile),
        },
        ExternalImport {
            module: "beanstalk_env".to_string(),
            function: "get_env".to_string(),
            signature: FunctionSignature {
                params: vec![WasmType::I32, WasmType::I32, WasmType::I32], // key_ptr, key_len, buffer_ptr
                returns: vec![WasmType::I32], // value length or -1 if not found
            },
            import_type: ImportType::BuiltIn(BuiltInFunction::GetEnv),
        },
        ExternalImport {
            module: "beanstalk_env".to_string(),
            function: "set_env".to_string(),
            signature: FunctionSignature {
                params: vec![WasmType::I32, WasmType::I32, WasmType::I32, WasmType::I32], // key_ptr, key_len, value_ptr, value_len
                returns: vec![WasmType::I32], // success/error code
            },
            import_type: ImportType::BuiltIn(BuiltInFunction::SetEnv),
        },
        ExternalImport {
            module: "beanstalk_sys".to_string(),
            function: "exit".to_string(),
            signature: FunctionSignature {
                params: vec![WasmType::I32], // exit code
                returns: vec![],
            },
            import_type: ImportType::BuiltIn(BuiltInFunction::Exit),
        },
    ]
}

/// Extract exported functions from the compilation
fn extract_exported_functions(exported_declarations: &[Arg]) -> Vec<String> {
    exported_declarations
        .iter()
        .map(|arg| arg.name.clone())
        .collect()
}

/// Compile a single module (for simple cases)
pub fn compile_single_module(
    module: InputModule,
    config: &Config,
    flags: &[Flag],
) -> Result<CompilationResult, Vec<CompileError>> {
    compile_modules(vec![module], config, flags)
}
