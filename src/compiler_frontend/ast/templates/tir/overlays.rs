//! Final TIR overlay sets and overlay ID types.
//!
//! WHAT: `TemplateOverlaySet` groups the three overlay dimensions — expression
//! overrides, slot resolution, and wrapper context — behind a single
//! registry-owned set ID. Each dimension carries a typed overlay ID that
//! indexes into a registry-owned overlay entry table.
//!
//! WHY: the final TIR system applies contextual changes as overlays rather than
//! mutating shared structural roots. Overlay sets are immutable once allocated
//! and canonicalized by the registry so equivalent sets share one ID. This lets
//! `TemplateTirReference` carry a single overlay-set ID instead of ad hoc maps.
//!
//! ## Canonical resolution order
//!
//! When overlay sets are composed, dimensions are resolved in canonical order:
//!
//! ```text
//! versioned structural root
//! -> wrapper context at child-template occurrence boundaries
//! -> slot resolution at slot occurrence boundaries
//! -> expression override at dynamic-expression nodes
//! -> consumer sees effective view
//! ```
//!
//! Within each dimension the last non-`None` value in composition order wins,
//! so later contextual overlays can replace earlier entries for the same
//! dimension.
//! See `TemplateIrRegistry::compose_overlay_sets`.
//!
//! ## Ownership contract
//!
//! Overlays are AST-local. They are not exposed to HIR, backends, or the public
//! API. The registry owns overlay set and overlay entry storage; IDs remain
//! valid only within the `TemplateIrRegistry` that created them.
//!
//! ## Payload coverage
//!
//! Expression and slot-resolution payloads are production surfaces. They carry
//! occurrence-keyed entries by `ExpressionSiteId` and `SlotOccurrenceId`.
//! Wrapper-context overlays carry inherited `$children(..)` wrapper sets and
//! `$fresh` suppression state for individual child-template occurrences. They
//! are read by the view-native classification and fold paths so finalization
//! can apply inherited wrappers without mutating shared child-template nodes.

use std::fmt;

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::template::SlotKey;
use crate::compiler_frontend::ast::templates::tir::ids::{
    ChildTemplateOccurrenceId, ExpressionSiteId, SlotOccurrenceId,
};
use crate::compiler_frontend::ast::templates::tir::refs::{TemplateRef, TemplateWrapperSetRef};

// -------------------------
//  Overlay set ID
// -------------------------

/// Stable index for an overlay set in `TemplateIrRegistry`.
///
/// WHAT: identifies one immutable `TemplateOverlaySet` allocated by the
/// registry. Equivalent overlay sets share one ID after canonicalization.
/// WHY: `TemplateTirReference` carries a single overlay-set ID so later phases
/// resolve contextual changes through one stable handle instead of ad hoc maps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TemplateOverlaySetId(u32);

impl TemplateOverlaySetId {
    /// Creates a new ID from a raw index.
    ///
    /// Panics if the index exceeds `u32::MAX`. This is an internal invariant —
    /// no realistic overlay set count will approach this bound.
    pub(crate) fn new(index: usize) -> Self {
        Self(
            u32::try_from(index)
                .expect("template overlay set index exceeds u32::MAX; this is a compiler bug"),
        )
    }

    /// Returns the raw index for registry lookups.
    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }

    /// Returns the canonical empty overlay-set ID.
    ///
    /// WHAT: the registry always allocates the empty set at index 0, so this is
    ///       a stable identity for "no overlays" even before a registry is
    ///       available.
    /// WHY: construction sites that emit `ChildTemplate` nodes before the
    ///      registry finalizes the parent reference (e.g. parser emission,
    ///      current-state materialization) still need a valid overlay-set ID.
    ///      Callers that already have a registry should prefer
    ///      `TemplateIrRegistry::allocate_overlay_set(TemplateOverlaySet::empty())`.
    pub(crate) fn empty() -> Self {
        Self(0)
    }

    /// Returns a zero-valued overlay-set ID for test fixtures without a registry.
    ///
    /// WHAT: alias for [`Self::empty`] kept for test fixtures that predate the
    ///       production-safe constructor.
    /// WHY: avoids churn in focused tests while making the production path
    ///      explicit.
    #[cfg(test)]
    pub(crate) fn empty_for_test() -> Self {
        Self::empty()
    }
}

