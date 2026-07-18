//! Central TIR storage.
//!
//! WHAT: `TemplateIrStore` owns every TIR template, node, wrapper set, and side-table
//! entry in contiguous vectors. Consumers obtain cheap `Copy` IDs from the store
//! and look up data by index.
//!
//! WHY: a single store with typed IDs avoids scattered `Box<TemplateIr>` allocations,
//! makes tree ownership trivial, and keeps the TIR data cache-friendly for later
//! folding and formatting passes.
//!
//! ## Ownership contract
//!
//! The store is AST-local. It is not shared with HIR, backends, or the public API.
//! Each module AST construction may create its own store; the store is dropped when
//! the AST stage finishes template processing for that module.

use crate::compiler_frontend::arena::capacity::FrontendArenaCapacityEstimate;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::{
    ReactiveSubscription, SlotPlaceholder, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::TemplateLoopHeader;
use crate::compiler_frontend::ast::templates::tir::control_flow_roots::ControlFlowBodyKind;
use crate::compiler_frontend::ast::templates::tir::ids::{
    ChildTemplateOccurrenceId, ExpressionSiteId, SlotOccurrenceId, TemplateIrId, TemplateIrNodeId,
    TemplateSlotPlanId, TemplateWrapperSetId,
};
use crate::compiler_frontend::ast::templates::tir::node::{
    TemplateIr, TemplateIrNode, TemplateIrNodeKind, TemplateLoopHeaderExpressionSites,
    TirSlotPlaceholder,
};
use crate::compiler_frontend::ast::templates::tir::overlays::{
    TemplateViewContext, TirExpressionOverlay, TirExpressionOverlayId, TirSlotResolutionOverlay,
    TirSlotResolutionOverlayId, TirWrapperContextOverlay, TirWrapperContextOverlayId,
};
use crate::compiler_frontend::ast::templates::tir::refs::TemplateWrapperReference;
use crate::compiler_frontend::ast::templates::tir::slot_plan::TemplateSlotPlan;
use crate::compiler_frontend::ast::templates::tir::wrapper_sets::wrapper_sets_are_equivalent;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::instrumentation::{AstCounter, increment_ast_counter};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use std::collections::HashSet;

// -------------------------
//  Side-table types
// -------------------------

// These tables keep bulky or rarely read template metadata outside the main
// `TemplateIr` and `TemplateIrNode` records. Some entries are still reserved
// for fields the remaining compatibility surface will drop as the
// replacement lands; slot plans already carry TIR-owned runtime slot
// application route data.

/// A reusable set of `$children(..)` wrapper template refs.
///
/// WHAT: groups wrapper templates as effective wrapper references behind a
/// typed side-table ID.
/// WHY: many sibling templates inherit wrappers from their parent; storing them
/// as effective refs outside `TemplateIr` keeps the core template record small,
/// avoids recursive `Template` ownership, makes store-local ownership explicit,
/// and gives later phases a clear place to deduplicate identical wrapper
/// combinations.
///
/// Design constraint: wrapper sets store effective wrapper references (root,
/// phase, and value-carried context). A wrapper's effective identity is not only its
/// structural root — it also has a phase and overlay context.
#[derive(Clone, Debug)]
pub(crate) struct TemplateWrapperSet {
    /// Effective wrapper template refs that must be applied around a child
    /// template's output during folding, ordered from innermost to outermost as
    /// they were stored on the AST `Template`.
    ///
    /// WHY: storing the effective view identity keeps wrapper reuse precise
    /// without duplicating template content.
    pub(crate) wrappers: Vec<TemplateWrapperReference>,
}

/// Opaque anchor for a formatter-produced region.
///
/// WHAT: records boundaries where a style formatter (e.g., `$md`) produced
/// output so fold and HIR can treat the region as opaque text.
/// WHY: separating formatter anchors from body text nodes keeps the formatter
/// boundary explicit without string-level guard characters.
#[derive(Clone, Debug)]
pub(crate) struct TemplateFormatterAnchor {
    /// Reserved anchor fields; populated as the remaining formatter surfaces move onto TIR.
    pub(crate) _reserved: (),
}

// -------------------------
//  Template IR Store
// -------------------------

/// Central owned storage for all TIR data within one module's template subsystem.
///
/// WHAT: contiguous vectors of templates, nodes, wrapper sets, and side-table entries
/// indexed by typed IDs.
///
/// WHY:
/// - Contiguous storage avoids per-template heap allocation and improves locality.
/// - Typed IDs prevent accidental cross-collection index misuse.
/// - The store is module-scoped so it can be dropped after AST template processing.
///
/// ## Invariants
///
/// - Every `TemplateIrId` indexes a valid entry in `templates`.
/// - Every `TemplateIrNodeId` indexes a valid entry in `nodes`.
/// - Every `TemplateWrapperSetId` indexes a valid entry in `wrapper_sets`.
/// - `TemplateIr::root` always points to a valid node in `nodes`.
///
/// Construction APIs preserve these invariants. Focused malformed-store tests
/// exercise them through `tests/validation_support.rs`.
#[derive(Debug)]
pub(crate) struct TemplateIrStore {
    /// Document-order counter for `Slot` node occurrences.
    ///
    /// WHAT: advances by one each time a `Slot` node is emitted into this store.
    /// WHY: gives overlay and slot-resolution phases a stable per-occurrence key
    /// that does not depend on traversal order or node-vector positions.
    next_slot_occurrence: u32,

    /// Document-order counter for `ChildTemplate` node occurrences.
    ///
    /// WHAT: advances by one each time a `ChildTemplate` node is emitted into
    /// this store.
    /// WHY: gives wrapper-overlay phases a stable per-occurrence key for each
    /// child-template boundary.
    next_child_template_occurrence: u32,

    /// Document-order counter for `DynamicExpression` node sites.
    ///
    /// WHAT: advances by one each time a `DynamicExpression` node is emitted
    /// into this store.
    /// WHY: gives expression-overlay phases a stable per-site key for each
    /// dynamic-expression splice. Branch-selector and loop-header expression
    /// sites will receive their own counters in a later Phase 3 slice.
    next_expression_site: u32,

    /// Top-level templates.
    pub(crate) templates: Vec<TemplateIr>,

    /// Nodes forming the template body trees.
    pub(crate) nodes: Vec<TemplateIrNode>,

    /// Wrapper sets for `$children(..)` wrappers.
    pub(crate) wrapper_sets: Vec<TemplateWrapperSet>,

    /// Slot routing plans.
    pub(crate) slot_plans: Vec<TemplateSlotPlan>,

    /// Overlay payloads for effective template views.
    pub(crate) expression_overlays: Vec<TirExpressionOverlay>,
    pub(crate) slot_resolution_overlays: Vec<TirSlotResolutionOverlay>,
    pub(crate) wrapper_context_overlays: Vec<TirWrapperContextOverlay>,

    /// Formatter opaque anchors.
    #[allow(
        dead_code,
        reason = "formatter anchors are reserved for the remaining formatter surfaces moving onto TIR"
    )]
    pub(crate) formatter_anchors: Vec<TemplateFormatterAnchor>,

    /// Reactive `$(source)` subscription metadata attached to individual TIR
    /// nodes. Indexed by `TemplateIrNodeId`; `None` means the node carries no
    /// reactive dependency.
    ///
    /// WHAT: stores subscriptions for node kinds that cannot carry the metadata
    ///       in their payload, currently `Text` nodes.
    /// WHY: reactive literal text must survive TIR formatting and current-state
    ///      materialization without broadening the `TemplateIrNodeKind` enum shape.
    pub(crate) node_reactive_subscriptions: Vec<Option<ReactiveSubscription>>,
}

