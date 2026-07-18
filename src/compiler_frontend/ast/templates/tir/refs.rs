//! Durable module-local TIR references.
//!
//! WHAT: stores the root, phase, and value-carried context needed to resolve a
//! template value inside one module-scoped [`TemplateIrStore`].
//! WHY: every TIR reference is local to the AST module that owns its store, so
//! no store qualification is needed to resolve it.

use std::fmt;

pub(crate) use super::ids::TemplateIrId;
use super::overlays::TemplateViewContext;
use super::view::TemplateTirPhase;

/// Durable reference to a finalized parser-emitted TIR root.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TemplateTirReference {
    pub(crate) root: TemplateIrId,
    pub(crate) phase: TemplateTirPhase,
    pub(crate) context: TemplateViewContext,
}

impl TemplateTirReference {
    #[cfg(test)]
    pub(crate) fn can_reuse_as_linear_current_state(&self) -> bool {
        self.phase.is_at_least(TemplateTirPhase::Composed)
    }
}

/// Module-local identity for a child-template occurrence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TemplateTirChildReference {
    pub(crate) root: TemplateIrId,
    pub(crate) phase: TemplateTirPhase,
    pub(crate) context: TemplateViewContext,
}

impl TemplateTirChildReference {
    pub(crate) fn new(
        root: TemplateIrId,
        phase: TemplateTirPhase,
        context: TemplateViewContext,
    ) -> Self {
        Self {
            root,
            phase,
            context,
        }
    }
}

/// Effective identity for a wrapper template in a wrapper set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TemplateWrapperReference {
    pub(crate) root: TemplateIrId,
    pub(crate) phase: TemplateTirPhase,
    pub(crate) context: TemplateViewContext,
}

impl TemplateWrapperReference {
    pub(crate) fn new(
        root: TemplateIrId,
        phase: TemplateTirPhase,
        context: TemplateViewContext,
    ) -> Self {
        Self {
            root,
            phase,
            context,
        }
    }
}

impl fmt::Display for TemplateWrapperReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TemplateWrapperReference({}, phase={:?}, context={:?})",
            self.root, self.phase, self.context
        )
    }
}