impl fmt::Display for TemplateOverlaySetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TemplateOverlaySetId({})", self.0)
    }
}

// -------------------------
//  Overlay dimension IDs
// -------------------------

/// Stable index for an expression overlay entry in `TemplateIrRegistry`.
///
/// WHAT: identifies a registry-owned expression override applied at
/// dynamic-expression nodes. The concrete payload carries expression overrides
/// keyed by `ExpressionSiteId`.
/// WHY: expression overrides are one of the three overlay dimensions; a typed
/// ID keeps the reference distinct from slot and wrapper overlays and lets the
/// registry own expression-overlay storage centrally.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TirExpressionOverlayId(u32);

impl TirExpressionOverlayId {
    /// Creates a new ID from a raw index.
    ///
    /// Panics if the index exceeds `u32::MAX`. This is an internal invariant.
    pub(crate) fn new(index: usize) -> Self {
        Self(
            u32::try_from(index)
                .expect("expression overlay index exceeds u32::MAX; this is a compiler bug"),
        )
    }

    /// Returns the raw index for registry lookups.
    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}

impl fmt::Display for TirExpressionOverlayId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TirExpressionOverlayId({})", self.0)
    }
}

/// Stable index for a slot resolution overlay entry in `TemplateIrRegistry`.
///
/// WHAT: identifies a registry-owned slot resolution applied at slot occurrence
/// boundaries. The concrete payload carries slot resolutions keyed by
/// `SlotOccurrenceId`.
/// WHY: slot resolution is one of the three overlay dimensions; a typed ID keeps
/// the reference distinct from expression and wrapper overlays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TirSlotResolutionOverlayId(u32);

impl TirSlotResolutionOverlayId {
    /// Creates a new ID from a raw index.
    ///
    /// Panics if the index exceeds `u32::MAX`. This is an internal invariant.
    pub(crate) fn new(index: usize) -> Self {
        Self(
            u32::try_from(index)
                .expect("slot resolution overlay index exceeds u32::MAX; this is a compiler bug"),
        )
    }

    /// Returns the raw index for registry lookups.
    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}

impl fmt::Display for TirSlotResolutionOverlayId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TirSlotResolutionOverlayId({})", self.0)
    }
}

/// Stable index for a wrapper context overlay entry in `TemplateIrRegistry`.
///
/// WHAT: identifies a registry-owned wrapper context applied at child-template
/// occurrence boundaries. The concrete payload carries inherited wrapper,
/// `$fresh`, and output-guard context keyed by child occurrence.
/// WHY: wrapper context is one of the three overlay dimensions; a typed ID
/// keeps the reference distinct from expression and slot overlays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TirWrapperContextOverlayId(u32);

impl TirWrapperContextOverlayId {
    /// Creates a new ID from a raw index.
    ///
    /// Panics if the index exceeds `u32::MAX`. This is an internal invariant.
    pub(crate) fn new(index: usize) -> Self {
        Self(
            u32::try_from(index)
                .expect("wrapper context overlay index exceeds u32::MAX; this is a compiler bug"),
        )
    }

    /// Returns the raw index for registry lookups.
    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}

impl fmt::Display for TirWrapperContextOverlayId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TirWrapperContextOverlayId({})", self.0)
    }
}

// -------------------------
//  Slot resolution value type
// -------------------------

