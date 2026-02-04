use crate::compiler::compiler_errors::CompilerError;
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::parsers::ast_nodes::Var;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::compiler::string_interning::{InternedString, StringTable};
use crate::return_compiler_error;
use std::collections::HashMap;
// ======================================================
//                    HOST ABI
// ======================================================

/// Backend-agnostic ABI values that cross the host boundary
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HostAbiType {
    I32,
    F64,
    Utf8Str,
    OpaquePtr,
    Void,
}

/// A single host-call parameter definition
#[derive(Debug, Clone)]
pub struct HostParameter {
    /// What the Beanstalk language accepts
    pub language_type: DataType,

    /// What crosses the ABI boundary
    pub abi_type: HostAbiType,
}

// ======================================================
//               HOST FUNCTION DEFINITION
// ======================================================

#[derive(Debug, Clone)]
pub struct HostFunctionDef {
    pub name: InternedString,
    pub parameters: Vec<HostParameter>,
    pub return_type: HostAbiType,
    pub ownership: Ownership,
    pub error_handling: ErrorHandling,
    pub description: String,
}

impl HostFunctionDef {
    pub fn as_function_type(&self, string_table: &mut StringTable) -> DataType {
        DataType::Function(Box::new(None), self.params_to_signature(string_table))
    }

    pub(crate) fn params_to_signature(&self, string_table: &mut StringTable) -> FunctionSignature {
        let parameters = self
            .parameters
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let name = string_table.intern(&format!("arg{}", i));
                Var {
                    id: name,
                    value: Expression {
                        kind: ExpressionKind::None,
                        location: Default::default(),
                        data_type: p.language_type.clone(),
                        ownership: Ownership::ImmutableReference,
                    },
                }
            })
            .collect();

        let returns = match self.return_type {
            HostAbiType::Void => vec![],
            _ => vec![self.return_type_to_datatype()],
        };

        FunctionSignature {
            parameters,
            returns,
        }
    }

    pub(crate) fn return_type_to_datatype(&self) -> DataType {
        match self.return_type {
            HostAbiType::I32 => DataType::Int,
            HostAbiType::F64 => DataType::Float,
            HostAbiType::Utf8Str => DataType::String,
            HostAbiType::OpaquePtr => DataType::Int,
            HostAbiType::Void => DataType::None,
        }
    }
}

// ======================================================
//                 BACKEND BINDINGS
// ======================================================

#[derive(Debug, Clone)]
pub struct JsHostBinding {
    pub js_path: String, // e.g. "console.log"
}

#[derive(Debug, Clone)]
pub struct WasmHostBinding {
    pub module: String,
    pub import_name: String,
}

#[derive(Debug, Clone, Default)]
pub struct HostBindings {
    pub js: Option<JsHostBinding>,
    pub wasm: Option<WasmHostBinding>,
}

// ======================================================
//                    REGISTRY
// ======================================================

#[derive(Clone, Default)]
pub struct HostRegistry {
    functions: HashMap<InternedString, HostFunctionDef>,
    bindings: HashMap<InternedString, HostBindings>,
}

impl HostRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_function(
        &mut self,
        function: HostFunctionDef,
        string_table: &StringTable,
    ) -> Result<(), CompilerError> {
        validate_host_function_def(&function, string_table)?;

        if self.functions.contains_key(&function.name) {
            return_compiler_error!(
                "Host function '{}' is already registered.",
                string_table.resolve(function.name)
            );
        }

        self.functions.insert(function.name, function);
        Ok(())
    }

    pub fn register_bindings(&mut self, name: InternedString, bindings: HostBindings) {
        self.bindings.insert(name, bindings);
    }

    pub fn get_function(&self, name: &InternedString) -> Option<&HostFunctionDef> {
        self.functions.get(name)
    }

    pub fn get_bindings(&self, name: &InternedString) -> Option<&HostBindings> {
        self.bindings.get(name)
    }

    pub fn list_functions(&self) -> impl Iterator<Item = &HostFunctionDef> {
        self.functions.values()
    }

    pub fn validate_io_availability(
        &self,
        string_table: &StringTable,
    ) -> Result<(), CompilerError> {
        let has_io = self
            .functions
            .keys()
            .any(|id| string_table.resolve(*id) == "io");

        if !has_io {
            return_compiler_error!("Build system does not provide required 'io()' function.");
        }

        Ok(())
    }
}

// ======================================================
//                  ERROR HANDLING
// ======================================================

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ErrorHandling {
    None,
    ReturnsError,
    Panics,
}

// ======================================================
//                 VALIDATION
// ======================================================

fn validate_host_function_def(
    function: &HostFunctionDef,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    let name = string_table.resolve(function.name);

    if name.is_empty() {
        return_compiler_error!("Host function has empty name.");
    }

    if name == "io" {
        validate_io_function(function, string_table)?;
    }

    Ok(())
}

fn validate_io_function(
    function: &HostFunctionDef,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    let name = string_table.resolve(function.name);

    if function.parameters.len() != 1 {
        return_compiler_error!("Function '{}' must take exactly one parameter.", name);
    }

    let param = &function.parameters[0];

    if !matches!(param.language_type, DataType::CoerceToString) {
        return_compiler_error!("Function '{}' parameter must be CoerceToString.", name);
    }

    if param.abi_type != HostAbiType::Utf8Str {
        return_compiler_error!("Function '{}' must use Utf8Str ABI.", name);
    }

    if function.return_type != HostAbiType::Void {
        return_compiler_error!("Function '{}' must return void.", name);
    }

    Ok(())
}

// ======================================================
//               BUILTIN REGISTRY
// ======================================================

pub fn create_builtin_registry(
    string_table: &mut StringTable,
) -> Result<HostRegistry, CompilerError> {
    let mut registry = HostRegistry::new();

    let io_name = string_table.intern("io");

    let io_function = HostFunctionDef {
        name: io_name,
        parameters: vec![HostParameter {
            language_type: DataType::CoerceToString,
            abi_type: HostAbiType::Utf8Str,
        }],
        return_type: HostAbiType::Void,
        ownership: Ownership::ImmutableReference,
        error_handling: ErrorHandling::None,
        description: "Output text to the host environment.".into(),
    };

    registry.register_function(io_function, string_table)?;

    registry.register_bindings(
        io_name,
        HostBindings {
            js: Some(JsHostBinding {
                js_path: "console.log".into(),
            }),
            wasm: Some(WasmHostBinding {
                module: "beanstalk_io".into(),
                import_name: "io".into(),
            }),
        },
    );

    registry.validate_io_availability(string_table)?;
    Ok(registry)
}
