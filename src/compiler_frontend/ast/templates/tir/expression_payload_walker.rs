//! TIR expression-payload walkers.
//!
//! WHAT: provides read-only traversals over every expression payload reachable
//!       from same-store TIR roots and from a finalized `TirView`, plus a strict
//!       mutation traversal for finalized body roots used during AST
//!       finalization.
//! WHY: final type-boundary validation and debug TypeId validation both need to
//!      inspect the same expression-bearing TIR nodes; centralizing the walks in
//!      TIR keeps the traversal authoritative and removes near-duplicate local
//!      helpers from AST finalization. The `TirView` walk reads effective
//!      expression overlays for dynamic-expression splices, branch selectors,
//!      and loop headers. The mutation walker is the Phase 7 body-authority
//!      handoff point: it can follow `ChildTemplate` refs and runtime slot plans
//!      without recovering the old AST `Template` payload.

use std::collections::HashSet;

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchSelector, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::tir::body_root_ref::TemplateTirBodyReference;
use crate::compiler_frontend::ast::templates::tir::ids::ExpressionSiteId;
use crate::compiler_frontend::ast::templates::tir::node::{
    TemplateIrNodeKind, TemplateLoopHeaderExpressionSites,
};
use crate::compiler_frontend::ast::templates::tir::slot_plan::TemplateSlotSiteRenderPiece;
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::ast::templates::tir::view::{TirSubtreeView, TirView};
use crate::compiler_frontend::ast::templates::tir::{
    SameStoreTirRoots, TemplateIrId, TemplateIrNodeId, TemplateSlotPlanId,
};
use crate::compiler_frontend::ast::templates::tir::{
    TemplateIrRegistry, TemplateNodeRef, TemplateOverlaySetId, TemplateRef, TemplateStoreId,
    TemplateTirPhase,
};
use crate::compiler_frontend::compiler_errors::CompilerError;

/// Visitor for expression payloads discovered during a TIR walk.
///
/// WHAT: implementors receive every dynamic-expression payload and every
///       selector/header expression reached from the starting roots. The walker
///       handles all structural recursion (sequences, branches, loops, wrappers,
///       child templates, insert contributions) so visitors only decide what to
///       do with each expression.
pub(crate) trait TirExpressionPayloadVisitor {
    /// Error type returned when a visitor decides a payload is invalid.
    type Error;

    /// Called once for each expression payload reachable from the roots.
    fn visit_expression_payload(&mut self, expression: &Expression) -> Result<(), Self::Error>;
}

/// Mutator for expression payloads discovered during a strict TIR walk.
///
/// WHAT: receives every mutable AST `Expression` payload reachable from a
///       finalized control-flow body root.
/// WHY: AST finalization needs to normalize or annotate body-root expressions
///      after parser TIR has become the body authority. The mutator owns the
///      policy; this module owns the TIR structural recursion.
#[cfg(test)]
pub(crate) trait TirExpressionPayloadMutator {
    /// Called once for each expression payload reachable from the body root.
    fn mutate_expression_payload(
        &mut self,
        expression: &mut Expression,
    ) -> Result<(), CompilerError>;
}

/// Walks every expression payload reachable from `roots` in same-store TIR.
///
/// WHAT: recursively traverses sequences, branch chains, loops, aggregate
///       wrappers, child templates, and insert contributions; calls the visitor
///       for every dynamic expression and every branch/loop-header expression.
///       A visited set keyed by `TemplateIrId` prevents infinite recursion
///       through child-template and insert-contribution references.
/// WHY: centralizes the read-only TIR expression-payload traversal shared by
///      final type-boundary validation and debug TypeId validation.
pub(crate) fn walk_tir_expression_payloads<V>(
    store: &TemplateIrStore,
    roots: &SameStoreTirRoots,
    visitor: &mut V,
) -> Result<(), V::Error>
where
    V: TirExpressionPayloadVisitor,
{
    let mut visited_templates = HashSet::new();
    if let Some(seed_template_id) = roots.seed_template_id {
        visited_templates.insert(seed_template_id);
    }

    for root in &roots.roots {
        walk_tir_node_expression_payloads(store, *root, visitor, &mut visited_templates)?;
    }

    Ok(())
}

/// Walks every expression payload reachable from `view`, reading effective
/// expression overlays for dynamic-expression nodes, branch selectors, and
/// loop headers.
///
/// WHAT: recursively traverses the structural root of `view` and its
///       store-qualified child-template descendants. Dynamic-expression splices,
///       branch selectors, and loop-header expressions prefer the override
///       expression provided by each effective view, falling back to the stored
///       structural expression. Insert-contribution references delegate to the
///       raw same-store walker because they do not carry an effective view
///       identity.
/// WHY: centralizes the view-based expression-payload traversal used by debug
///      TypeId validation and final type-boundary validation without
///      duplicating overlay-resolution logic in finalization.
pub(crate) fn walk_tir_view_expression_payloads(
    view: &TirView<'_>,
    visitor: &mut impl FnMut(&Expression) -> Result<(), CompilerError>,
) -> Result<(), CompilerError> {
    let root_node_id = {
        let root_template = view.root_template()?;
        root_template.root
    };
    let root_node_ref = TemplateNodeRef::new(view.root_ref().store_id, root_node_id);
    let mut visited_templates = HashSet::new();
    visited_templates.insert((view.root_ref(), view.phase(), view.overlay_set_id()));

    walk_tir_view_expression_payload_node(view, root_node_ref, visitor, &mut visited_templates)
}