impl TemplateIrStore {
    /// Creates an empty store with no pre-allocated capacity.
    pub(crate) fn new() -> Self {
        Self {
            next_slot_occurrence: 0,
            next_child_template_occurrence: 0,
            next_expression_site: 0,
            templates: Vec::new(),
            nodes: Vec::new(),
            wrapper_sets: Vec::new(),
            slot_plans: Vec::new(),
            expression_overlays: Vec::new(),
            slot_resolution_overlays: Vec::new(),
            wrapper_context_overlays: Vec::new(),
            formatter_anchors: Vec::new(),
            node_reactive_subscriptions: Vec::new(),
        }
    }

    /// Creates a store pre-sized from a module-level capacity estimate.
    ///
    /// WHAT: seeds `templates`, `nodes`, and side vectors with conservative capacities
    /// derived from `FrontendArenaCapacityEstimate` fields.
    /// WHY: avoids immediate reallocations when converting a module's templates into TIR;
    ///      the estimate is policy-only and does not affect correctness.
    pub(crate) fn with_capacity_estimate(estimate: FrontendArenaCapacityEstimate) -> Self {
        // Templates are typically fewer than nodes; use the estimate directly.
        let template_capacity = estimate.templates;

        // Nodes scale roughly with template atoms — each atom becomes at least one node.
        // Use `template_atoms` as a conservative base.
        let node_capacity = estimate.template_atoms;

        // Wrapper sets and slot plans are typically small; cap them at the template count.
        let side_capacity = template_capacity;

        Self {
            next_slot_occurrence: 0,
            next_child_template_occurrence: 0,
            next_expression_site: 0,
            templates: Vec::with_capacity(template_capacity),
            nodes: Vec::with_capacity(node_capacity),
            wrapper_sets: Vec::with_capacity(side_capacity),
            slot_plans: Vec::with_capacity(side_capacity),
            expression_overlays: Vec::with_capacity(side_capacity),
            slot_resolution_overlays: Vec::with_capacity(side_capacity),
            wrapper_context_overlays: Vec::with_capacity(side_capacity),
            formatter_anchors: Vec::with_capacity(side_capacity),
            node_reactive_subscriptions: Vec::with_capacity(node_capacity),
        }
    }

