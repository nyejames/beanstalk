//! Call-target semantic resolution for borrow transfer.
//!
//! Maps call targets to per-argument effects and result alias behavior.

use crate::compiler_frontend::analysis::borrow_checker::types::FunctionReturnAliasSummary;
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorLocation};
use crate::compiler_frontend::host_functions::{
    CallTarget, HostAccessKind, HostFunctionDef, HostReturnAlias,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::return_borrow_checker_error;

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
    location: ErrorLocation,
) -> Result<CallSemantics, CompilerError> {
    match target {
        CallTarget::UserFunction(path) => {
            let Some(function_id) = context.function_by_path.get(path).copied() else {
                return_borrow_checker_error!(
                    format!(
                        "Borrow checker could not resolve user call target '{}'",
                        context.diagnostics.path_name(path)
                    ),
                    location,
                    {
                        CompilationStage => "Borrow Checking",
                        PrimarySuggestion => "Ensure the called function is declared in the module before use",
                    }
                );
            };

            let Some(param_mutability) = context.function_param_mutability.get(&function_id) else {
                return_borrow_checker_error!(
                    format!(
                        "Borrow checker is missing parameter mutability metadata for function '{}'",
                        context.diagnostics.function_name(function_id)
                    ),
                    context.diagnostics.function_error_location(function_id),
                    {
                        CompilationStage => "Borrow Checking",
                    }
                );
            };

            if param_mutability.len() != arg_len {
                return_borrow_checker_error!(
                    format!(
                        "Borrow checker found argument count mismatch for function '{}': expected {}, got {}",
                        context.diagnostics.function_name(function_id),
                        param_mutability.len(),
                        arg_len
                    ),
                    location,
                    {
                        CompilationStage => "Borrow Checking",
                        PrimarySuggestion => "Ensure call argument count matches the function signature",
                    }
                );
            }

            let return_alias = match context.function_return_alias.get(&function_id) {
                Some(FunctionReturnAliasSummary::Fresh) => CallResultAlias::Fresh,
                Some(FunctionReturnAliasSummary::AliasParams(indices)) => {
                    validate_alias_indices(
                        indices,
                        arg_len,
                        location.clone(),
                        format!(
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

        CallTarget::HostFunction(path) => {
            let host_def = resolve_host_definition(context, path, location.clone())?;
            if host_def.parameters.len() != arg_len {
                return_borrow_checker_error!(
                    format!(
                        "Borrow checker found argument count mismatch for host function '{}': expected {}, got {}",
                        host_def.name,
                        host_def.parameters.len(),
                        arg_len
                    ),
                    location,
                    {
                        CompilationStage => "Borrow Checking",
                        PrimarySuggestion => "Ensure call argument count matches host function signature",
                    }
                );
            }

            let arg_effects = host_def
                .parameters
                .iter()
                .map(|param| match param.access_kind {
                    HostAccessKind::Shared => ArgEffect::SharedBorrow,
                    HostAccessKind::Mutable => ArgEffect::MutableBorrow,
                })
                .collect::<Vec<_>>();

            let return_alias = match host_def.return_alias {
                HostReturnAlias::Fresh => CallResultAlias::Fresh,
                HostReturnAlias::AliasArgs(ref indices) => {
                    validate_alias_indices(
                        indices,
                        arg_len,
                        location.clone(),
                        format!("host function '{}'", host_def.name),
                    )?;
                    CallResultAlias::AliasArgs(indices.clone())
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
    path: &InternedPath,
    location: ErrorLocation,
) -> Result<&'a HostFunctionDef, CompilerError> {
    // Full path lookup is authoritative. Leaf lookup is a compatibility fallback
    // for host registrations that only expose the leaf symbol.
    let full = path.to_string(context.string_table);
    if let Some(definition) = context.host_registry.get_function(&full) {
        return Ok(definition);
    }

    if let Some(name) = path.name_str(context.string_table)
        && let Some(definition) = context.host_registry.get_function(name)
    {
        return Ok(definition);
    }

    return_borrow_checker_error!(
        format!(
            "Borrow checker could not resolve host call target '{}'",
            context.diagnostics.path_name(path)
        ),
        location,
        {
            CompilationStage => "Borrow Checking",
            PrimarySuggestion => "Ensure host registry metadata includes this host function",
        }
    )
}

fn validate_alias_indices(
    indices: &[usize],
    arg_len: usize,
    location: ErrorLocation,
    callee_name: String,
) -> Result<(), CompilerError> {
    for index in indices {
        if *index < arg_len {
            continue;
        }

        return_borrow_checker_error!(
            format!(
                "Borrow checker found out-of-range return-alias index {} for {} with {} argument(s)",
                index, callee_name, arg_len
            ),
            location,
            {
                CompilationStage => "Borrow Checking",
                PrimarySuggestion => "Ensure return-alias metadata only references existing parameter indices",
            }
        );
    }

    Ok(())
}
