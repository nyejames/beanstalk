//! Shared receiver-access validation for postfix calls.
//!
//! WHAT: validates whether a receiver call needs `~`, a mutable place, or no mutable marker.
//! WHY: collection builtins, map builtins and user receiver methods share one access policy
//! but need caller-specific diagnostic wording. One classifier distinguishes non-place,
//! immutable-place and mutable-place receivers so each source state gets distinct guidance.
//!
//! All validation results are boxed `CompilerDiagnostic` values so this owner boundary does not
//! propagate large `Err` payloads through `Result<(), CompilerDiagnostic>` at every caller.
//! Callers that already hold `ExpressionParseError::Diagnostic(Box<CompilerDiagnostic>)` reuse
//! the boxed result directly via the `From<Box<CompilerDiagnostic>>` conversion.

use super::ReceiverAccessMode;
use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::place_access::{
    ReceiverSourceState, classify_receiver_source_state,
};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidReceiverCallReason, ReceiverCallKind,
};
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

/// Which receiver-call surface owns a receiver-access diagnostic, plus the method name.
///
/// WHAT: carries the method name and maps the access context to the `ReceiverCallKind` payload
///       fact the renderer uses to name the receiver kind.
/// WHY: source methods, collection builtins and map builtins share one classifier; the kind
///      only selects the rendered noun, so it stays out of the reason enum.
pub(super) enum ReceiverAccessDiagnostic {
    CollectionBuiltin { method_name: StringId },
    MapBuiltin { method_name: StringId },
    ReceiverMethod { method_name: StringId },
}

impl ReceiverAccessDiagnostic {
    fn method_name(&self) -> StringId {
        match self {
            ReceiverAccessDiagnostic::CollectionBuiltin { method_name }
            | ReceiverAccessDiagnostic::MapBuiltin { method_name }
            | ReceiverAccessDiagnostic::ReceiverMethod { method_name } => *method_name,
        }
    }

    fn receiver_kind(&self) -> ReceiverCallKind {
        match self {
            ReceiverAccessDiagnostic::CollectionBuiltin { .. } => {
                ReceiverCallKind::CollectionBuiltin
            }
            ReceiverAccessDiagnostic::MapBuiltin { .. } => ReceiverCallKind::MapBuiltin,
            ReceiverAccessDiagnostic::ReceiverMethod { .. } => ReceiverCallKind::SourceMethod,
        }
    }
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
    method_boundary: &SourceLocation,
    authored_marker_location: Option<&SourceLocation>,
    access_requirement: ReceiverAccessRequirement,
) -> ReceiverAccessResult {
    // A call that does not require mutable access rejects an authored `~` at the marker, since
    // the marker is the source the author must remove.
    if !access_requirement.requires_mutable {
        if access_mode == ReceiverAccessMode::Mutable {
            return reject_unneeded_mutable_access_marker(
                &access_requirement.diagnostic,
                authored_marker_location,
                method_boundary,
            );
        }
        return Ok(());
    }

    let source_state = classify_receiver_source_state(receiver_node);

    match (access_mode, source_state) {
        // An existing mutable place needs the explicit `~` marker. The method boundary is the
        // call site the author must prefix; the authored marker is absent here. The binding name
        // lets the renderer show a concrete `~name.method(...)` example when it is known.
        (ReceiverAccessMode::Shared, ReceiverSourceState::MutablePlace { binding_name }) => {
            reject_mutable_receiver_missing_marker(
                &access_requirement.diagnostic,
                binding_name,
                method_boundary,
            )
        }
        // An immutable existing place cannot be repaired by adding `~`: the binding itself must
        // be declared mutable. No marker was authored, so point at the method boundary.
        (ReceiverAccessMode::Shared, ReceiverSourceState::ImmutablePlace { binding_name }) => {
            reject_immutable_receiver_mutable_method(
                &access_requirement.diagnostic,
                binding_name,
                method_boundary,
            )
        }
        // A temporary or non-place receiver cannot be mutated through. No marker was authored,
        // so point at the method boundary.
        (ReceiverAccessMode::Shared, ReceiverSourceState::Temporary) => {
            reject_non_place_receiver_mutable_method(
                &access_requirement.diagnostic,
                method_boundary,
            )
        }
        // An existing mutable place with an authored `~` satisfies the call.
        (ReceiverAccessMode::Mutable, ReceiverSourceState::MutablePlace { .. }) => Ok(()),
        // `~` authored on an immutable place: the marker is the source the author must change,
        // and the binding must be declared mutable before the marker is valid.
        (ReceiverAccessMode::Mutable, ReceiverSourceState::ImmutablePlace { binding_name }) => {
            reject_mutable_marker_on_immutable_receiver(
                &access_requirement.diagnostic,
                binding_name,
                authored_marker_location,
                method_boundary,
            )
        }
        // `~` authored on a temporary or non-place value: the marker is invalid because `~`
        // accepts only an existing mutable place.
        (ReceiverAccessMode::Mutable, ReceiverSourceState::Temporary) => {
            reject_mutable_marker_on_non_place_receiver(
                &access_requirement.diagnostic,
                authored_marker_location,
                method_boundary,
            )
        }
    }
}

