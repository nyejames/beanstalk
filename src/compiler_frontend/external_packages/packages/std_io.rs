//! `@std/io` package registration.

use super::super::abi::{ExternalAbiType, ExternalAccessKind, ExternalReturnAlias};
use super::super::definitions::{
    ExternalFunctionDef, ExternalFunctionLowerings, ExternalPackage, ExternalTypeDef,
};
use super::super::ids::{
    ExternalFunctionId, ExternalSymbolId, ExternalTypeId, IO_FUNC_NAME, IO_TYPE_NAME,
};
use super::super::registry::ExternalPackageRegistry;

pub(crate) fn register_std_io_package(registry: &mut ExternalPackageRegistry) {
    registry
        .register_package(ExternalPackage::new("@std/io"))
        .expect("builtin package registration should not collide");

    registry
        .register_function_in_package(
            "@std/io",
            ExternalFunctionId::Io,
            ExternalFunctionDef {
                name: IO_FUNC_NAME,
                parameters: vec![super::super::abi::ExternalParameter {
                    language_type: ExternalAbiType::Inferred,
                    access_kind: ExternalAccessKind::Shared,
                }],
                return_type: ExternalAbiType::Void,
                return_alias: ExternalReturnAlias::Fresh,
                receiver_type: None,
                receiver_access: ExternalAccessKind::Shared,
                lowerings: ExternalFunctionLowerings {
                    js: Some(
                        super::super::definitions::ExternalJsLowering::RuntimeFunction("__bs_io"),
                    ),
                    wasm: None,
                },
            },
        )
        .expect("builtin function registration should not collide");

    registry
        .register_type_in_package(
            "@std/io",
            ExternalTypeId(0),
            ExternalTypeDef {
                name: IO_TYPE_NAME,
                package: "@std/io",
                abi_type: ExternalAbiType::Handle,
            },
        )
        .expect("builtin type registration should not collide");

    registry
        .register_prelude_symbol(
            IO_FUNC_NAME,
            ExternalSymbolId::Function(ExternalFunctionId::Io),
        )
        .expect("prelude registration should not collide");

    registry
        .register_prelude_symbol(IO_TYPE_NAME, ExternalSymbolId::Type(ExternalTypeId(0)))
        .expect("prelude registration should not collide");
}
