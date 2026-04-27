//! `@std/collections` package registration.

use super::super::abi::{ExternalAbiType, ExternalAccessKind, ExternalReturnAlias};
use super::super::definitions::{
    ExternalFunctionDef, ExternalFunctionLowerings, ExternalJsLowering, ExternalPackage,
};
use super::super::ids::{
    COLLECTION_GET_HOST_NAME, COLLECTION_LENGTH_HOST_NAME, COLLECTION_PUSH_HOST_NAME,
    COLLECTION_REMOVE_HOST_NAME, ExternalFunctionId,
};
use super::super::registry::ExternalPackageRegistry;

pub(crate) fn register_std_collections_package(registry: &mut ExternalPackageRegistry) {
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
                    super::super::abi::ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Shared,
                    },
                    super::super::abi::ExternalParameter {
                        language_type: ExternalAbiType::I32,
                        access_kind: ExternalAccessKind::Shared,
                    },
                ],
                return_type: ExternalAbiType::Void,
                return_alias: ExternalReturnAlias::Fresh,
                receiver_type: Some(ExternalAbiType::Inferred),
                receiver_access: ExternalAccessKind::Shared,
                lowerings: ExternalFunctionLowerings {
                    js: Some(ExternalJsLowering::RuntimeFunction("__bs_collection_get")),
                    wasm: None,
                },
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
                    super::super::abi::ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Mutable,
                    },
                    super::super::abi::ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Shared,
                    },
                ],
                return_type: ExternalAbiType::Void,
                return_alias: ExternalReturnAlias::Fresh,
                receiver_type: Some(ExternalAbiType::Inferred),
                receiver_access: ExternalAccessKind::Mutable,
                lowerings: ExternalFunctionLowerings {
                    js: Some(ExternalJsLowering::RuntimeFunction("__bs_collection_push")),
                    wasm: None,
                },
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
                    super::super::abi::ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Mutable,
                    },
                    super::super::abi::ExternalParameter {
                        language_type: ExternalAbiType::I32,
                        access_kind: ExternalAccessKind::Shared,
                    },
                ],
                return_type: ExternalAbiType::Void,
                return_alias: ExternalReturnAlias::Fresh,
                receiver_type: Some(ExternalAbiType::Inferred),
                receiver_access: ExternalAccessKind::Mutable,
                lowerings: ExternalFunctionLowerings {
                    js: Some(ExternalJsLowering::RuntimeFunction(
                        "__bs_collection_remove",
                    )),
                    wasm: None,
                },
            },
        )
        .expect("builtin function registration should not collide");

    registry
        .register_function_in_package(
            "@std/collections",
            ExternalFunctionId::CollectionLength,
            ExternalFunctionDef {
                name: COLLECTION_LENGTH_HOST_NAME,
                parameters: vec![super::super::abi::ExternalParameter {
                    language_type: ExternalAbiType::Inferred,
                    access_kind: ExternalAccessKind::Shared,
                }],
                return_type: ExternalAbiType::I32,
                return_alias: ExternalReturnAlias::Fresh,
                receiver_type: Some(ExternalAbiType::Inferred),
                receiver_access: ExternalAccessKind::Shared,
                lowerings: ExternalFunctionLowerings {
                    js: Some(ExternalJsLowering::RuntimeFunction(
                        "__bs_collection_length",
                    )),
                    wasm: None,
                },
            },
        )
        .expect("builtin function registration should not collide");
}
