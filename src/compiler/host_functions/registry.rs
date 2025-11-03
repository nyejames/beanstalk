use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::parsers::ast_nodes::Arg;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::return_compiler_error;
use std::collections::HashMap;
use wasm_encoder::ValType;

/// Runtime backend types for host function execution
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RuntimeBackend {
    /// Native execution via WASIX
    Wasix,
    /// Web execution via JavaScript bindings
    JavaScript,
    /// Direct native function calls
    Native,
}

impl Default for RuntimeBackend {
    fn default() -> Self {
        RuntimeBackend::Wasix
    }
}

/// Defines how a host function handles errors
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ErrorHandling {
    /// Function cannot fail
    None,
    /// Function returns Error type using Beanstalk's error handling syntax
    ReturnsError,
    /// Function panics on error (should be avoided)
    Panics,
}

// Parameters that don't have default arguments
// This is to avoid the rabbit hole of dynamic dispatching in templates
// Which makes multithreading unsafe in certain parts of the compiler pipeline.
// Update: Doing this changed nothing, there is still an issue with multithreading.
#[derive(Debug, Clone)]
pub struct BasicParameter {
    pub(crate) name: String,
    pub(crate) data_type: DataType,
    pub(crate) ownership: Ownership,
}

/// Defines a WASIX function mapping for native execution
#[derive(Debug, Clone)]
pub struct WasixFunctionDef {
    /// WASIX module name (e.g., "wasix_32v1")
    pub module: String,
    /// WASIX function name (e.g., "fd_write")
    pub name: String,
    /// Parameter types in WASM format
    pub parameters: Vec<ValType>,
    /// Return types in WASM format
    pub returns: Vec<ValType>,
    /// Human-readable description for documentation
    pub description: String,
}

impl WasixFunctionDef {
    /// Create a new WASIX function definition
    pub fn new(
        module: &str,
        name: &str,
        parameters: Vec<ValType>,
        returns: Vec<ValType>,
        description: &str,
    ) -> Self {
        WasixFunctionDef {
            module: module.to_string(),
            name: name.to_string(),
            parameters,
            returns,
            description: description.to_string(),
        }
    }
}

/// Defines a JavaScript function mapping for web execution
#[derive(Debug, Clone)]
pub struct JsFunctionDef {
    /// JavaScript module name (e.g., "beanstalk_io")
    pub module: String,
    /// JavaScript function name (e.g., "print")
    pub name: String,
    /// Parameter types in WASM format
    pub parameters: Vec<ValType>,
    /// Return types in WASM format
    pub returns: Vec<ValType>,
    /// Human-readable description for documentation
    pub description: String,
}

impl JsFunctionDef {
    /// Create a new JavaScript function definition
    pub fn new(
        module: &str,
        name: &str,
        parameters: Vec<ValType>,
        returns: Vec<ValType>,
        description: &str,
    ) -> Self {
        JsFunctionDef {
            module: module.to_string(),
            name: name.to_string(),
            parameters,
            returns,
            description: description.to_string(),
        }
    }
}

/// Defines a host function that can be called from Beanstalk code
/// Uses the same parameter and return type system as regular Beanstalk functions
#[derive(Debug, Clone)]
pub struct HostFunctionDef {
    /// Function name as used in Beanstalk code
    pub name: String,
    /// Function parameters using the same Arg structure as regular functions
    pub parameters: Vec<BasicParameter>,
    /// Return types using the same system as regular functions
    pub return_types: Vec<DataType>,
    /// WASM import module name (e.g., "beanstalk_io")
    pub module: String,
    /// WASM import function name (e.g., "print")
    pub import_name: String,
    /// Human-readable description for documentation
    pub description: String,
    /// Error handling strategy
    pub error_handling: ErrorHandling,
}

impl HostFunctionDef {
    /// Create a new host function definition
    pub fn new(
        name: &str,
        parameters: Vec<BasicParameter>,
        return_types: Vec<DataType>,
        module: &str,
        import_name: &str,
        description: &str,
    ) -> Self {
        HostFunctionDef {
            name: name.to_string(),
            parameters,
            return_types,
            module: module.to_string(),
            import_name: import_name.to_string(),
            description: description.to_string(),
            error_handling: ErrorHandling::None, // Default to no error handling
        }
    }