    /// Assigns and returns the next `SlotOccurrenceId` in document order.
    ///
    /// WHAT: returns the current counter value then advances it by one.
    /// WHY: both the builder and the materialization path call this when
    /// emitting a `Slot` node so every construction path shares one counter.
    pub(crate) fn next_slot_occurrence_id(&mut self) -> SlotOccurrenceId {
        let id = SlotOccurrenceId::new(self.next_slot_occurrence as usize);
        self.next_slot_occurrence = self
            .next_slot_occurrence
            .checked_add(1)
            .expect("slot occurrence counter overflow; this is a compiler bug");
        id
    }

    /// Assigns and returns the next `ChildTemplateOccurrenceId` in document order.
    ///
    /// WHAT: returns the current counter value then advances it by one.
    /// WHY: both the builder and the materialization path call this when
    /// emitting a `ChildTemplate` node so every construction path shares one
    /// counter.
    pub(crate) fn next_child_template_occurrence_id(&mut self) -> ChildTemplateOccurrenceId {
        let id = ChildTemplateOccurrenceId::new(self.next_child_template_occurrence as usize);
        self.next_child_template_occurrence = self
            .next_child_template_occurrence
            .checked_add(1)
            .expect("child-template occurrence counter overflow; this is a compiler bug");
        id
    }

    /// Assigns and returns the next `ExpressionSiteId` in document order.
    ///
    /// WHAT: returns the current counter value then advances it by one.
    /// WHY: both the builder and the materialization path call this when
    /// emitting a `DynamicExpression` node so every construction path shares
    /// one counter.
    pub(crate) fn next_expression_site_id(&mut self) -> ExpressionSiteId {
        let id = ExpressionSiteId::new(self.next_expression_site as usize);
        self.next_expression_site = self
            .next_expression_site
            .checked_add(1)
            .expect("expression site counter overflow; this is a compiler bug");
        id
    }