/// Effective content state for one slot occurrence.
///
/// WHAT: distinguishes slots that are resolved to one or more contribution
///       templates from slots that are known missing or still unresolved.
/// WHY: missing slots render as empty output, while unresolved slots remain
///      structural work for later phases. Keeping those states explicit avoids
///      treating "no contribution templates" as an ambiguous magic value.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TirSlotResolutionKind {
    /// The slot occurrence resolves to these contribution template refs.
    ///
    /// Repeated slot occurrences can carry equivalent source lists so replay is
    /// represented by data, not by consuming the routed contribution.
    Resolved { sources: Vec<TemplateRef> },

    /// The slot was routed and intentionally receives no content.
    Missing,

    /// The slot occurrence has not been routed by this overlay yet.
    #[cfg(test)]
    Unresolved,
}

/// One slot-resolution overlay entry for a slot occurrence.
///
/// WHAT: records the slot key and effective content state for one `Slot`
///       occurrence when a slot-resolution overlay applies.
/// WHY: slot resolution needs a typed value so overlay entries and `TirView`
///      lookups return a clear, testable result. The key makes default,
///      named, and positional routing explicit at the overlay boundary, while
///      the content state supports missing slots and repeated-slot replay.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TirSlotResolution {
    /// Slot key that this occurrence declared.
    pub(crate) key: SlotKey,
    /// Effective routed content for the occurrence.
    pub(crate) kind: TirSlotResolutionKind,
}

impl TirSlotResolution {
    /// Creates a resolved slot entry with one or more contribution sources.
    pub(crate) fn resolved(key: SlotKey, sources: Vec<TemplateRef>) -> Self {
        Self {
            key,
            kind: TirSlotResolutionKind::Resolved { sources },
        }
    }

    /// Creates an entry for a slot that intentionally renders empty.
    pub(crate) fn missing(key: SlotKey) -> Self {
        Self {
            key,
            kind: TirSlotResolutionKind::Missing,
        }
    }

    /// Creates an entry for a slot that remains structurally unresolved.
    #[cfg(test)]
    pub(crate) fn unresolved(key: SlotKey) -> Self {
        Self {
            key,
            kind: TirSlotResolutionKind::Unresolved,
        }
    }

    /// Returns the contribution source refs for a resolved slot.
    #[cfg(test)]
    pub(crate) fn sources(&self) -> &[TemplateRef] {
        match &self.kind {
            TirSlotResolutionKind::Resolved { sources } => sources,
            TirSlotResolutionKind::Missing | TirSlotResolutionKind::Unresolved => &[],
        }
    }

    /// Returns true when this occurrence intentionally renders no content.
    #[cfg(test)]
    pub(crate) fn is_missing(&self) -> bool {
        matches!(self.kind, TirSlotResolutionKind::Missing)
    }

    /// Returns true when this occurrence has not been routed yet.
    ///
    /// The `Unresolved` variant is only constructed in focused tests today; in
    /// production builds the state is unreachable, so this always returns false.
    pub(crate) fn is_unresolved(&self) -> bool {
        #[cfg(test)]
        {
            matches!(self.kind, TirSlotResolutionKind::Unresolved)
        }
        #[cfg(not(test))]
        {
            false
        }
    }
}

// -------------------------
//  Overlay payloads
// -------------------------

// These payload structs give the overlay dimension IDs real registry storage
// with final-system-oriented names. Each payload carries occurrence-keyed
// entries so `TirView` can resolve contextual template state centrally instead
// of making consumers combine ad hoc maps.

/// Registry-owned expression override payload.
///
/// WHAT: carries expression overrides keyed by `ExpressionSiteId`. Each entry
///       replaces the structural expression at one dynamic-expression splice
///       site (or branch-selector / loop-header site) when the overlay applies.
/// WHY: expression overlays are one of the three overlay dimensions; storing
///      overrides as a keyed list lets `TirView` resolve effective expressions
///      for a site without traversing the structural tree.
///
/// Entries are stored as a flat list rather than a hash map because
/// `Expression` does not implement `Hash` or `Eq`, and realistic overlays
/// contain few entries. Linear lookup is sufficient for the overlay sizes
/// expected in template composition.
#[derive(Clone, Debug, Default)]
pub(crate) struct TirExpressionOverlay {
    /// Expression overrides, keyed by document-order `ExpressionSiteId`.
    pub(crate) overrides: Vec<(ExpressionSiteId, Box<Expression>)>,
}