    /// Create a new host function definition that can fail
    pub fn new_with_error(
        name: &str,
        parameters: Vec<BasicParameter>,
        return_types: Vec<DataType>,
        module: &str,
        import_name: &str,
        description: &str,
    ) -> Self {
        HostFunctionDef {
            name: name.to_string(),
            parameters,
            return_types,
            module: module.to_string(),
            import_name: import_name.to_string(),
            description: description.to_string(),
            error_handling: ErrorHandling::ReturnsError,
        }
    }

    /// Get the function signature as a DataType::Function for compatibility
    pub fn as_function_type(&self) -> DataType {
        DataType::Function(self.params_to_signature())
    }

    pub fn params_to_signature(&self) -> FunctionSignature {
        // Convert return_types Vec<DataType> to Vec<Arg>
        let return_args: Vec<Arg> = self
            .return_types
            .iter()
            .enumerate()
            .map(|(i, data_type)| Arg {
                name: format!("return_{}", i),
                value: Expression::new(
                    ExpressionKind::None,
                    TextLocation::default(),
                    data_type.clone(),
                    Ownership::default(),
                ),
            })
            .collect();

        // Convert parameters Vec<Parameter> to Vec<Arg>
        let parameters = self
            .parameters
            .iter()
            .map(|param| Arg {
                name: param.name.to_owned(),
                value: Expression::new(
                    ExpressionKind::None,
                    TextLocation::default(),
                    param.data_type.to_owned(),
                    Ownership::default(),
                ),
            })
            .collect();

        FunctionSignature {
            parameters,
            returns: return_args,
        }
    }
}

/// Registry for managing host function definitions with runtime-specific mappings
#[derive(Clone)]
pub struct HostFunctionRegistry {
    /// Map from function name to function definition
    functions: HashMap<String, HostFunctionDef>,
    /// Map from Beanstalk function name to WASIX function definition
    wasix_mappings: HashMap<String, WasixFunctionDef>,
    /// Map from Beanstalk function name to JavaScript function definition
    js_mappings: HashMap<String, JsFunctionDef>,
    /// Current runtime backend
    current_backend: RuntimeBackend,
}

