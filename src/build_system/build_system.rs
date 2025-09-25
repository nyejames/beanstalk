// Beanstalk Build System
//
// Generalized build system that handles different project types:
// - HTML/JS projects with separate WASM files
// - Native/embedded projects with single WASM output
// - Development vs release build configurations

use crate::build_system::{embedded_project, html_project, jit, native_project};
use crate::compiler::compiler_errors::CompileError;
use crate::settings::{Config, ProjectType};
use crate::{Flag, InputModule, Project};
use std::path::Path;
use wasmer_types::target::Target;

/// Build configuration that determines how WASM files are generated and organized
#[derive(Debug, Clone)]
pub enum BuildTarget {
    /// HTML/JS project - generates separate WASM files for different HTML imports
    HtmlProject,

    /// Just runs the wasm and doesn't generate any output files
    Jit,

    /// Native project - single optimised WASM file
    Native {
        /// Target architecture (if applicable)
        target_arch: Option<Target>,
        /// Whether to enable native system calls
        enable_syscalls: bool,
    },
    /// Embedded project - WASM for embedding in other applications
    Embedded {
        /// Whether to enable hot reloading support
        hot_reload: bool,
        /// Custom IO interface configuration
        io_config: Option<String>,
    },
}

/// Unified build interface for all project types
pub trait ProjectBuilder {
    /// Build the project with the given configuration
    fn build_project(
        &self,
        modules: Vec<InputModule>,
        config: &Config,
        release_build: bool,
        flags: &[Flag],
    ) -> Result<Project, Vec<CompileError>>;

    /// Get the build target type
    fn target_type(&self) -> &BuildTarget;

    /// Validate the project configuration
    fn validate_config(&self, config: &Config) -> Result<(), CompileError>;
}

/// Create the appropriate project builder based on configuration
pub fn create_project_builder(target: BuildTarget) -> Box<dyn ProjectBuilder> {
    match target {
        BuildTarget::HtmlProject => Box::new(html_project::HtmlProjectBuilder::new(target)),
        BuildTarget::Native { .. } => Box::new(native_project::NativeProjectBuilder::new(target)),
        BuildTarget::Embedded { .. } => {
            Box::new(embedded_project::EmbeddedProjectBuilder::new(target))
        }
        BuildTarget::Jit => Box::new(jit::JitProjectBuilder::new(target)),
    }
}

/// Determine a build target from project configuration
pub fn determine_build_target(config: &Config, entry_path: &Path) -> BuildTarget {
    // Check if this is a single file or project
    if entry_path.extension().is_some() {
        // Single file - default to native
        BuildTarget::Native {
            target_arch: None,
            enable_syscalls: true,
        }
    } else {
        // Project directory - check config for the target type
        match &config.project_type {
            ProjectType::HTML => BuildTarget::HtmlProject,
            ProjectType::Native(target_arch) => BuildTarget::Native {
                target_arch: Some(target_arch.clone()),
                enable_syscalls: true,
            },
            ProjectType::Embedded => BuildTarget::Embedded {
                hot_reload: false, // Default to false for embedded
                io_config: None,
            },
            ProjectType::Jit => BuildTarget::Jit,

            // Currently not using JIT, just parsing a string template
            ProjectType::Repl => BuildTarget::Jit,
        }
    }
}