fn walk_tir_view_expression_payload_node(
    view: &TirView<'_>,
    node_ref: TemplateNodeRef,
    visitor: &mut impl FnMut(&Expression) -> Result<(), CompilerError>,
    visited_templates: &mut HashSet<(TemplateRef, TemplateTirPhase, TemplateOverlaySetId)>,
) -> Result<(), CompilerError> {
    let store_id = view.root_ref().store_id;
    let node = view.effective_node(node_ref)?;

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            let children = children.clone();
            drop(node);
            for child in children {
                walk_tir_view_expression_payload_node(
                    view,
                    TemplateNodeRef::new(store_id, child),
                    visitor,
                    visited_templates,
                )?;
            }
        }

        TemplateIrNodeKind::DynamicExpression {
            expression,
            site_id,
            ..
        } => {
            let effective_expression = view.effective_expression_for_site(*site_id)?;
            if let Some(expression) = effective_expression {
                visitor(expression)?;
            } else {
                visitor(expression.as_ref())?;
            }
            drop(node);
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            let branches = branches.clone();
            let fallback = *fallback;
            drop(node);
            for branch in &branches {
                let expression = view
                    .effective_expression_for_site(branch.selector_site_id)?
                    .unwrap_or(branch.condition_expression());
                visitor(expression)?;
                walk_tir_view_expression_payload_node(
                    view,
                    TemplateNodeRef::new(store_id, branch.body),
                    visitor,
                    visited_templates,
                )?;
            }
            if let Some(fallback_id) = fallback {
                walk_tir_view_expression_payload_node(
                    view,
                    TemplateNodeRef::new(store_id, fallback_id),
                    visitor,
                    visited_templates,
                )?;
            }
        }

        TemplateIrNodeKind::Loop {
            header,
            header_sites,
            body,
            aggregate_wrapper,
            ..
        } => {
            let header = header.clone();
            let header_sites = *header_sites;
            let body = *body;
            let aggregate_wrapper = *aggregate_wrapper;
            drop(node);
            visit_loop_header_effective_expressions(view, &header, header_sites, visitor)?;
            walk_tir_view_expression_payload_node(
                view,
                TemplateNodeRef::new(store_id, body),
                visitor,
                visited_templates,
            )?;
            if let Some(wrapper_id) = aggregate_wrapper {
                walk_tir_view_expression_payload_node(
                    view,
                    TemplateNodeRef::new(store_id, wrapper_id),
                    visitor,
                    visited_templates,
                )?;
            }
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            let reference = *reference;
            drop(node);
            let effective_identity = (reference.root, reference.phase, reference.overlay_set_id);
            if visited_templates.insert(effective_identity) {
                let child_view =
                    view.child_view(reference.root, reference.phase, reference.overlay_set_id)?;
                let child_root_node_id = {
                    let child_root_template = child_view.root_template()?;
                    child_root_template.root
                };
                let child_root_node_ref =
                    TemplateNodeRef::new(child_view.root_ref().store_id, child_root_node_id);

                // Foreign children still carry a complete effective view
                // identity. Follow that view through the registry rather than
                // silently treating the reference as an opaque leaf.
                walk_tir_view_expression_payload_node(
                    &child_view,
                    child_root_node_ref,
                    visitor,
                    visited_templates,
                )?;
            }
        }

        TemplateIrNodeKind::InsertContribution { template } => {
            let template_id = *template;
            drop(node);
            let effective_identity = (
                TemplateRef::new(store_id, template_id),
                view.phase(),
                view.overlay_set_id(),
            );
            if visited_templates.insert(effective_identity) {
                walk_raw_store_expression_payloads(
                    view.registry_ref(),
                    store_id,
                    template_id,
                    visitor,
                )?;
            }
        }

        TemplateIrNodeKind::Text { .. }
        | TemplateIrNodeKind::Slot { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::LoopControl { .. }
        | TemplateIrNodeKind::RuntimeSlotSite { .. } => {}
    }

    Ok(())
}

/// Visits the effective expressions for one loop header, resolving overrides
/// through the view when present.
///
/// WHAT: matches the header shape against its allocated expression sites and
///       calls the visitor for each site, preferring the overlay override and
///       falling back to the stored structural expression. A mismatched shape is
///       reported as an internal invariant error.
/// WHY: loop-header sites share the same `ExpressionSiteId` key space as
///      dynamic-expression and branch-selector sites; resolving them through the
///      view keeps overlay resolution in one place.
fn visit_loop_header_effective_expressions(
    view: &TirView<'_>,
    header: &TemplateLoopHeader,
    header_sites: TemplateLoopHeaderExpressionSites,
    visitor: &mut impl FnMut(&Expression) -> Result<(), CompilerError>,
) -> Result<(), CompilerError> {
    match (header, header_sites) {
        (
            TemplateLoopHeader::Conditional { condition },
            TemplateLoopHeaderExpressionSites::Conditional { condition: site_id },
        ) => {
            let expression = view
                .effective_expression_for_site(site_id)?
                .unwrap_or(condition.as_ref());
            visitor(expression)?;
        }

        (
            TemplateLoopHeader::Range { range, .. },
            TemplateLoopHeaderExpressionSites::Range { start, end, step },
        ) => {
            let start_expression = view
                .effective_expression_for_site(start)?
                .unwrap_or(&range.start);
            visitor(start_expression)?;

            let end_expression = view
                .effective_expression_for_site(end)?
                .unwrap_or(&range.end);
            visitor(end_expression)?;

            if let Some(step_site_id) = step {
                let step_expression = if let Some(expression) =
                    view.effective_expression_for_site(step_site_id)?
                {
                    expression
                } else {
                    range.step.as_ref().ok_or_else(|| {
                        CompilerError::compiler_error(
                            "TIR view expression-payload walk found a range loop step site without a structural step expression.",
                        )
                    })?
                };
                visitor(step_expression)?;
            }
        }

        (
            TemplateLoopHeader::Collection { iterable, .. },
            TemplateLoopHeaderExpressionSites::Collection { iterable: site_id },
        ) => {
            let expression = view
                .effective_expression_for_site(site_id)?
                .unwrap_or(iterable.as_ref());
            visitor(expression)?;
        }

        _ => {
            return Err(CompilerError::compiler_error(
                "TIR view expression-payload walk found mismatched loop-header expression sites.",
            ));
        }
    }

    Ok(())
}

/// Delegates a nested same-store template root to the raw store walker.
///
/// WHAT: borrows the registry-owned store for `store_id`, looks up
///       `template_id`, and walks its root through the existing raw
///       `walk_tir_expression_payloads` traversal.
/// WHY: the `TirView` walker keeps nested-shape fallback conservative by
///      reusing the proven raw-store path for child templates and insert
///      contributions that do not carry a usable view identity.
fn walk_raw_store_expression_payloads(
    registry: &TemplateIrRegistry,
    store_id: TemplateStoreId,
    template_id: TemplateIrId,
    visitor: &mut impl FnMut(&Expression) -> Result<(), CompilerError>,
) -> Result<(), CompilerError> {
    let store = registry.store(store_id).ok_or_else(|| {
        CompilerError::compiler_error(
            "TIR view walker could not borrow the store for a raw fallback walk.",
        )
    })?;

    let Some(template_ir) = store.get_template(template_id) else {
        return Ok(());
    };

    let roots = SameStoreTirRoots {
        roots: vec![template_ir.root],
        seed_template_id: Some(template_id),
    };
    let mut adapter = TirExpressionPayloadVisitorAdapter { visitor };
    walk_tir_expression_payloads(&store, &roots, &mut adapter)
}