    /// Allocates expression-site IDs for every expression-bearing position in a
    /// loop header, drawing from the same document-order counter as
    /// `DynamicExpression` and branch-selector sites.
    ///
    /// WHAT: mirrors the `TemplateLoopHeader` shape, assigning one
    /// `ExpressionSiteId` per expression (condition, range start/end/optional
    /// step, collection iterable) in a deterministic order.
    /// WHY: keeps loop-header site allocation in one place so the builder and
    /// direct-push materialization paths cannot diverge. The range variant
    /// allocates start, then end, then the optional step, matching source order.
    pub(crate) fn allocate_loop_header_expression_sites(
        &mut self,
        header: &TemplateLoopHeader,
    ) -> TemplateLoopHeaderExpressionSites {
        match header {
            TemplateLoopHeader::Conditional { .. } => {
                TemplateLoopHeaderExpressionSites::Conditional {
                    condition: self.next_expression_site_id(),
                }
            }
            TemplateLoopHeader::Range { range, .. } => TemplateLoopHeaderExpressionSites::Range {
                start: self.next_expression_site_id(),
                end: self.next_expression_site_id(),
                step: range.step.as_ref().map(|_| self.next_expression_site_id()),
            },
            TemplateLoopHeader::Collection { .. } => {
                TemplateLoopHeaderExpressionSites::Collection {
                    iterable: self.next_expression_site_id(),
                }
            }
        }
    }

    /// Returns the number of templates currently stored.
    pub(crate) fn template_count(&self) -> usize {
        self.templates.len()
    }

    /// Returns the number of nodes currently stored.
    pub(crate) fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Pushes a template into the store and returns its ID.
    pub(crate) fn push_template(&mut self, template: TemplateIr) -> TemplateIrId {
        let id = TemplateIrId::new(self.templates.len());
        self.templates.push(template);
        id
    }

    /// Replaces the top-level kind for an already-materialized template.
    ///
    /// WHAT: keeps the TIR record's classification aligned with the AST
    /// template after finalization refreshes the kind from current body shape.
    /// WHY: HIR handoff construction copies the kind from TIR, so a caller that
    /// materializes before refreshing `Template::kind` must update the store
    /// entry before building the owned handoff.
    pub(crate) fn set_template_kind(&mut self, id: TemplateIrId, kind: TemplateType) -> bool {
        let Some(template) = self.templates.get_mut(id.index()) else {
            return false;
        };
        template.kind = kind;
        true
    }

    /// Pushes a node into the store and returns its ID.
    pub(crate) fn push_node(&mut self, node: TemplateIrNode) -> TemplateIrNodeId {
        let id = TemplateIrNodeId::new(self.nodes.len());
        self.nodes.push(node);
        self.node_reactive_subscriptions.push(None);
        id
    }

    /// Returns the reactive subscription attached to a node, if any.
    pub(crate) fn node_reactive_subscription(
        &self,
        node_id: TemplateIrNodeId,
    ) -> Option<&ReactiveSubscription> {
        self.node_reactive_subscriptions
            .get(node_id.index())?
            .as_ref()
    }

    /// Attaches a reactive subscription to an existing node.
    pub(crate) fn set_node_reactive_subscription(
        &mut self,
        node_id: TemplateIrNodeId,
        subscription: ReactiveSubscription,
    ) {
        if let Some(entry) = self.node_reactive_subscriptions.get_mut(node_id.index()) {
            *entry = Some(subscription);
        }
    }

    /// Pushes a wrapper set into the store and returns its ID.
    pub(crate) fn push_wrapper_set(
        &mut self,
        wrapper_set: TemplateWrapperSet,
    ) -> TemplateWrapperSetId {
        let id = TemplateWrapperSetId::new(self.wrapper_sets.len());
        self.wrapper_sets.push(wrapper_set);
        id
    }

