//! Builtin external function metadata and registry.
//!
//! WHAT: defines the external-call surface the frontend and borrow checker understand today.
//! WHY: external calls need one canonical metadata source for signature lowering and call semantics.

use crate::compiler_frontend::ast::statements::functions::{FunctionReturn, ReturnSlot};
#[cfg(test)]
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::ids::FunctionId;
#[cfg(test)]
use crate::return_compiler_error;
use std::collections::HashMap;

pub const IO_FUNC_NAME: &str = "io";
pub const IO_TYPE_NAME: &str = "IO";
pub const COLLECTION_GET_HOST_NAME: &str = "__bs_collection_get";
pub const COLLECTION_PUSH_HOST_NAME: &str = "__bs_collection_push";
pub const COLLECTION_REMOVE_HOST_NAME: &str = "__bs_collection_remove";
pub const COLLECTION_LENGTH_HOST_NAME: &str = "__bs_collection_length";
pub const ERROR_WITH_LOCATION_HOST_NAME: &str = "__bs_error_with_location";
pub const ERROR_PUSH_TRACE_HOST_NAME: &str = "__bs_error_push_trace";
pub const ERROR_BUBBLE_HOST_NAME: &str = "__bs_error_bubble";

/// Stable identifier for an external function across all compiler stages and backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExternalFunctionId {
    Io,
    CollectionGet,
    CollectionPush,
    CollectionRemove,
    CollectionLength,
    ErrorWithLocation,
    ErrorPushTrace,
    ErrorBubble,
    /// Synthetic functions registered by tests. Never emitted by production parsers.
    Synthetic(u32),
}

impl ExternalFunctionId {
    /// Human-readable name for diagnostics and HIR display.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Io => IO_FUNC_NAME,
            Self::CollectionGet => COLLECTION_GET_HOST_NAME,
            Self::CollectionPush => COLLECTION_PUSH_HOST_NAME,
            Self::CollectionRemove => COLLECTION_REMOVE_HOST_NAME,
            Self::CollectionLength => COLLECTION_LENGTH_HOST_NAME,
            Self::ErrorWithLocation => ERROR_WITH_LOCATION_HOST_NAME,
            Self::ErrorPushTrace => ERROR_PUSH_TRACE_HOST_NAME,
            Self::ErrorBubble => ERROR_BUBBLE_HOST_NAME,
            Self::Synthetic(_) => "<synthetic>",
        }
    }
}

/// Stable identifier for an external type across all compiler stages and backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExternalTypeId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CallTarget {
    UserFunction(FunctionId),
    ExternalFunction(ExternalFunctionId),
}

/// Backend-agnostic ABI values that currently cross the host boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExternalAbiType {
    I32,
    Utf8Str,
    Void,
    /// Opaque handle to an external type (lowers to `i32` in Wasm, object reference in JS).
    Handle,
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
    /// If this function is a receiver method, the ABI type of the receiver.
    /// The first entry in `parameters` is the receiver argument.
    pub receiver_type: Option<ExternalAbiType>,
    /// Access kind required for the receiver when this is a method.
    pub receiver_access: ExternalAccessKind,
}

