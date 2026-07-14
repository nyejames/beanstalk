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
use crate::compiler_frontend::ast::templates::tir::refs::{
    TemplateRef, TemplateStoreId, TemplateStringDomainId, TemplateWrapperReference,
};
use crate::compiler_frontend::ast::templates::tir::slot_plan::TemplateSlotPlan;
use crate::compiler_frontend::ast::templates::tir::wrapper_sets::wrapper_sets_are_equivalent;
use crate::compiler_frontend::instrumentation::{AstCounter, increment_ast_counter};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use std::collections::HashSet;
use std::sync::Arc;

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
/// avoids recursive `Template` ownership, makes cross-store ownership explicit,
/// and gives later phases a clear place to deduplicate identical wrapper
/// combinations.
///
/// Design constraint: wrapper sets must store effective wrapper references
/// (root, phase, overlay-set ID), not bare `TemplateRef`. A wrapper's effective
/// identity is not only its structural root — it also has a phase and overlay-set
/// context. Storing all three fields prevents a subtle bug where a wrapper with
/// the same structural root but a different overlay context is treated as
/// equivalent. Content-based composition must NOT remain as a permanent
/// fallback for cross-store wrappers. Eager TIR copying is NOT the primary
/// model; copying is permitted only as copy-on-write materialization.
#[derive(Clone, Debug)]
pub(crate) struct TemplateWrapperSet {
    /// Effective wrapper template refs that must be applied around a child
    /// template's output during folding, ordered from innermost to outermost as
    /// they were stored on the AST `Template`.
    ///
    /// WHY: wrapper sets may be referenced cross-store through the registry;
    /// storing effective refs rather than bare store-local `TemplateIrId`s keeps
    /// that ownership explicit and lets validation reject out-of-bounds or
    /// wrong-store wrapper references without silently relying on the current
    /// store.
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
//  Template Store State
// -------------------------

/// Lifecycle state of a `TemplateIrStore` inside the module-local registry.
///
/// WHAT: distinguishes a store that is still accepting parser/builder mutations
/// from one that has been frozen into a module-local string domain.
/// WHY: cross-store references are only valid between frozen stores that share
/// the same string domain; a building store must not be referenced from outside.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TemplateStoreState {
    /// The store is under construction and may still receive parser/builder writes.
    Building,

    /// The store has been frozen and assigned to a module-local string domain.
    ///
    /// WHAT: all interned string identities in the store have been reconciled
    /// with the string domain identified by `string_domain`.
    /// WHY: cross-store references are valid only when both stores belong to the
    /// same domain, ensuring string IDs resolve consistently.
    #[allow(
        dead_code,
        reason = "cross-store freeze is test-only while TIR stays module-scoped; production match arms still cover it"
    )]
    FrozenModuleLocal {
        string_domain: TemplateStringDomainId,
    },
}

// -------------------------
//  Template IR Store
// -------------------------

/// Identity token for one logical TIR store origin.
///
/// WHAT: every directly constructed store receives a fresh token. Durable
///       template references and read-only snapshots cloned from that store
///       share the token, so `Arc::ptr_eq` proves common store origin.
/// WHY: `TemplateStoreId` is a registry-local index and can collide numerically
///      across registries. Direct-store consumers use this token when an active
///      borrow prevents resolving the reference back through the registry.
///
/// NOTE: `Arc` is used instead of `Rc` so that `Template` (which may carry a
///       finalized TIR reference) remains `Send + Sync` and existing
///       `Arc<Template>` usage keeps clippy's thread-safety lint happy.
#[derive(Debug)]
pub(crate) struct TemplateIrStoreOwner(());

