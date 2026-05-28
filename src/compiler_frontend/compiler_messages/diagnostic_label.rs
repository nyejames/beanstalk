//! Source labels attached to structured diagnostics.
//!
//! WHAT: represents primary and secondary spans with optional typed label messages.
//! WHY: diagnostics need enough structure for terminal rendering, dev-server rendering, and future
//! tooling without carrying final prose in compiler stages.

use crate::compiler_frontend::compiler_messages::source_location::SourceLocation;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringIdRemap};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticLabel {
    pub location: SourceLocation,
    pub style: DiagnosticLabelStyle,
    pub message: Option<DiagnosticLabelMessage>,
}

impl DiagnosticLabel {
    pub(crate) fn primary(location: SourceLocation) -> Self {
        Self {
            location,
            style: DiagnosticLabelStyle::Primary,
            message: None,
        }
    }

    pub(crate) fn secondary(
        location: SourceLocation,
        message: Option<DiagnosticLabelMessage>,
    ) -> Self {
        Self {
            location,
            style: DiagnosticLabelStyle::Secondary,
            message,
        }
    }

    pub(crate) fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.location.remap_string_ids(remap);

        if let Some(message) = &mut self.message {
            message.remap_string_ids(remap);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiagnosticLabelStyle {
    Primary,
    Secondary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiagnosticLabelMessage {
    PreviousDeclaration,
    ExistingBorrow,
    ExpectedTypeDeclaredHere,
    ValueMovedHere,
    /// Render-ready label text for diagnostics that need local phrasing.
    RenderedText(StringId),
    /// Marks the call site that triggered a generic function concrete instance emission.
    GenericInstantiationCallSite,
    /// Marks the generic function body location where the concrete instantiation failed.
    GenericInstantiationBodySite,
}

impl DiagnosticLabelMessage {
    pub(crate) fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        if let DiagnosticLabelMessage::RenderedText(message) = self {
            *message = remap.get(*message);
        }
    }
}
