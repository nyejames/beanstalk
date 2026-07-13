//! TIR node types.
//!
//! WHAT: defines `TemplateIr`, `TemplateIrNode`, and `TemplateIrNodeKind` — the
//! core data shapes of the Template IR. A template owns a root node ID; nodes
//! form a tree via child references back into the store.
//!
//! WHY: the AST template pipeline needs one structural owner for content,
//! control flow, slots, and formatting. Typed TIR nodes let folding, formatting,
//! and HIR preparation work from that stable representation without rebuilding
//! content trees on demand.
//!
//! ## Ownership contract
//!
//! TIR nodes are owned by `TemplateIrStore`. They do not own HIR or backend
//! data. Node children are `Copy` IDs that index back into the store, keeping
//! the tree cheap to traverse and clone.
//!
//! ## Semantic parity constraint
//!
//! TIR must preserve user-visible template semantics. Behaviour changes are out
//! of scope unless they are bug fixes with regression tests.
//!
//! ## No feature flag
//!
//! TIR types and the production route are implemented directly without a
//! feature flag.

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::template::{
    ReactiveSubscription, SlotKey, Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchSelector, TemplateLoopControlKind, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_slots::RuntimeSlotSiteId;
use crate::compiler_frontend::ast::templates::tir::ids::{
    ChildTemplateOccurrenceId, ExpressionSiteId, SlotOccurrenceId, TemplateIrId, TemplateIrNodeId,
    TemplateSlotPlanId, TemplateWrapperSetId,
};
use crate::compiler_frontend::ast::templates::tir::refs::TemplateTirChildReference;
use crate::compiler_frontend::ast::templates::tir::summary::TemplateIrSummary;
use crate::compiler_frontend::symbols::string_interning::StringId;
#[cfg(test)]
use crate::compiler_frontend::symbols::string_interning::StringIdRemap;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

// -------------------------
//  Top-Level Template IR
// -------------------------

/// A top-level template entry in the TIR store.
///
/// WHAT: connects a root node, style, kind, summary metadata, and source location.
/// WHY: TIR templates are the authoritative internal representation after parsing.
/// Each template is a thin handle; the body tree lives in `TemplateIrNode` entries
/// owned by the store.
#[derive(Clone, Debug)]
pub(crate) struct TemplateIr {
    /// Root node of this template's body tree.
    pub(crate) root: TemplateIrNodeId,

    /// Style directive configuration (e.g., `$md`, `$raw`).
    pub(crate) style: Style,

    /// High-level template classification (string, string-function, slot, etc.).
    pub(crate) kind: TemplateType,

    /// Summary metadata for capacity planning and quick shape queries.
    pub(crate) summary: TemplateIrSummary,

    /// Source location for diagnostics.
    pub(crate) location: SourceLocation,

    /// Parent `$children(..)` wrappers that must be applied around this
    /// template's output during folding.
    ///
    /// WHAT: references a `TemplateWrapperSet` side-table entry that stores
    /// store-local TIR IDs for wrappers inherited from the AST `Template`. The
    /// TIR fold path resolves the ID and applies those wrapper templates.
    /// WHY: wrapper sets live in the store so identical wrapper combinations can
    /// be shared in later phases, and so `TemplateIr` does not own recursive
    /// wrapper template values per template.
    pub(crate) conditional_child_wrapper_set: Option<TemplateWrapperSetId>,

    /// AST-prepared runtime slot application plan, when this template is the
    /// wrapper output for a runtime slot application.
    ///
    /// WHAT: references a `TemplateSlotPlan` side-table entry. Runtime slot site
    /// nodes inside this template also carry the same plan ID so each site ID is
    /// anchored to the plan that owns it.
    /// WHY: runtime slot routing remains AST-owned, but TIR needs a stable
    /// handoff before HIR/runtime metadata can migrate away from `Template`.
    pub(crate) runtime_slot_plan: Option<TemplateSlotPlanId>,
}

impl TemplateIr {
    /// Creates a top-level TIR template with no wrapper set or slot plan attached.
    ///
    /// WHAT: stores the root node, style, kind, summary, and source location.
    /// WHY: side-table links (`conditional_child_wrapper_set`, `runtime_slot_plan`)
    ///      are attached later by composition or slot-plan materialization.
    pub(crate) fn new(
        root: TemplateIrNodeId,
        style: Style,
        kind: TemplateType,
        summary: TemplateIrSummary,
        location: SourceLocation,
    ) -> Self {
        Self {
            root,
            style,
            kind,
            summary,
            location,
            conditional_child_wrapper_set: None,
            runtime_slot_plan: None,
        }
    }
}

// -------------------------
//  Template IR Node
// -------------------------

/// A single node in a template body tree.
///
/// WHAT: pairs a `TemplateIrNodeKind` with a source location.
/// WHY: every node carries its source position for diagnostics, and the kind
/// enum determines what the node represents (text, expression, slot, etc.).
#[derive(Clone, Debug)]
pub(crate) struct TemplateIrNode {
    /// Discriminant and payload for this node.
    pub(crate) kind: TemplateIrNodeKind,

    /// Source location for diagnostics and source-map output.
    pub(crate) location: SourceLocation,
}

impl TemplateIrNode {
    /// Creates a TIR node with the given structural kind and source location.
    pub(crate) fn new(kind: TemplateIrNodeKind, location: SourceLocation) -> Self {
        Self { kind, location }
    }
}

// -------------------------
//  Template IR Node Kind
// -------------------------

/// Structural variant for a TIR node.
///
/// WHAT: enumerates every kind of structural element a template body can contain.
/// WHY: text, expressions, child templates, and slot placeholders each need a
/// distinct structural role. TIR node kinds give each role its own variant so
/// folding, formatting, and HIR lowering can dispatch cleanly.
///
/// ## Design notes
///
/// - `Vec` fields in `Sequence` and `BranchChain` may be replaced with typed
///   ranges into store-owned side vectors in later phases if clone pressure
///   becomes measurable.
/// - `DynamicExpression` boxes the AST `Expression` to keep enum size reasonable.
///   TIR does not own expression resolution; it holds the AST-produced value.
#[derive(Clone, Debug)]
#[allow(
    clippy::large_enum_variant,
    reason = "DynamicExpression holds an AST Expression; boxing is sufficient"
)]
pub(crate) enum TemplateIrNodeKind {
    /// Ordered sequence of child nodes (the common case for template bodies).
    Sequence { children: Vec<TemplateIrNodeId> },

