//! Trait conformance diagnostic construction.
//!
//! WHAT: Constructs `CompilerDiagnostic` payloads and diagnostic labels for trait conformance failures.
//! WHY: Centralizes reporting structure for missing requirements, override issues, duplicate conformances,
//!      and signature mismatches, keeping them separated from validation logic.

use crate::compiler_frontend::ast::ReceiverMethodEntry;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DiagnosticLabel, DiagnosticLabelMessage, InvalidTraitConformanceReason,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::traits::definitions::ResolvedTraitRequirement;

pub(super) fn invalid_conformance(
    target_name: StringId,
    trait_name: Option<StringId>,
    reason: InvalidTraitConformanceReason,
    primary_location: SourceLocation,
    mut secondary_labels: Vec<DiagnosticLabel>,
) -> CompilerDiagnostic {
    let mut labels = vec![DiagnosticLabel::primary(primary_location.clone())];
    labels.append(&mut secondary_labels);

    CompilerDiagnostic::invalid_trait_conformance(target_name, trait_name, reason, primary_location)
        .with_labels(labels)
}

pub(super) fn previous_declaration_label(
    previous_location: Option<SourceLocation>,
) -> Vec<DiagnosticLabel> {
    previous_location
        .map(|location| {
            vec![DiagnosticLabel::secondary(
                location,
                Some(DiagnosticLabelMessage::PreviousDeclaration),
            )]
        })
        .unwrap_or_default()
}

pub(super) fn requirement_label(
    requirement: &ResolvedTraitRequirement,
    string_table: &mut StringTable,
) -> Vec<DiagnosticLabel> {
    vec![DiagnosticLabel::secondary(
        requirement.location.clone(),
        Some(DiagnosticLabelMessage::RenderedText(
            string_table.intern("trait requirement"),
        )),
    )]
}

pub(super) fn requirement_and_method_labels(
    requirement: &ResolvedTraitRequirement,
    method: &ReceiverMethodEntry,
    string_table: &mut StringTable,
) -> Vec<DiagnosticLabel> {
    vec![
        DiagnosticLabel::secondary(
            requirement.location.clone(),
            Some(DiagnosticLabelMessage::RenderedText(
                string_table.intern("trait requirement"),
            )),
        ),
        DiagnosticLabel::secondary(
            method
                .signature
                .parameters
                .first()
                .map(|parameter| parameter.value.location.clone())
                .unwrap_or_default(),
            Some(DiagnosticLabelMessage::RenderedText(
                string_table.intern("receiver method"),
            )),
        ),
    ]
}
