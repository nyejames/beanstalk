// Embedded project builder
//
// Builds Beanstalk projects for embedding in other applications,
// with support for hot reloading and custom IO interfaces.

use crate::build_system::build_system::{BuildTarget, ProjectBuilder};
use crate::build_system::core_build;
use crate::compiler::compiler_errors::{CompileError, CompilerMessages};
use crate::settings::Config;
use crate::{Flag, InputModule, OutputFile, Project, return_config_error};

pub struct EmbeddedProjectBuilder {
    target: BuildTarget,
}

impl EmbeddedProjectBuilder {
    pub fn new(target: BuildTarget) -> Self {
        Self { target }
    }

    /// Generate Rust embedding code for the compiled WASM
    fn generate_rust_embedding_code(&self, module_name: &str) -> String {
        format!(
            r#"
// Auto-generated Rust embedding code for Beanstalk module: {}
use beanstalk::runtime::embedding::{{EmbeddedRuntime, EmbeddedRuntimeBuilder}};
use beanstalk::runtime::{{IoBackend, CompilationMode}};

pub struct {}Module {{
    runtime: EmbeddedRuntime,
}}

impl {}Module {{
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {{
        let runtime = EmbeddedRuntimeBuilder::new()
            .with_hot_reload(true)
            .with_io_backend(IoBackend::Custom("embedded".to_string()))
            .build()?;
        
        // Load the compiled WASM module
        let wasm_bytes = include_bytes!("{}.wasm");
        runtime.load_module("{}", wasm_bytes)?;
        
        Ok(Self {{ runtime }})
    }}
    
    pub fn call_function(&self, function_name: &str, args: &[wasmer::Value]) -> Result<Vec<wasmer::Value>, Box<dyn std::error::Error>> {{
        Ok(self.runtime.call_function("{}", function_name, args)?)
    }}
    
    pub fn reload_from_bytes(&self, wasm_bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {{
        self.runtime.reload_module("{}", wasm_bytes)?;
        Ok(())
    }}
    
    pub fn reload_from_file(&self, wasm_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {{
        let wasm_bytes = std::fs::read(wasm_path)?;
        self.reload_from_bytes(&wasm_bytes)
    }}
}}

impl Default for {}Module {{
    fn default() -> Self {{
        Self::new().expect("Failed to create {} module")
    }}
}}
"#,
            module_name,
            to_pascal_case(module_name),
            to_pascal_case(module_name),
            module_name,
            module_name,
            module_name,
            module_name,
            to_pascal_case(module_name),
            module_name
        )
    }

    /// Generate C/C++ header for future C embedding support
    fn generate_c_header(&self, module_name: &str) -> String {
        format!(
            r#"
// Auto-generated C header for Beanstalk module: {}
#ifndef BEANSTALK_{}_H
#define BEANSTALK_{}_H

#ifdef __cplusplus
extern "C" {{
#endif

// Opaque handle to the Beanstalk module
typedef struct BeanstalkModule BeanstalkModule;

// Initialize the Beanstalk module
BeanstalkModule* beanstalk_{}_init(void);

// Cleanup the Beanstalk module
void beanstalk_{}_cleanup(BeanstalkModule* module);

// Call a function in the Beanstalk module
int beanstalk_{}_call_function(BeanstalkModule* module, const char* function_name);

// Reload the module from new WASM bytes (for hot reloading)
int beanstalk_{}_reload(BeanstalkModule* module, const unsigned char* wasm_bytes, size_t wasm_size);

#ifdef __cplusplus
}}
#endif

#endif // BEANSTALK_{}_H
"#,
            module_name,
            module_name.to_uppercase(),
            module_name.to_uppercase(),
            module_name,
            module_name,
            module_name,
            module_name,
            module_name.to_uppercase()
        )
    }
}

impl ProjectBuilder for EmbeddedProjectBuilder {
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
                warnings: vec![],
            });
        }

        let compilation_result = core_build::compile_modules(modules, config, flags)?;

        let mut output_files = vec![OutputFile::Wasm(compilation_result.wasm_bytes.clone())];

        if let BuildTarget::Embedded { hot_reload, .. } = &self.target {
            let module_name = &config.name;

            // Generate Rust embedding code
            let rust_code = self.generate_rust_embedding_code(module_name);
            output_files.push(OutputFile::Html(rust_code)); // Using HTML variant for code content

            // Generate C header for future C/C++ support
            let c_header = self.generate_c_header(module_name);
            output_files.push(OutputFile::Html(c_header)); // Using HTML variant for header content

            if *hot_reload {
                // Add hot reload configuration
                let hot_reload_config = format!(
                    r#"
# Hot Reload Configuration for {}
watch_paths = ["src/**/*.bst"]
reload_command = "cargo run build"
auto_reload = true
"#,
                    module_name
                );
                output_files.push(OutputFile::Html(hot_reload_config));
            }
        }

        Ok(Project {
            config: config.clone(),
            output_files,
            warnings: compilation_result.warnings,
        })
    }

    fn target_type(&self) -> &BuildTarget {
        &self.target
    }

    fn validate_config(&self, config: &Config) -> Result<(), CompileError> {
        // Validate embedded-specific configuration
        if let BuildTarget::Embedded { io_config, .. } = &self.target {
            if let Some(io_config_path) = io_config {
                // Validate IO configuration file exists
                if !std::path::Path::new(io_config_path).exists() {
                    return Err(CompileError::new_file_error(
                        std::path::Path::new(io_config_path),
                        "IO configuration file not found",
                        {
                            let mut map = std::collections::HashMap::new();
                            map.insert(crate::compiler::compiler_errors::ErrorMetaDataKey::CompilationStage, "Configuration");
                            map.insert(crate::compiler::compiler_errors::ErrorMetaDataKey::PrimarySuggestion, "Create the IO configuration file or update the path in your build configuration");
                            map
                        }
                    ));
                }
            }
        }

        // Embedded projects should have a clear module name
        if config.name.is_empty() {
            return_config_error!(
                "Embedded projects require a project_name to be specified",
                crate::compiler::compiler_errors::ErrorLocation::default(),
                {
                    CompilationStage => "Configuration",
                    PrimarySuggestion => "Add 'name' field to your project configuration",
                    SuggestedInsertion => "name = \"my_module\"",
                }
            );
        }

        Ok(())
    }
}

/// Convert snake_case to PascalCase
fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                }
            }
        })
        .collect()
}
