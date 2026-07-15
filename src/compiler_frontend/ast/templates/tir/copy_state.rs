//! TIR copy state: recursive copy-pass state, runtime slot-site cursor, and
//! copy-pass instrumentation counters.
//!
//! WHAT: owns `TirCopyState` (summary + depth + slot-site cursor) and
//!       `RuntimeSlotSiteCursor` (slot-copy traversal progress), plus the
//!       instrumentation counter helper that runtime slot planning calls after
//!       a copy pass.
//! WHY: these types live here so that `slot_plan.rs`, `subtree_copy.rs`, and
//!      the runtime slot planner can import them without
//!      circular module dependencies. Summary field updates live on
//!      `TemplateIrSummary` in `summary.rs`; this module owns the traversal
//!      state that wraps the summary during recursive copying.

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
//  RuntimeSlotSiteCursor
// -------------------------

/// Traversal cursor for runtime slot-site resolution during TIR subtree copying.
///
/// WHAT: tracks the next slot-site index to try for each slot plan during
///       active-context subtree copying and slot-to-runtime conversion.
/// WHY: the cursor is runtime slot-copy traversal state, not summary metadata.
///      Keeping it separate from `TemplateIrSummary` preserves the boundary
///      between summary facts (cheap, computed once) and traversal progress
///      (mutable, per-pass).
#[derive(Clone, Default)]
pub(crate) struct RuntimeSlotSiteCursor {
    cursors: FxHashMap<usize, usize>,
}

impl RuntimeSlotSiteCursor {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Finds the next matching runtime slot site for the given key and advances
    /// the cursor past it.
    pub(crate) fn next_site_for_key(
        &mut self,
        slot_plan_id: TemplateSlotPlanId,
        key: &SlotKey,
        store: &TemplateIrStore,
    ) -> Option<RuntimeSlotSiteId> {
        let slot_plan = store.slot_plans.get(slot_plan_id.index())?;
        let start_index = self
            .cursors
            .get(&slot_plan_id.index())
            .copied()
            .unwrap_or(0);

        for (index, site) in slot_plan.slot_sites.iter().enumerate().skip(start_index) {
            if &site.key == key {
                self.cursors.insert(slot_plan_id.index(), index + 1);
                return Some(site.site);
            }
        }

        None
    }

    /// Advances the cursor to at least the given site + 1.
    pub(crate) fn advance_to(&mut self, plan: TemplateSlotPlanId, site: RuntimeSlotSiteId) {
        let next_site_index = site.0.saturating_add(1);
        let cursor = self.cursors.entry(plan.index()).or_default();
        *cursor = (*cursor).max(next_site_index);
    }

    /// Clears the cursor for a given slot plan, starting a fresh traversal.
    pub(crate) fn reset(&mut self, slot_plan_id: TemplateSlotPlanId) {
        self.cursors.remove(&slot_plan_id.index());
    }
}

// -------------------------
//  TirCopyState
// -------------------------

/// Accumulates summary metadata and traversal state during TIR subtree copying.
///
/// WHAT: bundles a `TemplateIrSummary` (cheap shape facts), the current
///       traversal depth, and a `RuntimeSlotSiteCursor` (slot-copy traversal
///       progress) so subtree copy and runtime slot planning can thread one
///       mutable state value through recursive calls.
/// WHY: summary accumulation and slot-site cursor traversal are distinct
///      concerns that share the same recursive call chain. Keeping them on
///      one context struct avoids passing three separate mutable references
///      through every recursion level. Summary field updates delegate to
///      `TemplateIrSummary`; this struct owns only depth tracking and cursor
///      state.
pub(crate) struct TirCopyState {
    pub(crate) summary: TemplateIrSummary,
    current_depth: u16,
    pub(crate) runtime_slot_site_cursor: RuntimeSlotSiteCursor,
}

impl TirCopyState {
    /// Creates an empty copy state with zeroed counters.
    pub(crate) fn new() -> Self {
        Self {
            summary: TemplateIrSummary::empty(),
            current_depth: 0,
            runtime_slot_site_cursor: RuntimeSlotSiteCursor::new(),
        }
    }

    /// Records a text node and updates the running summary.
    pub(crate) fn record_text_node(&mut self, byte_len: usize) {
        self.summary.record_text_node(byte_len);
        self.update_depth();
    }

    /// Records a dynamic expression node.
    pub(crate) fn record_dynamic_expression(&mut self, has_reactive_subscription: bool) {
        self.summary
            .record_dynamic_expression(has_reactive_subscription);
        self.update_depth();
    }

    /// Records a child template reference.
    pub(crate) fn record_child_template(&mut self) {
        self.summary.record_child_template();
        self.update_depth();
    }

    /// Records a slot placeholder.
    pub(crate) fn record_slot(&mut self) {
        self.summary.record_slot();
        self.update_depth();
    }

    /// Records a control-flow node (branch, loop, or loop control).
    pub(crate) fn record_control_flow(&mut self) {
        self.summary.record_control_flow();
        self.update_depth();
    }

    /// Records a runtime slot site.
    pub(crate) fn record_runtime_slot_site(
        &mut self,
        plan: TemplateSlotPlanId,
        site: RuntimeSlotSiteId,
    ) {
        self.summary.record_runtime_slot_site();
        self.update_depth();
        self.runtime_slot_site_cursor.advance_to(plan, site);
    }

    /// Records an existing `RuntimeSlotSite` node that is being copied from a
    /// finalized subtree.
    pub(crate) fn record_existing_runtime_slot_site(&mut self) {
        self.summary.record_runtime_slot_site();
        self.update_depth();
    }

    /// Records an `$insert("name")` contribution node.
    pub(crate) fn record_insert_contribution(&mut self) {
        self.summary.record_insert_contribution();
        self.update_depth();
    }

    /// Finds the next matching runtime slot site for the given key, advancing
    /// the cursor past it.
    pub(crate) fn next_runtime_slot_site_for_key(
        &mut self,
        slot_plan_id: TemplateSlotPlanId,
        key: &SlotKey,
        store: &TemplateIrStore,
    ) -> Option<RuntimeSlotSiteId> {
        self.runtime_slot_site_cursor
            .next_site_for_key(slot_plan_id, key, store)
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

    /// Clears the per-plan runtime slot-site cursor for a new copy pass.
    pub(crate) fn reset_runtime_slot_site_cursor(&mut self, slot_plan_id: TemplateSlotPlanId) {
        self.runtime_slot_site_cursor.reset(slot_plan_id);
    }
}

// -------------------------
//  Copy-pass instrumentation
// -------------------------

/// Records the templates, nodes, text and depth produced by one TIR copy pass.
pub(crate) fn record_tir_copy_counters(
    store: &TemplateIrStore,
    templates_before: usize,
    nodes_before: usize,
    state: &TirCopyState,
) {
    let templates_created = store.template_count() - templates_before;
    let nodes_created = store.node_count() - nodes_before;

    add_ast_counter(AstCounter::TirTemplatesCreated, templates_created);
    add_ast_counter(AstCounter::TirNodesCreated, nodes_created);
    add_ast_counter(
        AstCounter::TirTextNodesCreated,
        state.summary.text_node_count as usize,
    );
    add_ast_counter(
        AstCounter::TirTextBytesRecorded,
        state.summary.text_byte_count,
    );
    record_ast_counter_max(AstCounter::TirMaxDepth, state.summary.max_depth as usize);
}