    /// Literal interned text.
    Text {
        /// Interned text content.
        text: StringId,

        /// Byte length of the original text segment.
        ///
        /// WHAT: records the UTF-8 byte length of the source text that was
        /// interned, used by formatting and span calculations.
        /// WHY: the interned string loses original byte-length information, but
        /// downstream formatting decisions need the original segment size.
        byte_len: u32,

        /// Origin classification for diagnostics and formatting.
        origin: TemplateSegmentOrigin,
    },

    /// Runtime or compile-time expression splice.
    DynamicExpression {
        expression: Box<Expression>,
        origin: TemplateSegmentOrigin,
        /// Direct `$(source)` reactive subscription carried by this splice, if any.
        ///
        /// WHAT: preserves the per-segment subscription marker that the AST
        /// attaches to `$(source)` template chunks.
        /// WHY: HIR lowering needs to distinguish direct subscriptions (lazy)
        /// from ordinary dynamic reads (snapshots) without re-parsing template
        /// directives or consulting legacy render-plan fixtures.
        reactive_subscription: Option<ReactiveSubscription>,
        /// Document-order site ID assigned when this node is emitted.
        ///
        /// WHAT: a per-store counter assigns this ID in construction order so
        /// expression overlays can address this splice site deterministically.
        /// WHY: stable site keys let later overlay phases map effective
        /// expressions to the correct splice without relying on traversal order.
        site_id: ExpressionSiteId,
    },

    /// Opaque child template reference.
    ChildTemplate {
        /// Store-qualified view identity for the referenced child template.
        ///
        /// WHAT: carries the root, phase, and overlay set needed to build a
        /// precise [`TirView`](super::view::TirView) when this child is folded.
        /// WHY: a bare `TemplateIrId` is not enough for cross-store folding or
        /// for cache keys that include phase and overlay context.
        reference: TemplateTirChildReference,

        /// Document-order occurrence ID assigned when this node is emitted.
        ///
        /// WHAT: a per-store counter assigns this ID in construction order so
        /// wrapper overlays can address this child-template boundary
        /// deterministically.
        /// WHY: stable occurrence keys let later overlay phases map wrapper
        /// contexts to the correct child-template without relying on traversal
        /// order.
        occurrence_id: ChildTemplateOccurrenceId,
    },

    /// Structural slot placeholder awaiting composition.
    Slot { placeholder: TirSlotPlaceholder },

    /// Content contributed by an `$insert("name")` directive.
    InsertContribution { template: TemplateIrId },