impl TemplateIrStoreOwner {
    /// Creates a new unique owner token.
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self(()))
    }
}

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
/// These invariants are enforced by the converter and validated by
/// `validation::validate_tir_store`.
#[derive(Clone, Debug)]
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

    /// Logical origin token for this store.
    ///
    /// WHAT: references finalized from this store carry a clone of the token.
    /// WHY: consumers compare it before using a store-local `TemplateIrId`, since
    ///      equal registry-local store IDs do not imply a common store origin.
    owner: Arc<TemplateIrStoreOwner>,

    /// Registry-level ID for this store.
    ///
    /// WHAT: assigned by `TemplateIrRegistry::adopt_store` when the store is
    ///       registered. Defaults to index 0 so stores created directly outside
    ///       the registry (tests) still produce well-formed `TemplateRef`s.
    /// WHY: the store needs its own `TemplateStoreId` to qualify store-local
    ///      `TemplateIrId`s into store-qualified `TemplateRef`s when building
    ///      wrapper sets, without callers having to thread the ID through every
    ///      store-local API.
    store_id: TemplateStoreId,
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
            formatter_anchors: Vec::new(),
            node_reactive_subscriptions: Vec::new(),
            owner: TemplateIrStoreOwner::new(),
            store_id: TemplateStoreId::new(0),
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
            formatter_anchors: Vec::with_capacity(side_capacity),
            node_reactive_subscriptions: Vec::with_capacity(node_capacity),
            owner: TemplateIrStoreOwner::new(),
            store_id: TemplateStoreId::new(0),
        }
    }

    /// Returns the logical origin token for this store.
    pub(crate) fn owner(&self) -> Arc<TemplateIrStoreOwner> {
        Arc::clone(&self.owner)
    }

    /// Returns the registry-level store ID assigned to this store.
    ///
    /// WHAT: defaults to index 0 when the store was created outside the
    ///       registry; set to the real ID by `TemplateIrRegistry::adopt_store`.
    /// WHY: callers and `push_or_reuse_wrapper_set` need the store's own ID to
    ///      qualify store-local `TemplateIrId`s into `TemplateRef`s.
    pub(crate) fn store_id(&self) -> TemplateStoreId {
        self.store_id
    }

    /// Stamps the store with its registry-assigned `TemplateStoreId`.
    ///
    /// WHAT: called once by `TemplateIrRegistry::adopt_store` after the store is
    ///       registered. Existing self-qualified wrapper refs are rewritten from
    ///       the previous store ID to the assigned ID.
    /// WHY: the store cannot know its own index at construction time because
    ///      the registry assigns it; this method closes that gap so the store
    ///      can self-qualify template IDs into store-qualified refs. Tests may
    ///      also adopt a prebuilt store, so wrapper refs created before adoption
    ///      must not keep the direct-construction default ID.
    pub(crate) fn set_store_id(&mut self, store_id: TemplateStoreId) {
        let previous_store_id = self.store_id;
        self.store_id = store_id;

        if previous_store_id == store_id {
            return;
        }

        for wrapper_set in &mut self.wrapper_sets {
            for wrapper_ref in &mut wrapper_set.wrappers {
                if wrapper_ref.root.store_id == previous_store_id {
                    wrapper_ref.root.store_id = store_id;
                }
            }
        }
    }

    /// Qualifies a store-local `TemplateIrId` into a store-qualified `TemplateRef`.
    ///
    /// WHAT: pairs the template ID with this store's registry-level `TemplateStoreId`.
    /// WHY: wrapper sets store `TemplateRef`s so cross-store ownership is explicit;
    ///      this helper is the single point where store-local IDs become qualified.
    pub(crate) fn qualify_template_ref(&self, template_id: TemplateIrId) -> TemplateRef {
        TemplateRef::new(self.store_id, template_id)
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
    /// `TemplateWrapperReference` values (root + phase + overlay_set_id). Empty
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

    /// Returns true when any node under one of the supplied roots is a `BranchChain` or `Loop`,
    /// including control flow nested inside referenced child templates.
    ///
    /// WHAT: lets an in-progress parser builder state check its root children
    ///       before they have been sealed into a single sequence node.
    pub(crate) fn subtree_contains_control_flow_from_roots(
        &self,
        roots: &[TemplateIrNodeId],
    ) -> bool {
        let mut visited = HashSet::new();
        roots
            .iter()
            .any(|root| self.subtree_contains_control_flow_impl(*root, &mut visited))
    }

    fn subtree_contains_control_flow_impl(
        &self,
        node_id: TemplateIrNodeId,
        visited: &mut HashSet<TemplateIrNodeId>,
    ) -> bool {
        if !visited.insert(node_id) {
            return false;
        }

        let Some(node) = self.nodes.get(node_id.index()) else {
            return false;
        };

        match &node.kind {
            TemplateIrNodeKind::BranchChain { .. }
            | TemplateIrNodeKind::Loop { .. }
            | TemplateIrNodeKind::LoopControl { .. } => true,

            TemplateIrNodeKind::Sequence { children } => children
                .iter()
                .any(|child| self.subtree_contains_control_flow_impl(*child, visited)),

            TemplateIrNodeKind::ChildTemplate { reference, .. } => {
                if let Some(template_id) = reference.template_id_in_store(self.store_id) {
                    self.templates
                        .get(template_id.index())
                        .is_some_and(|child_template| {
                            self.subtree_contains_control_flow_impl(child_template.root, visited)
                        })
                } else {
                    false
                }
            }
            TemplateIrNodeKind::InsertContribution { template } => self
                .templates
                .get(template.index())
                .is_some_and(|child_template| {
                    self.subtree_contains_control_flow_impl(child_template.root, visited)
                }),

            TemplateIrNodeKind::Text { .. }
            | TemplateIrNodeKind::DynamicExpression { .. }
            | TemplateIrNodeKind::Slot { .. }
            | TemplateIrNodeKind::AggregateOutput
            | TemplateIrNodeKind::RuntimeSlotSite { .. } => false,
        }
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

                let template_id = reference.template_id_in_store(self.store_id)?;
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
}

impl Default for TemplateIrStore {
    fn default() -> Self {
        Self::new()
    }
}
