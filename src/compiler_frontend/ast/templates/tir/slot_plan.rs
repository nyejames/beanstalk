//! TIR slot-plan handoff helpers.
//!
//! WHAT: owns the TIR-side representation of AST-prepared runtime slot
//! application plans.
//!
//! WHY: slot routing still belongs to AST template planning. TIR should carry
//! the already-routed source and site plans behind a typed side-table ID so
//! later folding and HIR handoff can consume slot applications without
//! rediscovering or re-routing slots.

use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::SlotKey;
use crate::compiler_frontend::ast::templates::template_slots::{
    RuntimeSlotContributionSourceId, RuntimeSlotSiteId,
};
use crate::compiler_frontend::ast::templates::tir::construction::CurrentStateMaterializationSummary;
use crate::compiler_frontend::ast::templates::tir::ids::{TemplateIrNodeId, TemplateSlotPlanId};
use crate::compiler_frontend::ast::templates::tir::node::{TemplateIrNode, TemplateIrNodeKind};
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

/// TIR side-table entry for a slot-routing plan.
///
/// WHAT: stores TIR-rendered contribution source plans and slot-site plans for
/// a runtime slot application. The wrapper plan itself is the owning
/// `TemplateIr` root that references this side-table entry through
/// `TemplateIr::runtime_slot_plan`.
/// WHY: this is the first TIR-owned slot-plan handoff. TIR no longer carries a
/// raw runtime-slot planner object; later HIR handoff can consume this route
/// view without re-running AST slot routing.
#[derive(Clone, Debug)]
pub(crate) struct TemplateSlotPlan {
    /// Source location for invariant reporting at the AST/HIR handoff.
    pub(crate) location: SourceLocation,

    /// TIR-rendered contribution source plans, one per runtime contribution.
    pub(crate) contribution_sources: Vec<TemplateSlotContributionSourcePlan>,

    /// TIR-rendered slot-site plans, one per wrapper placeholder occurrence.
    pub(crate) slot_sites: Vec<TemplateSlotSitePlan>,
}

/// TIR-side plan for one runtime slot contribution source.
///
/// WHAT: stores the source accumulator metadata plus a TIR root for the render
/// pieces that fill that accumulator.
/// WHY: source rendering is the next consumer-facing unit after routing. Keeping
/// it in TIR lets later HIR handoff avoid rebuilding render plans from AST
/// pieces.
#[derive(Clone, Debug)]
pub(crate) struct TemplateSlotContributionSourcePlan {
    pub(crate) source: RuntimeSlotContributionSourceId,
    pub(crate) target: SlotKey,
    pub(crate) render_root: TemplateIrNodeId,
    pub(crate) renders_wrapper_unconditionally: bool,
    pub(crate) location: SourceLocation,
}

/// TIR-side plan for one concrete runtime slot site.
#[derive(Clone, Debug)]
pub(crate) struct TemplateSlotSitePlan {
    pub(crate) site: RuntimeSlotSiteId,
    pub(crate) key: SlotKey,
    pub(crate) render_plan: TemplateSlotSiteRenderPlan,
    pub(crate) location: SourceLocation,
}

/// TIR-side render plan for one concrete slot site.
#[derive(Clone, Debug, Default)]
pub(crate) struct TemplateSlotSiteRenderPlan {
    pub(crate) pieces: Vec<TemplateSlotSiteRenderPiece>,
}

/// One TIR-side slot-site render piece.
#[derive(Clone, Debug)]
pub(crate) enum TemplateSlotSiteRenderPiece {
    Render(TemplateIrNodeId),
    ContributionSource(RuntimeSlotContributionSourceId),
}

pub(super) fn convert_runtime_slot_site(
    plan: TemplateSlotPlanId,
    site: RuntimeSlotSiteId,
    store: &mut TemplateIrStore,
    summary: &mut CurrentStateMaterializationSummary,
    location: &SourceLocation,
) -> TemplateIrNodeId {
    summary.record_runtime_slot_site(plan, site);

    store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::RuntimeSlotSite { plan, site },
        location.clone(),
    ))
}

// ---------------------------------------------------------------------------
//  Slot-to-RuntimeSlotSite conversion
// ---------------------------------------------------------------------------