    /// Conditional branch chain (`if` / `else if` / `else`).
    BranchChain {
        /// Branches evaluated in source order until one selector matches.
        branches: Vec<TemplateIrBranch>,

        /// Optional trailing `else` body executed when no branch matches.
        fallback: Option<TemplateIrNodeId>,
    },

    /// Loop with a body node and optional aggregate wrapper.
    ///
    /// WHAT: the `aggregate_wrapper` subtree carries the text, child templates,
    /// and dynamic expressions that surround the loop aggregate output. The
    /// `AggregateOutput` marker node inside that subtree is replaced at fold
    /// time with the already-folded aggregate string.
    /// WHY: representing wrapper contents directly in TIR removes the old
    /// AST aggregate-plan detour. Keeping the wrapper as a normal subtree lets
    /// folding reuse the existing node walker instead of a parallel render-piece
    /// fold path.
    Loop {
        header: TemplateLoopHeader,

        /// Document-order expression-site IDs for the loop header's expressions.
        ///
        /// WHAT: mirrors the `TemplateLoopHeader` shape with one
        /// `ExpressionSiteId` per expression-bearing position (condition,
        /// range start/end/optional step, collection iterable).
        /// WHY: stable site keys let later overlay phases address each
        /// loop-header expression deterministically without relying on
        /// traversal order. The IDs share the same document-order counter
        /// as `DynamicExpression` and branch-selector site IDs.
        header_sites: TemplateLoopHeaderExpressionSites,

        /// Body node executed once for each loop iteration.
        body: TemplateIrNodeId,

        /// Optional wrapper subtree surrounding the loop aggregate output.
        ///
        /// WHAT: see variant-level documentation above.
        /// WHY: keeping the wrapper reference inline lets the fold path replace
        /// the nested `AggregateOutput` marker without a separate render plan.
        aggregate_wrapper: Option<TemplateIrNodeId>,
    },

    /// Marker for the position of the loop aggregate output inside an
    /// `aggregate_wrapper` subtree.
    ///
    /// WHAT: a leaf that the TIR fold path replaces with the per-loop aggregate
    /// string after the body has been folded.
    /// WHY: this replaces the AST aggregate-plan placeholder and makes the
    /// aggregate wrapper a first-class TIR subtree.
    AggregateOutput,

    /// Loop control signal (`break` / `continue`).
    LoopControl { kind: TemplateLoopControlKind },

    /// Runtime slot site placeholder resolved by AST planning.
    RuntimeSlotSite {
        /// Runtime slot plan that owns this site.
        ///
        /// WHAT: anchors the site ID to the AST-prepared plan so later phases
        /// can resolve which slots are available and how they map to arguments.
        /// WHY: runtime slot routing is still AST-owned; TIR carries only a
        /// stable plan handle until the handoff to HIR/runtime metadata.
        plan: TemplateSlotPlanId,

        /// Site identity within the runtime slot plan.
        site: RuntimeSlotSiteId,
    },
}

// -------------------------
//  TIR Slot Placeholder
// -------------------------

/// Final TIR-owned payload for an unresolved slot occurrence.
///
/// WHAT: records the slot key, stable occurrence ID, source location, and the
/// TIR-owned wrapper-set IDs needed by the remaining runtime slot planner.
/// WHY: the legacy AST `SlotPlaceholder` stores recursive `Template` values.
/// TIR must not own those templates directly; wrapper sets store same-store
/// `TemplateIrId`s instead, preserving current behavior until wrapper context
/// moves fully into overlays.
#[derive(Clone, Debug)]
pub(crate) struct TirSlotPlaceholder {
    /// Slot key (name and source location from the original placeholder).
    pub(crate) key: SlotKey,

    /// Stable occurrence ID for this slot placeholder.
    pub(crate) occurrence_id: SlotOccurrenceId,

    /// Source location for diagnostics.
    pub(crate) location: SourceLocation,

    /// Wrappers already applied around this slot's fallback content.
    ///
    /// WHAT: references a `TemplateWrapperSet` that was already resolved and
    /// applied to the placeholder's fallback children in the AST path.
    /// WHY: TIR does not own recursive wrapper templates; it stores the
    /// same-store wrapper-set ID so folding can replay the applied wrappers.
    pub(crate) applied_child_wrapper_set: Option<TemplateWrapperSetId>,

