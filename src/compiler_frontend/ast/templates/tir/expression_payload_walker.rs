//! TIR expression-payload walkers.
//!
//! WHAT: provides read-only effective-view traversals over every expression
//!       payload reachable from a finalized `TirView`, plus the effective overlay
//!       collector used by AST finalization and the nested expression-and-TIR-view
//!       walker used by the head parser.
//! WHY: final type-boundary validation and debug TypeId validation both need to
//!      inspect the same expression-bearing TIR nodes; centralizing the walks in
//!      TIR keeps the traversal authoritative and removes near-duplicate local
//!      helpers from AST finalization. The `TirView` walk reads effective
//!      expression overlays for dynamic-expression splices, branch selectors
//!      and loop headers, and recurses into child-template and
//!      insert-contribution views through one shared visited set. The
//!      nested-expression walker additionally recurses into `ExpressionKind`
//!      internals and re-enters TIR views for template-valued expressions.

use std::collections::HashSet;

use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::expression_rpn::ExpressionRpnItem;
#[cfg(test)]
use crate::compiler_frontend::ast::templates::template_control_flow::TemplateBranchSelector;
use crate::compiler_frontend::ast::templates::template_control_flow::TemplateLoopHeader;
use crate::compiler_frontend::ast::templates::tir::ids::ExpressionSiteId;
use crate::compiler_frontend::ast::templates::tir::node::{
    TemplateIrNodeKind, TemplateLoopHeaderExpressionSites,
};
use crate::compiler_frontend::ast::templates::tir::slot_plan::TemplateSlotSiteRenderPiece;
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::ast::templates::tir::view::TirView;
use crate::compiler_frontend::ast::templates::tir::{
    TemplateIrId, TemplateIrNodeId, TemplateSlotPlanId,
};
use crate::compiler_frontend::ast::templates::tir::{
    TemplateOverlaySetId, TemplateTirPhase, TemplateTirReference,
};
use crate::compiler_frontend::compiler_errors::CompilerError;

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

/// Walks every expression payload reachable from `view`, reading effective
/// expression overlays for dynamic-expression nodes, branch selectors, and
/// loop headers.
///
/// WHAT: recursively traverses the structural root of `view` and its
///       module-local child-template and insert-contribution descendants.
///       Dynamic-expression splices, branch selectors and loop-header
///       expressions prefer the override expression provided by each effective
///       view, falling back to the stored structural expression. Insert
///       contributions recurse through a child `TirView` that inherits the
///       parent phase and overlay set, so every reachable payload is read
///       through the same effective-view authority.
/// WHY: centralizes the view-based expression-payload traversal used by debug
///      TypeId validation and final type-boundary validation without
///      duplicating overlay-resolution logic in finalization.
pub(crate) fn walk_tir_view_expression_payloads(
    view: &TirView<'_>,
    visitor: &mut impl FnMut(&Expression) -> Result<(), CompilerError>,
) -> Result<(), CompilerError> {
    let mut visited_templates = HashSet::new();
    walk_tir_view_expression_payloads_with_visited(view, visitor, &mut visited_templates)
}

/// Walks every expression payload reachable from `view`, sharing `visited_templates`.
///
/// WHAT: same structural coverage as [`walk_tir_view_expression_payloads`], but
///       accepts an external visited set so callers that also enter TIR views
///       through nested `ExpressionKind` paths can share one cycle-prevention
///       set keyed by `(TemplateIrId, TemplateTirPhase, TemplateOverlaySetId)`.
/// WHY: the nested-expression walker needs a single visited set across both
///      TIR-view child-template references and `ExpressionKind::Template`
///      re-entries; extracting this entry point avoids duplicating the
///      view-walk logic while keeping the standalone API unchanged for
///      type-boundary and debug-TypeId validation.
fn walk_tir_view_expression_payloads_with_visited(
    view: &TirView<'_>,
    visitor: &mut impl FnMut(&Expression) -> Result<(), CompilerError>,
    visited_templates: &mut HashSet<(TemplateIrId, TemplateTirPhase, TemplateOverlaySetId)>,
) -> Result<(), CompilerError> {
    let identity = (view.root_ref(), view.phase(), view.overlay_set_id());
    if !visited_templates.insert(identity) {
        return Ok(());
    }

    let root_node_id = {
        let root_template = view.root_template()?;
        root_template.root
    };
    let root_node_ref = root_node_id;

    walk_tir_view_expression_payload_node(view, root_node_ref, visitor, visited_templates)
}

