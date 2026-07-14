//! TIR construction helpers: materialization summary and atom-to-TIR
//! conversion utilities.
//!
//! WHAT: owns the materialization summary and atom-level materialization helpers
//!       that control-flow body-root recovery and runtime slot planning still
//!       depend on.
//! WHY: these types and functions live here so that `control_flow_roots.rs`,
//!      `render_unit.rs`, `slot_plan.rs`, and `subtree_copy.rs` can import
//!      them without circular module dependencies.

use crate::compiler_frontend::ast::templates::template::SlotKey;
use crate::compiler_frontend::ast::templates::template_slots::RuntimeSlotSiteId;
use crate::compiler_frontend::ast::templates::tir::ids::TemplateSlotPlanId;
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::ast::templates::tir::summary::TemplateIrSummary;
use crate::compiler_frontend::instrumentation::{
    AstCounter, add_ast_counter, record_ast_counter_max,
};
use rustc_hash::FxHashMap;

// -------------------------
//  CurrentStateMaterializationSummary
// -------------------------

/// Accumulates summary metadata and node depth during materialization.
pub(crate) struct CurrentStateMaterializationSummary {
    pub(crate) summary: TemplateIrSummary,
    current_depth: u16,
    pub(crate) runtime_slot_site_cursors: FxHashMap<usize, usize>,
}

impl CurrentStateMaterializationSummary {
    /// Creates an empty materialization summary with zeroed counters.
    pub(crate) fn new() -> Self {
        Self {
            summary: TemplateIrSummary::empty(),
            current_depth: 0,
            runtime_slot_site_cursors: FxHashMap::default(),
        }
    }

    /// Records a text node and updates the running summary.
    pub(crate) fn record_text_node(&mut self, byte_len: usize) {
        self.summary.text_node_count += 1;
        self.summary.text_byte_count += byte_len;
        self.summary.estimated_output_bytes += byte_len;
        self.update_depth();
    }

    /// Records a dynamic expression node.
    pub(crate) fn record_dynamic_expression(&mut self, has_reactive_subscription: bool) {
        self.summary.dynamic_expression_count += 1;
        if has_reactive_subscription {
            self.summary.has_reactivity = true;
        }
        self.summary.is_const_evaluable_shape = false;
        self.update_depth();
    }

    /// Records a child template reference.
    pub(crate) fn record_child_template(&mut self) {
        self.summary.child_template_count += 1;
        self.update_depth();
    }

    /// Records a slot placeholder.
    pub(crate) fn record_slot(&mut self) {
        self.summary.slot_count += 1;
        self.summary.has_slots = true;
        self.update_depth();
    }

    /// Records a control-flow node (branch, loop, or loop control).
    pub(crate) fn record_control_flow(&mut self) {
        self.summary.has_control_flow = true;
        self.summary.is_const_evaluable_shape = false;
        self.update_depth();
    }

    /// Records a runtime slot site.
    pub(crate) fn record_runtime_slot_site(
        &mut self,
        plan: TemplateSlotPlanId,
        site: RuntimeSlotSiteId,
    ) {
        self.summary.has_slots = true;
        self.summary.is_const_evaluable_shape = false;
        self.update_depth();

        let next_site_index = site.0.saturating_add(1);
        let cursor = self
            .runtime_slot_site_cursors
            .entry(plan.index())
            .or_default();
        *cursor = (*cursor).max(next_site_index);
    }

    /// Records an existing `RuntimeSlotSite` node that is being copied from a
    /// finalized subtree.
    pub(crate) fn record_existing_runtime_slot_site(&mut self) {
        self.summary.has_slots = true;
        self.summary.is_const_evaluable_shape = false;
        self.update_depth();
    }

    /// Records an `$insert("name")` contribution node.
    pub(crate) fn record_insert_contribution(&mut self) {
        self.summary.insert_contribution_count += 1;
        self.summary.has_insert_contributions = true;
        self.summary.is_const_evaluable_shape = false;
        self.update_depth();
    }

    pub(crate) fn next_runtime_slot_site_for_key(
        &mut self,
        slot_plan_id: TemplateSlotPlanId,
        key: &SlotKey,
        store: &TemplateIrStore,
    ) -> Option<RuntimeSlotSiteId> {
        let slot_plan = store.slot_plans.get(slot_plan_id.index())?;
        let start_index = self
            .runtime_slot_site_cursors
            .get(&slot_plan_id.index())
            .copied()
            .unwrap_or(0);

        for (index, site) in slot_plan.slot_sites.iter().enumerate().skip(start_index) {
            if &site.key == key {
                self.runtime_slot_site_cursors
                    .insert(slot_plan_id.index(), index + 1);
                return Some(site.site);
            }
        }

        None
    }

    /// Bumps depth tracking for the current node.
    fn update_depth(&mut self) {
        if self.current_depth > self.summary.max_depth {
            self.summary.max_depth = self.current_depth;
        }
    }

    /// Enters a nested level.
    pub(crate) fn enter_depth(&mut self) {
        self.current_depth += 1;
    }

    /// Exits a nested level.
    pub(crate) fn exit_depth(&mut self) {
        self.current_depth = self.current_depth.saturating_sub(1);
    }

    /// Clears the per-plan runtime slot-site cursor for a new materialization pass.
    pub(crate) fn reset_runtime_slot_site_cursor(&mut self, slot_plan_id: TemplateSlotPlanId) {
        self.runtime_slot_site_cursors.remove(&slot_plan_id.index());
    }

    /// Merges a converted scratch tree's summary into this summary.
    pub(crate) fn merge_converted_wrapper_tree_summary(&mut self, scratch: &TemplateIrSummary) {
        self.summary.estimated_output_bytes += scratch.estimated_output_bytes;
        self.summary.text_node_count += scratch.text_node_count;
        self.summary.text_byte_count += scratch.text_byte_count;
        self.summary.dynamic_expression_count += scratch.dynamic_expression_count;
        self.summary.child_template_count += scratch.child_template_count;
        self.summary.head_node_count += scratch.head_node_count;
        self.summary.insert_contribution_count += scratch.insert_contribution_count;
        self.summary.wrapper_count += scratch.wrapper_count;
        self.summary.max_depth = self.summary.max_depth.max(scratch.max_depth);
        self.summary.has_slots |= scratch.has_slots;
        self.summary.has_insert_contributions |= scratch.has_insert_contributions;
        self.summary.has_control_flow |= scratch.has_control_flow;
        self.summary.has_reactivity |= scratch.has_reactivity;
        self.summary.is_const_evaluable_shape = false;
    }
}

// -------------------------
//  Materialization counters
// -------------------------

/// Records the templates, nodes, text and depth produced by one TIR
/// materialization pass.
pub(crate) fn record_materialization_counters(
    store: &TemplateIrStore,
    templates_before: usize,
    nodes_before: usize,
    summary: &CurrentStateMaterializationSummary,
) {
    let templates_created = store.template_count() - templates_before;
    let nodes_created = store.node_count() - nodes_before;

    add_ast_counter(AstCounter::TirTemplatesCreated, templates_created);
    add_ast_counter(AstCounter::TirNodesCreated, nodes_created);
    add_ast_counter(
        AstCounter::TirTextNodesCreated,
        summary.summary.text_node_count as usize,
    );
    add_ast_counter(
        AstCounter::TirTextBytesRecorded,
        summary.summary.text_byte_count,
    );
    record_ast_counter_max(AstCounter::TirMaxDepth, summary.summary.max_depth as usize);
}