struct TirExpressionPayloadVisitorAdapter<'a, F> {
    visitor: &'a mut F,
}

impl<F> TirExpressionPayloadVisitor for TirExpressionPayloadVisitorAdapter<'_, F>
where
    F: FnMut(&Expression) -> Result<(), CompilerError>,
{
    type Error = CompilerError;

    fn visit_expression_payload(&mut self, expression: &Expression) -> Result<(), Self::Error> {
        (self.visitor)(expression)
    }
}

/// Mutates every expression payload reachable from one finalized body root.
///
/// WHAT: starts from a `TemplateIrNodeId`, mutates dynamic-expression payloads,
///       branch selectors, loop headers, aggregate-wrapper subtrees, ordinary
///       sequence children, insert-contribution children, and referenced child
///       templates. When a reached child template owns a runtime slot plan, the
///       walk also enters the wrapper root, contribution-source render roots,
///       and slot-site render-piece roots.
/// WHY: focused tests still need a strict mutation path to prove body-root
///      traversal invariants. Production finalization uses overlay-based
///      normalization so shared TIR nodes are no longer mutated.
#[cfg(test)]
pub(crate) fn mutate_finalized_tir_body_root_expression_payloads<M>(
    store: &mut TemplateIrStore,
    root: TemplateIrNodeId,
    mutator: &mut M,
) -> Result<(), CompilerError>
where
    M: TirExpressionPayloadMutator,
{
    let mut walker = FinalizedBodyRootExpressionMutator::new(store, mutator);
    walker.walk_node(root)
}

/// Collects cloned structural expression payloads reachable from a template.
///
/// WHAT: traverses template structure below `template_id`, including runtime
///       slot-plan roots owned by that template, records dynamic-expression
///       payloads, branch selectors, and loop-header expressions keyed by their
///       `ExpressionSiteId`.
/// WHY: AST normalization reads structural payloads before any overlay is
///      applied, so the first normalization pass sees the raw TIR expressions
///      that composition produced. Keeping the traversal in the TIR walker
///      module avoids another AST-local tree walk.
pub(crate) fn collect_tir_expression_overlay_payloads(
    store: &TemplateIrStore,
    template_id: TemplateIrId,
) -> Result<Vec<(ExpressionSiteId, Expression)>, CompilerError> {
    let mut collector = ExpressionOverlayPayloadCollector::new(store);
    collector.collect_template(template_id)?;
    Ok(collector.into_payloads())
}

/// Collects cloned structural expression payloads reachable from one body root.
///
/// WHAT: traverses the same-store TIR subtree below `root`, including runtime
///       slot-plan roots owned by reached child templates, and records
///       dynamic-expression payloads, branch selectors, and loop-header
///       expressions keyed by their `ExpressionSiteId`.
/// WHY: first-layer finalization passes (such as reactive metadata annotation)
///      read the structural payloads and write the first overlay, before any
///      earlier overlay exists for the body.
pub(crate) fn collect_tir_body_root_expression_overlay_payloads(
    store: &TemplateIrStore,
    root: TemplateIrNodeId,
) -> Result<Vec<(ExpressionSiteId, Expression)>, CompilerError> {
    let mut collector = ExpressionOverlayPayloadCollector::new(store);
    collector.collect_node(root)?;
    Ok(collector.into_payloads())
}

/// Collects effective expression payloads reachable from one body root.
///
/// WHAT: traverses the same-store TIR subtree below `body_reference.node_ref`
///       through a `TirSubtreeView`, reading the effective expression for every
///       discovered `ExpressionSiteId` from the view's overlay set. Child
///       templates are entered through their own `TirView` so their effective
///       overlays are honored too.
/// WHY: later finalization passes (such as AST normalization) must observe the
///      result of earlier overlay layers rather than replacing them silently.
pub(crate) fn collect_effective_tir_body_root_expression_overlay_payloads(
    registry: &TemplateIrRegistry,
    body_reference: &TemplateTirBodyReference,
) -> Result<Vec<(ExpressionSiteId, Expression)>, CompilerError> {
    let view = TirSubtreeView::new(registry, body_reference)?;
    let mut payloads = Vec::new();
    let mut visited_templates = HashSet::new();
    collect_effective_tir_subtree_expression_payloads(
        &view,
        body_reference.node_ref,
        &mut payloads,
        &mut visited_templates,
    )?;
    Ok(payloads)
}

/// Collects effective expression payloads from a body/root subtree view.
fn collect_effective_tir_subtree_expression_payloads(
    view: &TirSubtreeView<'_>,
    node_ref: TemplateNodeRef,
    payloads: &mut Vec<(ExpressionSiteId, Expression)>,
    visited_templates: &mut HashSet<TemplateIrId>,
) -> Result<(), CompilerError> {
    let store_id = node_ref.store_id;
    let node = view.effective_node(node_ref)?;

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            let children = children.clone();
            drop(node);
            for child in children {
                collect_effective_tir_subtree_expression_payloads(
                    view,
                    TemplateNodeRef::new(store_id, child),
                    payloads,
                    visited_templates,
                )?;
            }
        }

        TemplateIrNodeKind::DynamicExpression {
            expression,
            site_id,
            ..
        } => {
            let effective_expression = view
                .effective_expression_for_site(*site_id)?
                .cloned()
                .unwrap_or_else(|| expression.as_ref().clone());
            payloads.push((*site_id, effective_expression));
            drop(node);
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            let branches = branches.clone();
            let fallback = *fallback;
            drop(node);
            for branch in &branches {
                let expression = view
                    .effective_expression_for_site(branch.selector_site_id)?
                    .cloned()
                    .unwrap_or_else(|| branch.condition_expression().clone());
                payloads.push((branch.selector_site_id, expression));
                collect_effective_tir_subtree_expression_payloads(
                    view,
                    TemplateNodeRef::new(store_id, branch.body),
                    payloads,
                    visited_templates,
                )?;
            }
            if let Some(fallback_id) = fallback {
                collect_effective_tir_subtree_expression_payloads(
                    view,
                    TemplateNodeRef::new(store_id, fallback_id),
                    payloads,
                    visited_templates,
                )?;
            }
        }

        TemplateIrNodeKind::Loop {
            header,
            header_sites,
            body,
            aggregate_wrapper,
            ..
        } => {
            let header = header.clone();
            let header_sites = *header_sites;
            let body = *body;
            let aggregate_wrapper = *aggregate_wrapper;
            drop(node);
            collect_loop_header_effective_payloads(view, &header, header_sites, payloads)?;
            collect_effective_tir_subtree_expression_payloads(
                view,
                TemplateNodeRef::new(store_id, body),
                payloads,
                visited_templates,
            )?;
            if let Some(wrapper_id) = aggregate_wrapper {
                collect_effective_tir_subtree_expression_payloads(
                    view,
                    TemplateNodeRef::new(store_id, wrapper_id),
                    payloads,
                    visited_templates,
                )?;
            }
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            let reference = *reference;
            drop(node);
            if let Some(template_id) = reference.template_id_in_store(store_id)
                && visited_templates.insert(template_id)
            {
                let child_view = TirView::new(
                    view.registry_ref(),
                    reference.root,
                    reference.phase,
                    reference.overlay_set_id,
                )?;
                let child_root_node_id = {
                    let child_root_template = child_view.root_template()?;
                    child_root_template.root
                };
                let child_root_node_ref =
                    TemplateNodeRef::new(child_view.root_ref().store_id, child_root_node_id);
                collect_effective_tir_view_expression_payloads(
                    &child_view,
                    child_root_node_ref,
                    payloads,
                    visited_templates,
                )?;
            }
        }

        TemplateIrNodeKind::InsertContribution { template } => {
            let template_id = *template;
            drop(node);
            if visited_templates.insert(template_id) {
                let child_view = TirView::new(
                    view.registry_ref(),
                    TemplateRef::new(store_id, template_id),
                    TemplateTirPhase::Parsed,
                    TemplateOverlaySetId::empty(),
                )?;
                let child_root_node_id = {
                    let child_root_template = child_view.root_template()?;
                    child_root_template.root
                };
                let child_root_node_ref =
                    TemplateNodeRef::new(child_view.root_ref().store_id, child_root_node_id);
                collect_effective_tir_view_expression_payloads(
                    &child_view,
                    child_root_node_ref,
                    payloads,
                    visited_templates,
                )?;
            }
        }

        TemplateIrNodeKind::RuntimeSlotSite { .. }
        | TemplateIrNodeKind::Text { .. }
        | TemplateIrNodeKind::Slot { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::LoopControl { .. } => {
            drop(node);
        }
    }

    Ok(())
}

