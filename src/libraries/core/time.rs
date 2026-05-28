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
    ExternalFunctionLowerings, ExternalFunctionSpec, ExternalJsLowering, ExternalPackageOrigin,
    external_success_returns,
};

pub fn register_core_time_package(registry: &mut ExternalPackageRegistry) {
    let package_id = registry
        .register_package("@core/time", ExternalPackageOrigin::Builtin)
        .expect("builtin package registration should not collide");

    registry
        .register_external_function(
            package_id,
            ExternalFunctionSpec {
                name: "now_millis".to_owned(),
                parameters: Vec::new(),
                returns: external_success_returns(ExternalAbiType::I32, ExternalReturnAlias::Fresh),
                error_return_type: None,
                receiver_type: None,
                receiver_access: ExternalAccessKind::Shared,
                lowerings: ExternalFunctionLowerings {
                    js: Some(ExternalJsLowering::InlineExpression(
                        "Date.now()".to_owned(),
                    )),
                    wasm: None,
                },
            },
        )
        .expect("builtin now_millis registration should not collide");

    registry
        .register_external_function(
            package_id,
            ExternalFunctionSpec {
                name: "now_seconds".to_owned(),
                parameters: Vec::new(),
                returns: external_success_returns(ExternalAbiType::F64, ExternalReturnAlias::Fresh),
                error_return_type: None,
                receiver_type: None,
                receiver_access: ExternalAccessKind::Shared,
                lowerings: ExternalFunctionLowerings {
                    js: Some(ExternalJsLowering::RuntimeFunction(
                        "__bs_time_now_seconds".to_owned(),
                    )),
                    wasm: None,
                },
            },
        )
        .expect("builtin now_seconds registration should not collide");
}
