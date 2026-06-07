//! Shared receiver-access validation for postfix calls.
//!
//! WHAT: validates whether a receiver call needs `~`, a mutable place, or no mutable marker.
//! WHY: collection builtins and user receiver methods share the same access policy but need
//! caller-specific diagnostic wording.

#![allow(clippy::result_large_err)]

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

// --------------------------
//  Validation entry point
// --------------------------

pub(super) fn validate_receiver_access(
    receiver_node: &AstNode,
    access_mode: ReceiverAccessMode,
    location: &SourceLocation,
    access_requirement: ReceiverAccessRequirement,
) -> Result<(), CompilerDiagnostic> {
    if !access_requirement.requires_mutable {
        if access_mode == ReceiverAccessMode::Mutable {
            return reject_unneeded_mutable_access_marker(&access_requirement.diagnostic, location);
        }
        return Ok(());
    }

    if !ast_node_is_place(receiver_node) {
        return reject_non_place_receiver(&access_requirement.diagnostic, location);
    }

    if !ast_node_is_mutable_place(receiver_node) {
        return reject_immutable_receiver(&access_requirement.diagnostic, location);
    }

    if access_mode == ReceiverAccessMode::Shared {
        return reject_missing_mutable_access_marker(&access_requirement.diagnostic, location);
    }

    Ok(())
}

// --------------------------
//  Rejection helpers
// --------------------------

fn reject_non_place_receiver(
    access_diagnostic: &ReceiverAccessDiagnostic,
    location: &SourceLocation,
) -> Result<(), CompilerDiagnostic> {
    match access_diagnostic {
        ReceiverAccessDiagnostic::CollectionBuiltin { method_name } => {
            Err(CompilerDiagnostic::invalid_receiver_call(
                InvalidReceiverCallReason::MutableCollectionRequired,
                None,
                Some(*method_name),
                location.to_owned(),
            ))
        }
        ReceiverAccessDiagnostic::MapBuiltin { method_name } => {
            Err(CompilerDiagnostic::invalid_receiver_call(
                InvalidReceiverCallReason::MutableMapRequired,
                None,
                Some(*method_name),
                location.to_owned(),
            ))
        }
        ReceiverAccessDiagnostic::ReceiverMethod { method_name } => {
            Err(CompilerDiagnostic::invalid_receiver_call(
                InvalidReceiverCallReason::MutablePlaceRequired,
                None,
                Some(*method_name),
                location.to_owned(),
            ))
        }
    }
}

fn reject_immutable_receiver(
    access_diagnostic: &ReceiverAccessDiagnostic,
    location: &SourceLocation,
) -> Result<(), CompilerDiagnostic> {
    match access_diagnostic {
        ReceiverAccessDiagnostic::CollectionBuiltin { method_name } => {
            Err(CompilerDiagnostic::invalid_receiver_call(
                InvalidReceiverCallReason::MutableCollectionRequired,
                None,
                Some(*method_name),
                location.to_owned(),
            ))
        }
        ReceiverAccessDiagnostic::MapBuiltin { method_name } => {
            Err(CompilerDiagnostic::invalid_receiver_call(
                InvalidReceiverCallReason::MutableMapRequired,
                None,
                Some(*method_name),
                location.to_owned(),
            ))
        }
        ReceiverAccessDiagnostic::ReceiverMethod { method_name } => {
            Err(CompilerDiagnostic::invalid_receiver_call(
                InvalidReceiverCallReason::MutablePlaceRequired,
                None,
                Some(*method_name),
                location.to_owned(),
            ))
        }
    }
}

fn reject_missing_mutable_access_marker(
    access_diagnostic: &ReceiverAccessDiagnostic,
    location: &SourceLocation,
) -> Result<(), CompilerDiagnostic> {
    match access_diagnostic {
        ReceiverAccessDiagnostic::CollectionBuiltin { method_name }
        | ReceiverAccessDiagnostic::MapBuiltin { method_name }
        | ReceiverAccessDiagnostic::ReceiverMethod { method_name } => {
            Err(CompilerDiagnostic::invalid_receiver_call(
                InvalidReceiverCallReason::MissingMutableAccessMarker,
                None,
                Some(*method_name),
                location.to_owned(),
            ))
        }
    }
}

fn reject_unneeded_mutable_access_marker(
    access_diagnostic: &ReceiverAccessDiagnostic,
    location: &SourceLocation,
) -> Result<(), CompilerDiagnostic> {
    match access_diagnostic {
        ReceiverAccessDiagnostic::CollectionBuiltin { method_name }
        | ReceiverAccessDiagnostic::MapBuiltin { method_name }
        | ReceiverAccessDiagnostic::ReceiverMethod { method_name } => {
            Err(CompilerDiagnostic::invalid_receiver_call(
                InvalidReceiverCallReason::UnneededMutableAccessMarker,
                None,
                Some(*method_name),
                location.to_owned(),
            ))
        }
    }
}
