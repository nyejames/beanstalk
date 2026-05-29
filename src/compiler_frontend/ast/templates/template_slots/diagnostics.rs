//! Slot composition diagnostics.
//!
//! WHAT: Structured error construction for slot composition failures.
//!
//! WHY: Keeping error messages in one place makes them easier to review for
//! consistency and ensures every composition error path produces an actionable
//! diagnostic.

use crate::compiler_frontend::ast::templates::template::SlotKey;
use crate::compiler_frontend::ast::templates::template_slots::error::TemplateSlotError;
use crate::compiler_frontend::compiler_errors::SourceLocation;
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, InvalidTemplateSlotReason};
use crate::compiler_frontend::symbols::string_interning::StringTable;

pub(super) fn extra_loose_content_without_default_slot_error(
    location: &SourceLocation,
) -> TemplateSlotError {
    CompilerDiagnostic::invalid_template_slot(
        InvalidTemplateSlotReason::ExtraLooseContentWithoutDefaultSlot,
        None,
        location.to_owned(),
    )
    .into()
}

pub(super) fn loose_content_without_default_slot_error(
    location: &SourceLocation,
) -> TemplateSlotError {
    CompilerDiagnostic::invalid_template_slot(
        InvalidTemplateSlotReason::LooseContentWithoutDefaultSlot,
        None,
        location.to_owned(),
    )
    .into()
}

pub(super) fn unknown_slot_target_error(
    target: &SlotKey,
    location: &SourceLocation,
    _string_table: &StringTable,
) -> TemplateSlotError {
    match target {
        SlotKey::Default => CompilerDiagnostic::invalid_template_slot(
            InvalidTemplateSlotReason::InsertCannotTargetDefaultSlot,
            None,
            location.to_owned(),
        )
        .into(),
        SlotKey::Named(name) => CompilerDiagnostic::invalid_template_slot(
            InvalidTemplateSlotReason::InsertTargetsUnknownNamedSlot,
            Some(*name),
            location.to_owned(),
        )
        .into(),
        SlotKey::Positional(_) => CompilerDiagnostic::invalid_template_slot(
            InvalidTemplateSlotReason::InsertTargetsUnknownPositionalSlot,
            None,
            location.to_owned(),
        )
        .into(),
    }
}