/// Collects effective expression payloads from a top-level `TirView`.
fn collect_effective_tir_view_expression_payloads(
    view: &TirView<'_>,
    node_ref: TemplateNodeRef,
    payloads: &mut Vec<(ExpressionSiteId, Expression)>,
    visited_templates: &mut HashSet<TemplateIrId>,
) -> Result<(), CompilerError> {
    let store_id = node_ref.store_id;
    let node = view.effective_node(node_ref)?;

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            let children = children.clone();
            drop(node);
            for child in children {
                collect_effective_tir_view_expression_payloads(
                    view,
                    TemplateNodeRef::new(store_id, child),
                    payloads,
                    visited_templates,
                )?;
            }
        }

        TemplateIrNodeKind::DynamicExpression {
            expression,
            site_id,
            ..
        } => {
            let effective_expression = view
                .effective_expression_for_site(*site_id)?
                .cloned()
                .unwrap_or_else(|| expression.as_ref().clone());
            payloads.push((*site_id, effective_expression));
            drop(node);
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            let branches = branches.clone();
            let fallback = *fallback;
            drop(node);
            for branch in &branches {
                let expression = view
                    .effective_expression_for_site(branch.selector_site_id)?
                    .cloned()
                    .unwrap_or_else(|| branch.condition_expression().clone());
                payloads.push((branch.selector_site_id, expression));
                collect_effective_tir_view_expression_payloads(
                    view,
                    TemplateNodeRef::new(store_id, branch.body),
                    payloads,
                    visited_templates,
                )?;
            }
            if let Some(fallback_id) = fallback {
                collect_effective_tir_view_expression_payloads(
                    view,
                    TemplateNodeRef::new(store_id, fallback_id),
                    payloads,
                    visited_templates,
                )?;
            }
        }

        TemplateIrNodeKind::Loop {
            header,
            header_sites,
            body,
            aggregate_wrapper,
            ..
        } => {
            let header = header.clone();
            let header_sites = *header_sites;
            let body = *body;
            let aggregate_wrapper = *aggregate_wrapper;
            drop(node);
            collect_loop_header_effective_payloads_for_view(view, &header, header_sites, payloads)?;
            collect_effective_tir_view_expression_payloads(
                view,
                TemplateNodeRef::new(store_id, body),
                payloads,
                visited_templates,
            )?;
            if let Some(wrapper_id) = aggregate_wrapper {
                collect_effective_tir_view_expression_payloads(
                    view,
                    TemplateNodeRef::new(store_id, wrapper_id),
                    payloads,
                    visited_templates,
                )?;
            }
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            let reference = *reference;
            drop(node);
            if let Some(template_id) = reference.template_id_in_store(store_id)
                && visited_templates.insert(template_id)
            {
                let child_view =
                    view.child_view(reference.root, reference.phase, reference.overlay_set_id)?;
                let child_root_node_id = {
                    let child_root_template = child_view.root_template()?;
                    child_root_template.root
                };
                let child_root_node_ref =
                    TemplateNodeRef::new(child_view.root_ref().store_id, child_root_node_id);
                collect_effective_tir_view_expression_payloads(
                    &child_view,
                    child_root_node_ref,
                    payloads,
                    visited_templates,
                )?;
            }
        }

        TemplateIrNodeKind::InsertContribution { template } => {
            let template_id = *template;
            drop(node);
            if visited_templates.insert(template_id) {
                let child_view = TirView::new(
                    view.registry_ref(),
                    TemplateRef::new(store_id, template_id),
                    TemplateTirPhase::Parsed,
                    TemplateOverlaySetId::empty(),
                )?;
                let child_root_node_id = {
                    let child_root_template = child_view.root_template()?;
                    child_root_template.root
                };
                let child_root_node_ref =
                    TemplateNodeRef::new(child_view.root_ref().store_id, child_root_node_id);
                collect_effective_tir_view_expression_payloads(
                    &child_view,
                    child_root_node_ref,
                    payloads,
                    visited_templates,
                )?;
            }
        }

        TemplateIrNodeKind::RuntimeSlotSite { .. }
        | TemplateIrNodeKind::Text { .. }
        | TemplateIrNodeKind::Slot { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::LoopControl { .. } => {
            drop(node);
        }
    }

    Ok(())
}

