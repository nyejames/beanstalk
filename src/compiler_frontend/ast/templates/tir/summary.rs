//! TIR summary metadata.
//!
//! WHAT: `TemplateIrSummary` records cheap shape facts about a template —
//! estimated output size, node counts, depth, and feature flags — so capacity
//! planning, folding, and HIR preparation can make decisions without traversing
//! the full node tree.
//!
//! WHY: the current codebase already benefits from render-piece byte estimation
//! and atom-count capacity hints. TIR summaries extend
//! that pattern to the full tree shape so future phases can pre-size buffers,
//! skip unnecessary traversals, and flag templates that need special handling.
//!
//! ## Ownership contract
//!
//! Summaries are computed once during conversion and stored with
//! each `TemplateIr`. They do not own AST, HIR, or backend data. Summary
//! fields are cheap to copy and cheap to compute.

use crate::compiler_frontend::ast::templates::tir::ids::TemplateIrNodeId;
use crate::compiler_frontend::ast::templates::tir::node::TemplateIrNodeKind;
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;

// -------------------------
//  Template IR Summary
// -------------------------

/// Shape metadata for a single TIR template.
///
/// WHAT: collects counts, byte estimates, depth, and boolean feature flags.
/// WHY: avoiding a second traversal for common shape queries keeps the
/// folding and formatting paths fast. Counters also feed profiling so
/// the compiler can identify templates that deserve special handling.
///
/// ## Invariants
///
/// - All counts are zero for an empty template.
/// - `max_depth` is 0 for a flat (single-node) template.
/// - `estimated_output_bytes` is a conservative lower bound; actual output
///   may be larger when runtime expressions are involved.
#[derive(Clone, Debug)]
pub(crate) struct TemplateIrSummary {
    /// Conservative estimate of the final folded output size in bytes.
    ///
    /// Used to seed `String::with_capacity` in the TIR fold path.
    /// This is a lower bound — runtime expressions contribute zero to the estimate.
    pub(crate) estimated_output_bytes: usize,

    /// Number of `Text` nodes in the template tree.
    pub(crate) text_node_count: u32,

    /// Total interned text bytes across all `Text` nodes.
    pub(crate) text_byte_count: usize,

    /// Number of `DynamicExpression` nodes.
    pub(crate) dynamic_expression_count: u32,

    /// Number of `ChildTemplate` references.
    pub(crate) child_template_count: u32,

    /// Number of head-origin nodes recorded before the first body-origin node.
    ///
    /// WHAT: marks the boundary between head and body children in a TIR root
    ///       sequence so composition passes can tell which `ChildTemplate`
    ///       references are body direct children.
    /// WHY: `ChildTemplate` nodes do not carry an origin field; recording the
    ///      head count during parser emission lets `$children(..)` wrapper
    ///      application apply only to body direct children without re-scanning
    ///      text/dynamic-expression origins.
    pub(crate) head_node_count: u32,

    /// Number of `Slot` placeholders.
    pub(crate) slot_count: u32,

    /// Number of `InsertContribution` nodes (`$insert("name")` helpers).
    ///
    /// WHAT: counts escaped insert-contribution nodes that slot composition must
    ///       route before folding or HIR handoff.
    /// WHY: lets store-aware classification answer "does this template still
    ///      contain unresolved insert helpers?" from summary metadata without
    ///      a full tree walk.
    pub(crate) insert_contribution_count: u32,

    /// Number of wrapper set entries (from `$children(..)` directives).
    pub(crate) wrapper_count: u32,

    /// Maximum nesting depth of the node tree (root is depth 0).
    pub(crate) max_depth: u16,

    /// True if the template contains at least one `Slot` placeholder.
    pub(crate) has_slots: bool,

    /// True if the template contains at least one `InsertContribution` node.
    ///
    /// WHAT: mirrors `has_slots` for escaped `$insert("name")` contributions.
    /// WHY: store-aware classification uses this flag to decide whether a
    ///      template still carries unresolved slot-insertion helpers.
    pub(crate) has_insert_contributions: bool,

    /// True if the template contains `BranchChain`, `Loop`, or `LoopControl` nodes.
    pub(crate) has_control_flow: bool,

    /// True if the template contains at least one reactive subscription.
    ///
    /// WHAT: set when a `DynamicExpression` carries a reactive subscription in
    ///       its node payload, or when a `Text` node has a reactive subscription
    ///       stored in the TIR side table (`node_reactive_subscriptions`).
    /// WHY: reactive metadata traversal may attach subscriptions to text nodes
    ///      after the node payload is frozen. The summary must reflect both
    ///      payload-level and side-table-level reactivity so downstream
    ///      consumers (classification, folding) see the true reactive shape.
    pub(crate) has_reactivity: bool,