/// Converts `Slot` nodes in a TIR tree into `RuntimeSlotSite` nodes.
///
/// WHAT: walks the TIR tree starting at `root_node_id` in document order and
/// replaces each `Slot` node in-place with a `RuntimeSlotSite` node. The
/// matching site is found via the cursor in `summary`, which advances as each
/// slot is converted.
///
/// `ChildTemplate` nodes are recursed into so nested slots inside child
/// templates are converted in the same document-order pass. When a child
/// template has slots converted, its `TemplateIr.summary` is updated to
/// reflect the conversion: `slot_count` is set to 0 and
/// `is_const_evaluable_shape` is set to false.
///
/// WHY: after materializing a scratch wrapper tree with `Slot` nodes still
/// intact (for site-draft collection), this conversion replaces them with the
/// resolved `RuntimeSlotSite` nodes in-place, keeping the scratch tree as the
/// single source for the final wrapper tree.
///
/// Returns `true` when at least one `Slot` node was converted in this subtree.
pub(crate) fn convert_tir_tree_to_active_slot_plan(
    root_node_id: TemplateIrNodeId,
    slot_plan_id: TemplateSlotPlanId,
    store: &mut TemplateIrStore,
    summary: &mut CurrentStateMaterializationSummary,
) -> Result<bool, TemplateError> {
    let node_kind = store
        .get_node(root_node_id)
        .ok_or_else(|| {
            CompilerError::compiler_error(
                "TIR active-slot conversion: node ID was not present in the store.",
            )
        })?
        .kind
        .clone();

    let converted = match node_kind {
        TemplateIrNodeKind::Slot { placeholder } => {
            let site_id = summary
                .next_runtime_slot_site_for_key(slot_plan_id, &placeholder.key, store)
                .ok_or_else(|| {
                    CompilerError::compiler_error(
                        "TIR active-slot conversion: no matching site found for a slot placeholder.",
                    )
                })?;

            let node = &mut store.nodes[root_node_id.index()];
            node.kind = TemplateIrNodeKind::RuntimeSlotSite {
                plan: slot_plan_id,
                site: site_id,
            };

            true
        }

        TemplateIrNodeKind::Sequence { children } => {
            let mut any_converted = false;
            for child_id in children {
                any_converted |=
                    convert_tir_tree_to_active_slot_plan(child_id, slot_plan_id, store, summary)?;
            }
            any_converted
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            let child_template_id = reference.template_id_in_store(store.store_id()).ok_or_else(
                || CompilerError::compiler_error(
                    "TIR active-slot conversion: child template reference is not in the current store.",
                ),
            )?;

            let child_root = store
                .get_template(child_template_id)
                .ok_or_else(|| {
                    CompilerError::compiler_error(
                        "TIR active-slot conversion: child template ID was not present in the store.",
                    )
                })?
                .root;

            let child_converted =
                convert_tir_tree_to_active_slot_plan(child_root, slot_plan_id, store, summary)?;

            if child_converted {
                let child_template = &mut store.templates[child_template_id.index()];
                child_template.summary.slot_count = 0;
                child_template.summary.is_const_evaluable_shape = false;
            }

            child_converted
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            let mut any_converted = false;
            for branch in branches {
                any_converted |= convert_tir_tree_to_active_slot_plan(
                    branch.body,
                    slot_plan_id,
                    store,
                    summary,
                )?;
            }
            if let Some(fallback_id) = fallback {
                any_converted |= convert_tir_tree_to_active_slot_plan(
                    fallback_id,
                    slot_plan_id,
                    store,
                    summary,
                )?;
            }
            any_converted
        }

        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            let mut any_converted =
                convert_tir_tree_to_active_slot_plan(body, slot_plan_id, store, summary)?;
            if let Some(aggregate_wrapper_id) = aggregate_wrapper {
                any_converted |= convert_tir_tree_to_active_slot_plan(
                    aggregate_wrapper_id,
                    slot_plan_id,
                    store,
                    summary,
                )?;
            }
            any_converted
        }

        TemplateIrNodeKind::Text { .. }
        | TemplateIrNodeKind::DynamicExpression { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::InsertContribution { .. }
        | TemplateIrNodeKind::LoopControl { .. }
        | TemplateIrNodeKind::RuntimeSlotSite { .. } => false,
    };

    Ok(converted)
}
