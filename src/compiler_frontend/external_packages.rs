//! Builtin external function metadata and registry.
//!
//! WHAT: defines the external-call surface the frontend and borrow checker understand today.
//! WHY: external calls need one canonical metadata source for signature lowering and call semantics.
//!
//! External symbols are registered by package scope: `(package_path, symbol_name)` uniquely
//! identifies a function or type. The same symbol name may exist in multiple packages.
//! The prelude (`io`, `IO`) is the only exception where bare-name lookup is valid.
//! All other external symbol resolution must go through file-local `visible_external_symbols`.

use crate::compiler_frontend::ast::statements::functions::{FunctionReturn, ReturnSlot};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::ids::FunctionId;
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
    /// Test package A function `open`.
    /// Registered in `@test/pkg-a` for integration-test coverage of package-scoped resolution.
    TestPkgAOpen,
    /// Test package B function `open`.
    /// Registered in `@test/pkg-b` for integration-test coverage of package-scoped resolution.
    TestPkgBOpen,
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
            Self::TestPkgAOpen => "test_pkg_a_open",
            Self::TestPkgBOpen => "test_pkg_b_open",
            Self::Synthetic(_) => "<synthetic>",
        }
    }
}

/// Stable identifier for an external type across all compiler stages and backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExternalTypeId(pub u32);

/// Unified identifier for an external symbol visible from a single file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExternalSymbolId {
    Function(ExternalFunctionId),
    Type(ExternalTypeId),
}

/// Package-scoped key for looking up an external symbol in the registry.
///
/// WHAT: `(package_path, symbol_name)` pair that uniquely identifies an external
/// function or type within the registry.
/// WHY: prevents collisions when two packages expose the same symbol name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ExternalPackageSymbolKey {
    package_path: String,
    symbol_name: String,
}

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
    types_by_id: HashMap<ExternalTypeId, ExternalTypeDef>,
    /// Package-scoped function lookup: (package_path, symbol_name) -> ExternalFunctionId.
    function_ids_by_package_symbol: HashMap<ExternalPackageSymbolKey, ExternalFunctionId>,
    /// Package-scoped type lookup: (package_path, symbol_name) -> ExternalTypeId.
    type_ids_by_package_symbol: HashMap<ExternalPackageSymbolKey, ExternalTypeId>,
    /// Prelude symbols that are auto-imported into every module.
    /// Bare-name lookup is only valid for the prelude.
    prelude_symbols_by_name: HashMap<&'static str, ExternalSymbolId>,
    #[cfg(test)]
    next_synthetic_id: u32,
}

impl ExternalPackageRegistry {
    /// Builds the builtin external package registry used by normal frontend compilation.
    pub fn new() -> Self {
        let mut registry = ExternalPackageRegistry::default();

        // @std/io
        registry
            .register_package(ExternalPackage::new("@std/io"))
            .expect("builtin package registration should not collide");
        registry
            .register_function_in_package(
                "@std/io",
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
            )
            .expect("builtin function registration should not collide");
        registry
            .register_type_in_package(
                "@std/io",
                ExternalTypeId(0),
                ExternalTypeDef {
                    name: IO_TYPE_NAME,
                    package: "@std/io",
                    abi_type: ExternalAbiType::Handle,
                },
            )
            .expect("builtin type registration should not collide");
        registry
            .register_prelude_symbol(
                IO_FUNC_NAME,
                ExternalSymbolId::Function(ExternalFunctionId::Io),
            )
            .expect("prelude registration should not collide");
        registry
            .register_prelude_symbol(IO_TYPE_NAME, ExternalSymbolId::Type(ExternalTypeId(0)))
            .expect("prelude registration should not collide");

        // @std/collections
        registry
            .register_package(ExternalPackage::new("@std/collections"))
            .expect("builtin package registration should not collide");
        registry
            .register_function_in_package(
                "@std/collections",
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
            )
            .expect("builtin function registration should not collide");
        registry
            .register_function_in_package(
                "@std/collections",
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
            )
            .expect("builtin function registration should not collide");
        registry
            .register_function_in_package(
                "@std/collections",
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
            )
            .expect("builtin function registration should not collide");
        registry
            .register_function_in_package(
                "@std/collections",
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
            )
            .expect("builtin function registration should not collide");

        // @std/error
        registry
            .register_package(ExternalPackage::new("@std/error"))
            .expect("builtin package registration should not collide");
        registry
            .register_function_in_package(
                "@std/error",
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
            )
            .expect("builtin function registration should not collide");
        registry
            .register_function_in_package(
                "@std/error",
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
            )
            .expect("builtin function registration should not collide");
        registry
            .register_function_in_package(
                "@std/error",
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
            )
            .expect("builtin function registration should not collide");

        // Test packages with duplicate symbol names (for integration-test coverage).
        registry
            .register_package(ExternalPackage::new("@test/pkg-a"))
            .expect("test package registration should not collide");
        registry
            .register_function_in_package(
                "@test/pkg-a",
                ExternalFunctionId::TestPkgAOpen,
                ExternalFunctionDef {
                    name: "open",
                    parameters: vec![ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Shared,
                    }],
                    return_type: ExternalAbiType::Void,
                    return_alias: ExternalReturnAlias::Fresh,
                    receiver_type: None,
                    receiver_access: ExternalAccessKind::Shared,
                },
            )
            .expect("test function registration should not collide");