/// Collects loop-header effective expression payloads through a subtree view.
fn collect_loop_header_effective_payloads(
    view: &TirSubtreeView<'_>,
    header: &TemplateLoopHeader,
    header_sites: TemplateLoopHeaderExpressionSites,
    payloads: &mut Vec<(ExpressionSiteId, Expression)>,
) -> Result<(), CompilerError> {
    match (header, header_sites) {
        (
            TemplateLoopHeader::Conditional { condition },
            TemplateLoopHeaderExpressionSites::Conditional { condition: site_id },
        ) => {
            let expression = view
                .effective_expression_for_site(site_id)?
                .cloned()
                .unwrap_or_else(|| condition.as_ref().clone());
            payloads.push((site_id, expression));
        }

        (
            TemplateLoopHeader::Range { range, .. },
            TemplateLoopHeaderExpressionSites::Range { start, end, step },
        ) => {
            let start_expression = view
                .effective_expression_for_site(start)?
                .cloned()
                .unwrap_or_else(|| range.start.clone());
            payloads.push((start, start_expression));

            let end_expression = view
                .effective_expression_for_site(end)?
                .cloned()
                .unwrap_or_else(|| range.end.clone());
            payloads.push((end, end_expression));

            match (step, &range.step) {
                (Some(step_site_id), Some(step_expression)) => {
                    let expression = view
                        .effective_expression_for_site(step_site_id)?
                        .cloned()
                        .unwrap_or_else(|| step_expression.clone());
                    payloads.push((step_site_id, expression));
                }
                (None, None) => {}
                _ => {
                    return Err(CompilerError::compiler_error(
                        "TIR effective expression overlay collection found mismatched range loop step site.",
                    ));
                }
            }
        }

        (
            TemplateLoopHeader::Collection { iterable, .. },
            TemplateLoopHeaderExpressionSites::Collection { iterable: site_id },
        ) => {
            let expression = view
                .effective_expression_for_site(site_id)?
                .cloned()
                .unwrap_or_else(|| iterable.as_ref().clone());
            payloads.push((site_id, expression));
        }

        _ => {
            return Err(CompilerError::compiler_error(
                "TIR effective expression overlay collection found mismatched loop-header expression sites.",
            ));
        }
    }

    Ok(())
}

/// Collects loop-header effective expression payloads through a top-level view.
fn collect_loop_header_effective_payloads_for_view(
    view: &TirView<'_>,
    header: &TemplateLoopHeader,
    header_sites: TemplateLoopHeaderExpressionSites,
    payloads: &mut Vec<(ExpressionSiteId, Expression)>,
) -> Result<(), CompilerError> {
    match (header, header_sites) {
        (
            TemplateLoopHeader::Conditional { condition },
            TemplateLoopHeaderExpressionSites::Conditional { condition: site_id },
        ) => {
            let expression = view
                .effective_expression_for_site(site_id)?
                .cloned()
                .unwrap_or_else(|| condition.as_ref().clone());
            payloads.push((site_id, expression));
        }

        (
            TemplateLoopHeader::Range { range, .. },
            TemplateLoopHeaderExpressionSites::Range { start, end, step },
        ) => {
            let start_expression = view
                .effective_expression_for_site(start)?
                .cloned()
                .unwrap_or_else(|| range.start.clone());
            payloads.push((start, start_expression));

            let end_expression = view
                .effective_expression_for_site(end)?
                .cloned()
                .unwrap_or_else(|| range.end.clone());
            payloads.push((end, end_expression));

            match (step, &range.step) {
                (Some(step_site_id), Some(step_expression)) => {
                    let expression = view
                        .effective_expression_for_site(step_site_id)?
                        .cloned()
                        .unwrap_or_else(|| step_expression.clone());
                    payloads.push((step_site_id, expression));
                }
                (None, None) => {}
                _ => {
                    return Err(CompilerError::compiler_error(
                        "TIR effective expression overlay collection found mismatched range loop step site.",
                    ));
                }
            }
        }

        (
            TemplateLoopHeader::Collection { iterable, .. },
            TemplateLoopHeaderExpressionSites::Collection { iterable: site_id },
        ) => {
            let expression = view
                .effective_expression_for_site(site_id)?
                .cloned()
                .unwrap_or_else(|| iterable.as_ref().clone());
            payloads.push((site_id, expression));
        }

        _ => {
            return Err(CompilerError::compiler_error(
                "TIR effective expression overlay collection found mismatched loop-header expression sites.",
            ));
        }
    }

    Ok(())
}

/// Recursively visits expression payloads reachable from one TIR node.
fn walk_tir_node_expression_payloads<V>(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
    visitor: &mut V,
    visited_templates: &mut HashSet<TemplateIrId>,
) -> Result<(), V::Error>
where
    V: TirExpressionPayloadVisitor,
{
    let Some(node) = store.get_node(node_id) else {
        return Ok(());
    };

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            for child in children {
                walk_tir_node_expression_payloads(store, *child, visitor, visited_templates)?;
            }
        }

        TemplateIrNodeKind::DynamicExpression { expression, .. } => {
            visitor.visit_expression_payload(expression)?;
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            for branch in branches {
                visit_branch_selector_expression(&branch.selector, visitor)?;
                walk_tir_node_expression_payloads(store, branch.body, visitor, visited_templates)?;
            }

            if let Some(fallback_id) = fallback {
                walk_tir_node_expression_payloads(store, *fallback_id, visitor, visited_templates)?;
            }
        }

        TemplateIrNodeKind::Loop {
            header,
            body,
            aggregate_wrapper,
            ..
        } => {
            visit_loop_header_expressions(header, visitor)?;
            walk_tir_node_expression_payloads(store, *body, visitor, visited_templates)?;

            if let Some(wrapper_id) = aggregate_wrapper {
                walk_tir_node_expression_payloads(store, *wrapper_id, visitor, visited_templates)?;
            }
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            if let Some(template_id) = reference.template_id_in_store(store.store_id())
                && visited_templates.insert(template_id)
                && let Some(template_ir) = store.get_template(template_id)
            {
                walk_tir_node_expression_payloads(
                    store,
                    template_ir.root,
                    visitor,
                    visited_templates,
                )?;
            }
        }

        TemplateIrNodeKind::InsertContribution { template } => {
            if visited_templates.insert(*template)
                && let Some(template_ir) = store.get_template(*template)
            {
                walk_tir_node_expression_payloads(
                    store,
                    template_ir.root,
                    visitor,
                    visited_templates,
                )?;
            }
        }

        TemplateIrNodeKind::Text { .. }
        | TemplateIrNodeKind::Slot { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::LoopControl { .. }
        | TemplateIrNodeKind::RuntimeSlotSite { .. } => {}
    }

    Ok(())
}

