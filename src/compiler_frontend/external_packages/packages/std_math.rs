//! `@std/math` package registration.

use super::super::abi::{ExternalAbiType, ExternalAccessKind, ExternalReturnAlias};
use super::super::definitions::{
    ExternalConstantDef, ExternalConstantValue, ExternalFunctionLowerings, ExternalFunctionSpec,
    ExternalJsLowering, ExternalPackage,
};
use super::super::registry::ExternalPackageRegistry;

pub(crate) fn register_std_math_package(registry: &mut ExternalPackageRegistry) {
    registry
        .register_package(ExternalPackage::new("@std/math"))
        .expect("builtin package registration should not collide");

    let math_f64_param = |_name: &'static str| super::super::abi::ExternalParameter {
        language_type: ExternalAbiType::F64,
        access_kind: ExternalAccessKind::Shared,
    };

    let math_functions: &[(
        &'static str,
        &'static str,
        Vec<super::super::abi::ExternalParameter>,
    )] = &[
        ("sin", "__bs_math_sin", vec![math_f64_param("x")]),
        ("cos", "__bs_math_cos", vec![math_f64_param("x")]),
        ("tan", "__bs_math_tan", vec![math_f64_param("x")]),
        (
            "atan2",
            "__bs_math_atan2",
            vec![math_f64_param("y"), math_f64_param("x")],
        ),
        ("log", "__bs_math_log", vec![math_f64_param("x")]),
        ("log2", "__bs_math_log2", vec![math_f64_param("x")]),
        ("log10", "__bs_math_log10", vec![math_f64_param("x")]),
        ("exp", "__bs_math_exp", vec![math_f64_param("x")]),
        (
            "pow",
            "__bs_math_pow",
            vec![math_f64_param("base"), math_f64_param("exponent")],
        ),
        ("sqrt", "__bs_math_sqrt", vec![math_f64_param("x")]),
        ("abs", "__bs_math_abs", vec![math_f64_param("x")]),
        ("floor", "__bs_math_floor", vec![math_f64_param("x")]),
        ("ceil", "__bs_math_ceil", vec![math_f64_param("x")]),
        ("round", "__bs_math_round", vec![math_f64_param("x")]),
        ("trunc", "__bs_math_trunc", vec![math_f64_param("x")]),
        (
            "min",
            "__bs_math_min",
            vec![math_f64_param("a"), math_f64_param("b")],
        ),
        (
            "max",
            "__bs_math_max",
            vec![math_f64_param("a"), math_f64_param("b")],
        ),
        (
            "clamp",
            "__bs_math_clamp",
            vec![
                math_f64_param("x"),
                math_f64_param("min"),
                math_f64_param("max"),
            ],
        ),
    ];

    for (name, js_name, parameters) in math_functions {
        registry
            .register_external_function(
                "@std/math",
                ExternalFunctionSpec {
                    name,
                    parameters: parameters.clone(),
                    return_type: ExternalAbiType::F64,
                    return_alias: ExternalReturnAlias::Fresh,
                    receiver_type: None,
                    receiver_access: ExternalAccessKind::Shared,
                    lowerings: ExternalFunctionLowerings {
                        js: Some(ExternalJsLowering::RuntimeFunction(js_name)),
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
                "@std/math",
                ExternalConstantDef {
                    name,
                    data_type: ExternalAbiType::F64,
                    value: *value,
                },
            )
            .expect("builtin math constant registration should not collide");
    }
}
