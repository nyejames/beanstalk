//! `@std/error` package registration.

use super::super::abi::{ExternalAbiType, ExternalAccessKind, ExternalReturnAlias};
use super::super::definitions::{
    ExternalFunctionDef, ExternalFunctionLowerings, ExternalJsLowering, ExternalPackage,
};
use super::super::ids::{
    ERROR_BUBBLE_HOST_NAME, ERROR_PUSH_TRACE_HOST_NAME, ERROR_WITH_LOCATION_HOST_NAME,
    ExternalFunctionId,
};
use super::super::registry::ExternalPackageRegistry;

pub(crate) fn register_std_error_package(registry: &mut ExternalPackageRegistry) {
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
                    super::super::abi::ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Shared,
                    },
                    super::super::abi::ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Shared,
                    },
                ],
                return_type: ExternalAbiType::Void,
                return_alias: ExternalReturnAlias::Fresh,
                receiver_type: Some(ExternalAbiType::Inferred),
                receiver_access: ExternalAccessKind::Shared,
                lowerings: ExternalFunctionLowerings {
                    js: Some(ExternalJsLowering::RuntimeFunction(
                        "__bs_error_with_location",
                    )),
                    wasm: None,
                },
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
                    super::super::abi::ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Shared,
                    },
                    super::super::abi::ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Shared,
                    },
                ],
                return_type: ExternalAbiType::Void,
                return_alias: ExternalReturnAlias::Fresh,
                receiver_type: Some(ExternalAbiType::Inferred),
                receiver_access: ExternalAccessKind::Shared,
                lowerings: ExternalFunctionLowerings {
                    js: Some(ExternalJsLowering::RuntimeFunction("__bs_error_push_trace")),
                    wasm: None,
                },
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
                    super::super::abi::ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Shared,
                    },
                    super::super::abi::ExternalParameter {
                        language_type: ExternalAbiType::Utf8Str,
                        access_kind: ExternalAccessKind::Shared,
                    },
                    super::super::abi::ExternalParameter {
                        language_type: ExternalAbiType::I32,
                        access_kind: ExternalAccessKind::Shared,
                    },
                    super::super::abi::ExternalParameter {
                        language_type: ExternalAbiType::I32,
                        access_kind: ExternalAccessKind::Shared,
                    },
                    super::super::abi::ExternalParameter {
                        language_type: ExternalAbiType::Utf8Str,
                        access_kind: ExternalAccessKind::Shared,
                    },
                ],
                return_type: ExternalAbiType::Void,
                return_alias: ExternalReturnAlias::Fresh,
                receiver_type: Some(ExternalAbiType::Inferred),
                receiver_access: ExternalAccessKind::Shared,
                lowerings: ExternalFunctionLowerings {
                    js: Some(ExternalJsLowering::RuntimeFunction("__bs_error_bubble")),
                    wasm: None,
                },
            },
        )
        .expect("builtin function registration should not collide");
}