impl HostFunctionRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        HostFunctionRegistry {
            functions: HashMap::new(),
            wasix_mappings: HashMap::new(),
            js_mappings: HashMap::new(),
            current_backend: RuntimeBackend::default(),
        }
    }

    /// Create a new registry with a specific runtime backend
    pub fn new_with_backend(backend: RuntimeBackend) -> Self {
        HostFunctionRegistry {
            functions: HashMap::new(),
            wasix_mappings: HashMap::new(),
            js_mappings: HashMap::new(),
            current_backend: backend,
        }
    }

    /// Get a host function definition by name
    pub fn get_function(&self, name: &str) -> Option<&HostFunctionDef> {
        self.functions.get(name)
    }

    /// Register a new host function
    pub fn register_function(&mut self, function: HostFunctionDef) -> Result<(), CompileError> {
        // Validate the function definition first
        validate_host_function_def(&function)?;

        if self.functions.contains_key(&function.name) {
            return_compiler_error!(
                "Host function '{}' is already registered. This is a compiler bug - duplicate function registration.",
                function.name
            );
        }

        self.functions.insert(function.name.clone(), function);
        Ok(())
    }

    /// Register a new host function with runtime-specific mappings
    pub fn register_function_with_mappings(
        &mut self,
        function: HostFunctionDef,
        wasix_mapping: Option<WasixFunctionDef>,
        js_mapping: Option<JsFunctionDef>,
    ) -> Result<(), CompileError> {
        // Register the core function first
        self.register_function(function.clone())?;

        // Register WASIX mapping if provided
        if let Some(wasix_func) = wasix_mapping {
            self.register_wasix_mapping(&function.name, wasix_func)?;
        }

        // Register JavaScript mapping if provided
        if let Some(js_func) = js_mapping {
            self.register_js_mapping(&function.name, js_func)?;
        }

        Ok(())
    }

    /// List all registered host functions
    pub fn list_functions(&self) -> Vec<&HostFunctionDef> {
        self.functions.values().collect()
    }

    /// Check if a function is registered
    pub fn has_function(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }

    /// Get the number of registered functions
    pub fn count(&self) -> usize {
        self.functions.len()
    }

    /// Get the current runtime backend
    pub fn get_current_backend(&self) -> &RuntimeBackend {
        &self.current_backend
    }

    /// Set the current runtime backend
    pub fn set_current_backend(&mut self, backend: RuntimeBackend) {
        self.current_backend = backend;
    }

    /// Get a WASIX function mapping by Beanstalk function name
    pub fn get_wasix_mapping(&self, beanstalk_name: &str) -> Option<&WasixFunctionDef> {
        self.wasix_mappings.get(beanstalk_name)
    }

    /// Get a JavaScript function mapping by Beanstalk function name
    pub fn get_js_mapping(&self, beanstalk_name: &str) -> Option<&JsFunctionDef> {
        self.js_mappings.get(beanstalk_name)
    }

    /// Register a WASIX mapping for a Beanstalk function
    pub fn register_wasix_mapping(
        &mut self,
        beanstalk_name: &str,
        wasix_function: WasixFunctionDef,
    ) -> Result<(), CompileError> {
        // Validate the WASIX function definition
        validate_wasix_function_def(&wasix_function)?;

        if self.wasix_mappings.contains_key(beanstalk_name) {
            return_compiler_error!(
                "WASIX mapping for function '{}' is already registered. This is a compiler bug - duplicate mapping registration.",
                beanstalk_name
            );
        }

        self.wasix_mappings
            .insert(beanstalk_name.to_string(), wasix_function);
        Ok(())
    }

    /// Register a JavaScript mapping for a Beanstalk function
    pub fn register_js_mapping(
        &mut self,
        beanstalk_name: &str,
        js_function: JsFunctionDef,
    ) -> Result<(), CompileError> {
        // Validate the JavaScript function definition
        validate_js_function_def(&js_function)?;

        if self.js_mappings.contains_key(beanstalk_name) {
            return_compiler_error!(
                "JavaScript mapping for function '{}' is already registered. This is a compiler bug - duplicate mapping registration.",
                beanstalk_name
            );
        }

        self.js_mappings
            .insert(beanstalk_name.to_string(), js_function);
        Ok(())
    }

    /// Get runtime-specific function mapping based on current backend
    pub fn get_runtime_mapping<'a>(&'a self, beanstalk_name: &str) -> Option<RuntimeFunctionMapping<'a>> {
        match self.current_backend {
            RuntimeBackend::Wasix => self
                .get_wasix_mapping(beanstalk_name)
                .map(RuntimeFunctionMapping::Wasix),
            RuntimeBackend::JavaScript => self
                .get_js_mapping(beanstalk_name)
                .map(RuntimeFunctionMapping::JavaScript),
            RuntimeBackend::Native => self
                .get_function(beanstalk_name)
                .map(RuntimeFunctionMapping::Native),
        }
    }

    /// Check if a function has a mapping for the current backend
    pub fn has_runtime_mapping(&self, beanstalk_name: &str) -> bool {
        match self.current_backend {
            RuntimeBackend::Wasix => self.wasix_mappings.contains_key(beanstalk_name),
            RuntimeBackend::JavaScript => self.js_mappings.contains_key(beanstalk_name),
            RuntimeBackend::Native => self.functions.contains_key(beanstalk_name),
        }
    }

    /// List all WASIX mappings
    pub fn list_wasix_mappings(&self) -> Vec<(&String, &WasixFunctionDef)> {
        self.wasix_mappings.iter().collect()
    }

    /// List all JavaScript mappings
    pub fn list_js_mappings(&self) -> Vec<(&String, &JsFunctionDef)> {
        self.js_mappings.iter().collect()
    }

    /// Get the number of WASIX mappings
    pub fn wasix_mapping_count(&self) -> usize {
        self.wasix_mappings.len()
    }

    /// Get the number of JavaScript mappings
    pub fn js_mapping_count(&self) -> usize {
        self.js_mappings.len()
    }
}