    /// Wrappers to apply around this slot's resolved children.
    ///
    /// WHAT: references a `TemplateWrapperSet` that must wrap any children
    /// contributed into this slot during composition.
    /// WHY: this captures the wrapper context from the AST placeholder so
    /// composition can apply the correct wrappers without re-parsing the
    /// original template expression.
    pub(crate) child_wrapper_set: Option<TemplateWrapperSetId>,

    /// Whether parent-provided child wrappers should be skipped for this slot.
    ///
    /// WHAT: when true, the slot explicitly overrides the inherited wrapper
    /// context and only uses its own `child_wrapper_set`.
    /// WHY: some slot directives suppress outer wrappers to avoid double-wrapping
    /// or to force raw child insertion.
    pub(crate) skip_parent_child_wrappers: bool,
}

impl TirSlotPlaceholder {
    /// Creates a slot placeholder with no wrapper context.
    #[cfg(test)]
    pub(crate) fn new(
        key: SlotKey,
        occurrence_id: SlotOccurrenceId,
        location: SourceLocation,
    ) -> Self {
        Self {
            key,
            occurrence_id,
            location,
            applied_child_wrapper_set: None,
            child_wrapper_set: None,
            skip_parent_child_wrappers: false,
        }
    }

    /// Creates a slot placeholder with TIR-owned wrapper-set context.
    pub(crate) fn with_wrapper_sets(
        key: SlotKey,
        occurrence_id: SlotOccurrenceId,
        location: SourceLocation,
        applied_child_wrapper_set: Option<TemplateWrapperSetId>,
        child_wrapper_set: Option<TemplateWrapperSetId>,
        skip_parent_child_wrappers: bool,
    ) -> Self {
        Self {
            key,
            occurrence_id,
            location,
            applied_child_wrapper_set,
            child_wrapper_set,
            skip_parent_child_wrappers,
        }
    }

    /// Remaps interned names stored by the slot key and source location.
    #[cfg(test)]
    pub(crate) fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.key.remap_string_ids(remap);
        self.location.remap_string_ids(remap);
    }
}

// -------------------------
//  Loop Header Expression Sites
// -------------------------

/// TIR-local expression-site IDs for the expressions inside a `TemplateLoopHeader`.
///
/// WHAT: mirrors the `TemplateLoopHeader` enum shape, carrying one
/// `ExpressionSiteId` per expression-bearing position in the header.
/// WHY: overlay phases need stable keys for each loop-header expression site
/// (condition, range bounds, collection iterable) so they can address effective
/// expressions deterministically without relying on traversal order.
/// The IDs are allocated from the same document-order counter as
/// `DynamicExpression` and branch-selector site IDs.
///
/// ## Ownership contract
///
/// This type is TIR-local. It is not pushed into the AST-owned
/// `TemplateLoopHeader`, HIR handoff shapes, or backend data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TemplateLoopHeaderExpressionSites {
    /// `while (condition)` loop: one expression site for the condition.
    Conditional { condition: ExpressionSiteId },

    /// `for (item in start..end[ step])` loop: sites for start, end, and the
    /// optional step expression.
    Range {
        start: ExpressionSiteId,
        end: ExpressionSiteId,
        step: Option<ExpressionSiteId>,
    },

    /// `for (item in iterable)` loop: one expression site for the iterable.
    Collection { iterable: ExpressionSiteId },
}

// -------------------------
//  TIR Branch
// -------------------------

/// One conditional branch within a `BranchChain` node.
///
/// WHAT: pairs a branch selector with the body node that executes when
/// the selector condition is satisfied.
/// WHY: keeping branches as (selector, body) pairs matches the AST shape and
/// avoids encoding branch logic into the node kind itself. Storing the full
/// `TemplateBranchSelector` (rather than just the extracted expression) lets
/// the TIR fold path handle bool conditions and option-capture scrutinees
/// uniformly.
#[derive(Clone, Debug)]
pub(crate) struct TemplateIrBranch {
    /// Selector that determines when this branch is active.
    ///
    /// Stores the full `TemplateBranchSelector` from the AST so the fold path
    /// can distinguish bool conditions from option-capture scrutinees.
    pub(crate) selector: TemplateBranchSelector,

    /// Body node to execute when the selector condition is satisfied.
    pub(crate) body: TemplateIrNodeId,

    /// Source location for diagnostics.
    pub(crate) location: SourceLocation,

