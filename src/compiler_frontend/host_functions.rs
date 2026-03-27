//! Builtin host function metadata and registry.
//!
//! WHAT: defines the host-call surface the frontend and borrow checker understand today.
//! WHY: host calls need one canonical metadata source for signature lowering and call semantics.

use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::functions::{FunctionReturn, FunctionSignature};
#[cfg(test)]
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::hir::hir_nodes::FunctionId;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
#[cfg(test)]
use crate::return_compiler_error;
use std::collections::HashMap;
use std::path::PathBuf;

pub const IO_FUNC_NAME: &str = "io";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CallTarget {
    UserFunction(FunctionId),
    HostFunction(InternedPath),
}

/// Backend-agnostic ABI values that currently cross the host boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HostAbiType {
    #[cfg(test)]
    I32,
    Utf8Str,
    Void,
}

/// A single host-call parameter definition.
#[derive(Debug, Clone)]
pub struct HostParameter {
    /// What the Beanstalk language accepts.
    pub language_type: DataType,
    /// Borrow access mode required for this argument.
    pub access_kind: HostAccessKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostAccessKind {
    Shared,
    #[cfg(test)]
    Mutable,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HostReturnAlias {
    Fresh,
    #[cfg(test)]
    AliasArgs(Vec<usize>),
}

#[derive(Debug, Clone)]
pub struct HostFunctionDef {
    pub name: &'static str,
    pub parameters: Vec<HostParameter>,
    pub return_type: HostAbiType,
    pub return_alias: HostReturnAlias,
}

impl HostFunctionDef {
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
            #[cfg(test)]
            HostReturnAlias::AliasArgs(ref parameter_indices) if !returns.is_empty() => {
                vec![FunctionReturn::AliasCandidates {
                    parameter_indices: parameter_indices.clone(),
                    data_type: returns[0].clone(),
                }]
            }
            #[cfg(test)]
            HostReturnAlias::AliasArgs(_) => Vec::new(),
        };

        FunctionSignature {
            parameters,
            returns,
        }
    }

    pub(crate) fn return_type_to_datatype(&self) -> Option<DataType> {
        match self.return_type {
            #[cfg(test)]
            HostAbiType::I32 => Some(DataType::Int),
            HostAbiType::Utf8Str => Some(DataType::StringSlice),
            HostAbiType::Void => None,
        }
    }
}

#[derive(Clone, Default)]
pub struct HostRegistry {
    functions: HashMap<&'static str, HostFunctionDef>,
}

impl HostRegistry {
    /// Builds the builtin host registry used by normal frontend compilation.
    pub fn new() -> Self {
        let mut registry = HostRegistry::default();

        let io_function = HostFunctionDef {
            name: IO_FUNC_NAME,
            parameters: vec![HostParameter {
                language_type: DataType::CoerceToString,
                access_kind: HostAccessKind::Shared,
            }],
            return_type: HostAbiType::Void,
            return_alias: HostReturnAlias::Fresh,
        };

        registry.functions.insert(io_function.name, io_function);
        registry
    }

    /// Registers a synthetic host function for test-only lowering and borrow-check scenarios.
    #[cfg(test)]
    pub fn register_function(&mut self, function: HostFunctionDef) -> Result<(), CompilerError> {
        if self.functions.contains_key(&function.name) {
            return_compiler_error!("Host function '{:?}' is already registered.", function.name);
        }

        self.functions.insert(function.name, function);
        Ok(())
    }

    pub fn get_function(&self, name: &str) -> Option<&HostFunctionDef> {
        self.functions.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler_frontend::ast::statements::functions::FunctionReturn;

    #[test]
    fn params_to_signature_preserves_alias_metadata() {
        let mut string_table = StringTable::new();
        let host_function = HostFunctionDef {
            name: "concat_like",
            parameters: vec![
                HostParameter {
                    language_type: DataType::StringSlice,
                    access_kind: HostAccessKind::Shared,
                },
                HostParameter {
                    language_type: DataType::StringSlice,
                    access_kind: HostAccessKind::Shared,
                },
            ],
            return_type: HostAbiType::Utf8Str,
            return_alias: HostReturnAlias::AliasArgs(vec![1]),
        };

        let signature = host_function.params_to_signature(&mut string_table);
        assert_eq!(signature.parameters.len(), 2);
        assert_eq!(signature.returns.len(), 1);
        assert!(matches!(
            &signature.returns[0],
            FunctionReturn::AliasCandidates {
                parameter_indices,
                data_type
            } if parameter_indices == &vec![1] && data_type == &DataType::StringSlice
        ));
    }

    #[test]
    fn register_function_rejects_duplicates() {
        let mut registry = HostRegistry::new();
        let result = registry.register_function(HostFunctionDef {
            name: IO_FUNC_NAME,
            parameters: Vec::new(),
            return_type: HostAbiType::Void,
            return_alias: HostReturnAlias::Fresh,
        });

        assert!(result.is_err());
    }
}
