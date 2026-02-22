use crate::compiler_frontend::ast::ast_nodes::Var;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::return_compiler_error;
use std::collections::HashMap;
use std::path::PathBuf;

pub const IO_FUNC_NAME: &str = "io";
pub const ALLOC_FUNC_NAME: &str = "alloc";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Js,
    Wasm,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CallTarget {
    UserFunction(InternedPath),
    HostFunction(InternedPath),
}

impl CallTarget {
    pub fn as_string(&self, string_table: &StringTable) -> String {
        let path = match self {
            CallTarget::UserFunction(path) | CallTarget::HostFunction(path) => path,
        };

        path.name_str(string_table)
            .map(str::to_owned)
            .unwrap_or_else(|| path.to_string(string_table))
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

    /// Borrow access mode required for this argument.
    pub access_kind: HostAccessKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostAccessKind {
    Shared,
    Mutable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostReturnAlias {
    Fresh,
    AliasAnyArg,
    AliasMutableArgs,
}

// ======================================================
//               HOST FUNCTION DEFINITION
// ======================================================
#[derive(Debug, Clone)]
pub struct HostFunctionDef {
    pub name: &'static str, // A unique name for each supported host function
    pub parameters: Vec<HostParameter>,
    pub return_type: HostAbiType,
    pub return_alias: HostReturnAlias,
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
                let name = PathBuf::from(self.name).join(format!("_arg{}", i));

                Var {
                    id: InternedPath::from_path_buf(&name, string_table),
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
    functions: HashMap<&'static str, HostFunctionDef>,
    bindings: HashMap<&'static str, HostBindings>,
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
        let io_function = HostFunctionDef {
            name: IO_FUNC_NAME,
            parameters: vec![HostParameter {
                language_type: DataType::String,
                abi_type: HostAbiType::Utf8Str,
                access_kind: HostAccessKind::Shared,
            }],
            return_type: HostAbiType::Void,
            return_alias: HostReturnAlias::Fresh,
            ownership: Ownership::ImmutableReference,
            error_handling: ErrorHandling::None,
            description: "Output text to the host environment.".into(),
        };

        registry.functions.insert(io_function.name, io_function);

        registry
    }

    pub fn register_function(&mut self, function: HostFunctionDef) -> Result<(), CompilerError> {
        if self.functions.contains_key(&function.name) {
            return_compiler_error!("Host function '{:?}' is already registered.", function.name);
        }

        self.functions.insert(function.name, function);
        Ok(())
    }

    pub fn register_bindings(&mut self, name: &'static str, bindings: HostBindings) {
        self.bindings.insert(name, bindings);
    }

    pub fn get_function(&self, name: &str) -> Option<&HostFunctionDef> {
        self.functions.get(name)
    }

    pub fn get_bindings(&self, id: &str) -> Option<&HostBindings> {
        self.bindings.get(id)
    }

    pub fn list_functions(&self) -> impl Iterator<Item = &HostFunctionDef> {
        self.functions.values()
    }

    // ======================================================
    //                     VALIDATION
    // ======================================================
    pub fn validate_required_hosts(&self, required: &[&'static str]) -> Result<(), CompilerError> {
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
        required_hosts: &[&'static str],
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
