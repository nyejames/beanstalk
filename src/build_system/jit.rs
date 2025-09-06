// JIT project builder
//
// Builds and immediately executes Beanstalk code without creating any output files.
// This is useful for quick testing, debugging, and development iteration.

use crate::build_system::build_system::{BuildTarget, ProjectBuilder};
use crate::build_system::core_build;
use crate::compiler::compiler_errors::CompileError;
use crate::runtime::jit::execute_direct_jit;
use crate::settings::Config;
use crate::{timer_log, Flag, InputModule, Project};
use colour::{green_ln, grey_ln};

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
        _release_build: bool,
        flags: &[Flag],
    ) -> Result<Project, Vec<CompileError>> {
        // Validate configuration
        if let Err(e) = self.validate_config(config) {
            return Err(vec![e]);
        }

        // Use core build pipeline to compile to WASM
        let compilation_result = core_build::compile_modules(modules, config, flags)?;

        let time = std::time::Instant::now();

        // Execute the WASM directly using JIT
        match execute_direct_jit(&compilation_result.wasm_bytes, &config.runtime) {
            Ok(_) => {
                timer_log!(time, "Jit executed in: ");
                Ok(())
            }
            Err(e) => {
                Err(vec![e])
            }
        }?;

        // For JIT mode, we don't create any output files
        // Return an empty project to satisfy the interface
        Ok(Project {
            config: config.clone(),
            output_files: vec![],
        })
    }

    fn target_type(&self) -> &BuildTarget {
        &self.target
    }

    fn validate_config(&self, _config: &Config) -> Result<(), CompileError> {
        // JIT mode doesn't require specific configuration validation
        // The runtime configuration is already validated in the Config struct
        Ok(())
    }
}
