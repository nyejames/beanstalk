//! Call-target semantic resolution for borrow transfer.
//!
//! WHAT: maps HIR call targets to per-argument effects and result alias behavior.
//! WHY: external package access policy and user-function signature summaries are normalized once
//! here so statement transfer can stay focused on state transitions.

use crate::compiler_frontend::analysis::borrow_checker::BorrowCheckError;
use crate::compiler_frontend::analysis::borrow_checker::types::FunctionReturnAliasSummary;
use crate::compiler_frontend::compiler_errors::SourceLocation;
use crate::compiler_frontend::external_packages::{
    CallTarget, ExternalAccessKind, ExternalFunctionDef, ExternalFunctionId, ExternalReturnAlias,
};

use super::BorrowTransferContext;

#[derive(Debug, Clone)]
pub(super) struct CallSemantics {
    pub(super) arg_effects: Vec<ArgEffect>,
    pub(super) return_alias: CallResultAlias,
}

/// Per-argument effect contract consumed by transfer.
///
/// Why this exists:
/// `~` call parameters are not always plain mutable borrows. They can either
/// remain a mutable borrow or become a consuming move based on last-use facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ArgEffect {
    SharedBorrow,
    MutableBorrow,
    MayConsume,
}

#[derive(Debug, Clone)]
pub(super) enum CallResultAlias {
    Fresh,
    AliasArgs(Vec<usize>),
    Unknown,
}

pub(super) fn resolve_call_semantics(
    context: &BorrowTransferContext<'_>,
    target: &CallTarget,
    arg_len: usize,
    location: SourceLocation,
) -> Result<CallSemantics, BorrowCheckError> {
    match target {
        CallTarget::UserFunction(function_id) => {
            let function_id = *function_id;
            let Some(param_mutability) = context.function_param_mutability.get(&function_id) else {
                return Err(context.diagnostics.internal_error(
                    format!(
                        "Borrow checker is missing parameter mutability metadata for function '{}'",
                        context.diagnostics.function_name(function_id)
                    ),
                    context.diagnostics.function_error_location(function_id),
                ));
            };

            if param_mutability.len() != arg_len {
                return Err(context.diagnostics.internal_error(
                    format!(
                        "Borrow checker found argument count mismatch for function '{}': expected {}, got {}",
                        context.diagnostics.function_name(function_id),
                        param_mutability.len(),
                        arg_len
                    ),
                    location,
                ));
            }

            let return_alias = match context.function_return_alias.get(&function_id) {
                Some(FunctionReturnAliasSummary::Fresh) => CallResultAlias::Fresh,
                Some(FunctionReturnAliasSummary::AliasParams(indices)) => {
                    validate_alias_indices(
                        context,
                        indices,
                        arg_len,
                        location.clone(),
                        &format!(
                            "user function '{}'",
                            context.diagnostics.function_name(function_id)
                        ),
                    )?;
                    CallResultAlias::AliasArgs(indices.clone())
                }
                Some(FunctionReturnAliasSummary::Unknown) | None => CallResultAlias::Unknown,
            };

            Ok(CallSemantics {
                // Mutable user parameters can either borrow mutably or consume ownership.
                // Transfer chooses the concrete behavior with last-use analysis.
                arg_effects: param_mutability
                    .iter()
                    .map(|is_mutable| {
                        if *is_mutable {
                            ArgEffect::MayConsume
                        } else {
                            ArgEffect::SharedBorrow
                        }
                    })
                    .collect(),
                return_alias,
            })
        }

        CallTarget::ExternalFunction(id) => {
            let host_def = resolve_host_definition(context, *id, location.clone())?;
            if host_def.parameters.len() != arg_len {
                return Err(context.diagnostics.internal_error(
                    format!(
                        "Borrow checker found argument count mismatch for host function '{}': expected {}, got {}",
                        host_def.name,
                        host_def.parameters.len(),
                        arg_len
                    ),
                    location,
                ));
            }

            let arg_effects = host_def
                .parameters
                .iter()
                .map(|param| match param.access_kind {
                    ExternalAccessKind::Shared => ArgEffect::SharedBorrow,
                    ExternalAccessKind::Mutable => ArgEffect::MutableBorrow,
                })
                .collect::<Vec<_>>();

            let return_alias = match host_def.hir_return_alias() {
                ExternalReturnAlias::Fresh => CallResultAlias::Fresh,
                ExternalReturnAlias::AliasArgs(indices) => {
                    validate_alias_indices(
                        context,
                        &indices,
                        arg_len,
                        location.clone(),
                        &format!("host function '{}'", host_def.name),
                    )?;
                    CallResultAlias::AliasArgs(indices)
                }
            };

            Ok(CallSemantics {
                arg_effects,
                return_alias,
            })
        }
    }
}

fn resolve_host_definition<'a>(
    context: &'a BorrowTransferContext<'_>,
    id: ExternalFunctionId,
    location: SourceLocation,
) -> Result<&'a ExternalFunctionDef, BorrowCheckError> {
    // Host metadata is keyed by the stable ExternalFunctionId emitted into HIR.
    // Borrow checking should not silently reinterpret a missing symbol through a
    // second fallback path because that can hide registry drift.
    if let Some(definition) = context.external_package_registry.get_function_by_id(id) {
        return Ok(definition);
    }

    Err(context.diagnostics.internal_error(
        format!(
            "Borrow checker could not resolve host call target '{}'",
            id.name()
        ),
        location,
    ))
}

fn validate_alias_indices(
    context: &BorrowTransferContext<'_>,
    indices: &[usize],
    arg_len: usize,
    location: SourceLocation,
    callee_name: &str,
) -> Result<(), BorrowCheckError> {
    for index in indices {
        if *index < arg_len {
            continue;
        }

        return Err(context.diagnostics.internal_error(
            format!(
                "Borrow checker found out-of-range return-alias index {} for {} with {} argument(s)",
                index, callee_name, arg_len
            ),
            location,
        ));
    }

    Ok(())
}
