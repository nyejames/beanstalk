use crate::compiler::compiler_errors::CompilerError;
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::parsers::ast_nodes::Var;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::string_interning::{InternedString, StringTable};
use crate::return_compiler_error;
use std::collections::HashMap;
use wasm_encoder::ValType;

/// Runtime backend types for host function execution
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum RuntimeBackend {
    /// Web execution via JavaScript bindings
    #[default]
    JavaScript,
    /// For embedded projects
    Rust,
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
#[derive(Debug, Clone)]
pub struct BasicParameter {
    pub(crate) name: InternedString,
    pub(crate) data_type: DataType,
    pub(crate) ownership: Ownership,
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
    pub name: InternedString,
    /// Function parameters using the same Arg structure as regular functions
    pub parameters: Vec<BasicParameter>,
    /// Return types using the same system as regular functions
    pub return_types: Vec<DataType>,
    /// WASM import module name (e.g., "beanstalk_io")
    pub module: InternedString,
    /// WASM import function name (e.g., "print")
    pub import_name: InternedString,
    /// Human-readable description for documentation
    pub description: InternedString,
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
        string_table: &mut StringTable,
    ) -> Self {
        HostFunctionDef {
            name: string_table.intern(name),
            parameters,
            return_types,
            module: string_table.intern(module),
            import_name: string_table.intern(import_name),
            description: string_table.intern(description),
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
        string_table: &mut StringTable,
    ) -> Self {
        HostFunctionDef {
            name: string_table.intern(name),
            parameters,
            return_types,
            module: string_table.intern(module),
            import_name: string_table.intern(import_name),
            description: string_table.intern(description),
            error_handling: ErrorHandling::ReturnsError,
        }
    }

    /// Get the function signature as a DataType::Function for compatibility
    pub fn as_function_type(&self, string_table: &mut StringTable) -> DataType {
        DataType::Function(Box::new(None), self.params_to_signature(string_table))
    }

    pub fn params_to_signature(&self, string_table: &mut StringTable) -> FunctionSignature {
        // Convert return_types Vec<DataType> to Vec<Arg>
        let return_args: Vec<Var> = self
            .return_types
            .iter()
            .enumerate()
            .map(|(i, data_type)| Var {
                id: string_table.get_or_intern(i.to_string()),
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
            .map(|param| Var {
                id: param.name,
                value: Expression::new(
                    ExpressionKind::None,
                    TextLocation::default(),
                    param.data_type.to_owned(),
                    param.ownership.to_owned(),
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
pub struct HostRegistry {
    /// Map from function name to function definition
    functions: HashMap<InternedString, HostFunctionDef>,

    /// Map from Beanstalk function name to JavaScript function definition
    js_mappings: HashMap<InternedString, JsFunctionDef>,

    /// Host provided constants list
    /// This is an ExpressionKind as data type info is not needed
    constants: HashMap<InternedString, ExpressionKind>,

    /// Current runtime backend
    current_backend: RuntimeBackend,
}

impl HostRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        HostRegistry {
            functions: HashMap::new(),
            constants: HashMap::new(),
            js_mappings: HashMap::new(),
            current_backend: RuntimeBackend::default(),
        }
    }

    /// Create a new registry with a specific runtime backend
    pub fn new_with_backend(backend: RuntimeBackend) -> Self {
        HostRegistry {
            functions: HashMap::new(),
            constants: HashMap::new(),
            js_mappings: HashMap::new(),
            current_backend: backend,
        }
    }

    /// Get a host function definition by name
    pub fn get_function(&self, id: &InternedString) -> Option<&HostFunctionDef> {
        self.functions.get(id)
    }

    /// Register a new host function
    pub fn register_function(
        &mut self,
        function: HostFunctionDef,
        string_table: &StringTable,
    ) -> Result<(), CompilerError> {
        // Validate the function definition first
        validate_host_function_def(&function, string_table)?;

        if self.functions.contains_key(&function.name) {
            return_compiler_error!(
                "Host function '{}' is already registered. This is a compiler bug - duplicate function registration.",
                string_table.resolve(function.name)
            );
        }

        self.functions.insert(function.name, function);
        Ok(())
    }

    /// Register a new host function with runtime-specific mappings
    pub fn register_function_with_mappings(
        &mut self,
        function: HostFunctionDef,
        js_mapping: Option<JsFunctionDef>,
        string_table: &StringTable,
    ) -> Result<(), CompilerError> {
        // Register the core function first
        self.register_function(function.clone(), string_table)?;

        // Register JavaScript mapping if provided
        if let Some(js_func) = js_mapping {
            self.register_js_mapping(function.name, js_func, string_table)?;
        }

        Ok(())
    }

    /// List all registered host functions
    pub fn list_functions(&self) -> Vec<&HostFunctionDef> {
        self.functions.values().collect()
    }

    /// Check if a function is registered
    pub fn has_function(&self, name: &InternedString) -> bool {
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

    /// Get a JavaScript function mapping by Beanstalk function name
    pub fn get_js_mapping(&self, beanstalk_name: &InternedString) -> Option<&JsFunctionDef> {
        self.js_mappings.get(beanstalk_name)
    }

    /// Register a JavaScript mapping for a Beanstalk function
    pub fn register_js_mapping(
        &mut self,
        beanstalk_name: InternedString,
        js_function: JsFunctionDef,
        string_table: &StringTable,
    ) -> Result<(), CompilerError> {
        // Validate the JavaScript function definition
        validate_js_function_def(&js_function)?;

        if self.js_mappings.contains_key(&beanstalk_name) {
            return_compiler_error!(
                "JavaScript mapping for function '{}' is already registered. This is a compiler bug - duplicate mapping registration.",
                string_table.resolve(beanstalk_name)
            );
        }

        self.js_mappings.insert(beanstalk_name, js_function);
        Ok(())
    }

    /// Get runtime-specific function mapping based on current backend
    pub fn get_runtime_mapping(
        &self,
        beanstalk_name: &InternedString,
    ) -> Option<RuntimeFunctionMapping<'_>> {
        match self.current_backend {
            RuntimeBackend::JavaScript => self
                .get_js_mapping(beanstalk_name)
                .map(RuntimeFunctionMapping::JavaScript),
            RuntimeBackend::Rust => self
                .get_function(beanstalk_name)
                .map(RuntimeFunctionMapping::Rust),
        }
    }

    /// Check if a function has a mapping for the current backend
    pub fn has_runtime_mapping(&self, beanstalk_name: &InternedString) -> bool {
        match self.current_backend {
            RuntimeBackend::JavaScript => self.js_mappings.contains_key(beanstalk_name),
            RuntimeBackend::Rust => self.functions.contains_key(beanstalk_name),
        }
    }

    /// List all JavaScript mappings
    pub fn list_js_mappings(&self) -> Vec<(&InternedString, &JsFunctionDef)> {
        self.js_mappings.iter().collect()
    }

    /// Get the number of JavaScript mappings
    pub fn js_mapping_count(&self) -> usize {
        self.js_mappings.len()
    }

    /// Validate that the mandatory io() function is available
    /// This should be called during compilation to ensure build system contract compliance
    pub fn validate_io_availability(
        &self,
        string_table: &StringTable,
    ) -> Result<(), CompilerError> {
        validate_io_function_availability(self, string_table)
    }
}

/// Runtime-specific function mapping wrapper
#[derive(Debug, Clone)]
pub enum RuntimeFunctionMapping<'a> {
    /// Native Beanstalk host function
    Rust(&'a HostFunctionDef),
    /// JavaScript function mapping
    JavaScript(&'a JsFunctionDef),
}

impl Default for HostRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a registry populated with built-in host functions for a specific backend
pub fn create_builtin_registry(
    backend: RuntimeBackend,
    string_table: &mut StringTable,
) -> Result<HostRegistry, CompilerError> {
    let mut registry = HostRegistry::new_with_backend(backend);

    // Register the io() function with CoerceToString parameter
    // This function outputs content to stdout with an automatic newline
    let io_function = HostFunctionDef::new(
        "io",
        vec![BasicParameter {
            name: string_table.intern("content"),
            data_type: DataType::CoerceToString, // Accept any type and coerce to string
            ownership: Ownership::ImmutableReference, // Borrowed parameter
        }],
        vec![], // Void return type
        "beanstalk_io",
        "io",
        "Output content to stdout with automatic newline. Accepts any type through CoerceToString.",
        string_table,
    );

    let io_js_mapping = JsFunctionDef::new(
        "beanstalk_io",
        "io",
        vec![ValType::I32, ValType::I32], // ptr, len for string data in WASM linear memory
        vec![],                           // Void return
        "Output to console.log with automatic newline",
    );

    // Register a function with JavaScript mapping
    registry.register_function_with_mappings(io_function, Some(io_js_mapping), string_table)?;

    // Validate all registered functions
    validate_registry(&registry, string_table)?;

    Ok(registry)
}

/// Validate that all host function definitions in the registry are correct
fn validate_registry(
    registry: &HostRegistry,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    // Validate core host functions
    for function in registry.list_functions() {
        validate_host_function_def(function, string_table)?;
    }

    // Validate JavaScript mappings
    for (_, js_function) in registry.list_js_mappings() {
        validate_js_function_def(js_function)?;
    }

    // Validate that the mandatory io() function is present
    validate_io_function_availability(registry, string_table)?;

    Ok(())
}

/// Validate that the mandatory io() function is available in the registry
/// This is a build system contract requirement - every build system must provide io()
fn validate_io_function_availability(
    registry: &HostRegistry,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    // Check if io() function exists by looking through all registered functions
    let has_io = registry.list_functions().iter().any(|func| {
        let func_name = string_table.resolve(func.name);
        func_name == "io"
    });

    if !has_io {
        return_compiler_error!(
            "Build system does not provide required 'io()' function. \
            Every Beanstalk build system must provide at minimum the io() function for basic printing. \
            Check your build system configuration and ensure the Io struct includes the io() function."
        );
    }

    Ok(())
}

/// Validate a single JavaScript function definition
fn validate_js_function_def(function: &JsFunctionDef) -> Result<(), CompilerError> {
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
fn validate_host_function_def(
    function: &HostFunctionDef,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    let function_name = string_table.resolve(function.name);
    let module_name = string_table.resolve(function.module);
    let import_name = string_table.resolve(function.import_name);

    // Validate function name is not empty
    if function_name.is_empty() {
        return_compiler_error!(
            "Host function has empty name. Function definitions must have valid names."
        );
    }

    // Validate module name follows WASM import conventions
    if module_name.is_empty() {
        return_compiler_error!(
            "Host function '{}' has empty module name. WASM imports require valid module names.",
            function_name
        );
    }

    // Validate import name is not empty
    if import_name.is_empty() {
        return_compiler_error!(
            "Host function '{}' has empty import name. WASM imports require valid function names.",
            function_name
        );
    }

    // Validate module name is one of the standard Beanstalk modules
    match module_name {
        "beanstalk_io" | "beanstalk_env" | "beanstalk_sys" => {
            // Valid module names
        }
        _ => {
            return_compiler_error!(
                "Host function '{}' uses invalid module '{}'. Valid modules are: beanstalk_io, beanstalk_env, beanstalk_sys",
                function_name,
                module_name
            );
        }
    }

    // Validate parameter names are not empty
    for (i, param) in function.parameters.iter().enumerate() {
        let param_name = string_table.resolve(param.name);
        if param_name.is_empty() {
            return_compiler_error!(
                "Host function '{}' has parameter {} with empty name. All parameters must have names.",
                function_name,
                i + 1
            );
        }
    }

    // Special validation for the io() function
    if function_name == "io" {
        validate_io_function(function, string_table)?;
    }

    Ok(())
}

/// Validate the io() function definition meets requirements
fn validate_io_function(
    function: &HostFunctionDef,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    let function_name = string_table.resolve(function.name);
    let module_name = string_table.resolve(function.module);

    // Validate module name is "beanstalk_io"
    if module_name != "beanstalk_io" {
        return_compiler_error!(
            "Host function '{}' must use module 'beanstalk_io', but found '{}'. The io() function is part of the beanstalk_io module.",
            function_name,
            module_name
        );
    }

    // Validate exactly one parameter
    if function.parameters.len() != 1 {
        return_compiler_error!(
            "Host function '{}' must have exactly 1 parameter, but found {}. The io() function accepts a single CoerceToString parameter.",
            function_name,
            function.parameters.len()
        );
    }

    // Validate parameter is CoerceToString type
    let param = &function.parameters[0];
    if !matches!(param.data_type, DataType::CoerceToString) {
        return_compiler_error!(
            "Host function '{}' parameter must be CoerceToString type, but found {:?}. The io() function accepts any type through CoerceToString.",
            function_name,
            param.data_type
        );
    }

    // Validate return type is void (empty vector)
    if !function.return_types.is_empty() {
        return_compiler_error!(
            "Host function '{}' must have void return type (no return values), but found {} return types. The io() function does not return a value.",
            function_name,
            function.return_types.len()
        );
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
