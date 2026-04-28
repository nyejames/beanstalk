//! `@core/io` package registration.

use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::external_packages::{
    ExternalAbiType, ExternalAccessKind, ExternalReturnAlias,
};
use crate::compiler_frontend::external_packages::{
    ExternalFunctionDef, ExternalFunctionLowerings, ExternalPackage, ExternalTypeDef,
};
use crate::compiler_frontend::external_packages::{
    ExternalFunctionId, ExternalTypeId, IO_FUNC_NAME, IO_TYPE_NAME,
};

pub fn register_core_io_package(registry: &mut ExternalPackageRegistry) {
    registry
        .register_package(ExternalPackage::new("@core/io"))
        .expect("builtin package registration should not collide");

    registry
        .register_function_in_package(
            "@core/io",
            ExternalFunctionId::Io,
            ExternalFunctionDef {
                name: IO_FUNC_NAME,
                parameters: vec![crate::compiler_frontend::external_packages::ExternalParameter {
                    language_type: ExternalAbiType::Inferred,
                    access_kind: ExternalAccessKind::Shared,
                }],
                return_type: ExternalAbiType::Void,
                return_alias: ExternalReturnAlias::Fresh,
                receiver_type: None,
                receiver_access: ExternalAccessKind::Shared,
                lowerings: ExternalFunctionLowerings {
                    js: Some(
                        crate::compiler_frontend::external_packages::ExternalJsLowering::RuntimeFunction("__bs_io"),
                    ),
                    wasm: None,
                },
            },
        )
        .expect("builtin function registration should not collide");

    registry
        .register_type_in_package(
            "@core/io",
            ExternalTypeId(0),
            ExternalTypeDef {
                name: IO_TYPE_NAME,
                package: "@core/io",
                abi_type: ExternalAbiType::Handle,
            },
        )
        .expect("builtin type registration should not collide");
}