impl ExternalAbiType {
    /// Maps this ABI type to the corresponding frontend `DataType` when one exists.
    pub(crate) fn to_datatype(&self) -> Option<DataType> {
        match self {
            ExternalAbiType::I32 => Some(DataType::Int),
            ExternalAbiType::Utf8Str => Some(DataType::StringSlice),
            ExternalAbiType::Void => None,
            ExternalAbiType::Handle => None,
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

/// Definition of a single opaque external type exposed by a virtual package.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExternalTypeDef {
    pub name: &'static str,
    pub package: &'static str,
    pub abi_type: ExternalAbiType,
}

/// A single virtual package provided by a project builder.
#[derive(Clone, Debug, Default)]
pub struct ExternalPackage {
    pub path: &'static str,
    pub functions: HashMap<&'static str, ExternalFunctionDef>,
    pub types: HashMap<&'static str, ExternalTypeDef>,
}

impl ExternalPackage {
    pub fn new(path: &'static str) -> Self {
        Self {
            path,
            functions: HashMap::new(),
            types: HashMap::new(),
        }
    }

    pub fn with_function(mut self, function: ExternalFunctionDef) -> Self {
        self.functions.insert(function.name, function);
        self
    }

    pub fn with_type(mut self, type_def: ExternalTypeDef) -> Self {
        self.types.insert(type_def.name, type_def);
        self
    }
}

#[derive(Clone, Default)]
pub struct ExternalPackageRegistry {
    packages: HashMap<&'static str, ExternalPackage>,
    functions_by_id: HashMap<ExternalFunctionId, ExternalFunctionDef>,
    name_to_function_id: HashMap<&'static str, ExternalFunctionId>,
    types_by_id: HashMap<ExternalTypeId, ExternalTypeDef>,
    name_to_type_id: HashMap<&'static str, ExternalTypeId>,
    #[cfg(test)]
    next_synthetic_id: u32,
}

impl ExternalPackageRegistry {
    /// Builds the builtin external package registry used by normal frontend compilation.
    pub fn new() -> Self {
        let mut registry = ExternalPackageRegistry::default();

        let std_io = ExternalPackage::new("@std/io")
            .with_function(ExternalFunctionDef {
                name: IO_FUNC_NAME,
                parameters: vec![ExternalParameter {
                    language_type: ExternalAbiType::Inferred,
                    access_kind: ExternalAccessKind::Shared,
                }],
                return_type: ExternalAbiType::Void,
                return_alias: ExternalReturnAlias::Fresh,
                receiver_type: None,
                receiver_access: ExternalAccessKind::Shared,
            })
            .with_type(ExternalTypeDef {
                name: IO_TYPE_NAME,
                package: "@std/io",
                abi_type: ExternalAbiType::Handle,
            });
        registry.packages.insert(std_io.path, std_io);
        registry.functions_by_id.insert(
            ExternalFunctionId::Io,
            ExternalFunctionDef {
                name: IO_FUNC_NAME,
                parameters: vec![ExternalParameter {
                    language_type: ExternalAbiType::Inferred,
                    access_kind: ExternalAccessKind::Shared,
                }],
                return_type: ExternalAbiType::Void,
                return_alias: ExternalReturnAlias::Fresh,
                receiver_type: None,
                receiver_access: ExternalAccessKind::Shared,
            },
        );
        registry
            .name_to_function_id
            .insert(IO_FUNC_NAME, ExternalFunctionId::Io);
        registry.types_by_id.insert(
            ExternalTypeId(0),
            ExternalTypeDef {
                name: IO_TYPE_NAME,
                package: "@std/io",
                abi_type: ExternalAbiType::Handle,
            },
        );
        registry
            .name_to_type_id
            .insert(IO_TYPE_NAME, ExternalTypeId(0));

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
                receiver_type: Some(ExternalAbiType::Inferred),
                receiver_access: ExternalAccessKind::Shared,
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
                receiver_type: Some(ExternalAbiType::Inferred),
                receiver_access: ExternalAccessKind::Mutable,
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
                receiver_type: Some(ExternalAbiType::Inferred),
                receiver_access: ExternalAccessKind::Mutable,
            })
            .with_function(ExternalFunctionDef {
                name: COLLECTION_LENGTH_HOST_NAME,
                parameters: vec![ExternalParameter {
                    language_type: ExternalAbiType::Inferred,
                    access_kind: ExternalAccessKind::Shared,
                }],
                return_type: ExternalAbiType::I32,
                return_alias: ExternalReturnAlias::Fresh,
                receiver_type: Some(ExternalAbiType::Inferred),
                receiver_access: ExternalAccessKind::Shared,
            });
        registry
            .packages
            .insert(std_collections.path, std_collections);
        registry.functions_by_id.insert(
            ExternalFunctionId::CollectionGet,
            ExternalFunctionDef {
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
                receiver_type: Some(ExternalAbiType::Inferred),
                receiver_access: ExternalAccessKind::Shared,
            },
        );
        registry
            .name_to_function_id
            .insert(COLLECTION_GET_HOST_NAME, ExternalFunctionId::CollectionGet);
        registry.functions_by_id.insert(
            ExternalFunctionId::CollectionPush,
            ExternalFunctionDef {
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
                receiver_type: Some(ExternalAbiType::Inferred),
                receiver_access: ExternalAccessKind::Mutable,
            },
        );
        registry.name_to_function_id.insert(
            COLLECTION_PUSH_HOST_NAME,
            ExternalFunctionId::CollectionPush,
        );
        registry.functions_by_id.insert(
            ExternalFunctionId::CollectionRemove,
            ExternalFunctionDef {
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
                receiver_type: Some(ExternalAbiType::Inferred),
                receiver_access: ExternalAccessKind::Mutable,
            },
        );
        registry.name_to_function_id.insert(
            COLLECTION_REMOVE_HOST_NAME,
            ExternalFunctionId::CollectionRemove,
        );
        registry.functions_by_id.insert(
            ExternalFunctionId::CollectionLength,
            ExternalFunctionDef {
                name: COLLECTION_LENGTH_HOST_NAME,
                parameters: vec![ExternalParameter {
                    language_type: ExternalAbiType::Inferred,
                    access_kind: ExternalAccessKind::Shared,
                }],
                return_type: ExternalAbiType::I32,
                return_alias: ExternalReturnAlias::Fresh,
                receiver_type: Some(ExternalAbiType::Inferred),
                receiver_access: ExternalAccessKind::Shared,
            },
        );
        registry.name_to_function_id.insert(
            COLLECTION_LENGTH_HOST_NAME,
            ExternalFunctionId::CollectionLength,
        );

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
                receiver_type: Some(ExternalAbiType::Inferred),
                receiver_access: ExternalAccessKind::Shared,
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
                receiver_type: Some(ExternalAbiType::Inferred),
                receiver_access: ExternalAccessKind::Shared,
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
                receiver_type: Some(ExternalAbiType::Inferred),
                receiver_access: ExternalAccessKind::Shared,
            });
        registry.packages.insert(std_error.path, std_error);
        registry.functions_by_id.insert(
            ExternalFunctionId::ErrorWithLocation,
            ExternalFunctionDef {
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
                receiver_type: Some(ExternalAbiType::Inferred),
                receiver_access: ExternalAccessKind::Shared,
            },
        );
        registry.name_to_function_id.insert(
            ERROR_WITH_LOCATION_HOST_NAME,
            ExternalFunctionId::ErrorWithLocation,
        );
        registry.functions_by_id.insert(
            ExternalFunctionId::ErrorPushTrace,
            ExternalFunctionDef {
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
                receiver_type: Some(ExternalAbiType::Inferred),
                receiver_access: ExternalAccessKind::Shared,
            },
        );
        registry.name_to_function_id.insert(
            ERROR_PUSH_TRACE_HOST_NAME,
            ExternalFunctionId::ErrorPushTrace,
        );
        registry.functions_by_id.insert(
            ExternalFunctionId::ErrorBubble,
            ExternalFunctionDef {
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
                receiver_type: Some(ExternalAbiType::Inferred),
                receiver_access: ExternalAccessKind::Shared,
            },
        );
        registry
            .name_to_function_id
            .insert(ERROR_BUBBLE_HOST_NAME, ExternalFunctionId::ErrorBubble);

        registry
    }

    /// Resolves an external function by name, returning its stable ID and definition.
    pub fn resolve_function(
        &self,
        name: &str,
    ) -> Option<(ExternalFunctionId, &ExternalFunctionDef)> {
        self.name_to_function_id
            .get(name)
            .and_then(|id| self.functions_by_id.get(id).map(|def| (*id, def)))
    }

    /// Looks up an external function by its stable ID.
    pub fn get_function_by_id(&self, id: ExternalFunctionId) -> Option<&ExternalFunctionDef> {
        self.functions_by_id.get(&id)
    }

    /// Resolves an external type by name, returning its stable ID and definition.
    pub fn resolve_type(&self, name: &str) -> Option<(ExternalTypeId, &ExternalTypeDef)> {
        self.name_to_type_id
            .get(name)
            .and_then(|id| self.types_by_id.get(id).map(|def| (*id, def)))
    }

    /// Looks up an external type by its stable ID.
    pub fn get_type_by_id(&self, id: ExternalTypeId) -> Option<&ExternalTypeDef> {
        self.types_by_id.get(&id)
    }

    /// Registers a synthetic external function for test-only lowering and borrow-check scenarios.
    #[cfg(test)]
    pub fn register_function(
        &mut self,
        function: ExternalFunctionDef,
    ) -> Result<ExternalFunctionId, CompilerError> {
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
        let name = function.name;
        test_package.functions.insert(name, function.clone());
        let id = ExternalFunctionId::Synthetic(self.next_synthetic_id);
        self.next_synthetic_id += 1;
        self.functions_by_id.insert(id, function);
        self.name_to_function_id.insert(name, id);
        Ok(id)
    }

    /// Looks up an external function by name across all packages.
    /// Used for prelude-visible symbols and internal compiler-generated calls.
    pub fn get_function(&self, name: &str) -> Option<&ExternalFunctionDef> {
        self.resolve_function(name).map(|(_, def)| def)
    }

    /// Looks up an external type by name across all packages.
    pub fn get_type(&self, name: &str) -> Option<&ExternalTypeDef> {
        self.resolve_type(name).map(|(_, def)| def)
    }

    /// Looks up a specific package by path.
    pub fn get_package(&self, path: &str) -> Option<&ExternalPackage> {
        self.packages.get(path)
    }

    /// Resolves a function symbol within a specific package.
    pub fn resolve_package_symbol(
        &self,
        package_path: &str,
        symbol_name: &str,
    ) -> Option<&ExternalFunctionDef> {
        self.packages
            .get(package_path)
            .and_then(|package| package.functions.get(symbol_name))
    }

    /// Resolves a type symbol within a specific package.
    pub fn resolve_package_type(
        &self,
        package_path: &str,
        type_name: &str,
    ) -> Option<&ExternalTypeDef> {
        self.packages
            .get(package_path)
            .and_then(|package| package.types.get(type_name))
    }

    /// Returns true if the registry contains a package with the given path.
    pub fn has_package(&self, path: &str) -> bool {
        self.packages.contains_key(path)
    }

    /// Looks up an external receiver method by receiver type name and method name.
    pub fn resolve_method(
        &self,
        receiver_type_name: &str,
        method_name: &str,
    ) -> Option<(ExternalFunctionId, &ExternalFunctionDef)> {
        for package in self.packages.values() {
            for (name, function) in &package.functions {
                if *name == method_name
                    && let Some(receiver_type) = &function.receiver_type
                {
                    // Match by ABI type name for now.
                    // In Phase 6 this will use ExternalTypeId for stable matching.
                    let receiver_matches = match receiver_type {
                        ExternalAbiType::Handle => !receiver_type_name.is_empty(),
                        ExternalAbiType::Inferred => true,
                        ExternalAbiType::I32 => receiver_type_name == "Int",
                        ExternalAbiType::Utf8Str => receiver_type_name == "String",
                        ExternalAbiType::Void => false,
                    };
                    if receiver_matches {
                        return self
                            .name_to_function_id
                            .get(name)
                            .copied()
                            .map(|id| (id, function));
                    }
                }
            }
        }
        None
    }

    /// Returns the list of symbol names that should be auto-imported into every module.
    pub fn prelude_symbols(&self) -> Vec<&'static str> {
        vec![IO_FUNC_NAME, IO_TYPE_NAME]
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::{
        ExternalAbiType, ExternalAccessKind, ExternalFunctionDef, ExternalFunctionId,
        ExternalPackageRegistry, ExternalParameter, ExternalReturnAlias,
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
    ) -> Result<ExternalFunctionId, CompilerError> {
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
            receiver_type: None,
            receiver_access: ExternalAccessKind::Shared,
        })
    }
}

#[cfg(test)]
#[path = "tests/external_packages_tests.rs"]
mod external_packages_tests;
