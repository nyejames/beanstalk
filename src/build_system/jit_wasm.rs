// JIT project builder
//
// Builds and immediately executes Beanstalk code without creating any output files.
// This is useful for quick testing, debugging, and development iteration.
use crate::build::{BuildTarget, ProjectBuilder};
use crate::build_system::core_build;
use crate::compiler::compiler_errors::{CompilerError, CompilerMessages};
use crate::runtime::jit::execute_direct_jit;
use crate::settings::Config;
use crate::{Flag, InputModule, Project, generate_lir, generate_wasm, timer_log};
use std::time::Instant;

pub struct JitProjectBuilder {
    target: BuildTarget,
}

impl JitProjectBuilder {
    pub fn new(target: BuildTarget) -> Self {
        Self { target }
    }
}

impl ProjectBuilder for JitProjectBuilder {
    fn build_project(
        &self,
        modules: Vec<InputModule>,
        config: &Config,
        release_build: bool,
        flags: &[Flag],
    ) -> Result<Project, CompilerMessages> {
        // Validate configuration
        if let Err(e) = self.validate_config(config) {
            return Err(CompilerMessages {
                errors: vec![e],
                warnings: vec![],
            });
        }

        // Use the core build pipeline to compile to HIR
        let compilation_result =
            core_build::compile_modules(modules, &config, release_build, flags)?;

        let mut compiler_messages = CompilerMessages {
            errors: Vec::new(),
            warnings: compilation_result.warnings,
        };

        // ----------------------------------
        //          LIR generation
        // ----------------------------------
        let time = Instant::now();

        let lir_module = match generate_lir(compilation_result.hir_module) {
            Ok(lir) => lir,
            Err(e) => {
                compiler_messages.errors.extend(e.errors);
                compiler_messages.warnings.extend(e.warnings);
                return Err(compiler_messages);
            }
        };

        timer_log!(time, "LIR generated in: ");

        // Debug output for LIR if enabled
        #[cfg(feature = "show_lir")]
        {
            use crate::compiler::lir::display_lir;
            println!("=== LIR OUTPUT ===");
            println!("{}", display_lir(&lir_module));
            println!("=== END LIR OUTPUT ===");
        }

        // ----------------------------------
        //          WASM generation
        // ----------------------------------
        let time = Instant::now();

        let wasm_bytes = match generate_wasm(&lir_module) {
            Ok(wasm) => wasm,
            Err(e) => {
                compiler_messages.errors.push(e);
                return Err(compiler_messages);
            }
        };

        timer_log!(time, "WASM generated in: ");

        // Execute the WASM directly using JIT
        match execute_direct_jit(&wasm_bytes, &config.runtime_backend()) {
            Ok(_) => {
                // For JIT mode, we don't create any output files
                // Return an empty project to satisfy the interface
                Ok(Project {
                    config: config.clone(),
                    output_files: vec![],
                    warnings: compiler_messages.warnings,
                })
            }
            Err(e) => Err(CompilerMessages {
                errors: vec![e],
                warnings: vec![],
            }),
        }
    }

    fn target_type(&self) -> &BuildTarget {
        &self.target
    }

    fn validate_config(&self, _config: &Config) -> Result<(), CompilerError> {
        // JIT mode doesn't require specific configuration validation
        // The runtime configuration is already validated in the Config struct
        Ok(())
    }
}
