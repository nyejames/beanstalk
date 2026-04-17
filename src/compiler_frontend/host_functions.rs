//! Builtin host function metadata and registry.
//!
//! WHAT: defines the host-call surface the frontend and borrow checker understand today.
//! WHY: host calls need one canonical metadata source for signature lowering and call semantics.

use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnSlot,
};
#[cfg(test)]
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::source_location::SourceLocation;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::hir::hir_nodes::FunctionId;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
#[cfg(test)]
use crate::return_compiler_error;
use std::collections::HashMap;
use std::path::PathBuf;

pub const IO_FUNC_NAME: &str = "io";
pub const COLLECTION_GET_HOST_NAME: &str = "__bs_collection_get";
pub const COLLECTION_PUSH_HOST_NAME: &str = "__bs_collection_push";
pub const COLLECTION_REMOVE_HOST_NAME: &str = "__bs_collection_remove";
pub const COLLECTION_LENGTH_HOST_NAME: &str = "__bs_collection_length";
pub const ERROR_WITH_LOCATION_HOST_NAME: &str = "__bs_error_with_location";
pub const ERROR_PUSH_TRACE_HOST_NAME: &str = "__bs_error_push_trace";
pub const ERROR_BUBBLE_HOST_NAME: &str = "__bs_error_bubble";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CallTarget {
    UserFunction(FunctionId),
    HostFunction(InternedPath),
}

