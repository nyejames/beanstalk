//! `@core/math` package registration.

use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::external_packages::{
    ExternalAbiType, ExternalAccessKind, ExternalReturnAlias, ExternalSignatureType,
};
use crate::compiler_frontend::external_packages::{
    ExternalConstantDef, ExternalConstantValue, ExternalFunctionLowerings, ExternalFunctionSpec,
    ExternalJsLowering, external_success_returns,
};

pub fn register_core_math_package(registry: &mut ExternalPackageRegistry) {
    let package_id = registry
        .register_package(
            "@core/math",
            crate::builder_surface::PackageMetadata::binding(
                crate::builder_surface::PackageOrigin::Core,
            ),
        )
        .expect("builtin package registration should not collide");

    let math_f64_param =
        |_name: &'static str| crate::compiler_frontend::external_packages::ExternalParameter {
            language_type: ExternalSignatureType::Abi(ExternalAbiType::F64),
            access_kind: ExternalAccessKind::Shared,
        };

    let math_functions: &[(
        &'static str,
        ExternalJsLowering,
        Vec<crate::compiler_frontend::external_packages::ExternalParameter>,
    )] = &[
        (
            "sin",
            ExternalJsLowering::InlineExpression("Math.sin(#0)".to_owned()),
            vec![math_f64_param("x")],
        ),
        (
            "cos",
            ExternalJsLowering::InlineExpression("Math.cos(#0)".to_owned()),
            vec![math_f64_param("x")],
        ),
        (
            "tan",
            ExternalJsLowering::InlineExpression("Math.tan(#0)".to_owned()),
            vec![math_f64_param("x")],
        ),
        (
            "atan2",
            ExternalJsLowering::InlineExpression("Math.atan2(#0, #1)".to_owned()),
            vec![math_f64_param("y"), math_f64_param("x")],
        ),
        (
            "log",
            ExternalJsLowering::InlineExpression("Math.log(#0)".to_owned()),
            vec![math_f64_param("x")],
        ),
        (
            "log2",
            ExternalJsLowering::InlineExpression("Math.log2(#0)".to_owned()),
            vec![math_f64_param("x")],
        ),
        (
            "log10",
            ExternalJsLowering::InlineExpression("Math.log10(#0)".to_owned()),
            vec![math_f64_param("x")],
        ),
        (
            "exp",
            ExternalJsLowering::InlineExpression("Math.exp(#0)".to_owned()),
            vec![math_f64_param("x")],
        ),
        (
            "pow",
            ExternalJsLowering::InlineExpression("Math.pow(#0, #1)".to_owned()),
            vec![math_f64_param("base"), math_f64_param("exponent")],
        ),
        (
            "sqrt",
            ExternalJsLowering::InlineExpression("Math.sqrt(#0)".to_owned()),
            vec![math_f64_param("x")],
        ),
        (
            "abs",
            ExternalJsLowering::InlineExpression("Math.abs(#0)".to_owned()),
            vec![math_f64_param("x")],
        ),
        (
            "floor",
            ExternalJsLowering::InlineExpression("Math.floor(#0)".to_owned()),
            vec![math_f64_param("x")],
        ),
        (
            "ceil",
            ExternalJsLowering::InlineExpression("Math.ceil(#0)".to_owned()),
            vec![math_f64_param("x")],
        ),
        (
            "round",
            ExternalJsLowering::InlineExpression("Math.round(#0)".to_owned()),
            vec![math_f64_param("x")],
        ),
        (
            "trunc",
            ExternalJsLowering::InlineExpression("Math.trunc(#0)".to_owned()),
            vec![math_f64_param("x")],
        ),
        (
            "min",
            ExternalJsLowering::InlineExpression("Math.min(#0, #1)".to_owned()),
            vec![math_f64_param("a"), math_f64_param("b")],
        ),
        (
            "max",
            ExternalJsLowering::InlineExpression("Math.max(#0, #1)".to_owned()),
            vec![math_f64_param("a"), math_f64_param("b")],
        ),
        (
            "clamp",
            ExternalJsLowering::InlineExpression("Math.min(Math.max(#0, #1), #2)".to_owned()),
            vec![
                math_f64_param("x"),
                math_f64_param("min"),
                math_f64_param("max"),
            ],
        ),
    ];

    for (name, js_lowering, parameters) in math_functions {
        registry
            .register_external_function(
                package_id,
                ExternalFunctionSpec {
                    name: (*name).to_owned(),
                    parameters: parameters.clone(),
                    returns: external_success_returns(
                        ExternalAbiType::F64,
                        ExternalReturnAlias::Fresh,
                    ),
                    error_return_type: None,
                    lowerings: ExternalFunctionLowerings {
                        js: Some(js_lowering.clone()),
                        wasm: None,
                    },
                },
            )
            .expect("builtin math function registration should not collide");
    }

    let math_constants: &[(&'static str, ExternalConstantValue)] = &[
        ("PI", ExternalConstantValue::Float(std::f64::consts::PI)),
        ("TAU", ExternalConstantValue::Float(std::f64::consts::TAU)),
        ("E", ExternalConstantValue::Float(std::f64::consts::E)),
    ];

    for (name, value) in math_constants {
        registry
            .register_external_constant(
                package_id,
                ExternalConstantDef {
                    name: (*name).to_owned(),
                    data_type: ExternalAbiType::F64,
                    value: *value,
                },
            )
            .expect("builtin math constant registration should not collide");
    }
}