    /// Pushes a wrapper set or reuses an existing equivalent set.
    ///
    /// WHAT: searches the existing wrapper-set side table for a set that is
    /// equivalent to `wrappers`. Callers pass pre-qualified
    /// `TemplateWrapperReference` values (root + phase + context). Empty
    /// wrapper vectors always reuse. Non-empty sets reuse only when every
    /// `TemplateWrapperReference` matches an existing set in the same order. If a
    /// match is found, its ID is returned and `TirWrapperSetReuseHits` is
    /// incremented; otherwise a new entry is pushed and
    /// `TirWrapperSetsCreated` is incremented.
    ///
    /// WHY: many sibling templates inherit identical `$children(..)` wrapper
    /// combinations; sharing one side-table entry reduces allocation churn and
    /// keeps `TemplateIrSummary::wrapper_count` accurate as a per-template
    /// wrapper count. `TemplateWrapperReference` identity (all three fields) is
    /// the reuse authority; `Template` values and content comparison are
    /// no longer inspected.
    pub(crate) fn push_or_reuse_wrapper_set(
        &mut self,
        wrappers: Vec<TemplateWrapperReference>,
    ) -> TemplateWrapperSetId {
        for (index, existing) in self.wrapper_sets.iter().enumerate() {
            if wrapper_sets_are_equivalent(&existing.wrappers, &wrappers) {
                increment_ast_counter(AstCounter::TirWrapperSetReuseHits);
                return TemplateWrapperSetId::new(index);
            }
        }

        increment_ast_counter(AstCounter::TirWrapperSetsCreated);
        self.push_wrapper_set(TemplateWrapperSet { wrappers })
    }

    /// Stores or reuses a non-empty ordered wrapper-reference set.
    fn push_or_reuse_optional_wrapper_set(
        &mut self,
        wrappers: &[TemplateWrapperReference],
    ) -> Option<TemplateWrapperSetId> {
        if wrappers.is_empty() {
            return None;
        }

        Some(self.push_or_reuse_wrapper_set(wrappers.to_vec()))
    }

    /// Builds the final TIR payload for one parser slot placeholder.
    ///
    /// WHAT: copies only the slot key and skip flag from `SlotPlaceholder`,
    /// converts both wrapper vectors to wrapper-set IDs, and assigns a fresh
    /// `SlotOccurrenceId`.
    /// WHY: this is the parser/store boundary where recursive wrapper templates
    /// become store-owned wrapper-set IDs before the slot enters TIR.
    pub(crate) fn tir_slot_placeholder_from_ast(
        &mut self,
        placeholder: &SlotPlaceholder,
        location: SourceLocation,
    ) -> Result<TirSlotPlaceholder, TemplateError> {
        let occurrence_id = self.next_slot_occurrence_id();
        let applied_child_wrapper_set =
            self.push_or_reuse_optional_wrapper_set(&placeholder.applied_child_wrappers);
        let child_wrapper_set =
            self.push_or_reuse_optional_wrapper_set(&placeholder.child_wrappers);

        Ok(TirSlotPlaceholder::with_wrapper_sets(
            placeholder.key.to_owned(),
            occurrence_id,
            location,
            applied_child_wrapper_set,
            child_wrapper_set,
            placeholder.skip_parent_child_wrappers,
        ))
    }

    /// Pushes a slot plan into the store and returns its ID.
    pub(crate) fn push_slot_plan(&mut self, slot_plan: TemplateSlotPlan) -> TemplateSlotPlanId {
        let id = TemplateSlotPlanId::new(self.slot_plans.len());
        self.slot_plans.push(slot_plan);
        id
    }

    /// Returns the first control-flow node under a finalized template root.
    ///
    /// WHAT: parser-emitted control-flow templates store their `BranchChain` or
    ///       `Loop` node inside the root sequence. This lookup finds that node
    ///       without mutating the store.
    /// WHY: render-unit preparation resolves the exact owning node before it
    ///      builds replacement body roots, so a missing owner fails as an
    ///      internal invariant instead of leaving ambiguous state behind.
    pub(crate) fn control_flow_node_id_for_template(
        &self,
        owning_template_id: TemplateIrId,
    ) -> Option<TemplateIrNodeId> {
        let template = self.templates.get(owning_template_id.index())?;
        self.control_flow_node_id_in_subtree(template.root)
    }

    /// Recursively searches a TIR subtree for the template-owned control-flow node.
    ///
    /// WHAT: walks wrapper-shaped `Sequence` nodes until it finds a `BranchChain`
    ///       or `Loop` node. A single-child `ChildTemplate` forwarding root is
    ///       followed only after the current sequence has no local control-flow
    ///       node; arbitrary child-template references are not traversed.
    /// WHY: parser-time builder state and finalized references both need the
    ///      same ownership-preserving lookup after wrapper/head-chain composition
    ///      nests the owner control-flow node below the root sequence.
    pub(crate) fn control_flow_node_id_in_subtree(
        &self,
        root: TemplateIrNodeId,
    ) -> Option<TemplateIrNodeId> {
        let mut visited = HashSet::new();
        self.find_control_flow_node_in_subtree(root, &mut visited)
    }