impl TirExpressionOverlay {
    /// Looks up the override expression for a site, if one exists in this overlay.
    ///
    /// WHAT: linear scan of the override list. Returns the boxed expression
    ///       reference when the site has an override, or `None` when it does not.
    /// WHY: `TirView` calls this to resolve effective expressions for a site;
    ///      keeping the lookup on the payload centralizes the scan logic.
    pub(crate) fn expression_for_site(&self, site_id: ExpressionSiteId) -> Option<&Expression> {
        self.overrides
            .iter()
            .find(|(id, _)| *id == site_id)
            .map(|(_, expression)| expression.as_ref())
    }
}

/// Registry-owned slot resolution payload.
///
/// WHAT: carries slot resolutions keyed by `SlotOccurrenceId`. Each entry
///       describes what contribution fills one slot occurrence when the
///       overlay applies.
/// WHY: slot resolution is one of the three overlay dimensions; storing
///      resolutions as a keyed list lets `TirView` resolve effective slot
///      content for an occurrence without traversing the structural tree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TirSlotResolutionOverlay {
    /// Slot resolutions, keyed by document-order `SlotOccurrenceId`.
    pub(crate) resolutions: Vec<(SlotOccurrenceId, TirSlotResolution)>,
}

impl TirSlotResolutionOverlay {
    /// Looks up the slot resolution for an occurrence, if one exists.
    ///
    /// WHAT: linear scan of the resolution list. Returns the resolution
    ///       reference when the occurrence has a resolution, or `None` when it
    ///       does not.
    /// WHY: `TirView` calls this to resolve effective slot content for an
    ///      occurrence; keeping the lookup on the payload centralizes the scan.
    pub(crate) fn resolution_for_occurrence(
        &self,
        occurrence_id: SlotOccurrenceId,
    ) -> Option<&TirSlotResolution> {
        self.resolutions
            .iter()
            .find(|(id, _)| *id == occurrence_id)
            .map(|(_, resolution)| resolution)
    }
}

// -------------------------
//  Wrapper application mode
// -------------------------

/// Controls when inherited wrappers apply to a child-template occurrence.
///
/// WHAT: distinguishes ordinary direct children (wrappers always apply) from
///       control-flow children (wrappers apply only if the child structurally
///       emitted output).
/// WHY: the real semantic rule is not "guarded by this condition expression" —
///      it is "apply wrappers only if this child structurally emitted output."
///      That rule matters for false `if` branches, no-else branches, zero-
///      iteration loops, and break/continue behavior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TirWrapperApplicationMode {
    /// Wrappers always apply around this child occurrence.
    #[default]
    Always,

    /// Wrappers apply only when the child occurrence structurally emits output.
    /// Used for control-flow children where skipped branches or zero-iteration
    /// loops must not receive wrappers.
    IfChildEmits,
}

/// Wrapper context applied at one child-template occurrence.
///
/// WHAT: records the inherited direct-child wrapper set, `$fresh` suppression
///       state, and wrapper application mode that apply at a child-template
///       boundary.
/// WHY: direct-child wrapper application is contextual. Storing it by child
///      occurrence lets `TirView` wrapper resolution apply context without
///      mutating shared child-template nodes.
///
/// Design constraint: the conditional output guard must use an explicit
/// `TirWrapperApplicationMode` enum (`Always` / `IfChildEmits`), not loose
/// expression-site guard vectors. The real semantic rule is not "guarded by
/// this condition expression" — it is "apply wrappers only if this child
/// structurally emitted output." That rule matters for false `if` branches,
/// no-else branches, zero-iteration loops, and break/continue behavior.
///
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TirWrapperContext {
    /// Wrapper set inherited by this child occurrence, when one applies.
    pub(crate) inherited_wrapper_set: Option<TemplateWrapperSetRef>,
    /// True when `$fresh` suppresses the immediate parent wrapper context.
    pub(crate) skip_parent_child_wrappers: bool,
    /// When wrappers apply to this child occurrence.
    pub(crate) application_mode: TirWrapperApplicationMode,
}

