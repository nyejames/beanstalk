// For running snippets of Beanstalk directly from a REPL to quickly get back some results,
// The last statement or expression in the snippet is returned as the result for each line entered
// For now: we can just compile the Wasm and run it with Wasmer and return whatever is at the top of the stack

// NOT REALLY WORKING YET - JUST SOME SCAFFOLDING

use crate::build_system::build_system::{BuildTarget, ProjectBuilder};
use crate::compiler::compiler_errors::CompileError;
use crate::settings::Config;
use crate::{Flag, InputModule, OutputFile, Project};
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Instant;

/// Project builder for REPL sessions
pub struct ReplProjectBuilder {
    target: BuildTarget,
}

impl ReplProjectBuilder {
    pub fn new() -> Self {
        Self {
            target: BuildTarget::Repl,
        }
    }
}

impl ProjectBuilder for ReplProjectBuilder {
    fn build_project(
        &self,
        modules: Vec<InputModule>,
        _config: &Config,
        _release_build: bool,
        _flags: &[Flag],
    ) -> Result<Project, Vec<CompileError>> {
        // For REPL, we expect a single module with Beanstalk code
        if modules.len() != 1 {
            return Err(vec![CompileError::compiler_error(
                "REPL mode expects exactly one module",
            )]);
        }

        let module = &modules[0];
        let start_time = Instant::now();

        // Compile the Beanstalk code to WASM
        let wasm_bytes = compile_beanstalk_to_wasm(&module.source_code, &module.source_path)
            .map_err(|e| vec![e])?;

        let duration = start_time.elapsed();
        println!("Compiled in {:?}", duration);

        // Create a minimal project with the WASM output
        let project = Project {
            config: Config::default(),
            output_files: vec![OutputFile::Wasm(wasm_bytes)],
        };

        Ok(project)
    }

    fn target_type(&self) -> &BuildTarget {
        &self.target
    }

    fn validate_config(&self, _config: &Config) -> Result<(), CompileError> {
        // REPL mode doesn't need complex validation
        Ok(())
    }
}

/// Compile Beanstalk source code to WASM bytes
fn compile_beanstalk_to_wasm(
    source_code: &str,
    source_path: &PathBuf,
) -> Result<Vec<u8>, CompileError> {
    use crate::compiler::codegen::build_wasm::new_wasm_module;
    use crate::compiler::mir::mir::borrow_check_pipeline;
    use crate::compiler::parsers::build_ast::{ContextKind, ScopeContext, new_ast};
    use crate::compiler::parsers::tokenizer;

    // Tokenize the source code
    let mut tokenizer_output = tokenizer::tokenize(source_code, source_path)?;

    // Build AST
    let ast_context = ScopeContext::new(ContextKind::Module, source_path.clone(), &[]);
    let parser_output = new_ast(&mut tokenizer_output, ast_context, false)?;
    let ast_module = parser_output.ast;

    // Build MIR with borrow checking
    let mir_module = borrow_check_pipeline(ast_module).map_err(|errors| {
        // Convert Vec<CompileError> to single CompileError for now
        errors
            .into_iter()
            .next()
            .unwrap_or_else(|| CompileError::compiler_error("Unknown MIR compilation error"))
    })?;

    // Generate WASM
    let wasm_bytes = new_wasm_module(mir_module)?;

    Ok(wasm_bytes)
}

/// Start the REPL session
pub fn start_repl_session() {
    use crate::compiler::compiler_errors::print_formatted_error;
    use crate::runtime::{BeanstalkRuntime, RuntimeConfig};
    use colour::{green_ln_bold, grey_ln, red_ln};

    green_ln_bold!("Beanstalk REPL");
    grey_ln!("Enter Beanstalk code snippets. Type 'exit' to quit.");
    grey_ln!("The last expression will be evaluated and its result displayed.");
    println!();

    let runtime_config = RuntimeConfig::for_development();
    let runtime = BeanstalkRuntime::new(runtime_config);
    let builder = ReplProjectBuilder::new();

    loop {
        print!("[ ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(_) => {
                let input = input.trim();

                if input.is_empty() {
                    continue;
                }

                if input == "exit" {
                    println!("Closing REPL session.");
                    break;
                }

                // Compile and execute the input
                match execute_beanstalk_snippet(&builder, &runtime, input) {
                    Ok(result) => {
                        if let Some(value) = result {
                            println!("{}", value);
                        }
                    }
                    Err(e) => {
                        print_formatted_error(e);
                    }
                }
            }
            Err(e) => {
                red_ln!("Error reading input: {}", e);
                break;
            }
        }
    }
}

