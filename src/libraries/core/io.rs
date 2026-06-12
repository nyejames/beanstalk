//! `@core/io` package registration.

use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::external_packages::{
    ExternalAbiType, ExternalAccessKind, ExternalReturnAlias, ExternalSignatureType,
};
use crate::compiler_frontend::external_packages::{
    ExternalFunctionDef, ExternalFunctionLowerings, ExternalPackageOrigin, ExternalTypeDef,
    external_success_returns,
};
use crate::compiler_frontend::external_packages::{
    ExternalFunctionId, ExternalTypeId, IO_FUNC_NAME, IO_TYPE_NAME,
};

pub fn register_core_io_package(registry: &mut ExternalPackageRegistry) {
    let package_id = registry
        .register_package("@core/io", ExternalPackageOrigin::Builtin)
        .expect("builtin package registration should not collide");

    registry
        .register_function_in_package(
            package_id,
            ExternalFunctionId::Io,
            ExternalFunctionDef {
                name: IO_FUNC_NAME.to_owned(),
                parameters: vec![crate::compiler_frontend::external_packages::ExternalParameter {
                    language_type: ExternalSignatureType::Abi(ExternalAbiType::Inferred),
                    access_kind: ExternalAccessKind::Shared,
                }],
                returns: external_success_returns(ExternalAbiType::Void, ExternalReturnAlias::Fresh),
                error_return_type: None,
                lowerings: ExternalFunctionLowerings {
                    js: Some(
                        crate::compiler_frontend::external_packages::ExternalJsLowering::RuntimeFunction("__bs_io".to_owned()),
                    ),
                    wasm: None,
                },
            },
        )
        .expect("builtin function registration should not collide");

    registry
        .register_type_in_package(
            package_id,
            ExternalTypeId(0),
            ExternalTypeDef {
                name: IO_TYPE_NAME.to_owned(),
                package_id,
                abi_type: ExternalAbiType::Handle,
            },
        )
        .expect("builtin type registration should not collide");
}
