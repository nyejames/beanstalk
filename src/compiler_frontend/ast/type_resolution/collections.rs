//! Fixed collection capacity folding for type resolution.
//!
//! WHAT: resolves the narrow `ParsedCollectionCapacity` forms (integer literal or bare
//!       constant name) into a canonical `usize` capacity used by `TypeEnvironment`.
//! WHY: keeping capacity folding separate from the rest of type resolution lets
//!      `resolve_type.rs` focus on parsed-ref orchestration and diagnostic-type conversion,
//!      while this module owns the constant-evaluation boundary for collection sizes.

use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::module_ast::scope_context::ScopeContext;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidCollectionTypeReason,
};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::parsed::ParsedCollectionCapacity;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

/// Fold a parsed collection capacity into a canonical `usize`.
///
/// WHAT: resolves the narrow `ParsedCollectionCapacity` forms (integer literal or bare
///       constant name) directly without invoking the full expression parser.
/// WHY: the parser already rejected general capacity forms, so type resolution only
///      needs to validate literals and look up bare constants in the visible scope.
pub(crate) fn fold_collection_capacity(
    capacity: &ParsedCollectionCapacity,
    scope_context: Option<&ScopeContext>,
    type_environment: &mut TypeEnvironment,
) -> Result<usize, CompilerDiagnostic> {
    match capacity {
        ParsedCollectionCapacity::Literal { value, location } => {
            validate_capacity_value(*value, location)
        }

        ParsedCollectionCapacity::BareConstant { name, location } => {
            let Some(scope_context) = scope_context else {
                return Err(CompilerDiagnostic::invalid_collection_type(
                    InvalidCollectionTypeReason::CapacityNotConstant,
                    location.clone(),
                ));
            };

            let Some(declaration) = scope_context.get_reference(name) else {
                return Err(CompilerDiagnostic::invalid_collection_type(
                    InvalidCollectionTypeReason::CapacityNotConstant,
                    location.clone(),
                ));
            };

            if !scope_context.is_explicit_compile_time_constant(declaration)
                || !declaration.value.is_compile_time_constant()
            {
                return Err(CompilerDiagnostic::invalid_collection_type(
                    InvalidCollectionTypeReason::CapacityNotConstant,
                    location.clone(),
                ));
            }

            if declaration.value.type_id != type_environment.builtins().int {
                return Err(CompilerDiagnostic::invalid_collection_type(
                    InvalidCollectionTypeReason::CapacityNotInt,
                    location.clone(),
                ));
            }

            let ExpressionKind::Int(value) = &declaration.value.kind else {
                return Err(CompilerDiagnostic::invalid_collection_type(
                    InvalidCollectionTypeReason::CapacityNotInt,
                    location.clone(),
                ));
            };

            validate_capacity_value(*value, location)
        }
    }
}

fn validate_capacity_value(
    value: i64,
    location: &SourceLocation,
) -> Result<usize, CompilerDiagnostic> {
    if value < 0 {
        return Err(CompilerDiagnostic::invalid_collection_type(
            InvalidCollectionTypeReason::NegativeCapacity,
            location.clone(),
        ));
    }

    if value == 0 {
        return Err(CompilerDiagnostic::invalid_collection_type(
            InvalidCollectionTypeReason::ZeroCapacity,
            location.clone(),
        ));
    }

    usize::try_from(value).map_err(|_| {
        CompilerDiagnostic::invalid_collection_type(
            InvalidCollectionTypeReason::CapacityOverflow,
            location.clone(),
        )
    })
}
