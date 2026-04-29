//! `@core/text` package registration.
//!
//! WHAT: registers the minimal text helper surface for builders that opt into it.
//! WHY: text helpers are external package metadata so frontend visibility, type checking,
//! and backend lowering all share one canonical API shape.

use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::external_packages::{
    ExternalAbiType, ExternalAccessKind, ExternalReturnAlias,
};
use crate::compiler_frontend::external_packages::{
    ExternalFunctionLowerings, ExternalFunctionSpec, ExternalJsLowering, ExternalPackage,
    ExternalParameter,
};

pub fn register_core_text_package(registry: &mut ExternalPackageRegistry) {
    registry
        .register_package(ExternalPackage::new("@core/text"))
        .expect("builtin package registration should not collide");

    let text_param = ExternalParameter {
        language_type: ExternalAbiType::Utf8Str,
        access_kind: ExternalAccessKind::Shared,
    };

    let text_functions: &[(
        &'static str,
        &'static str,
        Vec<ExternalParameter>,
        ExternalAbiType,
    )] = &[
        (
            "length",
            "__bs_text_length",
            vec![text_param.clone()],
            ExternalAbiType::I32,
        ),
        (
            "is_empty",
            "__bs_text_is_empty",
            vec![text_param.clone()],
            ExternalAbiType::Bool,
        ),
        (
            "contains",
            "__bs_text_contains",
            vec![text_param.clone(), text_param.clone()],
            ExternalAbiType::Bool,
        ),
        (
            "starts_with",
            "__bs_text_starts_with",
            vec![text_param.clone(), text_param.clone()],
            ExternalAbiType::Bool,
        ),
        (
            "ends_with",
            "__bs_text_ends_with",
            vec![text_param.clone(), text_param],
            ExternalAbiType::Bool,
        ),
    ];

    for (name, js_name, parameters, return_type) in text_functions {
        registry
            .register_external_function(
                "@core/text",
                ExternalFunctionSpec {
                    name,
                    parameters: parameters.clone(),
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
            .expect("builtin text function registration should not collide");
    }
}