#[cfg(test)]
impl TirWrapperContext {
    /// Creates wrapper context with no inherited wrappers or special mode.
    pub(crate) fn empty() -> Self {
        Self::default()
    }

    /// Creates wrapper context for a child occurrence with inherited wrappers.
    pub(crate) fn inherited(wrapper_set: TemplateWrapperSetRef) -> Self {
        Self {
            inherited_wrapper_set: Some(wrapper_set),
            skip_parent_child_wrappers: false,
            application_mode: TirWrapperApplicationMode::Always,
        }
    }

    /// Returns true when no wrapper or guard context applies.
    pub(crate) fn is_empty(&self) -> bool {
        self.inherited_wrapper_set.is_none()
            && !self.skip_parent_child_wrappers
            && matches!(self.application_mode, TirWrapperApplicationMode::Always)
    }
}

/// Registry-owned wrapper context overlay payload.
///
/// WHAT: carries wrapper context keyed by `ChildTemplateOccurrenceId`.
/// WHY: wrapper context is one of the three overlay dimensions; storing
///      occurrence-keyed entries lets `TirView` resolve inherited wrappers and
///      `$fresh` suppression at child-template boundaries without ad hoc maps.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TirWrapperContextOverlay {
    /// Wrapper contexts keyed by document-order child-template occurrence ID.
    pub(crate) contexts: Vec<(ChildTemplateOccurrenceId, TirWrapperContext)>,
}

impl TirWrapperContextOverlay {
    /// Looks up wrapper context for a child occurrence, if one exists.
    pub(crate) fn context_for_occurrence(
        &self,
        occurrence_id: ChildTemplateOccurrenceId,
    ) -> Option<&TirWrapperContext> {
        self.contexts
            .iter()
            .find(|(id, _)| *id == occurrence_id)
            .map(|(_, context)| context)
    }
}

// -------------------------
//  Overlay set
// -------------------------

/// A registry-owned, immutable set of overlay dimension references.
///
/// WHAT: groups the three overlay dimensions — expression overrides, slot
/// resolution, and wrapper context — behind one canonical registry ID. Each
/// field is `None` when that dimension has no overlay for this set.
/// WHY: `TemplateTirReference` carries one overlay-set ID; the registry
/// canonicalizes equivalent sets so consumers never combine overlay maps ad hoc.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TemplateOverlaySet {
    /// Expression override applied at dynamic-expression nodes.
    pub(crate) expression_overrides: Option<TirExpressionOverlayId>,
    /// Slot resolution applied at slot occurrence boundaries.
    pub(crate) slot_resolution: Option<TirSlotResolutionOverlayId>,
    /// Wrapper context applied at child-template occurrence boundaries.
    pub(crate) wrapper_context: Option<TirWrapperContextOverlayId>,
}

impl TemplateOverlaySet {
    /// Creates an overlay set with no overlays in any dimension.
    ///
    /// WHAT: the canonical "no contextual changes" set. The registry
    /// canonicalizes all empty sets to this single entry.
    /// WHY: most templates carry no overlays; a named constructor makes the
    /// intent explicit at call sites instead of relying on `Default` derivation.
    pub(crate) fn empty() -> Self {
        Self::default()
    }

    /// Returns `true` when no overlay dimension is set.
    ///
    /// WHAT: a quick emptiness check used by the registry to keep the canonical
    /// empty set unique and by callers that want to short-circuit overlay work.
    pub(crate) fn is_empty(&self) -> bool {
        self.expression_overrides.is_none()
            && self.slot_resolution.is_none()
            && self.wrapper_context.is_none()
    }
}