/// Execute a Beanstalk code snippet and return the result
fn execute_beanstalk_snippet(
    builder: &ReplProjectBuilder,
    _runtime: &crate::runtime::BeanstalkRuntime,
    source_code: &str,
) -> Result<Option<String>, CompileError> {
    use crate::build::InputModule;
    use std::path::PathBuf;

    // Create a temporary module for the snippet
    let module = InputModule {
        source_code: source_code.to_string(),
        source_path: PathBuf::from("repl_input.bst"),
    };

    // Build the project to get WASM
    let project = builder
        .build_project(
            vec![module],
            &crate::settings::Config::default(),
            false,
            &[],
        )
        .map_err(|errors| {
            // Convert Vec<CompileError> to single CompileError for now
            errors
                .into_iter()
                .next()
                .unwrap_or_else(|| CompileError::compiler_error("Unknown build error"))
        })?;

    // Extract WASM bytes
    let wasm_bytes = match project.output_files.first() {
        Some(crate::build::OutputFile::Wasm(bytes)) => bytes,
        _ => return Err(CompileError::compiler_error("No WASM output generated")),
    };

    // Execute the WASM and capture the result
    let result = execute_wasm_and_get_result(_runtime, wasm_bytes)?;

    Ok(result)
}

/// Execute WASM and extract the top stack value as a result
fn execute_wasm_and_get_result(
    _runtime: &crate::runtime::BeanstalkRuntime,
    wasm_bytes: &[u8],
) -> Result<Option<String>, CompileError> {
    use wasmer::{Function, Instance, Module, Store, Value, imports};

    // Create Wasmer store
    let mut store = Store::default();

    // Compile WASM module
    let module = Module::new(&store, wasm_bytes).map_err(|e| {
        CompileError::compiler_error(&format!("Failed to compile WASM module: {}", e))
    })?;

    // Set up minimal imports for REPL execution
    let import_object = imports! {
        "beanstalk_io" => {
            "print" => Function::new_typed(&mut store, |msg_ptr: i32, msg_len: i32| {
                // For now, just capture the print calls
                println!("Print: ptr={}, len={}", msg_ptr, msg_len);
            }),
        }
    };

    // Instantiate the module
    let instance = Instance::new(&mut store, &module, &import_object).map_err(|e| {
        CompileError::compiler_error(&format!("Failed to instantiate WASM module: {}", e))
    })?;

    // Look for main function or entry point
    if let Ok(main_func) = instance.exports.get_function("main") {
        // Execute main function
        let result = main_func.call(&mut store, &[]);

        match result {
            Ok(values) => {
                if !values.is_empty() {
                    // Convert the first return value to a string representation
                    let result_str = match &values[0] {
                        Value::I32(v) => v.to_string(),
                        Value::I64(v) => v.to_string(),
                        Value::F32(v) => v.to_string(),
                        Value::F64(v) => v.to_string(),
                        _ => "unknown".to_string(),
                    };
                    Ok(Some(result_str))
                } else {
                    Ok(None)
                }
            }
            Err(e) => Err(CompileError::compiler_error(&format!(
                "Runtime error: {}",
                e
            ))),
        }
    } else {
        // Look for _start function (WASI entry point)
        if let Ok(start_func) = instance.exports.get_function("_start") {
            let result = start_func.call(&mut store, &[]);

            match result {
                Ok(_) => Ok(None), // _start doesn't return values
                Err(e) => Err(CompileError::compiler_error(&format!(
                    "Runtime error: {}",
                    e
                ))),
            }
        } else {
            Err(CompileError::compiler_error(
                "No entry point found in WASM module (expected 'main' or '_start')",
            ))
        }
    }
}
