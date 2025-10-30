use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::ast_nodes::Arg;
use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::return_compiler_error;
use std::collections::HashMap;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::tokenizer::tokens::TextLocation;

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
struct BasicParameter {
    name: String,
    data_type: DataType,
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
                    crate::compiler::datatypes::Ownership::default(),
                ),
            })
            .collect();

        // Convert parameters Vec<Parameter> to Vec<Arg>
        let parameters = self
            .parameters
            .iter()
            .map(|param|
                Arg {
                    name: param.name.to_owned(),
                    value: Expression::new(
                        ExpressionKind::None,
                        TextLocation::default(),
                        param.data_type.to_owned(),
                        crate::compiler::datatypes::Ownership::default(),
                    ),
                }
            )
            .collect();

        let signature = FunctionSignature {
            parameters,
            returns: return_args,
        };

        DataType::Function(signature)
    }
}

/// Registry for managing host function definitions
#[derive(Clone)]
pub struct HostFunctionRegistry {
    /// Map from function name to function definition
    functions: HashMap<String, HostFunctionDef>,
}

impl HostFunctionRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        HostFunctionRegistry {
            functions: HashMap::new(),
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
}

impl Default for HostFunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a registry populated with built-in host functions
pub fn create_builtin_registry() -> Result<HostFunctionRegistry, CompileError> {
    let mut registry = HostFunctionRegistry::new();

    let print_function = HostFunctionDef::new(
        "print",
        vec![BasicParameter {
            name: "message".to_string(),
            data_type: DataType::String,
        }],
        vec![], // No return value (void function)
        "wasix_32v1",
        "fd_write",
        "Print a message to stdout using WASIX fd_write",
    );

    registry.register_function(print_function)?;

    // Validate all registered functions
    validate_registry(&registry)?;

    Ok(registry)
}

/// Validate that all host function definitions in the registry are correct
fn validate_registry(registry: &HostFunctionRegistry) -> Result<(), CompileError> {
    for function in registry.list_functions() {
        validate_host_function_def(function)?;
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