/// Walks every expression payload reachable from `expression`, including nested
/// `ExpressionKind` internals and template-valued TIR views, using one shared
/// visited set keyed by `(TemplateIrId, TemplateTirPhase, TemplateOverlaySetId)`.
///
/// WHAT: starts from an AST expression, recursively inspects `ExpressionKind`
///       internals (`Runtime` operands, `Coerced` values), and enters the
///       effective TIR view for each `ExpressionKind::Template` encountered.
///       TIR view expression payloads are likewise inspected for nested
///       template-valued expressions. One visited set prevents infinite
///       recursion across both expression-kind and TIR-view paths. The visitor
///       receives every expression that is not a `Template`, `Runtime`, or
///       `Coerced` wrapper, including `RuntimeSlotApplicationHandoff` payloads.
/// WHY: centralizes the store-aware predicate traversal so the head parser
///      does not duplicate `ExpressionKind` recursion or maintain its own
///      effective-template visited set.
pub(crate) fn walk_expression_payloads_with_nested_tir_views(
    expression: &Expression,
    store: &TemplateIrStore,
    visitor: &mut impl FnMut(&Expression) -> Result<(), CompilerError>,
) -> Result<(), CompilerError> {
    let mut visited_templates = HashSet::new();
    let mut pending_template_views: Vec<TemplateTirReference> = Vec::new();

    inspect_nested_expression_kind(expression, visitor, &mut pending_template_views)?;

    drain_pending_template_views(
        store,
        visitor,
        &mut visited_templates,
        &mut pending_template_views,
    )
}

/// Processes each template reference discovered in nested expression kinds.
///
/// WHAT: for each pending `TemplateTirReference`, checks the shared visited
///       set, validates the module-local reference, creates a `TirView`, and walks
///       its expression payloads while collecting further nested template
///       references.
/// WHY: using a worklist instead of immediate re-entry avoids borrow conflicts
///      between the TIR view walker (which holds `&mut visited_templates`) and
///      the nested-expression inspector (which needs to push new pending
///      references). Traversal order is not part of this predicate-oriented
///      API, while coverage and one-set cycle semantics match the original
///      head-parser recursion.
fn drain_pending_template_views(
    store: &TemplateIrStore,
    visitor: &mut impl FnMut(&Expression) -> Result<(), CompilerError>,
    visited_templates: &mut HashSet<(TemplateIrId, TemplateTirPhase, TemplateOverlaySetId)>,
    pending_template_views: &mut Vec<TemplateTirReference>,
) -> Result<(), CompilerError> {
    while let Some(reference) = pending_template_views.pop() {
        let identity = (reference.root, reference.phase, reference.overlay_set_id);
        if visited_templates.contains(&identity) {
            continue;
        }

        let view = TirView::new(
            store,
            reference.root,
            reference.phase,
            reference.overlay_set_id,
        )?;

        let mut expression_visitor = |expression: &Expression| {
            inspect_nested_expression_kind(expression, visitor, pending_template_views)
        };
        walk_tir_view_expression_payloads_with_visited(
            &view,
            &mut expression_visitor,
            visited_templates,
        )?;
    }

    Ok(())
}

/// Recursively inspects `ExpressionKind` internals, collecting template
/// references and calling `visitor` for all other expression kinds.
///
/// WHAT: descends into `Runtime` operands and `Coerced` values, pushes
///       `ExpressionKind::Template` references to the pending list, and passes
///       every other kind (including `RuntimeSlotApplicationHandoff`) to the
///       visitor. Does not access the visited set; cycle prevention is handled
///       by the caller when draining pending references.
/// WHY: matches the `ExpressionKind` recursion previously duplicated in the
///      head parser so the central walker owns both TIR structural traversal
///      and nested expression inspection.
fn inspect_nested_expression_kind(
    expression: &Expression,
    visitor: &mut impl FnMut(&Expression) -> Result<(), CompilerError>,
    pending_template_views: &mut Vec<TemplateTirReference>,
) -> Result<(), CompilerError> {
    match &expression.kind {
        ExpressionKind::Template(template) => {
            pending_template_views.push(template.tir_reference.clone());
            Ok(())
        }

        ExpressionKind::Runtime(rpn) => {
            for item in &rpn.items {
                if let ExpressionRpnItem::Operand(operand) = item {
                    inspect_nested_expression_kind(operand, visitor, pending_template_views)?;
                }
            }
            Ok(())
        }

        ExpressionKind::Coerced { value, .. } => {
            inspect_nested_expression_kind(value, visitor, pending_template_views)
        }

        _ => visitor(expression),
    }
}

