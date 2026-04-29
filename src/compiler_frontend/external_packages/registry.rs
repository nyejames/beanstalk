//! External package registry and lookup APIs.
//!
//! WHAT: owns all registered virtual packages and provides resolution from package-scoped
//! names to stable IDs, and from stable IDs to definitions.
//! WHY: the frontend needs one canonical source for external symbol metadata. Keeping
//! registration and lookup in one place ensures consistency between the package map,
//! the ID-indexed maps, and the prelude.

use super::definitions::{
    ExternalConstantDef, ExternalFunctionDef, ExternalFunctionSpec, ExternalPackage,
    ExternalTypeDef, ExternalTypeSpec,
};
use super::ids::{ExternalConstantId, ExternalFunctionId, ExternalSymbolId, ExternalTypeId};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::return_compiler_error;
use std::collections::HashMap;

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

#[derive(Clone, Debug, Default)]
pub struct ExternalPackageRegistry {
    packages: HashMap<&'static str, ExternalPackage>,
    functions_by_id: HashMap<ExternalFunctionId, ExternalFunctionDef>,
    types_by_id: HashMap<ExternalTypeId, ExternalTypeDef>,
    constants_by_id: HashMap<ExternalConstantId, ExternalConstantDef>,
    /// Package-scoped function lookup: (package_path, symbol_name) -> ExternalFunctionId.
    function_ids_by_package_symbol: HashMap<ExternalPackageSymbolKey, ExternalFunctionId>,
    /// Package-scoped type lookup: (package_path, symbol_name) -> ExternalTypeId.
    type_ids_by_package_symbol: HashMap<ExternalPackageSymbolKey, ExternalTypeId>,
    /// Package-scoped constant lookup: (package_path, symbol_name) -> ExternalConstantId.
    constant_ids_by_package_symbol: HashMap<ExternalPackageSymbolKey, ExternalConstantId>,
    /// Prelude symbols that are auto-imported into every module.
    /// Bare-name lookup is only valid for the prelude.
    prelude_symbols_by_name: HashMap<&'static str, ExternalSymbolId>,
    /// Counter for dynamically assigned synthetic IDs.
    next_synthetic_id: u32,
}

impl ExternalPackageRegistry {
    /// Builds the builtin external package registry used by normal frontend compilation.
    pub fn new() -> Self {
        super::build_builtin_registry()
    }

    /// Attaches test packages to this registry for integration-test coverage.
    pub fn with_test_packages_for_integration(mut self) -> Self {
        super::packages::test_packages::register_test_packages_for_integration(&mut self);
        self
    }

    // ------------------------------------------------------------------
    // Registration helpers (centralized)
    // ------------------------------------------------------------------

    /// Registers a new virtual package in the registry.
    pub fn register_package(&mut self, package: ExternalPackage) -> Result<(), CompilerError> {
        if self.packages.contains_key(package.path) {
            return_compiler_error!("External package '{}' is already registered.", package.path);
        }
        self.packages.insert(package.path, package);
        Ok(())
    }

    /// Registers an external function within a specific package.
    pub fn register_function_in_package(
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

        // Store in the package map by cloning, then move the original into the ID map.
        package.functions.insert(function.name, function.clone());
        self.functions_by_id.insert(id, function);
        self.function_ids_by_package_symbol.insert(key, id);
        Ok(())
    }

    /// Registers an external type within a specific package.
    pub fn register_type_in_package(
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
        self.types_by_id.insert(id, type_def);
        self.type_ids_by_package_symbol.insert(key, id);
        Ok(())
    }

