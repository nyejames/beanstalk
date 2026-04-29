//! Builtin external function metadata and registry.
//!
//! WHAT: defines the external-call surface the frontend and borrow checker understand today.
//! WHY: external calls need one canonical metadata source for signature lowering and call semantics.
//!
//! External symbols are registered by package scope: `(package_path, symbol_name)` uniquely
//! identifies a function or type. The same symbol name may exist in multiple packages.
//! The prelude (`io`, `IO`) is the only exception where bare-name lookup is valid.
//! All other external symbol resolution must go through file-local `visible_external_symbols`.
//!
//! ## Module layout
//!
//! - `ids`: stable identifiers (`ExternalFunctionId`, `ExternalTypeId`, etc.)
//! - `abi`: backend-agnostic ABI types (`ExternalAbiType`, `ExternalParameter`, etc.)
//! - `definitions`: function/type/constant definitions and lowering metadata
//! - `registry`: `ExternalPackageRegistry` with registration and lookup APIs
//! - `packages/`: test-only package definition files

mod abi;
mod definitions;
mod ids;
mod registry;

mod packages;

#[cfg(test)]
mod tests;

pub use abi::*;
pub use definitions::*;
pub use ids::*;
pub use registry::*;

/// Builds the mandatory external package registry used by normal frontend compilation.
///
/// WHAT: calls each package-specific registration helper in order to produce a fully
/// populated `ExternalPackageRegistry`.
/// WHY: the registry constructor should read like orchestration, not like a data dump.
/// Keeping package definitions in `src/libraries/core/` prevents the constructor from
/// growing into an unmaintainable wall of struct literals and separates library
/// identity from registry mechanics.
pub fn build_builtin_registry() -> ExternalPackageRegistry {
    let mut registry = ExternalPackageRegistry::default();
    crate::libraries::core::register_core_io_package(&mut registry);
    crate::libraries::core::register_core_collections_package(&mut registry);
    crate::libraries::core::register_core_error_package(&mut registry);
    crate::libraries::core::register_core_prelude(&mut registry);
    registry
}
