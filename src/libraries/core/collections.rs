//! `@core/collections` package registration.

use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::external_packages::{
    COLLECTION_GET_HOST_NAME, COLLECTION_LENGTH_HOST_NAME, COLLECTION_PUSH_HOST_NAME,
    COLLECTION_REMOVE_HOST_NAME, ExternalFunctionId,
};
use crate::compiler_frontend::external_packages::{
    ExternalAbiType, ExternalAccessKind, ExternalReturnAlias,
};
use crate::compiler_frontend::external_packages::{
    ExternalFunctionDef, ExternalFunctionLowerings, ExternalJsLowering, ExternalPackage,
};

pub fn register_core_collections_package(registry: &mut ExternalPackageRegistry) {
    registry
        .register_package(ExternalPackage::new("@core/collections"))
        .expect("builtin package registration should not collide");

    registry
        .register_function_in_package(
            "@core/collections",
            ExternalFunctionId::CollectionGet,
            ExternalFunctionDef {
                name: COLLECTION_GET_HOST_NAME,
                parameters: vec![
                    crate::compiler_frontend::external_packages::ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Shared,
                    },
                    crate::compiler_frontend::external_packages::ExternalParameter {
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
            "@core/collections",
            ExternalFunctionId::CollectionPush,
            ExternalFunctionDef {
                name: COLLECTION_PUSH_HOST_NAME,
                parameters: vec![
                    crate::compiler_frontend::external_packages::ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Mutable,
                    },
                    crate::compiler_frontend::external_packages::ExternalParameter {
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
            "@core/collections",
            ExternalFunctionId::CollectionRemove,
            ExternalFunctionDef {
                name: COLLECTION_REMOVE_HOST_NAME,
                parameters: vec![
                    crate::compiler_frontend::external_packages::ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Mutable,
                    },
                    crate::compiler_frontend::external_packages::ExternalParameter {
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
            "@core/collections",
            ExternalFunctionId::CollectionLength,
            ExternalFunctionDef {
                name: COLLECTION_LENGTH_HOST_NAME,
                parameters: vec![
                    crate::compiler_frontend::external_packages::ExternalParameter {
                        language_type: ExternalAbiType::Inferred,
                        access_kind: ExternalAccessKind::Shared,
                    },
                ],
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