    fn find_control_flow_node_in_subtree(
        &self,
        node_id: TemplateIrNodeId,
        visited: &mut HashSet<TemplateIrNodeId>,
    ) -> Option<TemplateIrNodeId> {
        if !visited.insert(node_id) {
            return None;
        }

        let node = self.nodes.get(node_id.index())?;

        match &node.kind {
            TemplateIrNodeKind::BranchChain { .. } | TemplateIrNodeKind::Loop { .. } => {
                Some(node_id)
            }

            TemplateIrNodeKind::Sequence { children } => {
                if let Some(control_flow_node) = children
                    .iter()
                    .copied()
                    .find_map(|child| self.find_control_flow_node_in_subtree(child, visited))
                {
                    return Some(control_flow_node);
                }

                // Runtime slot/head-chain composition can produce a forwarding
                // template whose root sequence contains only one child-template
                // reference. In that narrow shape, the referenced child is the
                // owner's control-flow tree, not arbitrary nested content.
                let [only_child] = children.as_slice() else {
                    return None;
                };

                let child_node = self.nodes.get(only_child.index())?;
                let TemplateIrNodeKind::ChildTemplate { reference, .. } = &child_node.kind else {
                    return None;
                };

                let template_id = reference.root;
                let template_ir = self.templates.get(template_id.index())?;
                self.find_control_flow_node_in_subtree(template_ir.root, visited)
            }

            _ => None,
        }
    }

    /// Replaces one body node ID inside a specific control-flow parser-TIR node.
    ///
    /// WHAT: the caller supplies the control-flow node ID from the in-progress
    ///       parser builder state so body preparation can run before the
    ///       builder state is finished into a `TemplateIrId`.
    /// WHY: render-unit preparation runs before parser construction finishes,
    ///      so the finalized owning-template ID does not exist yet.
    pub(crate) fn replace_control_flow_body_node_by_id(
        &mut self,
        control_flow_node_id: TemplateIrNodeId,
        body_kind: ControlFlowBodyKind,
        new_body_root: TemplateIrNodeId,
    ) -> bool {
        let Some(control_flow_node) = self.nodes.get_mut(control_flow_node_id.index()) else {
            return false;
        };

        match (&mut control_flow_node.kind, body_kind) {
            (
                TemplateIrNodeKind::BranchChain { branches, .. },
                ControlFlowBodyKind::Branch { index },
            ) => {
                if let Some(branch) = branches.get_mut(index) {
                    branch.body = new_body_root;
                    return true;
                }
                false
            }

            (TemplateIrNodeKind::BranchChain { fallback, .. }, ControlFlowBodyKind::Fallback) => {
                if let Some(fallback_body) = fallback.as_mut() {
                    *fallback_body = new_body_root;
                    return true;
                }
                false
            }

            (TemplateIrNodeKind::Loop { body, .. }, ControlFlowBodyKind::LoopBody) => {
                *body = new_body_root;
                true
            }

            _ => false,
        }
    }

    /// Replaces the `aggregate_wrapper` field of a `Loop` control-flow node.
    ///
    /// WHAT: render-unit preparation builds the loop aggregate-wrapper TIR
    ///       subtree after the parser has already emitted the `Loop` node with
    ///       `aggregate_wrapper: None`. This helper installs the composed
    ///       wrapper root in place.
    /// WHY: keeps loop aggregate-wrapper mutation in the same store-owned
    ///      helper family as body-root replacement.
    pub(crate) fn replace_loop_aggregate_wrapper_node_by_id(
        &mut self,
        control_flow_node_id: TemplateIrNodeId,
        new_aggregate_wrapper_root: TemplateIrNodeId,
    ) -> bool {
        let Some(control_flow_node) = self.nodes.get_mut(control_flow_node_id.index()) else {
            return false;
        };

        match &mut control_flow_node.kind {
            TemplateIrNodeKind::Loop {
                aggregate_wrapper, ..
            } => {
                *aggregate_wrapper = Some(new_aggregate_wrapper_root);
                true
            }
            _ => false,
        }
    }