/// Runtime-specific function mapping wrapper
#[derive(Debug, Clone)]
pub enum RuntimeFunctionMapping<'a> {
    /// Native Beanstalk host function
    Native(&'a HostFunctionDef),
    /// WASIX function mapping
    Wasix(&'a WasixFunctionDef),
    /// JavaScript function mapping
    JavaScript(&'a JsFunctionDef),
}

impl Default for HostFunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a registry populated with built-in host functions for a specific backend
pub fn create_builtin_registry_with_backend(backend: RuntimeBackend) -> Result<HostFunctionRegistry, CompileError> {
    let mut registry = HostFunctionRegistry::new_with_backend(backend);

    // Register the template_output function with all runtime mappings
    let template_output_function = HostFunctionDef::new(
        "template_output",
        vec![BasicParameter {
            name: "content".to_string(),
            data_type: DataType::Template, // Accept Template (mutable string)
            ownership: Ownership::MutableOwned,
        }],
        vec![], // No return value (void function)
        "beanstalk_io",
        "template_output",
        "Output a template string to the host-defined output mechanism",
    );

    // WASIX fd_write signature: (fd: i32, iovs: i32, iovs_len: i32, nwritten: i32) -> i32
    let template_output_wasix_mapping = WasixFunctionDef::new(
        "wasix_32v1",
        "fd_write",
        vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32], // fd, iovs, iovs_len, nwritten
        vec![ValType::I32], // errno result
        "Write template output to stdout using WASIX fd_write",
    );

    let template_output_js_mapping = JsFunctionDef::new(
        "beanstalk_io",
        "template_output",
        vec![ValType::I32, ValType::I32], // ptr, len for string data
        vec![], // No return value
        "Output template to console using JavaScript console.log",
    );

    // Register function with all mappings at once
    registry.register_function_with_mappings(
        template_output_function,
        Some(template_output_wasix_mapping),
        Some(template_output_js_mapping),
    )?;

    // Validate all registered functions
    validate_registry(&registry)?;

    Ok(registry)
}

/// Create a registry populated with built-in host functions (uses default WASIX backend)
pub fn create_builtin_registry() -> Result<HostFunctionRegistry, CompileError> {
    create_builtin_registry_with_backend(RuntimeBackend::default())
}

/// Validate that all host function definitions in the registry are correct
fn validate_registry(registry: &HostFunctionRegistry) -> Result<(), CompileError> {
    // Validate core host functions
    for function in registry.list_functions() {
        validate_host_function_def(function)?;
    }

    // Validate WASIX mappings
    for (_, wasix_function) in registry.list_wasix_mappings() {
        validate_wasix_function_def(wasix_function)?;
    }

    // Validate JavaScript mappings
    for (_, js_function) in registry.list_js_mappings() {
        validate_js_function_def(js_function)?;
    }

    Ok(())
}

/// Validate a single WASIX function definition
fn validate_wasix_function_def(function: &WasixFunctionDef) -> Result<(), CompileError> {
    // Validate function name is not empty
    if function.name.is_empty() {
        return_compiler_error!(
            "WASIX function has empty name. Function definitions must have valid names."
        );
    }

    // Validate module name follows WASIX conventions
    if function.module.is_empty() {
        return_compiler_error!(
            "WASIX function '{}' has empty module name. WASIX imports require valid module names.",
            function.name
        );
    }

    // Validate module name is a standard WASIX module
    match function.module.as_str() {
        "wasix_32v1" | "wasix_64v1" | "wasix_snapshot_preview1" | "wasi_snapshot_preview1" | "wasi_unstable" => {
            // Valid WASIX module names
        }
        _ => {
            return_compiler_error!(
                "WASIX function '{}' uses invalid module '{}'. Valid WASIX modules are: wasix_32v1, wasix_64v1, wasix_snapshot_preview1, wasi_snapshot_preview1, wasi_unstable",
                function.name,
                function.module
            );
        }
    }

    // Validate reasonable parameter counts (WASIX functions typically have 0-10 parameters)
    if function.parameters.len() > 10 {
        return_compiler_error!(
            "WASIX function '{}' has {} parameters, which exceeds the reasonable limit of 10. This may indicate an error in the function definition.",
            function.name,
            function.parameters.len()
        );
    }

    // Validate reasonable return counts (WASIX functions typically return 0-2 values)
    if function.returns.len() > 2 {
        return_compiler_error!(
            "WASIX function '{}' has {} return values, which exceeds the reasonable limit of 2. This may indicate an error in the function definition.",
            function.name,
            function.returns.len()
        );
    }

    Ok(())
}

/// Validate a single JavaScript function definition
fn validate_js_function_def(function: &JsFunctionDef) -> Result<(), CompileError> {
    // Validate function name is not empty
    if function.name.is_empty() {
        return_compiler_error!(
            "JavaScript function has empty name. Function definitions must have valid names."
        );
    }

    // Validate module name follows JavaScript conventions
    if function.module.is_empty() {
        return_compiler_error!(
            "JavaScript function '{}' has empty module name. JavaScript imports require valid module names.",
            function.name
        );
    }

    // Validate module name is one of the standard Beanstalk JavaScript modules
    match function.module.as_str() {
        "beanstalk_io" | "beanstalk_env" | "beanstalk_sys" | "beanstalk_dom" => {
            // Valid JavaScript module names
        }
        _ => {
            return_compiler_error!(
                "JavaScript function '{}' uses invalid module '{}'. Valid JavaScript modules are: beanstalk_io, beanstalk_env, beanstalk_sys, beanstalk_dom",
                function.name,
                function.module
            );
        }
    }

    // Validate reasonable parameter counts (JavaScript functions typically have 0-8 parameters)
    if function.parameters.len() > 8 {
        return_compiler_error!(
            "JavaScript function '{}' has {} parameters, which exceeds the reasonable limit of 8. This may indicate an error in the function definition.",
            function.name,
            function.parameters.len()
        );
    }

    // Validate reasonable return counts (JavaScript functions typically return 0-1 values)
    if function.returns.len() > 1 {
        return_compiler_error!(
            "JavaScript function '{}' has {} return values, which exceeds the reasonable limit of 1. This may indicate an error in the function definition.",
            function.name,
            function.returns.len()
        );
    }

    Ok(())
}

/// Validate a single host function definition
fn validate_host_function_def(function: &HostFunctionDef) -> Result<(), CompileError> {
    // Validate function name is not empty
    if function.name.is_empty() {
        return_compiler_error!(
            "Host function has empty name. Function definitions must have valid names."
        );
    }

    // Validate module name follows WASM import conventions
    if function.module.is_empty() {
        return_compiler_error!(
            "Host function '{}' has empty module name. WASM imports require valid module names.",
            function.name
        );
    }

    // Validate import name is not empty
    if function.import_name.is_empty() {
        return_compiler_error!(
            "Host function '{}' has empty import name. WASM imports require valid function names.",
            function.name
        );
    }

    // Validate module name is one of the standard Beanstalk modules
    match function.module.as_str() {
        "beanstalk_io" | "beanstalk_env" | "beanstalk_sys" | "wasix_32v1" => {
            // Valid module names
        }
        _ => {
            return_compiler_error!(
                "Host function '{}' uses invalid module '{}'. Valid modules are: beanstalk_io, beanstalk_env, beanstalk_sys, wasix_32v1",
                function.name,
                function.module
            );
        }
    }

    // Validate parameter names are not empty
    for (i, param) in function.parameters.iter().enumerate() {
        if param.name.is_empty() {
            return_compiler_error!(
                "Host function '{}' has parameter {} with empty name. All parameters must have names.",
                function.name,
                i + 1
            );
        }
    }

    Ok(())
}

impl PartialEq for HostFunctionDef {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.module == other.module
            && self.import_name == other.import_name
            && self.description == other.description
            && self.error_handling == other.error_handling
    }
}

impl Eq for HostFunctionDef {}

impl std::hash::Hash for HostFunctionDef {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.module.hash(state);
        self.import_name.hash(state);
        self.description.hash(state);
        self.error_handling.hash(state);
    }
}
