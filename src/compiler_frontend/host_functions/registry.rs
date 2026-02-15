use crate::compiler_frontend::ast::ast_nodes::Var;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::string_interning::{InternedString, StringId, StringTable};
use crate::return_compiler_error;
use std::collections::HashMap;
use std::fmt::{self, Display};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Js,
    Wasm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostFunctionId {
    Io,
    Alloc, // Host environment manages heap
           // Future:
           // Now,
           // Fetch,
           // Alert,
}

impl Display for HostFunctionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostFunctionId::Io => write!(f, "io"),
            HostFunctionId::Alloc => write!(f, "alloc"),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq)]
pub enum CallTarget {
    UserFunction(InternedString),
    HostFunction(HostFunctionId),
}

impl CallTarget {
    pub fn as_string(&self, string_table: &StringTable) -> String {
        match self {
            CallTarget::UserFunction(user_function_id) => {
                string_table.resolve(*user_function_id).to_string()
            }
            CallTarget::HostFunction(host_function_id) => host_function_id.to_string(),
        }
    }
}

// ======================================================
//                    HOST ABI
// ======================================================

/// Backend-agnostic ABI values that cross the host boundary
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HostAbiType {
    I32,
    F64,
    Utf8Str,
    OpaquePtr, // Might want to turn this into ExternRef / Any at some point
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
    pub host_func_id: HostFunctionId,
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

    /// Converts host function parameters into a Beanstalk FunctionSignature.
    /// This allows host functions to be type-checked like regular Beanstalk functions.
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
    functions: HashMap<HostFunctionId, HostFunctionDef>,
    bindings: HashMap<HostFunctionId, HostBindings>,
}

impl HostRegistry {
    pub fn new(string_table: &mut StringTable) -> Self {
        let mut registry = HostRegistry::default();

        // This function creates the built-in Beanstalk registry
        // This must be the same for all projects regardless of the build system.
        // So all new registries always start with the following
        // ======================================================
        //                   BUILTIN REGISTRY
        // ======================================================
        let io_name = string_table.intern("io");
        let io_function = HostFunctionDef {
            name: io_name,
            host_func_id: HostFunctionId::Io,
            parameters: vec![HostParameter {
                language_type: DataType::String,
                abi_type: HostAbiType::Utf8Str,
            }],
            return_type: HostAbiType::Void,
            ownership: Ownership::ImmutableReference,
            error_handling: ErrorHandling::None,
            description: "Output text to the host environment.".into(),
        };

        registry
            .functions
            .insert(io_function.host_func_id, io_function);

        registry
    }

    pub fn register_function(&mut self, function: HostFunctionDef) -> Result<(), CompilerError> {
        if self.functions.contains_key(&function.host_func_id) {
            return_compiler_error!(
                "Host function '{}' is already registered.",
                function.host_func_id
            );
        }

        self.functions.insert(function.host_func_id, function);
        Ok(())
    }

    pub fn register_bindings(&mut self, id: HostFunctionId, bindings: HostBindings) {
        self.bindings.insert(id, bindings);
    }

    pub fn get_function(&self, name: &StringId) -> Option<&HostFunctionDef> {
        self.functions.values().find(|f| f.name == *name)
    }

    pub fn get_bindings(&self, id: &HostFunctionId) -> Option<&HostBindings> {
        self.bindings.get(id)
    }

    pub fn list_functions(&self) -> impl Iterator<Item = &HostFunctionDef> {
        self.functions.values()
    }

    // ======================================================
    //                     VALIDATION
    // ======================================================

    pub fn validate_required_hosts(
        &self,
        required: &[HostFunctionId],
    ) -> Result<(), CompilerError> {
        for id in required {
            if !self.functions.contains_key(id) {
                return_compiler_error!("Required host function '{}' is not registered.", id);
            }
        }

        Ok(())
    }
    pub fn validate_backend_bindings(&self, backend: BackendKind) -> Result<(), CompilerError> {
        for (id, _) in &self.functions {
            let bindings = self.bindings.get(id).ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "Host function '{}' has no backend bindings registered.",
                    id
                ))
            })?;

            let has_binding = match backend {
                BackendKind::Js => bindings.js.is_some(),
                BackendKind::Wasm => bindings.wasm.is_some(),
            };

            if !has_binding {
                return_compiler_error!(
                    "Host function '{}' is not available on the {:?} backend.",
                    id,
                    backend
                );
            }
        }

        Ok(())
    }
    pub fn validate_for_backend(
        &self,
        backend: BackendKind,
        required_hosts: &[HostFunctionId],
    ) -> Result<(), CompilerError> {
        self.validate_required_hosts(required_hosts)?;
        self.validate_backend_bindings(backend)?;
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
