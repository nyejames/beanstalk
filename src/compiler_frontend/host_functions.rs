use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::functions::{FunctionReturn, FunctionSignature};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::return_compiler_error;
use std::collections::HashMap;
use std::path::PathBuf;

pub const IO_FUNC_NAME: &str = "io";
#[allow(dead_code)] // todo
pub const ALLOC_FUNC_NAME: &str = "alloc";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // todo
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
    #[allow(dead_code)] // todo
    pub fn as_string(&self, string_table: &StringTable) -> String {
        let path = match self {
            CallTarget::UserFunction(path) | CallTarget::HostFunction(path) => path,
        };

        path.name_str(string_table)
            .map(str::to_owned)
            .unwrap_or_else(|| path.to_string(string_table))
    }
}

/// Backend-agnostic ABI values that cross the host boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HostAbiType {
    #[allow(dead_code)] // todo
    I32,
    #[allow(dead_code)] // todo
    F64,
    Utf8Str,
    #[allow(dead_code)] // todo
    OpaquePtr,
    Void,
}

/// A single host-call parameter definition.
#[derive(Debug, Clone)]
pub struct HostParameter {
    /// What the Beanstalk language accepts.
    pub language_type: DataType,
    /// What crosses the ABI boundary.
    #[allow(dead_code)] // todo
    pub abi_type: HostAbiType,
    /// Borrow access mode required for this argument.
    pub access_kind: HostAccessKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostAccessKind {
    Shared,
    #[allow(dead_code)] // todo
    Mutable,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HostReturnAlias {
    Fresh,
    #[allow(dead_code)] // todo
    AliasArgs(Vec<usize>),
}

#[derive(Debug, Clone)]
pub struct HostFunctionDef {
    pub name: &'static str,
    pub parameters: Vec<HostParameter>,
    pub return_type: HostAbiType,
    pub return_alias: HostReturnAlias,
    #[allow(dead_code)] // todo
    pub ownership: Ownership,
    #[allow(dead_code)] // todo
    pub error_handling: ErrorHandling,
    #[allow(dead_code)] // todo
    pub description: String,
}

impl HostFunctionDef {
    #[allow(dead_code)] // todo
    pub fn as_function_type(&self, string_table: &mut StringTable) -> DataType {
        DataType::Function(Box::new(None), self.params_to_signature(string_table))
    }

    /// Converts host function parameters into a Beanstalk `FunctionSignature`.
    pub(crate) fn params_to_signature(&self, string_table: &mut StringTable) -> FunctionSignature {
        let parameters = self
            .parameters
            .iter()
            .enumerate()
            .map(|(i, parameter)| {
                let name = PathBuf::from(self.name).join(format!("_arg{i}"));

                Declaration {
                    id: InternedPath::from_path_buf(&name, string_table),
                    value: Expression {
                        kind: ExpressionKind::None,
                        location: Default::default(),
                        data_type: parameter.language_type.clone(),
                        ownership: Ownership::ImmutableReference,
                    },
                }
            })
            .collect();

        let returns = self
            .return_type_to_datatype()
            .into_iter()
            .collect::<Vec<_>>();
        let returns = match self.return_alias {
            HostReturnAlias::Fresh => returns.iter().cloned().map(FunctionReturn::Value).collect(),
            HostReturnAlias::AliasArgs(ref parameter_indices) if !returns.is_empty() => {
                vec![FunctionReturn::AliasCandidates {
                    parameter_indices: parameter_indices.clone(),
                    data_type: returns[0].clone(),
                }]
            }
            _ => Vec::new(),
        };

        FunctionSignature {
            parameters,
            returns,
        }
    }

    pub(crate) fn return_type_to_datatype(&self) -> Option<DataType> {
        match self.return_type {
            HostAbiType::I32 => Some(DataType::Int),
            HostAbiType::F64 => Some(DataType::Float),
            HostAbiType::Utf8Str => Some(DataType::StringSlice),
            HostAbiType::OpaquePtr => Some(DataType::Int),
            HostAbiType::Void => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct JsHostBinding {
    #[allow(dead_code)] // todo
    pub js_path: String,
}

#[derive(Debug, Clone)]
pub struct WasmHostBinding {
    #[allow(dead_code)] // todo
    pub module: String,
    #[allow(dead_code)] // todo
    pub import_name: String,
}

#[derive(Debug, Clone, Default)]
pub struct HostBindings {
    pub js: Option<JsHostBinding>,
    pub wasm: Option<WasmHostBinding>,
}

#[derive(Clone, Default)]
pub struct HostRegistry {
    functions: HashMap<&'static str, HostFunctionDef>,
    bindings: HashMap<&'static str, HostBindings>,
}

impl HostRegistry {
    pub fn new(string_table: &mut StringTable) -> Self {
        let mut registry = HostRegistry::default();

        let io_function = HostFunctionDef {
            name: IO_FUNC_NAME,
            parameters: vec![HostParameter {
                language_type: DataType::CoerceToString,
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
        let _ = string_table;
        registry
    }

    #[allow(dead_code)] // Used only in tests
    pub fn register_function(&mut self, function: HostFunctionDef) -> Result<(), CompilerError> {
        if self.functions.contains_key(&function.name) {
            return_compiler_error!("Host function '{:?}' is already registered.", function.name);
        }

        self.functions.insert(function.name, function);
        Ok(())
    }

    #[allow(dead_code)] // todo
    pub fn register_bindings(&mut self, name: &'static str, bindings: HostBindings) {
        self.bindings.insert(name, bindings);
    }

    pub fn get_function(&self, name: &str) -> Option<&HostFunctionDef> {
        self.functions.get(name)
    }

    #[allow(dead_code)] // todo
    pub fn get_bindings(&self, id: &str) -> Option<&HostBindings> {
        self.bindings.get(id)
    }

    #[allow(dead_code)] // todo
    pub fn list_functions(&self) -> impl Iterator<Item = &HostFunctionDef> {
        self.functions.values()
    }

    #[allow(dead_code)] // todo
    pub fn validate_required_hosts(&self, required: &[&'static str]) -> Result<(), CompilerError> {
        for id in required {
            if !self.functions.contains_key(id) {
                return_compiler_error!("Required host function '{}' is not registered.", id);
            }
        }

        Ok(())
    }

    #[allow(dead_code)] // todo
    pub fn validate_backend_bindings(&self, backend: BackendKind) -> Result<(), CompilerError> {
        for id in self.functions.keys() {
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

    #[allow(dead_code)] // todo
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ErrorHandling {
    None,
    #[allow(dead_code)] // todo
    ReturnsError,
    #[allow(dead_code)] // todo
    Panics,
}