        registry
            .register_package(ExternalPackage::new("@test/pkg-b"))
            .expect("test package registration should not collide");
        registry
            .register_function_in_package(
                "@test/pkg-b",
                ExternalFunctionId::TestPkgBOpen,
                ExternalFunctionDef {
                    name: "open",
                    parameters: vec![ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Shared,
                    }],
                    return_type: ExternalAbiType::Void,
                    return_alias: ExternalReturnAlias::Fresh,
                    receiver_type: None,
                    receiver_access: ExternalAccessKind::Shared,
                },
            )
            .expect("test function registration should not collide");

        registry
    }

    // ------------------------------------------------------------------
    // Registration helpers (centralized)
    // ------------------------------------------------------------------

    /// Registers a new virtual package in the registry.
    fn register_package(&mut self, package: ExternalPackage) -> Result<(), CompilerError> {
        if self.packages.contains_key(package.path) {
            return_compiler_error!("External package '{}' is already registered.", package.path);
        }
        self.packages.insert(package.path, package);
        Ok(())
    }

    /// Registers an external function within a specific package.
    fn register_function_in_package(
        &mut self,
        package_path: &'static str,
        id: ExternalFunctionId,
        function: ExternalFunctionDef,
    ) -> Result<(), CompilerError> {
        let package = self.packages.get_mut(package_path).ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "Cannot register function '{}' in unknown package '{}'.",
                function.name, package_path
            ))
        })?;

        if package.functions.contains_key(function.name) {
            return_compiler_error!(
                "External function '{}' is already registered in package '{}'.",
                function.name,
                package_path
            );
        }

        let key = ExternalPackageSymbolKey {
            package_path: package_path.to_string(),
            symbol_name: function.name.to_string(),
        };
        if self.function_ids_by_package_symbol.contains_key(&key) {
            return_compiler_error!(
                "External function '{}' is already registered in package '{}'.",
                function.name,
                package_path
            );
        }

        package.functions.insert(function.name, function.clone());
        self.functions_by_id.insert(id, function.clone());
        self.function_ids_by_package_symbol.insert(key, id);
        Ok(())
    }

    /// Registers an external type within a specific package.
    fn register_type_in_package(
        &mut self,
        package_path: &'static str,
        id: ExternalTypeId,
        type_def: ExternalTypeDef,
    ) -> Result<(), CompilerError> {
        let package = self.packages.get_mut(package_path).ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "Cannot register type '{}' in unknown package '{}'.",
                type_def.name, package_path
            ))
        })?;

        if package.types.contains_key(type_def.name) {
            return_compiler_error!(
                "External type '{}' is already registered in package '{}'.",
                type_def.name,
                package_path
            );
        }

        let key = ExternalPackageSymbolKey {
            package_path: package_path.to_string(),
            symbol_name: type_def.name.to_string(),
        };
        if self.type_ids_by_package_symbol.contains_key(&key) {
            return_compiler_error!(
                "External type '{}' is already registered in package '{}'.",
                type_def.name,
                package_path
            );
        }

        package.types.insert(type_def.name, type_def.clone());
        self.types_by_id.insert(id, type_def.clone());
        self.type_ids_by_package_symbol.insert(key, id);
        Ok(())
    }

    /// Registers a prelude symbol that is auto-imported into every module.
    fn register_prelude_symbol(
        &mut self,
        public_name: &'static str,
        symbol_id: ExternalSymbolId,
    ) -> Result<(), CompilerError> {
        if self.prelude_symbols_by_name.contains_key(public_name) {
            return_compiler_error!("Prelude symbol '{}' is already registered.", public_name);
        }
        self.prelude_symbols_by_name.insert(public_name, symbol_id);
        Ok(())
    }

    // ------------------------------------------------------------------
    // Lookup by stable ID (always safe, no visibility involved)
    // ------------------------------------------------------------------

    /// Looks up an external function by its stable ID.
    pub fn get_function_by_id(&self, id: ExternalFunctionId) -> Option<&ExternalFunctionDef> {
        self.functions_by_id.get(&id)
    }

    /// Looks up an external type by its stable ID.
    pub fn get_type_by_id(&self, id: ExternalTypeId) -> Option<&ExternalTypeDef> {
        self.types_by_id.get(&id)
    }

    // ------------------------------------------------------------------
    // Test-only registration
    // ------------------------------------------------------------------

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
        self.functions_by_id.insert(id, function.clone());
        self.function_ids_by_package_symbol.insert(
            ExternalPackageSymbolKey {
                package_path: "@test/default".to_string(),
                symbol_name: name.to_string(),
            },
            id,
        );
        Ok(id)
    }

    // ------------------------------------------------------------------
    // Package-scoped resolution (used by import binding)
    // ------------------------------------------------------------------

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

    /// Resolves a function symbol within a specific package, returning its ID and definition.
    pub fn resolve_package_function(
        &self,
        package_path: &str,
        symbol_name: &str,
    ) -> Option<(ExternalFunctionId, &ExternalFunctionDef)> {
        let package = self.packages.get(package_path)?;
        let def = package.functions.get(symbol_name)?;
        let key = ExternalPackageSymbolKey {
            package_path: package_path.to_string(),
            symbol_name: symbol_name.to_string(),
        };
        let id = *self.function_ids_by_package_symbol.get(&key)?;
        Some((id, def))
    }

    /// Resolves a type symbol within a specific package, returning its ID and definition.
    pub fn resolve_package_type(
        &self,
        package_path: &str,
        type_name: &str,
    ) -> Option<(ExternalTypeId, &ExternalTypeDef)> {
        let package = self.packages.get(package_path)?;
        let def = package.types.get(type_name)?;
        let key = ExternalPackageSymbolKey {
            package_path: package_path.to_string(),
            symbol_name: type_name.to_string(),
        };
        let id = *self.type_ids_by_package_symbol.get(&key)?;
        Some((id, def))
    }

    /// Returns true if the registry contains a package with the given path.
    pub fn has_package(&self, path: &str) -> bool {
        self.packages.contains_key(path)
    }

    /// Checks whether an import path should be treated as a virtual package import
    /// rather than a file-system import.
    ///
    /// WHAT: tries progressively shorter prefixes of the import path against known packages.
    /// WHY: file discovery must skip imports that target virtual packages so AST resolution
    ///      can handle them with proper error messages.
    pub fn is_virtual_package_import(
        &self,
        import_path: &crate::compiler_frontend::interned_path::InternedPath,
        string_table: &crate::compiler_frontend::symbols::string_interning::StringTable,
    ) -> bool {
        let components = import_path.as_components();
        if components.is_empty() {
            return false;
        }
        for package_len in (1..=components.len()).rev() {
            let package_path = format!(
                "@{}",
                components[..package_len]
                    .iter()
                    .map(|&id| string_table.resolve(id))
                    .collect::<Vec<_>>()
                    .join("/")
            );
            if self.has_package(&package_path) {
                return true;
            }
        }
        false
    }

    /// Looks up an external receiver method by receiver type name and method name.
    pub fn resolve_method(
        &self,
        receiver_type_name: &str,
        method_name: &str,
    ) -> Option<(ExternalFunctionId, &ExternalFunctionDef)> {
        for (package_path, package) in &self.packages {
            for (name, function) in &package.functions {
                if *name == method_name
                    && let Some(receiver_type) = &function.receiver_type
                {
                    // Match by ABI type name for now.
                    let receiver_matches = match receiver_type {
                        ExternalAbiType::Handle => !receiver_type_name.is_empty(),
                        ExternalAbiType::Inferred => true,
                        ExternalAbiType::I32 => receiver_type_name == "Int",
                        ExternalAbiType::Utf8Str => receiver_type_name == "String",
                        ExternalAbiType::Void => false,
                    };
                    if receiver_matches {
                        let key = ExternalPackageSymbolKey {
                            package_path: package_path.to_string(),
                            symbol_name: name.to_string(),
                        };
                        return self
                            .function_ids_by_package_symbol
                            .get(&key)
                            .copied()
                            .map(|id| (id, function));
                    }
                }
            }
        }
        None
    }

    // ------------------------------------------------------------------
    // Prelude
    // ------------------------------------------------------------------

    /// Returns the prelude symbol map.
    /// Bare-name lookup is only valid for the prelude.
    pub fn prelude_symbols_by_name(&self) -> &HashMap<&'static str, ExternalSymbolId> {
        &self.prelude_symbols_by_name
    }

    /// Returns true if the given name is a prelude function.
    pub fn is_prelude_function(&self, name: &str) -> bool {
        self.prelude_symbols_by_name
            .get(name)
            .is_some_and(|symbol_id| matches!(symbol_id, ExternalSymbolId::Function(_)))
    }

    /// Returns true if the given name is a prelude type.
    pub fn is_prelude_type(&self, name: &str) -> bool {
        self.prelude_symbols_by_name
            .get(name)
            .is_some_and(|symbol_id| matches!(symbol_id, ExternalSymbolId::Type(_)))
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
