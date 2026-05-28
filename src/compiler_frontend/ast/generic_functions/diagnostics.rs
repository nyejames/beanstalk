//! Generic function diagnostics owned by AST call/template handling.
//!
//! WHAT: provides focused constructors and helpers for generic free-function inference,
//! concrete instantiation context, and unsupported generic function value use.
//! WHY: call parsing and instance emission should report structured generic facts without
//! knowing diagnostic rendering details.

use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DiagnosticLabel, DiagnosticLabelMessage, DiagnosticLabelStyle,
    InvalidGenericInstantiationReason,
};
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

pub(crate) fn cannot_infer_generic_function_arguments(
    function_name: Option<StringId>,
    missing_parameters: Vec<StringId>,
    location: SourceLocation,
) -> CompilerDiagnostic {
    CompilerDiagnostic::invalid_generic_instantiation(
        function_name,
        InvalidGenericInstantiationReason::CannotInferFunctionArguments { missing_parameters },
        location,
    )
}

pub(crate) fn conflicting_generic_function_argument(
    function_name: Option<StringId>,
    parameter_name: StringId,
    location: SourceLocation,
) -> CompilerDiagnostic {
    CompilerDiagnostic::invalid_generic_instantiation(
        function_name,
        InvalidGenericInstantiationReason::ConflictingFunctionArgument { parameter_name },
        location,
    )
}

pub(crate) fn recursive_generic_function_instantiation(
    function_name: Option<StringId>,
    location: SourceLocation,
) -> CompilerDiagnostic {
    CompilerDiagnostic::invalid_generic_instantiation(
        function_name,
        InvalidGenericInstantiationReason::RecursiveFunctionInstantiation,
        location,
    )
}

/// Rebuild a concrete-body diagnostic so the generic call site is primary.
///
/// WHAT: stores the original diagnostic primary location as the generic body location,
/// makes the call location the diagnostic primary location, and rebuilds labels so the
/// first label is a primary call-site label with `GenericInstantiationCallSite`.
/// WHY: the call selected the concrete type arguments, so the call site should be primary
/// and the generic body span should be secondary.
pub(crate) fn with_generic_instantiation_context(
    mut diagnostic: CompilerDiagnostic,
    call_location: SourceLocation,
) -> CompilerDiagnostic {
    let body_location = diagnostic.primary_location.clone();

    // The call site selected the concrete type arguments, so it becomes primary.
    diagnostic.primary_location = call_location.clone();

    let mut new_labels = Vec::with_capacity(diagnostic.labels.len() + 2);

    // Primary call-site label.
    new_labels.push(DiagnosticLabel {
        location: call_location,
        style: DiagnosticLabelStyle::Primary,
        message: Some(DiagnosticLabelMessage::GenericInstantiationCallSite),
    });

    // Avoid duplicate body-site secondary if that body location is already present.
    let body_already_secondary = diagnostic.labels.iter().any(|label| {
        label.style == DiagnosticLabelStyle::Secondary && label.location == body_location
    });

    if !body_already_secondary {
        new_labels.push(DiagnosticLabel::secondary(
            body_location,
            Some(DiagnosticLabelMessage::GenericInstantiationBodySite),
        ));
    }

    // Preserve existing non-primary labels from the original diagnostic.
    for label in diagnostic.labels {
        if label.style != DiagnosticLabelStyle::Primary {
            new_labels.push(label);
        }
    }

    diagnostic.labels = new_labels;
    diagnostic
}
