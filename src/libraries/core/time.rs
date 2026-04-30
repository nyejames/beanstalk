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

    registry
        .register_external_function(
            "@core/time",
            ExternalFunctionSpec {
                name: "now_millis",
                parameters: Vec::new(),
                return_type: ExternalAbiType::I32,
                return_alias: ExternalReturnAlias::Fresh,
                receiver_type: None,
                receiver_access: ExternalAccessKind::Shared,
                lowerings: ExternalFunctionLowerings {
                    js: Some(ExternalJsLowering::InlineExpression("Date.now()")),
                    wasm: None,
                },
            },
        )
        .expect("builtin now_millis registration should not collide");

    registry
        .register_external_function(
            "@core/time",
            ExternalFunctionSpec {
                name: "now_seconds",
                parameters: Vec::new(),
                return_type: ExternalAbiType::F64,
                return_alias: ExternalReturnAlias::Fresh,
                receiver_type: None,
                receiver_access: ExternalAccessKind::Shared,
                lowerings: ExternalFunctionLowerings {
                    js: Some(ExternalJsLowering::RuntimeFunction("__bs_time_now_seconds")),
                    wasm: None,
                },
            },
        )
        .expect("builtin now_seconds registration should not collide");
}