// --------------------------
//  Rejection helpers
// --------------------------

fn reject_mutable_receiver_missing_marker(
    access_diagnostic: &ReceiverAccessDiagnostic,
    binding_name: Option<StringId>,
    method_boundary: &SourceLocation,
) -> ReceiverAccessResult {
    reject(
        InvalidReceiverCallReason::MutableReceiverMissingMarker,
        access_diagnostic,
        binding_name,
        method_boundary,
    )
}

fn reject_immutable_receiver_mutable_method(
    access_diagnostic: &ReceiverAccessDiagnostic,
    binding_name: Option<StringId>,
    method_boundary: &SourceLocation,
) -> ReceiverAccessResult {
    reject(
        InvalidReceiverCallReason::ImmutableReceiverMutableMethod,
        access_diagnostic,
        binding_name,
        method_boundary,
    )
}

fn reject_non_place_receiver_mutable_method(
    access_diagnostic: &ReceiverAccessDiagnostic,
    method_boundary: &SourceLocation,
) -> ReceiverAccessResult {
    reject(
        InvalidReceiverCallReason::NonPlaceReceiverMutableMethod,
        access_diagnostic,
        None,
        method_boundary,
    )
}

fn reject_mutable_marker_on_immutable_receiver(
    access_diagnostic: &ReceiverAccessDiagnostic,
    binding_name: Option<StringId>,
    authored_marker_location: Option<&SourceLocation>,
    method_boundary: &SourceLocation,
) -> ReceiverAccessResult {
    reject(
        InvalidReceiverCallReason::MutableMarkerOnImmutableReceiver,
        access_diagnostic,
        binding_name,
        authored_marker_location.unwrap_or(method_boundary),
    )
}

fn reject_mutable_marker_on_non_place_receiver(
    access_diagnostic: &ReceiverAccessDiagnostic,
    authored_marker_location: Option<&SourceLocation>,
    method_boundary: &SourceLocation,
) -> ReceiverAccessResult {
    reject(
        InvalidReceiverCallReason::MutableMarkerOnNonPlaceReceiver,
        access_diagnostic,
        None,
        authored_marker_location.unwrap_or(method_boundary),
    )
}

fn reject_unneeded_mutable_access_marker(
    access_diagnostic: &ReceiverAccessDiagnostic,
    authored_marker_location: Option<&SourceLocation>,
    method_boundary: &SourceLocation,
) -> ReceiverAccessResult {
    reject(
        InvalidReceiverCallReason::UnneededMutableAccessMarker,
        access_diagnostic,
        None,
        authored_marker_location.unwrap_or(method_boundary),
    )
}

/// Builds the shared receiver-access diagnostic from the reason, access context and location.
///
/// WHAT: threads the method name, receiver kind and optional simple receiver binding name into
///       the structured payload, and never repurposes the type field as a value name.
/// WHY: every receiver-access rejection shares one payload shape, so the renderer can name the
///      receiver kind and binding from facts instead of guessing from a type label.
fn reject(
    reason: InvalidReceiverCallReason,
    access_diagnostic: &ReceiverAccessDiagnostic,
    receiver_binding_name: Option<StringId>,
    location: &SourceLocation,
) -> ReceiverAccessResult {
    Err(Box::new(CompilerDiagnostic::invalid_receiver_call(
        reason,
        None,
        Some(access_diagnostic.method_name()),
        Some(access_diagnostic.receiver_kind()),
        receiver_binding_name,
        location.to_owned(),
    )))
}