#[cfg(test)]
struct FinalizedBodyRootExpressionMutator<'store, 'visitor, M>
where
    M: TirExpressionPayloadMutator,
{
    store: &'store mut TemplateIrStore,
    mutator: &'visitor mut M,
    active_nodes: HashSet<TemplateIrNodeId>,
    completed_nodes: HashSet<TemplateIrNodeId>,
    active_templates: HashSet<TemplateIrId>,
    completed_templates: HashSet<TemplateIrId>,
    active_slot_plans: HashSet<TemplateSlotPlanId>,
    completed_slot_plans: HashSet<TemplateSlotPlanId>,
}

#[cfg(test)]
impl<'store, 'visitor, M> FinalizedBodyRootExpressionMutator<'store, 'visitor, M>
where
    M: TirExpressionPayloadMutator,
{
    fn new(store: &'store mut TemplateIrStore, mutator: &'visitor mut M) -> Self {
        Self {
            store,
            mutator,
            active_nodes: HashSet::new(),
            completed_nodes: HashSet::new(),
            active_templates: HashSet::new(),
            completed_templates: HashSet::new(),
            active_slot_plans: HashSet::new(),
            completed_slot_plans: HashSet::new(),
        }
    }

    fn walk_template(&mut self, template_id: TemplateIrId) -> Result<(), CompilerError> {
        if self.completed_templates.contains(&template_id) {
            return Ok(());
        }

        if !self.active_templates.insert(template_id) {
            return Err(CompilerError::compiler_error(
                "TIR expression mutation found a recursive child-template reference.",
            ));
        }

        let (root, runtime_slot_plan) = {
            let template = self.store.get_template(template_id).ok_or_else(|| {
                CompilerError::compiler_error(
                    "TIR expression mutation referenced a missing child template.",
                )
            })?;
            (template.root, template.runtime_slot_plan)
        };

        let result = if let Some(slot_plan_id) = runtime_slot_plan {
            self.walk_runtime_slot_application(root, slot_plan_id)
        } else {
            self.walk_node(root)
        };

        self.active_templates.remove(&template_id);
        if result.is_ok() {
            self.completed_templates.insert(template_id);
        }
        result
    }

    fn walk_runtime_slot_application(
        &mut self,
        wrapper_root: TemplateIrNodeId,
        slot_plan_id: TemplateSlotPlanId,
    ) -> Result<(), CompilerError> {
        if self.completed_slot_plans.contains(&slot_plan_id) {
            return self.walk_node(wrapper_root);
        }

        if !self.active_slot_plans.insert(slot_plan_id) {
            return Err(CompilerError::compiler_error(
                "TIR expression mutation found a recursive runtime slot plan.",
            ));
        }

        let (contribution_roots, site_render_roots) =
            runtime_slot_plan_roots(self.store, slot_plan_id)?;

        let result = self.walk_node(wrapper_root).and_then(|()| {
            for root in contribution_roots {
                self.walk_node(root)?;
            }

            for root in site_render_roots {
                self.walk_node(root)?;
            }

            Ok(())
        });

        self.active_slot_plans.remove(&slot_plan_id);
        if result.is_ok() {
            self.completed_slot_plans.insert(slot_plan_id);
        }
        result
    }

    fn walk_node(&mut self, node_id: TemplateIrNodeId) -> Result<(), CompilerError> {
        if self.completed_nodes.contains(&node_id) {
            return Ok(());
        }

        if !self.active_nodes.insert(node_id) {
            return Err(CompilerError::compiler_error(
                "TIR expression mutation found a recursive node reference.",
            ));
        }

        let children = self.mutate_node_and_collect_children(node_id);
        let result = match children {
            Ok(children) => self.walk_children(children),
            Err(error) => Err(error),
        };

        self.active_nodes.remove(&node_id);
        if result.is_ok() {
            self.completed_nodes.insert(node_id);
        }
        result
    }

    fn mutate_node_and_collect_children(
        &mut self,
        node_id: TemplateIrNodeId,
    ) -> Result<Vec<TirExpressionWalkChild>, CompilerError> {
        let store_id = self.store.store_id();
        let node = self.store.nodes.get_mut(node_id.index()).ok_or_else(|| {
            CompilerError::compiler_error("TIR expression mutation referenced a missing node.")
        })?;

        match &mut node.kind {
            TemplateIrNodeKind::Sequence { children } => Ok(children
                .iter()
                .copied()
                .map(TirExpressionWalkChild::Node)
                .collect()),

            TemplateIrNodeKind::DynamicExpression { expression, .. } => {
                self.mutator.mutate_expression_payload(expression)?;
                Ok(Vec::new())
            }

            TemplateIrNodeKind::BranchChain { branches, fallback } => {
                let mut children =
                    Vec::with_capacity(branches.len() + usize::from(fallback.is_some()));
                for branch in branches {
                    mutate_branch_selector_expression(&mut branch.selector, self.mutator)?;
                    children.push(TirExpressionWalkChild::Node(branch.body));
                }

                if let Some(fallback_id) = fallback {
                    children.push(TirExpressionWalkChild::Node(*fallback_id));
                }

                Ok(children)
            }

            TemplateIrNodeKind::Loop {
                header,
                body,
                aggregate_wrapper,
                ..
            } => {
                mutate_loop_header_expressions(header, self.mutator)?;

                let mut children = Vec::with_capacity(1 + usize::from(aggregate_wrapper.is_some()));
                children.push(TirExpressionWalkChild::Node(*body));

                if let Some(wrapper_id) = aggregate_wrapper {
                    children.push(TirExpressionWalkChild::Node(*wrapper_id));
                }

                Ok(children)
            }

            TemplateIrNodeKind::ChildTemplate { reference, .. } => {
                let template_id = reference.template_id_in_store(store_id).ok_or_else(|| {
                    CompilerError::compiler_error(
                        "TIR expression payload walk: child template reference is not in the current store.",
                    )
                })?;
                Ok(vec![TirExpressionWalkChild::Template(template_id)])
            }

            TemplateIrNodeKind::InsertContribution { template } => {
                Ok(vec![TirExpressionWalkChild::Template(*template)])
            }

            TemplateIrNodeKind::RuntimeSlotSite { plan, site } => {
                let Some(slot_plan) = self.store.slot_plans.get(plan.index()) else {
                    return Err(CompilerError::compiler_error(
                        "TIR expression mutation referenced a missing runtime slot plan.",
                    ));
                };

                let Some(indexed_site) = slot_plan.slot_sites.get(site.0) else {
                    return Err(CompilerError::compiler_error(
                        "TIR expression mutation referenced a missing runtime slot site.",
                    ));
                };

                if indexed_site.site != *site {
                    return Err(CompilerError::compiler_error(
                        "TIR expression mutation found a runtime slot site index mismatch.",
                    ));
                }

                Ok(Vec::new())
            }

            TemplateIrNodeKind::Text { .. }
            | TemplateIrNodeKind::Slot { .. }
            | TemplateIrNodeKind::AggregateOutput
            | TemplateIrNodeKind::LoopControl { .. } => Ok(Vec::new()),
        }
    }

    fn walk_children(
        &mut self,
        children: Vec<TirExpressionWalkChild>,
    ) -> Result<(), CompilerError> {
        for child in children {
            match child {
                TirExpressionWalkChild::Node(node_id) => self.walk_node(node_id)?,
                TirExpressionWalkChild::Template(template_id) => self.walk_template(template_id)?,
            }
        }

        Ok(())
    }
}

