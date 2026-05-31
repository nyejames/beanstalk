//! Runtime slot plan data shapes.
//!
//! WHAT: Defines the AST handoff objects for runtime slot applications:
//! contribution sources, concrete wrapper slot sites, and their render pieces.
//!
//! WHY: HIR lowering needs stable IDs and already-routed plans, while AST keeps
//! ownership of schema validation, helper interpretation, and site construction.

use crate::compiler_frontend::ast::templates::template::{SlotKey, TemplateAtom, TemplateContent};
use crate::compiler_frontend::ast::templates::template_render_plan::{
    RenderPiece, TemplateRenderPlan,
};
use crate::compiler_frontend::compiler_errors::SourceLocation;

/// Result of resolving a slot application against a wrapper template.
///
/// WHAT:
/// - `Composed`: fully static expansion where every placeholder was replaced.
/// - `Runtime`: the application is valid but contains runtime-producing content,
///   so HIR lowering must handle it via accumulator locals.
///
/// WHY: A single reusable routing path should feed both compile-time expansion
/// and runtime planning without duplicating target validation or loose routing.
#[derive(Debug)]
pub(crate) enum SlotResolutionOutcome {
    Composed(TemplateContent),
    Runtime(RuntimeSlotApplicationPlan),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::compiler_frontend::ast::templates) enum SlotResolutionMode {
    AllowRuntimePlans,
    ComposeOnly,
}

impl SlotResolutionMode {
    pub(super) fn allows_runtime_plans(self) -> bool {
        matches!(self, Self::AllowRuntimePlans)
    }
}

/// AST handoff object for a runtime slot application.
///
/// WHAT: Carries the wrapper's render plan with explicit slot-site references,
/// plus the already-routed contribution sources each site can replay.
///
/// WHY: HIR lowering needs both pieces together so it can:
/// 1. allocate one accumulator per contribution source,
/// 2. lower each source exactly once,
/// 3. lower every wrapper site by loading the prepared source accumulators.
#[derive(Clone, Debug)]
pub(crate) struct RuntimeSlotApplicationPlan {
    /// The wrapper template's render plan, with placeholder occurrences replaced by site IDs.
    pub(crate) wrapper_plan: TemplateRenderPlan,

    /// Contribution chunks evaluated once before any repeated slot site renders.
    pub(crate) contribution_sources: Vec<RuntimeSlotContributionSource>,

    /// Placeholder occurrences in wrapper source order.
    pub(crate) slot_sites: Vec<RuntimeSlotSitePlan>,

    /// Source location for diagnostics and invariant reporting.
    pub(crate) location: SourceLocation,
}

/// Stable ID for a contribution source accumulator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct RuntimeSlotContributionSourceId(pub(crate) usize);

/// Stable ID for a wrapper placeholder occurrence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RuntimeSlotSiteId(pub(crate) usize);

/// One authored contribution chunk that HIR must evaluate exactly once.
#[derive(Clone, Debug)]
pub(crate) struct RuntimeSlotContributionSource {
    pub(crate) id: RuntimeSlotContributionSourceId,
    pub(crate) target: SlotKey,
    pub(crate) render_plan: TemplateRenderPlan,
    pub(crate) renders_wrapper_unconditionally: bool,
    pub(crate) location: SourceLocation,
}

/// One concrete `$slot` placeholder occurrence in the wrapper tree.
#[derive(Clone, Debug)]
pub(crate) struct RuntimeSlotSitePlan {
    pub(crate) id: RuntimeSlotSiteId,
    pub(crate) key: SlotKey,
    pub(crate) render_plan: RuntimeSlotSiteRenderPlan,
    pub(crate) location: SourceLocation,
}

/// Render plan for one slot site. Source references load already-filled source accumulators.
#[derive(Clone, Debug, Default)]
pub(crate) struct RuntimeSlotSiteRenderPlan {
    pub(crate) pieces: Vec<RuntimeSlotSitePiece>,
}

#[derive(Clone, Debug)]
pub(crate) enum RuntimeSlotSitePiece {
    Render(Box<RenderPiece>),
    ContributionSource(RuntimeSlotContributionSourceId),
}

#[derive(Clone, Debug)]
pub(super) struct RuntimeSlotContributionSourceDraft {
    pub(super) source: RuntimeSlotContributionSource,
    pub(super) atom: TemplateAtom,
}

pub(super) struct RuntimeContributionRenderPlan {
    pub(super) render_plan: TemplateRenderPlan,
    pub(super) renders_wrapper_unconditionally: bool,
}
