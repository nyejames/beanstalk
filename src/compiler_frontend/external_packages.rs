//! Builtin external function metadata and registry.
//!
//! WHAT: defines the external-call surface the frontend and borrow checker understand today.
//! WHY: external calls need one canonical metadata source for signature lowering and call semantics.

use crate::compiler_frontend::ast::statements::functions::{FunctionReturn, ReturnSlot};
#[cfg(test)]
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::ids::FunctionId;
use crate::compiler_frontend::interned_path::InternedPath;
#[cfg(test)]
use crate::return_compiler_error;
use std::collections::HashMap;

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
    ExternalFunction(InternedPath),
}

/// Backend-agnostic ABI values that currently cross the host boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExternalAbiType {
    I32,
    Utf8Str,
    Void,
    /// Parameter accepts any language type (used for polymorphic external functions
    /// such as collection helpers and `io()` during the transition to explicit ABI types).
    Inferred,
}

/// A single external-call parameter definition.
#[derive(Debug, Clone)]
pub struct ExternalParameter {
    /// What the Beanstalk language accepts.
    pub language_type: ExternalAbiType,
    /// Borrow access mode required for this argument.
    pub access_kind: ExternalAccessKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExternalAccessKind {
    Shared,
    Mutable,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExternalReturnAlias {
    Fresh,
    AliasArgs(Vec<usize>),
}

#[derive(Debug, Clone)]
pub struct ExternalFunctionDef {
    pub name: &'static str,
    pub parameters: Vec<ExternalParameter>,
    pub return_type: ExternalAbiType,
    pub return_alias: ExternalReturnAlias,
}

impl ExternalAbiType {
    /// Maps this ABI type to the corresponding frontend `DataType` when one exists.
    pub(crate) fn to_datatype(&self) -> Option<DataType> {
        match self {
            ExternalAbiType::I32 => Some(DataType::Int),
            ExternalAbiType::Utf8Str => Some(DataType::StringSlice),
            ExternalAbiType::Void => None,
            ExternalAbiType::Inferred => Some(DataType::Inferred),
        }
    }
}

impl ExternalFunctionDef {
    pub(crate) fn return_type_to_datatype(&self) -> Option<DataType> {
        self.return_type.to_datatype()
    }

    pub(crate) fn return_slots(&self) -> Vec<ReturnSlot> {
        let Some(return_data_type) = self.return_type_to_datatype() else {
            return Vec::new();
        };

        match self.return_alias {
            ExternalReturnAlias::Fresh => {
                vec![ReturnSlot::success(FunctionReturn::Value(return_data_type))]
            }
            ExternalReturnAlias::AliasArgs(ref parameter_indices) => {
                vec![ReturnSlot::success(FunctionReturn::AliasCandidates {
                    parameter_indices: parameter_indices.clone(),
                    data_type: return_data_type,
                })]
            }
        }
    }

    pub(crate) fn return_data_types(&self) -> Vec<DataType> {
        self.return_slots()
            .iter()
            .map(|slot| slot.data_type().clone())
            .collect()
    }
}

/// A single virtual package provided by a project builder.
#[derive(Clone, Debug, Default)]
pub struct ExternalPackage {
    pub path: &'static str,
    pub functions: HashMap<&'static str, ExternalFunctionDef>,
}

impl ExternalPackage {
    pub fn new(path: &'static str) -> Self {
        Self {
            path,
            functions: HashMap::new(),
        }
    }

    pub fn with_function(mut self, function: ExternalFunctionDef) -> Self {
        self.functions.insert(function.name, function);
        self
    }
}

#[derive(Clone, Default)]
pub struct ExternalPackageRegistry {
    packages: HashMap<&'static str, ExternalPackage>,
}

impl ExternalPackageRegistry {
    /// Builds the builtin external package registry used by normal frontend compilation.
    pub fn new() -> Self {
        let mut registry = ExternalPackageRegistry::default();

        let std_io = ExternalPackage::new("@std/io").with_function(ExternalFunctionDef {
            name: IO_FUNC_NAME,
            parameters: vec![ExternalParameter {
                language_type: ExternalAbiType::Inferred,
                access_kind: ExternalAccessKind::Shared,
            }],
            return_type: ExternalAbiType::Void,
            return_alias: ExternalReturnAlias::Fresh,
        });
        registry.packages.insert(std_io.path, std_io);

        let std_collections = ExternalPackage::new("@std/collections")
            .with_function(ExternalFunctionDef {
                name: COLLECTION_GET_HOST_NAME,
                parameters: vec![
                    ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Shared,
                    },
                    ExternalParameter {
                        language_type: ExternalAbiType::I32,
                        access_kind: ExternalAccessKind::Shared,
                    },
                ],
                return_type: ExternalAbiType::Void,
                return_alias: ExternalReturnAlias::Fresh,
            })
            .with_function(ExternalFunctionDef {
                name: COLLECTION_PUSH_HOST_NAME,
                parameters: vec![
                    ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Mutable,
                    },
                    ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Shared,
                    },
                ],
                return_type: ExternalAbiType::Void,
                return_alias: ExternalReturnAlias::Fresh,
            })
            .with_function(ExternalFunctionDef {
                name: COLLECTION_REMOVE_HOST_NAME,
                parameters: vec![
                    ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Mutable,
                    },
                    ExternalParameter {
                        language_type: ExternalAbiType::I32,
                        access_kind: ExternalAccessKind::Shared,
                    },
                ],
                return_type: ExternalAbiType::Void,
                return_alias: ExternalReturnAlias::Fresh,
            })
            .with_function(ExternalFunctionDef {
                name: COLLECTION_LENGTH_HOST_NAME,
                parameters: vec![ExternalParameter {
                    language_type: ExternalAbiType::Inferred,
                    access_kind: ExternalAccessKind::Shared,
                }],
                return_type: ExternalAbiType::I32,
                return_alias: ExternalReturnAlias::Fresh,
            });
        registry
            .packages
            .insert(std_collections.path, std_collections);

        let std_error = ExternalPackage::new("@std/error")
            .with_function(ExternalFunctionDef {
                name: ERROR_WITH_LOCATION_HOST_NAME,
                parameters: vec![
                    ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Shared,
                    },
                    ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Shared,
                    },
                ],
                return_type: ExternalAbiType::Void,
                return_alias: ExternalReturnAlias::Fresh,
            })
            .with_function(ExternalFunctionDef {
                name: ERROR_PUSH_TRACE_HOST_NAME,
                parameters: vec![
                    ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Shared,
                    },
                    ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Shared,
                    },
                ],
                return_type: ExternalAbiType::Void,
                return_alias: ExternalReturnAlias::Fresh,
            })
            .with_function(ExternalFunctionDef {
                name: ERROR_BUBBLE_HOST_NAME,
                parameters: vec![
                    ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Shared,
                    },
                    ExternalParameter {
                        language_type: ExternalAbiType::Utf8Str,
                        access_kind: ExternalAccessKind::Shared,
                    },
                    ExternalParameter {
                        language_type: ExternalAbiType::I32,
                        access_kind: ExternalAccessKind::Shared,
                    },
                    ExternalParameter {
                        language_type: ExternalAbiType::I32,
                        access_kind: ExternalAccessKind::Shared,
                    },
                    ExternalParameter {
                        language_type: ExternalAbiType::Utf8Str,
                        access_kind: ExternalAccessKind::Shared,
                    },
                ],
                return_type: ExternalAbiType::Void,
                return_alias: ExternalReturnAlias::Fresh,
            });
        registry.packages.insert(std_error.path, std_error);

        registry
    }

    /// Registers a synthetic external function for test-only lowering and borrow-check scenarios.
    #[cfg(test)]
    pub fn register_function(
        &mut self,
        function: ExternalFunctionDef,
    ) -> Result<(), CompilerError> {
        let test_package = self
            .packages
            .entry("@test/default")
            .or_insert_with(|| ExternalPackage::new("@test/default"));
        if test_package.functions.contains_key(&function.name) {
            return_compiler_error!(
                "External function '{:?}' is already registered.",
                function.name
            );
        }
        test_package.functions.insert(function.name, function);
        Ok(())
    }

    /// Looks up an external function by name across all packages.
    /// Used for prelude-visible symbols and internal compiler-generated calls.
    pub fn get_function(&self, name: &str) -> Option<&ExternalFunctionDef> {
        for package in self.packages.values() {
            if let Some(function) = package.functions.get(name) {
                return Some(function);
            }
        }
        None
    }

    /// Looks up a specific package by path.
    pub fn get_package(&self, path: &str) -> Option<&ExternalPackage> {
        self.packages.get(path)
    }

    /// Resolves a symbol within a specific package.
    pub fn resolve_package_symbol(
        &self,
        package_path: &str,
        symbol_name: &str,
    ) -> Option<&ExternalFunctionDef> {
        self.packages
            .get(package_path)
            .and_then(|package| package.functions.get(symbol_name))
    }

    /// Returns true if the registry contains a package with the given path.
    pub fn has_package(&self, path: &str) -> bool {
        self.packages.contains_key(path)
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::{
        ExternalAbiType, ExternalAccessKind, ExternalFunctionDef, ExternalPackageRegistry,
        ExternalParameter, ExternalReturnAlias,
    };
    use crate::compiler_frontend::compiler_errors::CompilerError;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum TestExternalAbiType {
        I32,
        Utf8Str,
        Void,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum TestExternalAccessKind {
        Shared,
        Mutable,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub enum TestExternalReturnAlias {
        Fresh,
        AliasArgs(Vec<usize>),
    }

    impl From<TestExternalAbiType> for ExternalAbiType {
        fn from(value: TestExternalAbiType) -> Self {
            match value {
                TestExternalAbiType::I32 => ExternalAbiType::I32,
                TestExternalAbiType::Utf8Str => ExternalAbiType::Utf8Str,
                TestExternalAbiType::Void => ExternalAbiType::Void,
            }
        }
    }

    impl From<TestExternalAccessKind> for ExternalAccessKind {
        fn from(value: TestExternalAccessKind) -> Self {
            match value {
                TestExternalAccessKind::Shared => ExternalAccessKind::Shared,
                TestExternalAccessKind::Mutable => ExternalAccessKind::Mutable,
            }
        }
    }

    impl From<TestExternalReturnAlias> for ExternalReturnAlias {
        fn from(value: TestExternalReturnAlias) -> Self {
            match value {
                TestExternalReturnAlias::Fresh => ExternalReturnAlias::Fresh,
                TestExternalReturnAlias::AliasArgs(indices) => {
                    ExternalReturnAlias::AliasArgs(indices)
                }
            }
        }
    }

    /// Registers a synthetic external function using test-local metadata wrappers.
    pub fn register_test_external_function(
        registry: &mut ExternalPackageRegistry,
        name: &'static str,
        parameters: Vec<(ExternalAbiType, TestExternalAccessKind)>,
        return_alias: TestExternalReturnAlias,
        return_type: TestExternalAbiType,
    ) -> Result<(), CompilerError> {
        registry.register_function(ExternalFunctionDef {
            name,
            parameters: parameters
                .into_iter()
                .map(|(language_type, access_kind)| ExternalParameter {
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
#[path = "tests/external_packages_tests.rs"]
mod external_packages_tests;
