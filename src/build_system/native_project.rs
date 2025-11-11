// Native project builder
//
// Builds Beanstalk projects for native execution, producing a single optimized WASM file
// that can be executed with the Beanstalk runtime or embedded in other applications.

use crate::build_system::build_system::{BuildTarget, ProjectBuilder};
use crate::build_system::core_build;
use crate::compiler::compiler_errors::{CompileError, CompilerMessages};
use crate::settings::Config;
use crate::{Flag, InputModule, OutputFile, Project, return_config_error};
use crate::compiler::parsers::tokenizer::tokens::TextLocation;

pub struct NativeProjectBuilder {
    target: BuildTarget,
}

impl NativeProjectBuilder {
    pub fn new(target: BuildTarget) -> Self {
        Self { target }
    }
}

impl ProjectBuilder for NativeProjectBuilder {
    fn build_project(
        &self,
        modules: Vec<InputModule>,
        config: &Config,
        _release_build: bool,
        flags: &[Flag],
    ) -> Result<Project, CompilerMessages> {
        // Validate configuration
        if let Err(e) = self.validate_config(config) {
            return Err(CompilerMessages {
                errors: vec![e],
                warnings: Vec::new(),
            });
        }

        // Use the core build pipeline
        let compilation_result = core_build::compile_modules(modules, config, flags)?;

        // For native projects, we produce a single WASM file
        let output_files = vec![OutputFile::Wasm(compilation_result.wasm_bytes)];

        // Required imports are handled by the runtime - no need to print them

        Ok(Project {
            config: config.clone(),
            output_files,
        })
    }

    fn target_type(&self) -> &BuildTarget {
        &self.target
    }

    fn validate_config(&self, _config: &Config) -> Result<(), CompileError> {
        // Validate native-specific configuration
        if let BuildTarget::Native { target_arch: _, .. } = &self.target {
            // Don't bother checking for valid targets for now
            // The list of valid target types is built into Cranelift itself so should always be supported here
            return Ok(());
        }
        
        let target_str: &'static str = Box::leak(format!("{:?}", &self.target).into_boxed_str());
        return_config_error!(
            format!("Wrong target specified in project config: {:?}", &self.target),
            TextLocation::default(),
            {
                CompilationStage => "Configuration",
                PrimarySuggestion => "Use BuildTarget::Native for native projects",
                FoundType => target_str,
            }
        )
    }
}