fn walk_tir_view_expression_payload_node(
    view: &TirView<'_>,
    node_ref: TemplateIrNodeId,
    visitor: &mut impl FnMut(&Expression) -> Result<(), CompilerError>,
    visited_templates: &mut HashSet<(TemplateIrId, TemplateTirPhase, TemplateOverlaySetId)>,
) -> Result<(), CompilerError> {
    let node = view.effective_node(node_ref)?;

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            let children = children.clone();
            for child in children {
                walk_tir_view_expression_payload_node(view, child, visitor, visited_templates)?;
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
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            let branches = branches.clone();
            let fallback = *fallback;
            for branch in &branches {
                let expression = view
                    .effective_expression_for_site(branch.selector_site_id)?
                    .unwrap_or(branch.condition_expression());
                visitor(expression)?;
                walk_tir_view_expression_payload_node(
                    view,
                    branch.body,
                    visitor,
                    visited_templates,
                )?;
            }
            if let Some(fallback_id) = fallback {
                walk_tir_view_expression_payload_node(
                    view,
                    fallback_id,
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
            visit_loop_header_effective_expressions(view, &header, header_sites, visitor)?;
            walk_tir_view_expression_payload_node(view, body, visitor, visited_templates)?;
            if let Some(wrapper_id) = aggregate_wrapper {
                walk_tir_view_expression_payload_node(
                    view,
                    wrapper_id,
                    visitor,
                    visited_templates,
                )?;
            }
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            let reference = *reference;
            let effective_identity = (reference.root, reference.phase, reference.overlay_set_id);
            if visited_templates.insert(effective_identity) {
                let child_view =
                    view.child_view(reference.root, reference.phase, reference.overlay_set_id)?;
                let child_root_node_id = {
                    let child_root_template = child_view.root_template()?;
                    child_root_template.root
                };
                let child_root_node_ref = child_root_node_id;

                // Child references still carry a complete effective view
                // identity. Follow that view through the store rather than
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
            let insert_root = template_id;
            let effective_identity = (insert_root, view.phase(), view.overlay_set_id());
            if visited_templates.insert(effective_identity) {
                // Insert contributions inherit the parent phase and overlay set,
                // so they recurse through a child `TirView` instead of a raw
                // same-store walk. A missing insert template or overlay set is an
                // explicit internal error from `child_view` / `root_template`.
                let insert_view =
                    view.child_view(insert_root, view.phase(), view.overlay_set_id())?;
                let insert_root_node_id = {
                    let insert_root_template = insert_view.root_template()?;
                    insert_root_template.root
                };
                let insert_root_node_ref = insert_root_node_id;

                walk_tir_view_expression_payload_node(
                    &insert_view,
                    insert_root_node_ref,
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
/// WHY: focused walker tests compare raw structural coverage with the effective
///      production collector without exposing collector internals.
#[cfg(test)]
pub(crate) fn collect_tir_expression_overlay_payloads(
    store: &TemplateIrStore,
    template_id: TemplateIrId,
) -> Result<Vec<(ExpressionSiteId, Expression)>, CompilerError> {
    let mut collector = ExpressionOverlayPayloadCollector::new(store);
    collector.collect_template(template_id)?;
    Ok(collector.into_payloads())
}

/// Collects effective expression payloads from one template root.
///
/// WHAT: traverses the same structural coverage as
///       [`collect_tir_expression_overlay_payloads`] while resolving each site
///       through a root-first overlay stack. Same-store child-template
///       references temporarily add their own overlay identity, and runtime
///       slot-plan roots retain the active owning-template stack.
/// WHY: finalization writes one new root overlay, but must normalize the
///      effective result of every earlier root and child overlay rather than
///      replacing child-specific expressions with stale structural payloads.
pub(crate) fn collect_effective_tir_expression_overlay_payloads(
    store: &TemplateIrStore,
    template_id: TemplateIrId,
    root_overlay_set_id: TemplateOverlaySetId,
) -> Result<Vec<(ExpressionSiteId, Expression)>, CompilerError> {
    let mut collector =
        ExpressionOverlayPayloadCollector::new_effective(store, root_overlay_set_id)?;
    collector.collect_template(template_id)?;
    Ok(collector.into_payloads())
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
                Ok(vec![TirExpressionWalkChild::Template(reference.root)])
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

#[cfg(test)]
enum TirExpressionWalkChild {
    Node(TemplateIrNodeId),
    Template(TemplateIrId),
}

enum ExpressionOverlayCollectionChild {
    Node(TemplateIrNodeId),
    Template {
        template_id: TemplateIrId,
        overlay_set_id: Option<TemplateOverlaySetId>,
    },
}

struct ExpressionOverlayPayloadCollector<'store> {
    store: &'store TemplateIrStore,
    overlay_set_stack: Vec<TemplateOverlaySetId>,
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
            overlay_set_stack: Vec::new(),
            payloads: Vec::new(),
            active_nodes: HashSet::new(),
            completed_nodes: HashSet::new(),
            active_templates: HashSet::new(),
            completed_templates: HashSet::new(),
            active_slot_plans: HashSet::new(),
            completed_slot_plans: HashSet::new(),
        }
    }

    fn new_effective(
        store: &'store TemplateIrStore,
        root_overlay_set_id: TemplateOverlaySetId,
    ) -> Result<Self, CompilerError> {
        store.overlay_set(root_overlay_set_id).ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "TIR expression overlay collection referenced missing root overlay set {}",
                root_overlay_set_id
            ))
        })?;

        let mut collector = Self::new(store);
        collector.overlay_set_stack.push(root_overlay_set_id);
        Ok(collector)
    }

    fn into_payloads(self) -> Vec<(ExpressionSiteId, Expression)> {
        self.payloads
    }

    fn effective_expression(
        &self,
        site_id: ExpressionSiteId,
        structural_expression: &Expression,
    ) -> Result<Expression, CompilerError> {
        if self.overlay_set_stack.is_empty() {
            return Ok(structural_expression.clone());
        }

        Ok(self
            .store
            .expression_for_overlay_stack(&self.overlay_set_stack, site_id)?
            .cloned()
            .unwrap_or_else(|| structural_expression.clone()))
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
                        ExpressionOverlayCollectionChild::Node(node_id) => {
                            self.collect_node(node_id)?;
                        }
                        ExpressionOverlayCollectionChild::Template {
                            template_id,
                            overlay_set_id,
                        } => {
                            if let Some(overlay_set_id) = overlay_set_id {
                                self.overlay_set_stack.push(overlay_set_id);
                            }
                            let result = self.collect_template(template_id);
                            if overlay_set_id.is_some() {
                                self.overlay_set_stack.pop();
                            }
                            result?;
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
    ) -> Result<Vec<ExpressionOverlayCollectionChild>, CompilerError> {
        let node = self.store.get_node(node_id).ok_or_else(|| {
            CompilerError::compiler_error(
                "TIR expression overlay collection referenced a missing node.",
            )
        })?;

        match &node.kind {
            TemplateIrNodeKind::Sequence { children } => Ok(children
                .iter()
                .copied()
                .map(ExpressionOverlayCollectionChild::Node)
                .collect()),

            TemplateIrNodeKind::DynamicExpression {
                expression,
                site_id,
                ..
            } => {
                self.payloads
                    .push((*site_id, self.effective_expression(*site_id, expression)?));
                Ok(Vec::new())
            }

            TemplateIrNodeKind::BranchChain { branches, fallback } => {
                let mut children =
                    Vec::with_capacity(branches.len() + usize::from(fallback.is_some()));
                for branch in branches {
                    self.payloads.push((
                        branch.selector_site_id,
                        self.effective_expression(
                            branch.selector_site_id,
                            branch.condition_expression(),
                        )?,
                    ));
                    children.push(ExpressionOverlayCollectionChild::Node(branch.body));
                }

                if let Some(fallback) = fallback {
                    children.push(ExpressionOverlayCollectionChild::Node(*fallback));
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
                children.push(ExpressionOverlayCollectionChild::Node(*body));

                if let Some(wrapper) = aggregate_wrapper {
                    children.push(ExpressionOverlayCollectionChild::Node(*wrapper));
                }

                Ok(children)
            }

            TemplateIrNodeKind::ChildTemplate { reference, .. } => {
                Ok(vec![ExpressionOverlayCollectionChild::Template {
                    template_id: reference.root,
                    overlay_set_id: Some(reference.overlay_set_id),
                }])
            }

            TemplateIrNodeKind::InsertContribution { template } => {
                Ok(vec![ExpressionOverlayCollectionChild::Template {
                    template_id: *template,
                    overlay_set_id: None,
                }])
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
                self.payloads
                    .push((*site_id, self.effective_expression(*site_id, condition)?));
            }

            (
                TemplateLoopHeader::Range { range, .. },
                TemplateLoopHeaderExpressionSites::Range { start, end, step },
            ) => {
                self.payloads
                    .push((*start, self.effective_expression(*start, &range.start)?));
                self.payloads
                    .push((*end, self.effective_expression(*end, &range.end)?));

                match (step, &range.step) {
                    (Some(step_site_id), Some(step_expression)) => {
                        self.payloads.push((
                            *step_site_id,
                            self.effective_expression(*step_site_id, step_expression)?,
                        ));
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
                self.payloads
                    .push((*site_id, self.effective_expression(*site_id, iterable)?));
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
