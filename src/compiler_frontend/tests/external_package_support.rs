//! External package fixture support for frontend unit tests.
//!
//! WHAT: registers small synthetic external package functions used by borrow and call-summary
//!       tests.
//! WHY: external package metadata is a separate frontend input surface, so the helpers stay
//!      outside AST/HIR fixture modules.

use crate::compiler_frontend::external_packages::test_support::{
    TestExternalAbiType, TestExternalAccessKind, TestExternalReturnAlias,
    register_test_external_function,
};
use crate::compiler_frontend::external_packages::{
    ExternalAbiType, ExternalFunctionId, ExternalPackageRegistry, ExternalSignatureType,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;

pub(crate) fn default_external_package_registry(
    _string_table: &mut StringTable,
) -> ExternalPackageRegistry {
    ExternalPackageRegistry::new()
}

pub(crate) fn register_external_function(
    registry: &mut ExternalPackageRegistry,
    name: &'static str,
    param_access: Vec<TestExternalAccessKind>,
    return_alias: TestExternalReturnAlias,
    return_type: TestExternalAbiType,
) -> ExternalFunctionId {
    let parameters = param_access
        .into_iter()
        .map(|access_kind| {
            (
                ExternalSignatureType::Abi(ExternalAbiType::I32),
                access_kind,
            )
        })
        .collect::<Vec<_>>();

    register_test_external_function(registry, name, parameters, return_alias, return_type)
        .expect("external function registration should succeed")
}