enum TirExpressionWalkChild {
    Node(TemplateIrNodeId),
    Template(TemplateIrId),
}

struct ExpressionOverlayPayloadCollector<'store> {
    store: &'store TemplateIrStore,
    payloads: Vec<(ExpressionSiteId, Expression)>,
    active_nodes: HashSet<TemplateIrNodeId>,
    completed_nodes: HashSet<TemplateIrNodeId>,
    active_templates: HashSet<TemplateIrId>,
    completed_templates: HashSet<TemplateIrId>,
    active_slot_plans: HashSet<TemplateSlotPlanId>,
    completed_slot_plans: HashSet<TemplateSlotPlanId>,
}

impl<'store> ExpressionOverlayPayloadCollector<'store> {
    fn new(store: &'store TemplateIrStore) -> Self {
        Self {
            store,
            payloads: Vec::new(),
            active_nodes: HashSet::new(),
            completed_nodes: HashSet::new(),
            active_templates: HashSet::new(),
            completed_templates: HashSet::new(),
            active_slot_plans: HashSet::new(),
            completed_slot_plans: HashSet::new(),
        }
    }

    fn into_payloads(self) -> Vec<(ExpressionSiteId, Expression)> {
        self.payloads
    }

    fn collect_template(&mut self, template_id: TemplateIrId) -> Result<(), CompilerError> {
        if self.completed_templates.contains(&template_id) {
            return Ok(());
        }

        if !self.active_templates.insert(template_id) {
            return Err(CompilerError::compiler_error(
                "TIR expression overlay collection found a recursive child-template reference.",
            ));
        }

        let (root, runtime_slot_plan) = self
            .store
            .get_template(template_id)
            .map(|template| (template.root, template.runtime_slot_plan))
            .ok_or_else(|| {
                CompilerError::compiler_error(
                    "TIR expression overlay collection referenced a missing child template.",
                )
            })?;
        let result = if let Some(slot_plan_id) = runtime_slot_plan {
            self.collect_runtime_slot_application(root, slot_plan_id)
        } else {
            self.collect_node(root)
        };

        self.active_templates.remove(&template_id);
        if result.is_ok() {
            self.completed_templates.insert(template_id);
        }

        result
    }

    fn collect_runtime_slot_application(
        &mut self,
        wrapper_root: TemplateIrNodeId,
        slot_plan_id: TemplateSlotPlanId,
    ) -> Result<(), CompilerError> {
        if self.completed_slot_plans.contains(&slot_plan_id) {
            return self.collect_node(wrapper_root);
        }

        if !self.active_slot_plans.insert(slot_plan_id) {
            return Err(CompilerError::compiler_error(
                "TIR expression overlay collection found a recursive runtime slot plan.",
            ));
        }

        let (contribution_roots, site_render_roots) =
            runtime_slot_plan_roots(self.store, slot_plan_id)?;

        let result = self.collect_node(wrapper_root).and_then(|()| {
            for root in contribution_roots {
                self.collect_node(root)?;
            }

            for root in site_render_roots {
                self.collect_node(root)?;
            }

            Ok(())
        });

        self.active_slot_plans.remove(&slot_plan_id);
        if result.is_ok() {
            self.completed_slot_plans.insert(slot_plan_id);
        }

        result
    }

    fn collect_node(&mut self, node_id: TemplateIrNodeId) -> Result<(), CompilerError> {
        if self.completed_nodes.contains(&node_id) {
            return Ok(());
        }

        if !self.active_nodes.insert(node_id) {
            return Err(CompilerError::compiler_error(
                "TIR expression overlay collection found a recursive node reference.",
            ));
        }

        let children = self.collect_node_payload_and_children(node_id);
        let result = match children {
            Ok(children) => {
                for child in children {
                    match child {
                        TirExpressionWalkChild::Node(node_id) => self.collect_node(node_id)?,
                        TirExpressionWalkChild::Template(template_id) => {
                            self.collect_template(template_id)?;
                        }
                    }
                }

                Ok(())
            }
            Err(error) => Err(error),
        };

        self.active_nodes.remove(&node_id);
        if result.is_ok() {
            self.completed_nodes.insert(node_id);
        }

        result
    }

    fn collect_node_payload_and_children(
        &mut self,
        node_id: TemplateIrNodeId,
    ) -> Result<Vec<TirExpressionWalkChild>, CompilerError> {
        let node = self.store.get_node(node_id).ok_or_else(|| {
            CompilerError::compiler_error(
                "TIR expression overlay collection referenced a missing node.",
            )
        })?;

        match &node.kind {
            TemplateIrNodeKind::Sequence { children } => Ok(children
                .iter()
                .copied()
                .map(TirExpressionWalkChild::Node)
                .collect()),

            TemplateIrNodeKind::DynamicExpression {
                expression,
                site_id,
                ..
            } => {
                self.payloads.push((*site_id, expression.as_ref().clone()));
                Ok(Vec::new())
            }

            TemplateIrNodeKind::BranchChain { branches, fallback } => {
                let mut children =
                    Vec::with_capacity(branches.len() + usize::from(fallback.is_some()));
                for branch in branches {
                    self.payloads.push((
                        branch.selector_site_id,
                        branch.condition_expression().clone(),
                    ));
                    children.push(TirExpressionWalkChild::Node(branch.body));
                }

                if let Some(fallback) = fallback {
                    children.push(TirExpressionWalkChild::Node(*fallback));
                }

                Ok(children)
            }

            TemplateIrNodeKind::Loop {
                header,
                header_sites,
                body,
                aggregate_wrapper,
                ..
            } => {
                self.collect_loop_header_payloads(header, header_sites)?;

                let mut children = Vec::with_capacity(1 + usize::from(aggregate_wrapper.is_some()));
                children.push(TirExpressionWalkChild::Node(*body));

                if let Some(wrapper) = aggregate_wrapper {
                    children.push(TirExpressionWalkChild::Node(*wrapper));
                }

                Ok(children)
            }

            TemplateIrNodeKind::ChildTemplate { reference, .. } => {
                let Some(template_id) = reference.template_id_in_store(self.store.store_id())
                else {
                    return Ok(Vec::new());
                };

                Ok(vec![TirExpressionWalkChild::Template(template_id)])
            }

            TemplateIrNodeKind::InsertContribution { template } => {
                Ok(vec![TirExpressionWalkChild::Template(*template)])
            }

            TemplateIrNodeKind::Text { .. }
            | TemplateIrNodeKind::Slot { .. }
            | TemplateIrNodeKind::AggregateOutput
            | TemplateIrNodeKind::LoopControl { .. }
            | TemplateIrNodeKind::RuntimeSlotSite { .. } => Ok(Vec::new()),
        }
    }