    /// Returns a reference to the template at the given ID, or `None` if out of bounds.
    pub(crate) fn get_template(&self, id: TemplateIrId) -> Option<&TemplateIr> {
        self.templates.get(id.index())
    }

    /// Returns a reference to the node at the given ID, or `None` if out of bounds.
    pub(crate) fn get_node(&self, id: TemplateIrNodeId) -> Option<&TemplateIrNode> {
        self.nodes.get(id.index())
    }

    /// Returns a reference to the wrapper set at the given ID, or `None` if out of bounds.
    pub(crate) fn get_wrapper_set(&self, id: TemplateWrapperSetId) -> Option<&TemplateWrapperSet> {
        self.wrapper_sets.get(id.index())
    }

    /// Returns a reference to the slot plan at the given ID, or `None` if out of bounds.
    pub(crate) fn get_slot_plan(&self, id: TemplateSlotPlanId) -> Option<&TemplateSlotPlan> {
        self.slot_plans.get(id.index())
    }

    // -------------------------
    //  Overlay storage
    // -------------------------

    /// Allocates an expression overlay payload.
    pub(crate) fn allocate_expression_overlay(
        &mut self,
        overlay: TirExpressionOverlay,
    ) -> TirExpressionOverlayId {
        let id = TirExpressionOverlayId::new(self.expression_overlays.len());
        self.expression_overlays.push(overlay);
        id
    }

    /// Allocates a slot-resolution overlay payload.
    pub(crate) fn allocate_slot_resolution_overlay(
        &mut self,
        overlay: TirSlotResolutionOverlay,
    ) -> TirSlotResolutionOverlayId {
        let id = TirSlotResolutionOverlayId::new(self.slot_resolution_overlays.len());
        self.slot_resolution_overlays.push(overlay);
        id
    }

    /// Allocates a wrapper-context overlay payload.
    pub(crate) fn allocate_wrapper_context_overlay(
        &mut self,
        overlay: TirWrapperContextOverlay,
    ) -> TirWrapperContextOverlayId {
        let id = TirWrapperContextOverlayId::new(self.wrapper_context_overlays.len());
        self.wrapper_context_overlays.push(overlay);
        id
    }

    /// Returns an expression overlay payload by ID.
    pub(crate) fn expression_overlay(
        &self,
        id: TirExpressionOverlayId,
    ) -> Option<&TirExpressionOverlay> {
        self.expression_overlays.get(id.index())
    }

    /// Returns a slot-resolution overlay payload by ID.
    pub(crate) fn slot_resolution_overlay(
        &self,
        id: TirSlotResolutionOverlayId,
    ) -> Option<&TirSlotResolutionOverlay> {
        self.slot_resolution_overlays.get(id.index())
    }

    /// Returns a wrapper-context overlay payload by ID.
    pub(crate) fn wrapper_context_overlay(
        &self,
        id: TirWrapperContextOverlayId,
    ) -> Option<&TirWrapperContextOverlay> {
        self.wrapper_context_overlays.get(id.index())
    }

    /// Resolves a site through value-carried contexts in outer-to-inner order.
    /// The first matching context wins, preserving the existing root-first
    /// expression-stack behavior.
    pub(crate) fn expression_for_context_stack(
        &self,
        contexts: &[TemplateViewContext],
        site_id: ExpressionSiteId,
    ) -> Result<Option<&Expression>, CompilerError> {
        for context in contexts.iter().copied() {
            let Some(overlay_id) = context.expression_overlay else {
                continue;
            };

            let overlay = self.expression_overlay(overlay_id).ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "TIR expression resolution referenced missing expression overlay {overlay_id}"
                ))
            })?;

            if let Some(expression) = overlay.expression_for_site(site_id) {
                return Ok(Some(expression));
            }
        }

        Ok(None)
    }
}

impl Default for TemplateIrStore {
    fn default() -> Self {
        Self::new()
    }
}
