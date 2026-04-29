//! `@core/time` package registration.
//!
//! WHAT: registers a minimal wall-clock time surface for builders that opt into it.
//! WHY: richer date/time types remain deferred; this keeps the current external package
//! contract small and testable.

use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::external_packages::{
    ExternalAbiType, ExternalAccessKind, ExternalReturnAlias,
};
use crate::compiler_frontend::external_packages::{
    ExternalFunctionLowerings, ExternalFunctionSpec, ExternalJsLowering, ExternalPackage,
};

pub fn register_core_time_package(registry: &mut ExternalPackageRegistry) {
    registry
        .register_package(ExternalPackage::new("@core/time"))
        .expect("builtin package registration should not collide");

    let time_functions: &[(&'static str, &'static str, ExternalAbiType)] = &[
        ("now_millis", "__bs_time_now_millis", ExternalAbiType::I32),
        ("now_seconds", "__bs_time_now_seconds", ExternalAbiType::F64),
    ];

    for (name, js_name, return_type) in time_functions {
        registry
            .register_external_function(
                "@core/time",
                ExternalFunctionSpec {
                    name,
                    parameters: Vec::new(),
                    return_type: return_type.clone(),
                    return_alias: ExternalReturnAlias::Fresh,
                    receiver_type: None,
                    receiver_access: ExternalAccessKind::Shared,
                    lowerings: ExternalFunctionLowerings {
                        js: Some(ExternalJsLowering::RuntimeFunction(js_name)),
                        wasm: None,
                    },
                },
            )
            .expect("builtin time function registration should not collide");
    }
}
