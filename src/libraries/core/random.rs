//! `@core/random` package registration.
//!
//! WHAT: registers a minimal random-number surface for builders that opt into it.
//! WHY: this proves optional core external packages can grow without making the compiler
//! assume every builder supports them.

use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::external_packages::{
    ExternalAbiType, ExternalAccessKind, ExternalReturnAlias,
};
use crate::compiler_frontend::external_packages::{
    ExternalFunctionLowerings, ExternalFunctionSpec, ExternalJsLowering, ExternalPackage,
    ExternalParameter,
};

pub fn register_core_random_package(registry: &mut ExternalPackageRegistry) {
    registry
        .register_package(ExternalPackage::new("@core/random"))
        .expect("builtin package registration should not collide");

    registry
        .register_external_function(
            "@core/random",
            ExternalFunctionSpec {
                name: "random_float",
                parameters: Vec::new(),
                return_type: ExternalAbiType::F64,
                return_alias: ExternalReturnAlias::Fresh,
                receiver_type: None,
                receiver_access: ExternalAccessKind::Shared,
                lowerings: ExternalFunctionLowerings {
                    js: Some(ExternalJsLowering::RuntimeFunction("__bs_random_float")),
                    wasm: None,
                },
            },
        )
        .expect("builtin random_float registration should not collide");

    let int_param = ExternalParameter {
        language_type: ExternalAbiType::I32,
        access_kind: ExternalAccessKind::Shared,
    };

    registry
        .register_external_function(
            "@core/random",
            ExternalFunctionSpec {
                name: "random_int",
                parameters: vec![int_param.clone(), int_param],
                return_type: ExternalAbiType::I32,
                return_alias: ExternalReturnAlias::Fresh,
                receiver_type: None,
                receiver_access: ExternalAccessKind::Shared,
                lowerings: ExternalFunctionLowerings {
                    js: Some(ExternalJsLowering::RuntimeFunction("__bs_random_int")),
                    wasm: None,
                },
            },
        )
        .expect("builtin random_int registration should not collide");
}