    /// True if the template shape can be fully evaluated at compile time.
    pub(crate) is_const_evaluable_shape: bool,
}

impl TemplateIrSummary {
    /// Creates an all-zero summary for an empty or placeholder template.
    pub(crate) fn empty() -> Self {
        Self {
            estimated_output_bytes: 0,
            text_node_count: 0,
            text_byte_count: 0,
            dynamic_expression_count: 0,
            child_template_count: 0,
            head_node_count: 0,
            slot_count: 0,
            insert_contribution_count: 0,
            wrapper_count: 0,
            max_depth: 0,
            has_slots: false,
            has_insert_contributions: false,
            has_control_flow: false,
            has_reactivity: false,
            is_const_evaluable_shape: true,
        }
    }
}

impl Default for TemplateIrSummary {
    fn default() -> Self {
        Self::empty()
    }
}

// -------------------------
//  Incremental record helpers
// -------------------------

impl TemplateIrSummary {
    /// Records a text node and updates byte/node counters.
    ///
    /// WHAT: increments text_node_count, adds byte_len to text_byte_count
    ///       and estimated_output_bytes.
    /// WHY: the text-byte and output-estimate updates are identical across
    ///      parser builder, subtree copy, render-unit, and summary-walking
    ///      paths. Centralizing them here avoids drifted copies.
    pub(crate) fn record_text_node(&mut self, byte_len: usize) {
        self.text_node_count += 1;
        self.text_byte_count += byte_len;
        self.estimated_output_bytes += byte_len;
    }

    /// Records a dynamic-expression node.
    pub(crate) fn record_dynamic_expression(&mut self, has_reactive_subscription: bool) {
        self.dynamic_expression_count += 1;
        if has_reactive_subscription {
            self.has_reactivity = true;
        }
        self.is_const_evaluable_shape = false;
    }

    /// Records a child-template reference.
    pub(crate) fn record_child_template(&mut self) {
        self.child_template_count += 1;
    }

    /// Records a slot placeholder.
    ///
    /// WHAT: increments slot_count and sets has_slots.
    /// WHY: the construction/copy path does not set is_const_evaluable_shape
    ///      to false here because an unresolved slot placeholder may be
    ///      converted to a runtime slot site by a later pass.
    pub(crate) fn record_slot(&mut self) {
        self.slot_count += 1;
        self.has_slots = true;
    }

    /// Records a control-flow node (branch, loop, or loop control).
    pub(crate) fn record_control_flow(&mut self) {
        self.has_control_flow = true;
        self.is_const_evaluable_shape = false;
    }

    /// Records a runtime slot site.
    pub(crate) fn record_runtime_slot_site(&mut self) {
        self.has_slots = true;
        self.is_const_evaluable_shape = false;
    }

    /// Records an insert-contribution node.
    pub(crate) fn record_insert_contribution(&mut self) {
        self.insert_contribution_count += 1;
        self.has_insert_contributions = true;
        self.is_const_evaluable_shape = false;
    }

    /// Merges a converted wrapper tree summary into this one.
    ///
    /// WHAT: adds copied-node and output counters, ORs feature flags, takes the
    ///       maximum depth and forces `is_const_evaluable_shape` to false.
    ///       `slot_count` is deliberately not merged because the scratch tree's
    ///       slot placeholders have already become runtime slot sites.
    /// WHY: runtime slot-plan materialization merges a scratch wrapper copy
    ///      summary into the main construction summary. The merge is a pure
    ///      summary operation, so it belongs on TemplateIrSummary rather than
    ///      on the copy-state type that owns depth and cursor traversal state.
    pub(crate) fn merge_converted_wrapper_tree(&mut self, other: &TemplateIrSummary) {
        self.estimated_output_bytes += other.estimated_output_bytes;
        self.text_node_count += other.text_node_count;
        self.text_byte_count += other.text_byte_count;
        self.dynamic_expression_count += other.dynamic_expression_count;
        self.child_template_count += other.child_template_count;
        self.head_node_count += other.head_node_count;
        self.insert_contribution_count += other.insert_contribution_count;
        self.wrapper_count += other.wrapper_count;
        self.max_depth = self.max_depth.max(other.max_depth);
        self.has_slots |= other.has_slots;
        self.has_insert_contributions |= other.has_insert_contributions;
        self.has_control_flow |= other.has_control_flow;
        self.has_reactivity |= other.has_reactivity;
        self.is_const_evaluable_shape = false;
    }
}

