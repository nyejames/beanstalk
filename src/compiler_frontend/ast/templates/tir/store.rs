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
use crate::compiler_frontend::ast::templates::tir::ids::{
    TemplateIrId, TemplateIrNodeId, TemplateWrapperSetId,
};
use crate::compiler_frontend::ast::templates::tir::node::TemplateIr;
use crate::compiler_frontend::ast::templates::tir::node::TemplateIrNode;

// -------------------------
//  Placeholder side-table types
// -------------------------

// These are minimal scaffolding types. They will be fleshed out in later phases
// when the converter, formatter view, and slot-plan routes land.

/// A reusable set of `$children(..)` wrapper templates.
///
/// WHAT: groups wrapper templates so identical wrapper combinations share storage.
/// WHY: many sibling templates inherit the same wrappers from their parent;
/// deduplicating avoids redundant node trees.
#[derive(Clone, Debug)]
pub(crate) struct TemplateWrapperSet {
    /// Placeholder for the wrapper nodes that belong to this set.
    pub(crate) _reserved: (),
}

/// Routing plan for slot placeholders within a TIR template.
///
/// WHAT: maps `$slot` placeholders to their resolved contribution structure.
/// WHY: slot routing is computed once during conversion; storing it alongside
/// the TIR avoids recomposing during fold or HIR lowering.
#[derive(Clone, Debug)]
pub(crate) struct TemplateSlotPlan {
    /// Placeholder for slot plan fields added in Phase B1.
    pub(crate) _reserved: (),
}

/// Opaque anchor for a formatter-produced region.
///
/// WHAT: records boundaries where a style formatter (e.g., `$markdown`) produced
/// output so fold and HIR can treat the region as opaque text.
/// WHY: separating formatter anchors from body text nodes keeps the formatter
/// boundary explicit without string-level guard characters.
#[derive(Clone, Debug)]
pub(crate) struct TemplateFormatterAnchor {
    /// Placeholder for anchor fields added in Phase B3.
    pub(crate) _reserved: (),
}

/// Metadata for a reactive `$(source)` subscription within TIR.
///
/// WHAT: records that a TIR node depends on a reactive source for live updates.
/// WHY: HIR lowering needs subscription metadata to emit correct fragment
/// boundaries; storing it in TIR avoids re-traversing expressions.
#[derive(Clone, Debug)]
pub(crate) struct TemplateReactiveSubscription {
    /// Placeholder for subscription fields added in Phase B1.
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
/// These invariants are enforced by the converter (Phase B1) and validated
/// by `validation::validate_tir_store` (Phase B1).
#[derive(Debug)]
pub(crate) struct TemplateIrStore {
    /// Top-level templates.
    pub(crate) templates: Vec<TemplateIr>,

    /// Nodes forming the template body trees.
    pub(crate) nodes: Vec<TemplateIrNode>,

    /// Deduplicated wrapper sets for `$children(..)` wrappers.
    pub(crate) wrapper_sets: Vec<TemplateWrapperSet>,

    /// Slot routing plans.
    pub(crate) slot_plans: Vec<TemplateSlotPlan>,

    /// Formatter opaque anchors.
    pub(crate) formatter_anchors: Vec<TemplateFormatterAnchor>,

    /// Reactive `$(source)` subscription metadata.
    pub(crate) reactive_subscriptions: Vec<TemplateReactiveSubscription>,
}

impl TemplateIrStore {
    /// Creates an empty store with no pre-allocated capacity.
    pub(crate) fn new() -> Self {
        Self {
            templates: Vec::new(),
            nodes: Vec::new(),
            wrapper_sets: Vec::new(),
            slot_plans: Vec::new(),
            formatter_anchors: Vec::new(),
            reactive_subscriptions: Vec::new(),
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
            templates: Vec::with_capacity(template_capacity),
            nodes: Vec::with_capacity(node_capacity),
            wrapper_sets: Vec::with_capacity(side_capacity),
            slot_plans: Vec::with_capacity(side_capacity),
            formatter_anchors: Vec::with_capacity(side_capacity),
            reactive_subscriptions: Vec::with_capacity(side_capacity),
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

    /// Pushes a node into the store and returns its ID.
    pub(crate) fn push_node(&mut self, node: TemplateIrNode) -> TemplateIrNodeId {
        let id = TemplateIrNodeId::new(self.nodes.len());
        self.nodes.push(node);
        id
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

    /// Returns a reference to the template at the given ID, or `None` if out of bounds.
    pub(crate) fn get_template(&self, id: TemplateIrId) -> Option<&TemplateIr> {
        self.templates.get(id.index())
    }

    /// Returns a reference to the node at the given ID, or `None` if out of bounds.
    pub(crate) fn get_node(&self, id: TemplateIrNodeId) -> Option<&TemplateIrNode> {
        self.nodes.get(id.index())
    }
}

impl Default for TemplateIrStore {
    fn default() -> Self {
        Self::new()
    }
}