    /// Document-order expression-site ID for the branch selector expression.
    ///
    /// WHAT: a per-store counter assigns this ID so expression overlays can
    /// address the branch selector's effective expression deterministically.
    /// WHY: stable site keys let later overlay phases map effective branch
    /// selectors without relying on traversal order or node-vector positions.
    /// The ID shares the same document-order counter as `DynamicExpression`
    /// site IDs and loop-header expression sites.
    pub(crate) selector_site_id: ExpressionSiteId,
}

impl TemplateIrBranch {
    /// Creates one branch inside a `BranchChain` node.
    ///
    /// The `selector_site_id` is initialized to a placeholder and overwritten
    /// by the TIR builder when the branch is pushed, or set explicitly via
    /// `with_selector_site_id` by direct-push construction paths.
    pub(crate) fn new(
        selector: TemplateBranchSelector,
        body: TemplateIrNodeId,
        location: SourceLocation,
    ) -> Self {
        Self {
            selector,
            body,
            location,
            selector_site_id: ExpressionSiteId::new(0),
        }
    }

    /// Sets the branch selector's expression-site ID, returning the updated branch.
    ///
    /// WHAT: used by direct-push construction paths (current-state materialization,
    /// slot expansion) that allocate or preserve a site ID outside the builder.
    /// WHY: keeps site-ID assignment in one clear place per construction path
    /// without changing the `new` signature used by the parser-facing builder.
    pub(crate) fn with_selector_site_id(mut self, site_id: ExpressionSiteId) -> Self {
        self.selector_site_id = site_id;
        self
    }

    /// Returns the condition expression for this branch.
    ///
    /// WHAT: for `Bool` selectors, returns the bool expression directly.
    /// For `OptionPresentCapture` selectors, returns the scrutinee expression.
    /// WHY: validation and fold passes that only need the condition expression
    /// can call this helper without repeating the selector match at every site.
    pub(crate) fn condition_expression(&self) -> &Expression {
        match &self.selector {
            TemplateBranchSelector::Bool(expression) => expression,
            TemplateBranchSelector::OptionPresentCapture { scrutinee, .. } => scrutinee,
        }
    }
}

// -------------------------
//  String-table remapping
// -------------------------

impl TemplateIr {
    /// Remap interned string identities stored on this template entry.
    ///
    /// WHAT: remaps the source location, template kind (for slot keys), and any
    /// owned wrapper child templates carried by the style.
    /// WHY: per-file string-table merges require every interned path/name in the
    /// TIR store to be rewritten to the merged table's IDs.
    #[cfg(test)]
    pub(crate) fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.location.remap_string_ids(remap);
        self.kind.remap_string_ids(remap);
    }
}

impl TemplateIrNode {
    /// Remap interned string identities stored on this node.
    #[cfg(test)]
    pub(crate) fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.location.remap_string_ids(remap);
        self.kind.remap_string_ids(remap);
    }
}

impl TemplateIrNodeKind {
    /// Remap interned string identities inside the node payload.
    ///
    /// NOTE: store-local IDs such as `TemplateIrId` and `TemplateIrNodeId` are
    /// indexes, not string identities, and must not be remapped. The store-level
    /// walk visits every template and node directly, so child/parent references do
    /// not need recursive traversal here.
    #[cfg(test)]
    pub(crate) fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            TemplateIrNodeKind::Sequence { .. } => {}

            TemplateIrNodeKind::Text { text, .. } => {
                *text = remap.get(*text);
            }

            TemplateIrNodeKind::DynamicExpression {
                expression,
                reactive_subscription,
                ..
            } => {
                expression.remap_string_ids(remap);
                if let Some(subscription) = reactive_subscription {
                    subscription.remap_string_ids(remap);
                }
            }

            TemplateIrNodeKind::ChildTemplate { .. } => {}
            TemplateIrNodeKind::InsertContribution { .. } => {}

            TemplateIrNodeKind::Slot { placeholder } => {
                placeholder.remap_string_ids(remap);
            }

            TemplateIrNodeKind::BranchChain { branches, .. } => {
                for branch in branches {
                    branch.remap_string_ids(remap);
                }
            }

            TemplateIrNodeKind::Loop { header, .. } => {
                header.remap_string_ids(remap);
            }

            TemplateIrNodeKind::AggregateOutput => {}
            TemplateIrNodeKind::LoopControl { .. } => {}
            TemplateIrNodeKind::RuntimeSlotSite { .. } => {}
        }
    }
}

impl TemplateIrBranch {
    /// Remap interned string identities stored on this branch.
    #[cfg(test)]
    pub(crate) fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.selector.remap_string_ids(remap);
        self.location.remap_string_ids(remap);
    }
}