// -------------------------
//  Existing-node summary
// -------------------------

/// Computes a `TemplateIrSummary` by walking existing TIR nodes in a store.
///
/// WHAT: recursively walks `root_children` and their descendants, accumulating
///       accurate counts, byte estimates, feature flags, and depth into a
///       `TemplateIrSummary`. The children are assumed to live inside a
///       wrapping `Sequence` root (depth 0), so they are summarized at depth 1.
/// WHY: derived and proxy templates wrap existing nodes in a new `Sequence`
///      root. Using a false all-zero default summary hides real reactivity,
///      control flow, slots, dynamic expressions, and text content that
///      downstream consumers (capacity planning, folding, classification) rely
///      on. This helper gives those templates an honest shape without a separate
///      materialization pass.
pub(crate) fn summarize_existing_nodes(
    store: &TemplateIrStore,
    root_children: &[TemplateIrNodeId],
) -> TemplateIrSummary {
    let mut summary = TemplateIrSummary::empty();
    accumulate_nodes(store, root_children, 1, &mut summary);
    summary
}

/// Computes a summary for an existing node used directly as a template root.
pub(crate) fn summarize_existing_root(
    store: &TemplateIrStore,
    root_node_id: TemplateIrNodeId,
) -> TemplateIrSummary {
    let mut summary = TemplateIrSummary::empty();
    accumulate_nodes(store, std::slice::from_ref(&root_node_id), 0, &mut summary);
    summary
}

/// Recursively accumulates summary facts for a slice of node IDs.
fn accumulate_nodes(
    store: &TemplateIrStore,
    node_ids: &[TemplateIrNodeId],
    depth: u16,
    summary: &mut TemplateIrSummary,
) {
    for &node_id in node_ids {
        let Some(node) = store.get_node(node_id) else {
            continue;
        };

        if depth > summary.max_depth {
            summary.max_depth = depth;
        }

        match &node.kind {
            TemplateIrNodeKind::Sequence { children } => {
                accumulate_nodes(store, children, depth.saturating_add(1), summary);
            }

            TemplateIrNodeKind::Text { byte_len, .. } => {
                let len = *byte_len as usize;
                summary.record_text_node(len);

                // Text nodes can carry a reactive subscription in the TIR side
                // table (set by reactive metadata traversal) even though the
                // node payload itself has no subscription field. Check the side
                // table so the summary reflects true reactivity.
                if store.node_reactive_subscription(node_id).is_some() {
                    summary.has_reactivity = true;
                }
            }

            TemplateIrNodeKind::DynamicExpression {
                reactive_subscription,
                ..
            } => {
                summary.record_dynamic_expression(reactive_subscription.is_some());
            }

            TemplateIrNodeKind::ChildTemplate { .. } => {
                summary.record_child_template();
            }

            TemplateIrNodeKind::Slot { .. } => {
                summary.slot_count += 1;
                summary.has_slots = true;
                summary.is_const_evaluable_shape = false;
            }

            TemplateIrNodeKind::InsertContribution { .. } => {
                summary.record_insert_contribution();
            }

            TemplateIrNodeKind::BranchChain { branches, fallback } => {
                summary.record_control_flow();

                for branch in branches {
                    accumulate_nodes(
                        store,
                        std::slice::from_ref(&branch.body),
                        depth.saturating_add(1),
                        summary,
                    );
                }
                if let Some(fallback_id) = fallback {
                    accumulate_nodes(
                        store,
                        std::slice::from_ref(fallback_id),
                        depth.saturating_add(1),
                        summary,
                    );
                }
            }

            TemplateIrNodeKind::Loop {
                body,
                aggregate_wrapper,
                ..
            } => {
                summary.record_control_flow();

                accumulate_nodes(
                    store,
                    std::slice::from_ref(body),
                    depth.saturating_add(1),
                    summary,
                );
                if let Some(wrapper_id) = aggregate_wrapper {
                    accumulate_nodes(
                        store,
                        std::slice::from_ref(wrapper_id),
                        depth.saturating_add(1),
                        summary,
                    );
                }
            }

            TemplateIrNodeKind::LoopControl { .. } => {
                summary.record_control_flow();
            }

            TemplateIrNodeKind::RuntimeSlotSite { .. } => {
                summary.record_runtime_slot_site();
            }

            // Leaf marker — no children or output bytes to accumulate.
            TemplateIrNodeKind::AggregateOutput => {}
        }
    }
}
