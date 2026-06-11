//! Test-only synthetic package registration.

use super::super::abi::{
    ExternalAbiType, ExternalAccessKind, ExternalReturnAlias, ExternalSignatureType,
};
use super::super::definitions::{
    ExternalConstantDef, ExternalConstantValue, ExternalFunctionDef, ExternalFunctionLowerings,
    ExternalJsLowering, ExternalReturnSlot, ExternalTypeDef, external_success_returns,
};
use super::super::ids::ExternalPackageOrigin;
use super::super::ids::{ExternalConstantId, ExternalFunctionId, ExternalTypeId};
use super::super::registry::ExternalPackageRegistry;

/// Registers test packages `@test/pkg-a` and `@test/pkg-b` with a duplicate
/// symbol name for integration-test coverage of package-scoped resolution.
pub(crate) fn register_test_packages_for_integration(registry: &mut ExternalPackageRegistry) {
    let pkg_a_id = registry
        .register_package("@test/pkg-a", ExternalPackageOrigin::BuilderRuntime)
        .expect("test package registration should not collide");
    registry
        .register_type_in_package(
            pkg_a_id,
            ExternalTypeId(1005),
            ExternalTypeDef {
                name: "PkgError".to_owned(),
                package_id: pkg_a_id,
                abi_type: ExternalAbiType::Handle,
            },
        )
        .expect("test external type registration should not collide");
    registry
        .register_function_in_package(
            pkg_a_id,
            ExternalFunctionId::Synthetic(1000),
            ExternalFunctionDef {
                name: "open".to_owned(),
                parameters: vec![super::super::abi::ExternalParameter {
                    language_type: ExternalSignatureType::Abi(ExternalAbiType::Inferred),
                    access_kind: ExternalAccessKind::Shared,
                }],
                returns: external_success_returns(
                    ExternalAbiType::Void,
                    ExternalReturnAlias::Fresh,
                ),
                error_return_type: None,
                lowerings: ExternalFunctionLowerings {
                    js: Some(ExternalJsLowering::RuntimeFunction(
                        "__bs_test_pkg_a_open".to_owned(),
                    )),
                    wasm: None,
                },
            },
        )
        .expect("test function registration should not collide");

    registry
        .register_function_in_package(
            pkg_a_id,
            ExternalFunctionId::Synthetic(1003),
            ExternalFunctionDef {
                name: "fallible_text_ok".to_owned(),
                parameters: vec![super::super::abi::ExternalParameter {
                    language_type: ExternalSignatureType::Abi(ExternalAbiType::Utf8Str),
                    access_kind: ExternalAccessKind::Shared,
                }],
                returns: vec![ExternalReturnSlot::fresh(ExternalAbiType::Utf8Str)],
                error_return_type: Some(ExternalSignatureType::BuiltinError),
                lowerings: ExternalFunctionLowerings {
                    js: Some(ExternalJsLowering::InlineExpression(
                        "({ tag: \"ok\", value: #0 })".to_owned(),
                    )),
                    wasm: None,
                },
            },
        )
        .expect("test fallible function registration should not collide");

    registry
        .register_function_in_package(
            pkg_a_id,
            ExternalFunctionId::Synthetic(1004),
            ExternalFunctionDef {
                name: "fallible_text_err".to_owned(),
                parameters: Vec::new(),
                returns: vec![ExternalReturnSlot::fresh(ExternalAbiType::Utf8Str)],
                error_return_type: Some(ExternalSignatureType::BuiltinError),
                lowerings: ExternalFunctionLowerings {
                    js: Some(ExternalJsLowering::InlineExpression(
                        "({ tag: \"err\", value: { message: \"external failed\", code: 91 } })"
                            .to_owned(),
                    )),
                    wasm: None,
                },
            },
        )
        .expect("test fallible function registration should not collide");

    registry
        .register_function_in_package(
            pkg_a_id,
            ExternalFunctionId::Synthetic(1006),
            ExternalFunctionDef {
                name: "fallible_custom_error_ok".to_owned(),
                parameters: Vec::new(),
                returns: vec![ExternalReturnSlot::fresh(ExternalAbiType::Utf8Str)],
                error_return_type: Some(ExternalSignatureType::External(ExternalTypeId(1005))),
                lowerings: ExternalFunctionLowerings {
                    js: Some(ExternalJsLowering::InlineExpression(
                        "({ tag: \"ok\", value: \"custom-ok\" })".to_owned(),
                    )),
                    wasm: None,
                },
            },
        )
        .expect("test custom-error fallible function registration should not collide");

    let pkg_b_id = registry
        .register_package("@test/pkg-b", ExternalPackageOrigin::BuilderRuntime)
        .expect("test package registration should not collide");
    registry
        .register_function_in_package(
            pkg_b_id,
            ExternalFunctionId::Synthetic(1001),
            ExternalFunctionDef {
                name: "open".to_owned(),
                parameters: vec![super::super::abi::ExternalParameter {
                    language_type: ExternalSignatureType::Abi(ExternalAbiType::Inferred),
                    access_kind: ExternalAccessKind::Shared,
                }],
                returns: external_success_returns(
                    ExternalAbiType::Void,
                    ExternalReturnAlias::Fresh,
                ),
                error_return_type: None,
                lowerings: ExternalFunctionLowerings {
                    js: Some(ExternalJsLowering::RuntimeFunction(
                        "__bs_test_pkg_b_open".to_owned(),
                    )),
                    wasm: None,
                },
            },
        )
        .expect("test function registration should not collide");

    registry
        .register_constant_in_package(
            pkg_b_id,
            ExternalConstantId(1002),
            ExternalConstantDef {
                name: "TEST_NON_SCALAR_CONST".to_owned(),
                data_type: ExternalAbiType::Utf8Str,
                value: ExternalConstantValue::StringSlice("test"),
            },
        )
        .expect("test constant registration should not collide");
}
