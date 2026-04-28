//! `@core/error` package registration.

use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::external_packages::{
    ERROR_BUBBLE_HOST_NAME, ERROR_PUSH_TRACE_HOST_NAME, ERROR_WITH_LOCATION_HOST_NAME,
    ExternalFunctionId,
};
use crate::compiler_frontend::external_packages::{
    ExternalAbiType, ExternalAccessKind, ExternalReturnAlias,
};
use crate::compiler_frontend::external_packages::{
    ExternalFunctionDef, ExternalFunctionLowerings, ExternalJsLowering, ExternalPackage,
};

pub fn register_core_error_package(registry: &mut ExternalPackageRegistry) {
    registry
        .register_package(ExternalPackage::new("@core/error"))
        .expect("builtin package registration should not collide");

    registry
        .register_function_in_package(
            "@core/error",
            ExternalFunctionId::ErrorWithLocation,
            ExternalFunctionDef {
                name: ERROR_WITH_LOCATION_HOST_NAME,
                parameters: vec![
                    crate::compiler_frontend::external_packages::ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Shared,
                    },
                    crate::compiler_frontend::external_packages::ExternalParameter {
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
            "@core/error",
            ExternalFunctionId::ErrorPushTrace,
            ExternalFunctionDef {
                name: ERROR_PUSH_TRACE_HOST_NAME,
                parameters: vec![
                    crate::compiler_frontend::external_packages::ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Shared,
                    },
                    crate::compiler_frontend::external_packages::ExternalParameter {
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
            "@core/error",
            ExternalFunctionId::ErrorBubble,
            ExternalFunctionDef {
                name: ERROR_BUBBLE_HOST_NAME,
                parameters: vec![
                    crate::compiler_frontend::external_packages::ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Shared,
                    },
                    crate::compiler_frontend::external_packages::ExternalParameter {
                        language_type: ExternalAbiType::Utf8Str,
                        access_kind: ExternalAccessKind::Shared,
                    },
                    crate::compiler_frontend::external_packages::ExternalParameter {
                        language_type: ExternalAbiType::I32,
                        access_kind: ExternalAccessKind::Shared,
                    },
                    crate::compiler_frontend::external_packages::ExternalParameter {
                        language_type: ExternalAbiType::I32,
                        access_kind: ExternalAccessKind::Shared,
                    },
                    crate::compiler_frontend::external_packages::ExternalParameter {
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
