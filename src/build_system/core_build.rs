// Core build functionality shared across all project types
//
// Contains the common compilation pipeline steps that are used by all project builders:
// - Tokenization
// - AST generation
// - WIR generation
// - WASM generation

use crate::compiler::codegen::build_wasm::new_wasm_module;
use crate::compiler::compiler_errors::{CompileError, CompilerMessages};
use crate::compiler::compiler_warnings::CompilerWarning;
use crate::compiler::host_functions::registry::HostFunctionRegistry;
use crate::compiler::parsers::ast_nodes::{Arg, AstNode};
use crate::settings::Config;
use crate::{Compiler, Flag, InputModule, timer_log};
use colour::green_ln;
// use rayon::prelude::*;
use std::time::Instant;
use crate::compiler::parsers::tokenizer::tokens::FileTokens;

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
    pub required_module_imports: Vec<ExternalImport>,
    pub exported_functions: Vec<String>,
    pub warnings: Vec<CompilerWarning>,
}

/// Perform the core compilation pipeline shared by all project types
pub fn compile_modules(
    modules: Vec<InputModule>,
    config: &Config,
    flags: &[Flag],
) -> Result<CompilationResult, CompilerMessages> {
    let time = Instant::now();

    // TODO: define the host function registry based on the config
    let compiler = Compiler::new(config, HostFunctionRegistry::new());

    // ----------------------------------
    //         Token generation
    // ----------------------------------
    let tokenizer_result: Vec<Result<FileTokens, CompileError>> = modules
        .iter()
        .map(|module| compiler.source_to_tokens(&module.source_code, &module.source_path))
        .collect();

    // Check for any errors first
    let mut project_tokens = Vec::new();
    let mut errors: Vec<CompileError> = Vec::new();
    for file in tokenizer_result {
        match file {
            Ok(tokens) => {
                project_tokens.push(tokens);
            }
            Err(e) => {
                errors.push(e);
            }
        }
    }

    if !errors.is_empty() {
        let mut messages = CompilerMessages::new();
        messages.errors = errors;
        return Err(messages);
    }

    timer_log!(time, "Tokenized in: ");

    // ----------------------------------
    //           Parse Headers
    // ----------------------------------
    // This will parse all the top level declarations across the token_stream
    // This is to split up the AST generation into discreet blocks and make all the public declarations known during AST generation.
    // All imports are figured out at this stage, so each header can be ordered depending on their dependencies.
    let time = Instant::now();
    let mut compiler_messages = CompilerMessages::new();
    let module_headers = match compiler.tokens_to_headers(project_tokens, &mut compiler_messages.warnings) {
        Ok(headers) => headers,
        Err(e) => {
        compiler_messages.errors.extend(e);
            return Err(compiler_messages)
        }
    };

    timer_log!(time, "Headers Parsed in: ");

    // ----------------------------------
    //       Dependency resolution
    // ----------------------------------
    let time = Instant::now();
    let sorted_modules = match compiler.sort_headers(module_headers) {
        Ok(modules) => modules,
        Err(error) => {
            compiler_messages.errors.extend(error);
            return Err(compiler_messages)
        }
    };

    timer_log!(time, "Dependency graph created in: ");

    // ----------------------------------
    //          AST generation
    // ----------------------------------
    let time = Instant::now();
    //let mut exported_declarations: Vec<Arg> = Vec::with_capacity(EXPORTS_CAPACITY);
    let mut messages: CompilerMessages = CompilerMessages::new();
    let mut module_ast: Vec<AstNode> = Vec::with_capacity(sorted_modules.len());

    // Combine all the headers into one AST
    match compiler.headers_to_ast(sorted_modules) {
        Ok(parser_output) => {
            module_ast.extend(parser_output.nodes);

            // Extends the compiler messages with warnings and errors from the parser
            messages.warnings.extend(parser_output.warnings);
        }
        Err(e) => {
            messages.errors.extend(e.errors);
            return Err(messages)
        }
    }

    timer_log!(time, "AST created in: ");

    // ----------------------------------
    //          WIR generation
    // ----------------------------------
    let wir = match compiler.ast_to_ir(module_ast) {
        Ok(wir) => {
            if !flags.contains(&Flag::DisableTimers) {
                print!("Wasm Intermediate Representation generated in: ");
                green_ln!("{:?}", time.elapsed());
            }
            wir
        }
        Err(e) => {
            messages.errors.extend(e);
            return Err(messages);
        }
    };

    // ----------------------------------
    //          WASM generation
    // ----------------------------------
    let wasm_bytes = match new_wasm_module(wir) {
        Ok(w) => w,
        Err(e) => {
            messages.errors.push(e);
            return Err(messages);
        }
    };

    if !flags.contains(&Flag::DisableTimers) {
        print!("WASM generated in: ");
        green_ln!("{:?}", time.elapsed());
    }

    // -----------------------------------
    //      Extract required imports
    // -----------------------------------
    // let exported_functions = extract_exported_functions(&exported_declarations);

    Ok(CompilationResult {
        wasm_bytes,
        required_module_imports: Vec::new(), //TODO: parse imports for external modules and add to requirements list
        exported_functions: Vec::new(),      //TODO: Get the list of exported functions from the AST (with their signatures)
        warnings: messages.warnings,
    })
}

/// Extract required external imports from the compilation
fn extract_required_imports(exported_declarations: &[Arg]) -> Vec<ExternalImport> {
    let mut imports = Vec::new();

    // Add standard IO imports that are always required
    imports.extend(get_standard_io_imports());

    // TODO: Scan the WIR/AST for user-defined external function calls
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
) -> Result<CompilationResult, CompilerMessages> {
    compile_modules(vec![module], config, flags)
}