/// Backend-agnostic ABI values that currently cross the host boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HostAbiType {
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
    Mutable,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HostReturnAlias {
    Fresh,
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
                        kind: ExpressionKind::NoValue,
                        location: SourceLocation::default(),
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
            HostReturnAlias::Fresh => returns
                .iter()
                .cloned()
                .map(FunctionReturn::Value)
                .map(ReturnSlot::success)
                .collect(),
            HostReturnAlias::AliasArgs(ref parameter_indices) if !returns.is_empty() => {
                vec![ReturnSlot::success(FunctionReturn::AliasCandidates {
                    parameter_indices: parameter_indices.clone(),
                    data_type: returns[0].clone(),
                })]
            }
            HostReturnAlias::AliasArgs(_) => Vec::new(),
        };

        FunctionSignature {
            parameters,
            returns,
        }
    }

    pub(crate) fn return_type_to_datatype(&self) -> Option<DataType> {
        match self.return_type {
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
                language_type: DataType::Inferred,
                access_kind: HostAccessKind::Shared,
            }],
            return_type: HostAbiType::Void,
            return_alias: HostReturnAlias::Fresh,
        };

        registry.functions.insert(io_function.name, io_function);
        registry.functions.insert(
            COLLECTION_GET_HOST_NAME,
            HostFunctionDef {
                name: COLLECTION_GET_HOST_NAME,
                parameters: vec![
                    HostParameter {
                        language_type: DataType::Inferred,
                        access_kind: HostAccessKind::Shared,
                    },
                    HostParameter {
                        language_type: DataType::Int,
                        access_kind: HostAccessKind::Shared,
                    },
                ],
                return_type: HostAbiType::Void,
                return_alias: HostReturnAlias::Fresh,
            },
        );
        registry.functions.insert(
            COLLECTION_PUSH_HOST_NAME,
            HostFunctionDef {
                name: COLLECTION_PUSH_HOST_NAME,
                parameters: vec![
                    HostParameter {
                        language_type: DataType::Inferred,
                        access_kind: HostAccessKind::Mutable,
                    },
                    HostParameter {
                        language_type: DataType::Inferred,
                        access_kind: HostAccessKind::Shared,
                    },
                ],
                return_type: HostAbiType::Void,
                return_alias: HostReturnAlias::Fresh,
            },
        );
        registry.functions.insert(
            COLLECTION_REMOVE_HOST_NAME,
            HostFunctionDef {
                name: COLLECTION_REMOVE_HOST_NAME,
                parameters: vec![
                    HostParameter {
                        language_type: DataType::Inferred,
                        access_kind: HostAccessKind::Mutable,
                    },
                    HostParameter {
                        language_type: DataType::Int,
                        access_kind: HostAccessKind::Shared,
                    },
                ],
                return_type: HostAbiType::Void,
                return_alias: HostReturnAlias::Fresh,
            },
        );
        registry.functions.insert(
            COLLECTION_LENGTH_HOST_NAME,
            HostFunctionDef {
                name: COLLECTION_LENGTH_HOST_NAME,
                parameters: vec![HostParameter {
                    language_type: DataType::Inferred,
                    access_kind: HostAccessKind::Shared,
                }],
                return_type: HostAbiType::I32,
                return_alias: HostReturnAlias::Fresh,
            },
        );
        registry.functions.insert(
            ERROR_WITH_LOCATION_HOST_NAME,
            HostFunctionDef {
                name: ERROR_WITH_LOCATION_HOST_NAME,
                parameters: vec![
                    HostParameter {
                        language_type: DataType::Inferred,
                        access_kind: HostAccessKind::Shared,
                    },
                    HostParameter {
                        language_type: DataType::Inferred,
                        access_kind: HostAccessKind::Shared,
                    },
                ],
                return_type: HostAbiType::Void,
                return_alias: HostReturnAlias::Fresh,
            },
        );
        registry.functions.insert(
            ERROR_PUSH_TRACE_HOST_NAME,
            HostFunctionDef {
                name: ERROR_PUSH_TRACE_HOST_NAME,
                parameters: vec![
                    HostParameter {
                        language_type: DataType::Inferred,
                        access_kind: HostAccessKind::Shared,
                    },
                    HostParameter {
                        language_type: DataType::Inferred,
                        access_kind: HostAccessKind::Shared,
                    },
                ],
                return_type: HostAbiType::Void,
                return_alias: HostReturnAlias::Fresh,
            },
        );
        registry.functions.insert(
            ERROR_BUBBLE_HOST_NAME,
            HostFunctionDef {
                name: ERROR_BUBBLE_HOST_NAME,
                parameters: vec![
                    HostParameter {
                        language_type: DataType::Inferred,
                        access_kind: HostAccessKind::Shared,
                    },
                    HostParameter {
                        language_type: DataType::StringSlice,
                        access_kind: HostAccessKind::Shared,
                    },
                    HostParameter {
                        language_type: DataType::Int,
                        access_kind: HostAccessKind::Shared,
                    },
                    HostParameter {
                        language_type: DataType::Int,
                        access_kind: HostAccessKind::Shared,
                    },
                    HostParameter {
                        language_type: DataType::StringSlice,
                        access_kind: HostAccessKind::Shared,
                    },
                ],
                return_type: HostAbiType::Void,
                return_alias: HostReturnAlias::Fresh,
            },
        );
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
pub(crate) mod test_support {
    use super::{
        HostAbiType, HostAccessKind, HostFunctionDef, HostParameter, HostRegistry, HostReturnAlias,
    };
    use crate::compiler_frontend::compiler_errors::CompilerError;
    use crate::compiler_frontend::datatypes::DataType;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum TestHostAbiType {
        I32,
        Utf8Str,
        Void,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum TestHostAccessKind {
        Shared,
        Mutable,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub enum TestHostReturnAlias {
        Fresh,
        AliasArgs(Vec<usize>),
    }

    impl From<TestHostAbiType> for HostAbiType {
        fn from(value: TestHostAbiType) -> Self {
            match value {
                TestHostAbiType::I32 => HostAbiType::I32,
                TestHostAbiType::Utf8Str => HostAbiType::Utf8Str,
                TestHostAbiType::Void => HostAbiType::Void,
            }
        }
    }

    impl From<TestHostAccessKind> for HostAccessKind {
        fn from(value: TestHostAccessKind) -> Self {
            match value {
                TestHostAccessKind::Shared => HostAccessKind::Shared,
                TestHostAccessKind::Mutable => HostAccessKind::Mutable,
            }
        }
    }

    impl From<TestHostReturnAlias> for HostReturnAlias {
        fn from(value: TestHostReturnAlias) -> Self {
            match value {
                TestHostReturnAlias::Fresh => HostReturnAlias::Fresh,
                TestHostReturnAlias::AliasArgs(indices) => HostReturnAlias::AliasArgs(indices),
            }
        }
    }

    /// Registers a synthetic host function using test-local metadata wrappers.
    pub fn register_test_host_function(
        registry: &mut HostRegistry,
        name: &'static str,
        parameters: Vec<(DataType, TestHostAccessKind)>,
        return_alias: TestHostReturnAlias,
        return_type: TestHostAbiType,
    ) -> Result<(), CompilerError> {
        registry.register_function(HostFunctionDef {
            name,
            parameters: parameters
                .into_iter()
                .map(|(language_type, access_kind)| HostParameter {
                    language_type,
                    access_kind: access_kind.into(),
                })
                .collect(),
            return_type: return_type.into(),
            return_alias: return_alias.into(),
        })
    }
}

#[cfg(test)]
#[path = "tests/host_functions_tests.rs"]
mod host_functions_tests;
