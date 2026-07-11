//! Shared receiver-access validation for postfix calls.
//!
//! WHAT: validates whether a receiver call needs `~`, a mutable place, or no mutable marker.
//! WHY: collection builtins and user receiver methods share the same access policy but need
//! caller-specific diagnostic wording.
//!
//! All validation results are boxed `CompilerDiagnostic` values so this owner boundary does not
//! propagate large `Err` payloads through `Result<(), CompilerDiagnostic>` at every caller.
//! Callers that already hold `ExpressionParseError::Diagnostic(Box<CompilerDiagnostic>)` reuse
//! the boxed result directly via the `From<Box<CompilerDiagnostic>>` conversion.

use super::ReceiverAccessMode;
use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::place_access::{ast_node_is_mutable_place, ast_node_is_place};
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, InvalidReceiverCallReason};
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

pub(super) enum ReceiverAccessDiagnostic {
    CollectionBuiltin { method_name: StringId },
    MapBuiltin { method_name: StringId },
    ReceiverMethod { method_name: StringId },
}

pub(super) struct ReceiverAccessRequirement {
    pub requires_mutable: bool,
    pub diagnostic: ReceiverAccessDiagnostic,
}

type ReceiverAccessResult = Result<(), Box<CompilerDiagnostic>>;

// --------------------------
//  Validation entry point
// --------------------------

pub(super) fn validate_receiver_access(
    receiver_node: &AstNode,
    access_mode: ReceiverAccessMode,
    location: &SourceLocation,
    access_requirement: ReceiverAccessRequirement,
) -> ReceiverAccessResult {
    if !access_requirement.requires_mutable {
        if access_mode == ReceiverAccessMode::Mutable {
            return reject_unneeded_mutable_access_marker(&access_requirement.diagnostic, location);
        }
        return Ok(());
    }

    if !ast_node_is_place(receiver_node) {
        return reject_receiver_requires_mutable(&access_requirement.diagnostic, location);
    }

    if !ast_node_is_mutable_place(receiver_node) {
        return reject_receiver_requires_mutable(&access_requirement.diagnostic, location);
    }

    if access_mode == ReceiverAccessMode::Shared {
        return reject_missing_mutable_access_marker(&access_requirement.diagnostic, location);
    }

    Ok(())
}

// --------------------------
//  Rejection helpers
// --------------------------

/// Rejects a receiver that must be mutable but is either not a place at all or is an
/// immutable place. Both cases share the same diagnostic policy: the caller needs a mutable
/// receiver, so the reason maps to the collection/map/method-specific "mutable required"
/// diagnostic for the access context. The distinction between "not a place" and "immutable
/// place" is not surfaced to the user — the fix is the same (bind a mutable variable).
fn reject_receiver_requires_mutable(
    access_diagnostic: &ReceiverAccessDiagnostic,
    location: &SourceLocation,
) -> ReceiverAccessResult {
    let reason = match access_diagnostic {
        ReceiverAccessDiagnostic::CollectionBuiltin { .. } => {
            InvalidReceiverCallReason::MutableCollectionRequired
        }
        ReceiverAccessDiagnostic::MapBuiltin { .. } => {
            InvalidReceiverCallReason::MutableMapRequired
        }
        ReceiverAccessDiagnostic::ReceiverMethod { .. } => {
            InvalidReceiverCallReason::MutablePlaceRequired
        }
    };

    let method_name = receiver_method_name(access_diagnostic);
    Err(Box::new(CompilerDiagnostic::invalid_receiver_call(
        reason,
        None,
        Some(method_name),
        location.to_owned(),
    )))
}

fn reject_missing_mutable_access_marker(
    access_diagnostic: &ReceiverAccessDiagnostic,
    location: &SourceLocation,
) -> ReceiverAccessResult {
    let method_name = receiver_method_name(access_diagnostic);
    Err(Box::new(CompilerDiagnostic::invalid_receiver_call(
        InvalidReceiverCallReason::MissingMutableAccessMarker,
        None,
        Some(method_name),
        location.to_owned(),
    )))
}

fn reject_unneeded_mutable_access_marker(
    access_diagnostic: &ReceiverAccessDiagnostic,
    location: &SourceLocation,
) -> ReceiverAccessResult {
    let method_name = receiver_method_name(access_diagnostic);
    Err(Box::new(CompilerDiagnostic::invalid_receiver_call(
        InvalidReceiverCallReason::UnneededMutableAccessMarker,
        None,
        Some(method_name),
        location.to_owned(),
    )))
}

/// Extracts the shared `method_name` field from any access diagnostic variant.
fn receiver_method_name(access_diagnostic: &ReceiverAccessDiagnostic) -> StringId {
    match access_diagnostic {
        ReceiverAccessDiagnostic::CollectionBuiltin { method_name }
        | ReceiverAccessDiagnostic::MapBuiltin { method_name }
        | ReceiverAccessDiagnostic::ReceiverMethod { method_name } => *method_name,
    }
}
