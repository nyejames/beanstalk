//! AST runtime slot application planning.
//!
//! WHAT: TIR-native runtime slot plan materialization that produces owned
//! handoff payloads for the AST/HIR boundary.
//!
//! WHY: HIR should only consume prepared source/site plans. The runtime slot
//! planner writes side-tables into the module-scoped TIR store, then returns
//! neutral owned handoff shapes defined in `runtime_handoff.rs`.

mod sites;
mod sources;
mod types;

use super::error::TemplateSlotError;
use crate::compiler_frontend::ast::templates::tir::{
    TemplateIr, TemplateIrStore, TemplateSlotPlan, TirCopyState,
    convert_tir_tree_to_active_slot_plan, copy_tir_subtree_with_active_slot_plan,
    record_tir_copy_counters,
};
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::instrumentation::{AstCounter, add_ast_counter};
use crate::compiler_frontend::symbols::string_interning::StringTable;

pub(in crate::compiler_frontend::ast::templates) use sources::tir_contributions_need_runtime;
pub(crate) use types::{RuntimeSlotContributionSourceId, RuntimeSlotSiteId};

/// Materializes a TIR-native runtime slot plan from routed TIR contributions.
///
/// WHAT: when the TIR-native head-chain composition detects that a wrapper's
///       fill content is non-const-evaluable (runtime), this function produces a
///       new TIR template entry whose `runtime_slot_plan` carries the
///       contribution sources and slot sites, mirroring the atom-based
///       `materialize_runtime_slot_handoff` but starting from already-routed
///       TIR node IDs instead of atoms.
/// WHY: the HIR materializes runtime slot plans through the template's
///      `runtime_slot_plan` field. Without this path, TIR-native composition
///      would structurally expand runtime fills, flattening wrapper text and
///      fill content together — which breaks loop-control semantics (wrapper
///      text would render before `continue` is reached) and drops runtime
///      slot-site boundaries. Producing a runtime plan here ensures the HIR
///      sees the same `RuntimeSlotSite` / contribution-source structure the
///      atom-based path would have produced.
pub(in crate::compiler_frontend::ast::templates) fn materialize_tir_native_runtime_slot_plan(
    store: &mut TemplateIrStore,
    wrapper_template_id: crate::compiler_frontend::ast::templates::tir::TemplateIrId,
    routed: &crate::compiler_frontend::ast::templates::tir::RoutedTirSlotContributions,
    string_table: &StringTable,
    location: &SourceLocation,
) -> Result<crate::compiler_frontend::ast::templates::tir::TemplateIrId, TemplateSlotError> {
    use crate::compiler_frontend::ast::templates::tir::collect_tir_slot_schema;

    // Clone the wrapper template's style, kind, and root up front so the
    // immutable store borrow ends before the mutable borrows that follow.
    let (wrapper_root, wrapper_style, wrapper_kind) = store
        .get_template(wrapper_template_id)
        .map(|template| (template.root, template.style.clone(), template.kind.clone()))
        .ok_or_else(|| {
            CompilerError::compiler_error(
                "TIR-native runtime slot plan: wrapper template ID was not present in the store.",
            )
        })?;

    let mut copy_state = TirCopyState::new();
    let mut scratch_copy_state = TirCopyState::new();

    // Copy the wrapper's TIR root as a scratch tree. No active slot plan is
    // passed so Slot nodes stay as Slot nodes for site-draft collection, then
    // get converted to RuntimeSlotSite nodes after the site plan is built.
    let scratch_tir_root =
        copy_tir_subtree_with_active_slot_plan(wrapper_root, None, store, &mut scratch_copy_state)
            .map_err(TemplateSlotError::from)?;

    let templates_before = store.template_count();
    let nodes_before = store.node_count();
    let slot_plan_id = store.push_slot_plan(TemplateSlotPlan {
        location: location.clone(),
        contribution_sources: vec![],
        slot_sites: vec![],
    });

    // Re-collect the wrapper's slot schema in production, since
    // RoutedTirSlotContributions only carries the schema in test builds.
    let schema = collect_tir_slot_schema(store, wrapper_template_id, string_table)
        .map_err(TemplateSlotError::from)?;

    let sources = sources::build_tir_native_contribution_sources(
        &schema,
        &routed.contributions,
        location,
        string_table,
        store,
        &mut copy_state,
    )
    .map_err(TemplateSlotError::from)?;

    let source_plans = sources
        .iter()
        .map(|source| source.source.clone())
        .collect::<Vec<_>>();
    let slot_sites = sites::build_runtime_wrapper_site_plan(
        scratch_tir_root,
        &sources,
        slot_plan_id,
        store,
        string_table,
        &mut copy_state,
    )?;

    let Some(slot_plan) = store.slot_plans.get_mut(slot_plan_id.index()) else {
        return Err(CompilerError::compiler_error(
            "TIR-native runtime slot plan materialization lost its TIR slot-plan entry.",
        )
        .into());
    };
    slot_plan.contribution_sources = source_plans;
    slot_plan.slot_sites = slot_sites;

    // Convert the scratch tree's Slot nodes into RuntimeSlotSite nodes using
    // the active slot plan's cursor, matching the atom-based path's conversion.
    copy_state.reset_runtime_slot_site_cursor(slot_plan_id);
    convert_tir_tree_to_active_slot_plan(scratch_tir_root, slot_plan_id, store, &mut copy_state)
        .map_err(TemplateSlotError::from)?;

    copy_state
        .summary
        .merge_converted_wrapper_tree(&scratch_copy_state.summary);

    let mut tir_template = TemplateIr::new(
        scratch_tir_root,
        wrapper_style,
        wrapper_kind,
        copy_state.summary.clone(),
        location.clone(),
    );
    tir_template.runtime_slot_plan = Some(slot_plan_id);

    let template_id = store.push_template(tir_template);
    record_tir_copy_counters(store, templates_before, nodes_before, &copy_state);
    add_ast_counter(AstCounter::RuntimeSlotHandoffsMaterialized, 1);

    Ok(template_id)
}
