//! Test-only synthetic package registration.

use super::super::abi::{ExternalAbiType, ExternalAccessKind, ExternalReturnAlias};
use super::super::definitions::{
    ExternalFunctionDef, ExternalFunctionLowerings, ExternalJsLowering, ExternalPackage,
};
use super::super::ids::ExternalFunctionId;
use super::super::registry::ExternalPackageRegistry;

/// Registers test packages `@test/pkg-a` and `@test/pkg-b` with a duplicate
/// symbol name for integration-test coverage of package-scoped resolution.
pub(crate) fn register_test_packages_for_integration(registry: &mut ExternalPackageRegistry) {
    registry
        .register_package(ExternalPackage::new("@test/pkg-a"))
        .expect("test package registration should not collide");
    registry
        .register_function_in_package(
            "@test/pkg-a",
            ExternalFunctionId::Synthetic(1000),
            ExternalFunctionDef {
                name: "open",
                parameters: vec![super::super::abi::ExternalParameter {
                    language_type: ExternalAbiType::Inferred,
                    access_kind: ExternalAccessKind::Shared,
                }],
                return_type: ExternalAbiType::Void,
                return_alias: ExternalReturnAlias::Fresh,
                receiver_type: None,
                receiver_access: ExternalAccessKind::Shared,
                lowerings: ExternalFunctionLowerings {
                    js: Some(ExternalJsLowering::RuntimeFunction("__bs_test_pkg_a_open")),
                    wasm: None,
                },
            },
        )
        .expect("test function registration should not collide");

    registry
        .register_package(ExternalPackage::new("@test/pkg-b"))
        .expect("test package registration should not collide");
    registry
        .register_function_in_package(
            "@test/pkg-b",
            ExternalFunctionId::Synthetic(1001),
            ExternalFunctionDef {
                name: "open",
                parameters: vec![super::super::abi::ExternalParameter {
                    language_type: ExternalAbiType::Inferred,
                    access_kind: ExternalAccessKind::Shared,
                }],
                return_type: ExternalAbiType::Void,
                return_alias: ExternalReturnAlias::Fresh,
                receiver_type: None,
                receiver_access: ExternalAccessKind::Shared,
                lowerings: ExternalFunctionLowerings {
                    js: Some(ExternalJsLowering::RuntimeFunction("__bs_test_pkg_b_open")),
                    wasm: None,
                },
            },
        )
        .expect("test function registration should not collide");
}