    fn collect_loop_header_payloads(
        &mut self,
        header: &TemplateLoopHeader,
        header_sites: &TemplateLoopHeaderExpressionSites,
    ) -> Result<(), CompilerError> {
        match (header, header_sites) {
            (
                TemplateLoopHeader::Conditional { condition },
                TemplateLoopHeaderExpressionSites::Conditional { condition: site_id },
            ) => {
                self.payloads.push((*site_id, condition.as_ref().clone()));
            }

            (
                TemplateLoopHeader::Range { range, .. },
                TemplateLoopHeaderExpressionSites::Range { start, end, step },
            ) => {
                self.payloads.push((*start, range.start.clone()));
                self.payloads.push((*end, range.end.clone()));

                match (step, &range.step) {
                    (Some(step_site_id), Some(step_expression)) => {
                        self.payloads.push((*step_site_id, step_expression.clone()));
                    }
                    (None, None) => {}
                    _ => {
                        return Err(CompilerError::compiler_error(
                            "TIR expression overlay collection found mismatched range loop step site.",
                        ));
                    }
                }
            }

            (
                TemplateLoopHeader::Collection { iterable, .. },
                TemplateLoopHeaderExpressionSites::Collection { iterable: site_id },
            ) => {
                self.payloads.push((*site_id, iterable.as_ref().clone()));
            }

            _ => {
                return Err(CompilerError::compiler_error(
                    "TIR expression overlay collection found mismatched loop-header expression sites.",
                ));
            }
        }

        Ok(())
    }
}

fn runtime_slot_plan_roots(
    store: &TemplateIrStore,
    slot_plan_id: TemplateSlotPlanId,
) -> Result<(Vec<TemplateIrNodeId>, Vec<TemplateIrNodeId>), CompilerError> {
    let slot_plan = store.get_slot_plan(slot_plan_id).ok_or_else(|| {
        CompilerError::compiler_error(
            "TIR expression payload walk referenced a missing runtime slot plan.",
        )
    })?;

    let contribution_roots: Vec<TemplateIrNodeId> = slot_plan
        .contribution_sources
        .iter()
        .map(|source| source.render_root)
        .collect();

    let mut site_render_roots = Vec::new();
    for site in &slot_plan.slot_sites {
        for piece in &site.render_plan.pieces {
            match piece {
                TemplateSlotSiteRenderPiece::Render(root) => {
                    site_render_roots.push(*root);
                }

                TemplateSlotSiteRenderPiece::ContributionSource(source_id) => {
                    if source_id.0 >= slot_plan.contribution_sources.len() {
                        return Err(CompilerError::compiler_error(
                            "TIR expression payload walk referenced a missing runtime slot contribution source.",
                        ));
                    }
                }
            }
        }
    }

    Ok((contribution_roots, site_render_roots))
}

/// Visits the expression payload inside one branch selector.
fn visit_branch_selector_expression<V>(
    selector: &TemplateBranchSelector,
    visitor: &mut V,
) -> Result<(), V::Error>
where
    V: TirExpressionPayloadVisitor,
{
    match selector {
        TemplateBranchSelector::Bool(condition) => visitor.visit_expression_payload(condition),

        TemplateBranchSelector::OptionPresentCapture { scrutinee, .. } => {
            visitor.visit_expression_payload(scrutinee)
        }
    }
}

/// Visits every expression payload referenced by a loop header.
fn visit_loop_header_expressions<V>(
    header: &TemplateLoopHeader,
    visitor: &mut V,
) -> Result<(), V::Error>
where
    V: TirExpressionPayloadVisitor,
{
    match header {
        TemplateLoopHeader::Conditional { condition } => {
            visitor.visit_expression_payload(condition)
        }

        TemplateLoopHeader::Range { range, .. } => {
            visitor.visit_expression_payload(&range.start)?;
            visitor.visit_expression_payload(&range.end)?;
            if let Some(step) = &range.step {
                visitor.visit_expression_payload(step)?;
            }
            Ok(())
        }

        TemplateLoopHeader::Collection { iterable, .. } => {
            visitor.visit_expression_payload(iterable)
        }
    }
}

#[cfg(test)]
fn mutate_branch_selector_expression<M>(
    selector: &mut TemplateBranchSelector,
    mutator: &mut M,
) -> Result<(), CompilerError>
where
    M: TirExpressionPayloadMutator,
{
    match selector {
        TemplateBranchSelector::Bool(condition) => mutator.mutate_expression_payload(condition),

        TemplateBranchSelector::OptionPresentCapture { scrutinee, .. } => {
            mutator.mutate_expression_payload(scrutinee)
        }
    }
}

#[cfg(test)]
fn mutate_loop_header_expressions<M>(
    header: &mut TemplateLoopHeader,
    mutator: &mut M,
) -> Result<(), CompilerError>
where
    M: TirExpressionPayloadMutator,
{
    match header {
        TemplateLoopHeader::Conditional { condition } => {
            mutator.mutate_expression_payload(condition)
        }

        TemplateLoopHeader::Range { range, .. } => {
            mutator.mutate_expression_payload(&mut range.start)?;
            mutator.mutate_expression_payload(&mut range.end)?;
            if let Some(step) = &mut range.step {
                mutator.mutate_expression_payload(step)?;
            }
            Ok(())
        }

        TemplateLoopHeader::Collection { iterable, .. } => {
            mutator.mutate_expression_payload(iterable)
        }
    }
}