    /// Registers a prelude symbol that is auto-imported into every module.
    pub(crate) fn register_prelude_symbol(
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

    /// Looks up an external constant by its stable ID.
    pub fn get_constant_by_id(&self, id: ExternalConstantId) -> Option<&ExternalConstantDef> {
        self.constants_by_id.get(&id)
    }

    // ------------------------------------------------------------------
    // Dynamic registration (alpha: assigns Synthetic IDs)
    // ------------------------------------------------------------------

    /// Registers an external function in a package, assigning the next available
    /// synthetic ID automatically.
    ///
    /// WHAT: builder-friendly entry point that does not require hardcoding an
    /// `ExternalFunctionId` enum variant.
    /// WHY: alpha short-cut until the backend supports fully dynamic host imports.
    pub fn register_external_function(
        &mut self,
        package_path: &'static str,
        spec: ExternalFunctionSpec,
    ) -> Result<ExternalFunctionId, CompilerError> {
        let id = ExternalFunctionId::Synthetic(self.next_synthetic_id);
        self.next_synthetic_id += 1;
        self.register_function_in_package(package_path, id, spec.into())?;
        Ok(id)
    }

    /// Registers an external type in a package, assigning the next available
    /// dynamic ID automatically.
    pub fn register_external_type(
        &mut self,
        package_path: &'static str,
        spec: ExternalTypeSpec,
    ) -> Result<ExternalTypeId, CompilerError> {
        let id = ExternalTypeId(self.next_synthetic_id);
        self.next_synthetic_id += 1;
        self.register_type_in_package(
            package_path,
            id,
            ExternalTypeDef {
                name: spec.name,
                package: package_path,
                abi_type: spec.abi_type,
            },
        )?;
        Ok(id)
    }

    /// Registers an external constant in a package, assigning the next available
    /// dynamic ID automatically.
    pub fn register_external_constant(
        &mut self,
        package_path: &'static str,
        constant: ExternalConstantDef,
    ) -> Result<ExternalConstantId, CompilerError> {
        let id = ExternalConstantId(self.next_synthetic_id);
        self.next_synthetic_id += 1;
        self.register_constant_in_package(package_path, id, constant)?;
        Ok(id)
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
        self.functions_by_id.insert(id, function);
        self.function_ids_by_package_symbol.insert(
            ExternalPackageSymbolKey {
                package_path: "@test/default".to_string(),
                symbol_name: name.to_string(),
            },
            id,
        );
        Ok(id)
    }

    /// Registers an external constant within a specific package.
    pub fn register_constant_in_package(
        &mut self,
        package_path: &'static str,
        id: ExternalConstantId,
        constant: ExternalConstantDef,
    ) -> Result<(), CompilerError> {
        let package = self.packages.get_mut(package_path).ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "Cannot register constant '{}' in unknown package '{}'.",
                constant.name, package_path
            ))
        })?;

        if package.constants.contains_key(constant.name) {
            return_compiler_error!(
                "External constant '{}' is already registered in package '{}'.",
                constant.name,
                package_path
            );
        }

        let key = ExternalPackageSymbolKey {
            package_path: package_path.to_string(),
            symbol_name: constant.name.to_string(),
        };
        if self.constant_ids_by_package_symbol.contains_key(&key) {
            return_compiler_error!(
                "External constant '{}' is already registered in package '{}'.",
                constant.name,
                package_path
            );
        }

        package.constants.insert(constant.name, constant.clone());
        self.constants_by_id.insert(id, constant);
        self.constant_ids_by_package_symbol.insert(key, id);
        Ok(())
    }

    // ------------------------------------------------------------------
    // Package-scoped resolution (used by import binding)
    // ------------------------------------------------------------------

    /// Looks up a specific package by path.
    pub fn get_package(&self, path: &str) -> Option<&ExternalPackage> {
        self.packages.get(path)
    }

    /// Resolves any symbol (function, type, or constant) within a specific package.
    pub fn resolve_package_symbol(
        &self,
        package_path: &str,
        symbol_name: &str,
    ) -> Option<ExternalSymbolId> {
        let package = self.packages.get(package_path)?;
        if package.functions.contains_key(symbol_name) {
            let key = ExternalPackageSymbolKey {
                package_path: package_path.to_string(),
                symbol_name: symbol_name.to_string(),
            };
            let id = *self.function_ids_by_package_symbol.get(&key)?;
            return Some(ExternalSymbolId::Function(id));
        }
        if package.types.contains_key(symbol_name) {
            let key = ExternalPackageSymbolKey {
                package_path: package_path.to_string(),
                symbol_name: symbol_name.to_string(),
            };
            let id = *self.type_ids_by_package_symbol.get(&key)?;
            return Some(ExternalSymbolId::Type(id));
        }
        if package.constants.contains_key(symbol_name) {
            let key = ExternalPackageSymbolKey {
                package_path: package_path.to_string(),
                symbol_name: symbol_name.to_string(),
            };
            let id = *self.constant_ids_by_package_symbol.get(&key)?;
            return Some(ExternalSymbolId::Constant(id));
        }
        None
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

    /// Resolves a constant symbol within a specific package, returning its ID and definition.
    pub fn resolve_package_constant(
        &self,
        package_path: &str,
        constant_name: &str,
    ) -> Option<(ExternalConstantId, &ExternalConstantDef)> {
        let package = self.packages.get(package_path)?;
        let def = package.constants.get(constant_name)?;
        let key = ExternalPackageSymbolKey {
            package_path: package_path.to_string(),
            symbol_name: constant_name.to_string(),
        };
        let id = *self.constant_ids_by_package_symbol.get(&key)?;
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

    /// Returns a known optional package path when an import targets a package this builder
    /// did not expose.
    ///
    /// WHAT: recognizes compiler-known optional core package prefixes before path resolution falls
    /// back to filesystem imports.
    /// WHY: `@core/text` missing from a builder is a library-surface error, not a confusing
    /// missing source file.
    pub fn unsupported_known_package_import(
        &self,
        import_path: &crate::compiler_frontend::interned_path::InternedPath,
        string_table: &crate::compiler_frontend::symbols::string_interning::StringTable,
    ) -> Option<&'static str> {
        let components = import_path.as_components();
        for package_path in crate::libraries::core::OPTIONAL_CORE_PACKAGE_PATHS {
            if self.has_package(package_path) {
                continue;
            }

            let package_components = package_path
                .strip_prefix('@')
                .unwrap_or(package_path)
                .split('/')
                .collect::<Vec<_>>();
            if components.len() < package_components.len() {
                continue;
            }

            let matches_prefix = package_components
                .iter()
                .enumerate()
                .all(|(index, expected)| string_table.resolve(components[index]) == *expected);
            if matches_prefix {
                return Some(*package_path);
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
    use super::super::abi::ExternalParameter;
    use super::super::abi::{ExternalAbiType, ExternalAccessKind, ExternalReturnAlias};
    use super::super::definitions::{ExternalFunctionDef, ExternalFunctionLowerings};
    use super::super::ids::ExternalFunctionId;
    use super::ExternalPackageRegistry;
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
            lowerings: ExternalFunctionLowerings::default(),
        })
    }
}
