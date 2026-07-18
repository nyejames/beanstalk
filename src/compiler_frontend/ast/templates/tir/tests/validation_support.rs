//! Test-only TIR store validation support.
//!
//! WHAT: structural validation for `TemplateIrStore` in focused TIR tests. Checks
//! for impossible IDs, missing roots, malformed ranges, invalid side-table
//! references, occurrence/site ID uniqueness and correspondence, and recursive
//! cycles within a reasonable depth bound.
//!
//! WHY: focused TIR tests need one structural invariant checker for malformed
//! stores so individual test cases can assert the relevant failure without
//! duplicating validation logic.
//!
//! ## Ownership contract
//!
//! Validation reads the store without mutating it. It reports problems through
//! `CompilerDiagnostic` so the diagnostic system stays unified. Validation is
//! not a user-facing feature. It protects internal invariants in the focused
//! TIR tests.

use crate::compiler_frontend::ast::templates::tir::ids::{
    ChildTemplateOccurrenceId, ExpressionSiteId, SlotOccurrenceId, TemplateIrId, TemplateIrNodeId,
    TemplateSlotPlanId, TemplateWrapperSetId,
};
use crate::compiler_frontend::ast::templates::tir::node::{
    TemplateIrNodeKind, TemplateLoopHeaderExpressionSites,
};
use crate::compiler_frontend::ast::templates::tir::refs::TemplateWrapperReference;
use crate::compiler_frontend::ast::templates::tir::slot_plan::TemplateSlotSiteRenderPiece;
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::compiler_errors::ErrorType;
use crate::compiler_frontend::compiler_messages::diagnostic_kind::{
    DiagnosticKind, InfrastructureDiagnosticKind,
};
use crate::compiler_frontend::compiler_messages::diagnostic_payload::DiagnosticPayload;
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, DiagnosticSeverity};
use crate::compiler_frontend::instrumentation::{AstCounter, add_ast_counter};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use std::collections::HashSet;

// -------------------------
//  Validation Constants
// -------------------------

/// Maximum tree depth before we declare a cycle.
///
/// WHAT: prevents infinite traversal when a node graph contains a back-edge.
/// WHY: realistic template trees are shallow (rarely exceeding 20 levels);
///      a depth of 1024 is generous enough for legitimate nesting while
///      catching real cycles early.
const MAX_CYCLE_DEPTH: usize = 1024;

// -------------------------
//  Public Validation Entry Point
// -------------------------

/// Validates the structural integrity of a TIR store.
///
/// WHAT: checks that every `TemplateIrId`, `TemplateIrNodeId`, and
/// `TemplateWrapperSetId` indexes a valid entry, every template's root node
/// exists, side-table references from nodes are in bounds, and no node tree
/// contains cycles within `MAX_CYCLE_DEPTH`.
///
/// WHY: validation catches construction defects before malformed TIR reaches
/// folding, formatting, or HIR lowering.
///
/// Returns `Some(CompilerDiagnostic)` describing the first problem found,
/// or `None` when the store is structurally valid.
pub(crate) fn validate_tir_store(store: &TemplateIrStore) -> Option<CompilerDiagnostic> {
    // Check that every template's root node exists and that any wrapper-set
    // reference points inside the wrapper_sets side vector.
    for (index, template) in store.templates.iter().enumerate() {
        let _template_id = TemplateIrId::new(index);
        if template.root.index() >= store.nodes.len() {
            return Some(invalid_root_diagnostic(
                index,
                template.root,
                store.nodes.len(),
            ));
        }
        if let Some(wrapper_set_id) = template.conditional_child_wrapper_set
            && wrapper_set_id.index() >= store.wrapper_sets.len()
        {
            return Some(out_of_bounds_wrapper_set_ref_diagnostic(
                TemplateIrId::new(index),
                wrapper_set_id,
                store.wrapper_sets.len(),
            ));
        }
        if let Some(slot_plan_id) = template.runtime_slot_plan
            && slot_plan_id.index() >= store.slot_plans.len()
        {
            return Some(out_of_bounds_slot_plan_ref_diagnostic(
                TemplateIrId::new(index),
                slot_plan_id,
                store.slot_plans.len(),
            ));
        }
    }

    // Check that every wrapper-set entry's template refs point to valid
    // templates in this store. Validation ensures the template ID is in bounds,
    // so out-of-bounds wrapper refs are caught here rather than during folding.
    if let Some(diagnostic) = validate_wrapper_sets(store) {
        return Some(diagnostic);
    }

    // Populated slot-plan side tables must keep source and site IDs indexed
    // consistently for runtime handoff consumers.
    if let Some(diagnostic) = validate_slot_plans(store) {
        return Some(diagnostic);
    }

    if let Some(diagnostic) = validate_node_reactive_subscriptions(store) {
        return Some(diagnostic);
    }

    // Check that slot occurrence IDs, child-template occurrence IDs, and
    // expression site IDs are unique within each reachable template root.
    if let Some(diagnostic) = validate_occurrence_and_site_ids(store) {
        return Some(diagnostic);
    }

    // Check that all node references are in bounds and acyclic.
    for node_index in 0..store.nodes.len() {
        let node_id = TemplateIrNodeId::new(node_index);
        add_ast_counter(AstCounter::TirValidationNodesVisited, 1);

        if let Some(diagnostic) = validate_node_references(store, node_id) {
            return Some(diagnostic);
        }
    }

    // Check cycle freedom by walking each template root with a visited set.
    for (index, template) in store.templates.iter().enumerate() {
        if let Some(diagnostic) = validate_no_cycles(store, template.root, index) {
            return Some(diagnostic);
        }
    }

    None
}

// -------------------------
//  Wrapper Set Validation
// -------------------------

/// Validates that every wrapper-set template ref points to a valid template.
///
/// WHAT: for each entry in the wrapper-set side table, checks that each local
/// `TemplateIrId` is in bounds.
fn validate_wrapper_sets(store: &TemplateIrStore) -> Option<CompilerDiagnostic> {
    for (set_index, wrapper_set) in store.wrapper_sets.iter().enumerate() {
        let wrapper_set_id = TemplateWrapperSetId::new(set_index);

        for reference in &wrapper_set.wrappers {
            if reference.root.index() >= store.templates.len() {
                return Some(out_of_bounds_wrapper_template_ref_diagnostic(
                    wrapper_set_id,
                    reference,
                    store.templates.len(),
                ));
            }
        }
    }

    None
}

// -------------------------
//  Slot Plan Validation
// -------------------------

/// Validates populated slot-plan side tables.
///
/// WHAT: checks that TIR-rendered contribution sources and slot-site plans are
/// internally coherent and reference valid TIR nodes.
/// WHY: validation protects the source/site ID contract consumed by runtime
/// handoff materialization.
fn validate_slot_plans(store: &TemplateIrStore) -> Option<CompilerDiagnostic> {
    for (index, slot_plan) in store.slot_plans.iter().enumerate() {
        let slot_plan_id = TemplateSlotPlanId::new(index);

        for source_plan in &slot_plan.contribution_sources {
            let Some(indexed_source) = slot_plan.contribution_sources.get(source_plan.source.0)
            else {
                return Some(slot_plan_side_table_diagnostic(
                    slot_plan_id,
                    format!(
                        "references contribution source {} but the TIR plan has {} sources",
                        source_plan.source.0,
                        slot_plan.contribution_sources.len()
                    ),
                ));
            };

            if indexed_source.source != source_plan.source {
                return Some(slot_plan_side_table_diagnostic(
                    slot_plan_id,
                    format!(
                        "contribution source index {} contains source {:?}",
                        source_plan.source.0, indexed_source.source
                    ),
                ));
            }

            if source_plan.render_root.index() >= store.nodes.len() {
                return Some(slot_plan_side_table_diagnostic(
                    slot_plan_id,
                    format!(
                        "contribution source {:?} references render root {} which is out of bounds",
                        source_plan.source, source_plan.render_root
                    ),
                ));
            }
        }

        for site_plan in &slot_plan.slot_sites {
            let Some(indexed_site) = slot_plan.slot_sites.get(site_plan.site.0) else {
                return Some(slot_plan_side_table_diagnostic(
                    slot_plan_id,
                    format!(
                        "references slot site {} but the TIR plan has {} sites",
                        site_plan.site.0,
                        slot_plan.slot_sites.len()
                    ),
                ));
            };

            if indexed_site.site != site_plan.site {
                return Some(slot_plan_side_table_diagnostic(
                    slot_plan_id,
                    format!(
                        "slot-site index {} contains site {:?}",
                        site_plan.site.0, indexed_site.site
                    ),
                ));
            }

            for piece in &site_plan.render_plan.pieces {
                match piece {
                    TemplateSlotSiteRenderPiece::Render(render_root) => {
                        if render_root.index() >= store.nodes.len() {
                            return Some(slot_plan_side_table_diagnostic(
                                slot_plan_id,
                                format!(
                                    "slot site {:?} references render root {} which is out of bounds",
                                    site_plan.site, render_root
                                ),
                            ));
                        }
                    }

                    TemplateSlotSiteRenderPiece::ContributionSource(source_id) => {
                        let Some(source_plan) = slot_plan.contribution_sources.get(source_id.0)
                        else {
                            return Some(slot_plan_side_table_diagnostic(
                                slot_plan_id,
                                format!(
                                    "slot site {:?} references contribution source {} but the TIR plan has {} sources",
                                    site_plan.site,
                                    source_id.0,
                                    slot_plan.contribution_sources.len()
                                ),
                            ));
                        };

                        if source_plan.source != *source_id {
                            return Some(slot_plan_side_table_diagnostic(
                                slot_plan_id,
                                format!(
                                    "slot site {:?} references contribution source {} but that index stores {:?}",
                                    site_plan.site, source_id.0, source_plan.source
                                ),
                            ));
                        }
                    }
                }
            }
        }
    }

    None
}

// -------------------------
//  Node Metadata Validation
// -------------------------

/// Validates node-indexed reactive subscription metadata.
///
/// WHAT: checks that the side-table has one entry per TIR node and that
/// populated entries attach only to text nodes.
/// WHY: the side-table is intentionally node-indexed so `Text` can stay a small
/// payload. Validation keeps that compact representation from becoming a loose
/// parallel metadata path.
fn validate_node_reactive_subscriptions(store: &TemplateIrStore) -> Option<CompilerDiagnostic> {
    if store.node_reactive_subscriptions.len() != store.nodes.len() {
        return Some(node_reactive_subscription_diagnostic(format!(
            "has {} reactive subscription entries for {} nodes",
            store.node_reactive_subscriptions.len(),
            store.nodes.len()
        )));
    }

    for (index, subscription) in store.node_reactive_subscriptions.iter().enumerate() {
        if subscription.is_some()
            && !matches!(store.nodes[index].kind, TemplateIrNodeKind::Text { .. })
        {
            return Some(node_reactive_subscription_diagnostic(format!(
                "attaches reactive subscription metadata to non-text node {}",
                TemplateIrNodeId::new(index)
            )));
        }
    }

    None
}

// -------------------------
//  Occurrence and Site ID Validation
// -------------------------

/// Validates that occurrence and site IDs are unique inside each reachable
/// template root.
///
/// WHAT: walks each template root and checks three invariants:
/// - `SlotOccurrenceId` values on `Slot` nodes are unique within that root.
/// - `ChildTemplateOccurrenceId` values on `ChildTemplate` nodes are unique
///   within that root.
/// - `ExpressionSiteId` values across `DynamicExpression` nodes, branch
///   selectors in `BranchChain` nodes, and loop-header expression sites are
///   unique within that root (they share one document-order counter).
///
/// WHY: occurrence and site IDs are the stable keys that overlay phases use to
/// address specific splice sites, slot boundaries, and child-template
/// boundaries. Duplicate IDs would make overlay resolution ambiguous and cause
/// the wrong expression override or slot resolution to apply inside an
/// effective view. Uniqueness is checked per reachable template root rather
/// than across the whole append-only store because derived roots can leave
/// older structural nodes behind or preserve IDs in a replacement tree while
/// the original root remains separately valid. Contiguity is intentionally not
/// required because structural sharing across derived roots may produce
/// non-contiguous ID sequences.
///
/// Returns `Some(CompilerDiagnostic)` describing the first duplicate found,
/// or `None` when all occurrence and site IDs are unique.
fn validate_occurrence_and_site_ids(store: &TemplateIrStore) -> Option<CompilerDiagnostic> {
    for (template_index, template) in store.templates.iter().enumerate() {
        let mut state = OccurrenceSiteValidationState::default();
        let mut visited_nodes = HashSet::new();

        if let Some(diagnostic) = validate_occurrence_and_site_ids_from_node(
            store,
            template.root,
            TemplateIrId::new(template_index),
            &mut state,
            &mut visited_nodes,
        ) {
            return Some(diagnostic);
        }
    }

    None
}

#[derive(Default)]
struct OccurrenceSiteValidationState {
    seen_slot_occurrences: HashSet<SlotOccurrenceId>,
    seen_child_template_occurrences: HashSet<ChildTemplateOccurrenceId>,
    seen_expression_sites: HashSet<ExpressionSiteId>,
}

fn validate_occurrence_and_site_ids_from_node(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
    template_id: TemplateIrId,
    state: &mut OccurrenceSiteValidationState,
    visited_nodes: &mut HashSet<TemplateIrNodeId>,
) -> Option<CompilerDiagnostic> {
    if node_id.index() >= store.nodes.len() {
        // Out-of-bounds references are reported by reference validation.
        return None;
    }

    if !visited_nodes.insert(node_id) {
        // Cycles are reported by cycle validation. Avoid looping here so the
        // uniqueness check stays focused on IDs.
        return None;
    }

    let node = store.get_node(node_id)?;

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            let mut found = None;
            for child_id in children {
                if let Some(diagnostic) = validate_occurrence_and_site_ids_from_node(
                    store,
                    *child_id,
                    template_id,
                    state,
                    visited_nodes,
                ) {
                    found = Some(diagnostic);
                    break;
                }
            }
            found
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            let mut found = None;

            for branch in branches {
                if !state.seen_expression_sites.insert(branch.selector_site_id) {
                    found = Some(duplicate_expression_site_diagnostic(
                        template_id,
                        node_id,
                        branch.selector_site_id,
                    ));
                    break;
                }

                if let Some(diagnostic) = validate_occurrence_and_site_ids_from_node(
                    store,
                    branch.body,
                    template_id,
                    state,
                    visited_nodes,
                ) {
                    found = Some(diagnostic);
                    break;
                }
            }

            if found.is_none()
                && let Some(fallback_id) = fallback
            {
                found = validate_occurrence_and_site_ids_from_node(
                    store,
                    *fallback_id,
                    template_id,
                    state,
                    visited_nodes,
                );
            }

            found
        }

        TemplateIrNodeKind::Loop {
            header_sites,
            body,
            aggregate_wrapper,
            ..
        } => {
            for site_id in loop_header_expression_site_ids(header_sites) {
                if !state.seen_expression_sites.insert(site_id) {
                    return Some(duplicate_expression_site_diagnostic(
                        template_id,
                        node_id,
                        site_id,
                    ));
                }
            }

            if let Some(diagnostic) = validate_occurrence_and_site_ids_from_node(
                store,
                *body,
                template_id,
                state,
                visited_nodes,
            ) {
                return Some(diagnostic);
            }

            if let Some(aggregate_wrapper_id) = aggregate_wrapper {
                return validate_occurrence_and_site_ids_from_node(
                    store,
                    *aggregate_wrapper_id,
                    template_id,
                    state,
                    visited_nodes,
                );
            }

            None
        }

        TemplateIrNodeKind::Slot { placeholder } => {
            let occurrence_id = placeholder.occurrence_id;
            if !state.seen_slot_occurrences.insert(occurrence_id) {
                Some(duplicate_slot_occurrence_diagnostic(
                    template_id,
                    node_id,
                    occurrence_id,
                ))
            } else {
                None
            }
        }

        TemplateIrNodeKind::ChildTemplate { occurrence_id, .. } => {
            if !state.seen_child_template_occurrences.insert(*occurrence_id) {
                Some(duplicate_child_template_occurrence_diagnostic(
                    template_id,
                    node_id,
                    *occurrence_id,
                ))
            } else {
                None
            }
        }

        TemplateIrNodeKind::DynamicExpression { site_id, .. } => {
            if !state.seen_expression_sites.insert(*site_id) {
                Some(duplicate_expression_site_diagnostic(
                    template_id,
                    node_id,
                    *site_id,
                ))
            } else {
                None
            }
        }

        // Node kinds without occurrence or site IDs. `ChildTemplate` and
        // `InsertContribution` template refs are view boundaries, so this check
        // does not descend into referenced templates; each template root is
        // validated independently by the outer loop.
        TemplateIrNodeKind::Text { .. }
        | TemplateIrNodeKind::InsertContribution { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::LoopControl { .. }
        | TemplateIrNodeKind::RuntimeSlotSite { .. } => None,
    }
}

/// Collects every `ExpressionSiteId` carried by a `TemplateLoopHeaderExpressionSites`.
///
/// WHAT: matches the loop-header variant shape, yielding one site ID per
///       expression-bearing position (condition, range start/end/optional step,
///       collection iterable).
/// WHY: validation needs to check all loop-header site IDs for uniqueness
///      against the shared expression-site key space. Centralizing the
///      extraction here keeps the variant match in one place.
fn loop_header_expression_site_ids(
    header_sites: &TemplateLoopHeaderExpressionSites,
) -> Vec<ExpressionSiteId> {
    match header_sites {
        TemplateLoopHeaderExpressionSites::Conditional { condition } => vec![*condition],
        TemplateLoopHeaderExpressionSites::Range { start, end, step } => {
            let mut ids = vec![*start, *end];
            if let Some(step_id) = step {
                ids.push(*step_id);
            }
            ids
        }
        TemplateLoopHeaderExpressionSites::Collection { iterable } => vec![*iterable],
    }
}

// -------------------------
//  Node Reference Validation
// -------------------------

/// Validates that all child references within a node are in bounds.
///
/// WHAT: checks every `TemplateIrNodeId` and `TemplateIrId` referenced by the
/// node's kind. Template-level wrapper-set references are validated before the
/// node walk.
/// WHY: out-of-bounds references would cause panics or silent data corruption
/// during downstream passes.
fn validate_node_references(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
) -> Option<CompilerDiagnostic> {
    let node = match store.get_node(node_id) {
        Some(node) => node,
        None => return Some(missing_node_diagnostic(node_id)),
    };

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            for &child_id in children {
                if child_id.index() >= store.nodes.len() {
                    return Some(out_of_bounds_node_ref_diagnostic(node_id, child_id));
                }
            }
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            if reference.root.index() >= store.templates.len() {
                return Some(out_of_bounds_template_ref_diagnostic(
                    node_id,
                    reference.root,
                ));
            }
        }

        TemplateIrNodeKind::InsertContribution { template } => {
            if template.index() >= store.templates.len() {
                return Some(out_of_bounds_template_ref_diagnostic(node_id, *template));
            }
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            for branch in branches {
                if branch.body.index() >= store.nodes.len() {
                    return Some(out_of_bounds_node_ref_diagnostic(node_id, branch.body));
                }
            }
            if let Some(fallback_id) = fallback
                && fallback_id.index() >= store.nodes.len()
            {
                return Some(out_of_bounds_node_ref_diagnostic(node_id, *fallback_id));
            }
        }

        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            if body.index() >= store.nodes.len() {
                return Some(out_of_bounds_node_ref_diagnostic(node_id, *body));
            }
            if let Some(agg_id) = aggregate_wrapper
                && agg_id.index() >= store.nodes.len()
            {
                return Some(out_of_bounds_node_ref_diagnostic(node_id, *agg_id));
            }
        }

        TemplateIrNodeKind::RuntimeSlotSite { plan, site } => {
            let Some(slot_plan) = store.get_slot_plan(*plan) else {
                return Some(out_of_bounds_runtime_slot_site_plan_diagnostic(
                    node_id,
                    *plan,
                    store.slot_plans.len(),
                ));
            };

            let matching_site = slot_plan
                .slot_sites
                .get(site.0)
                .is_some_and(|candidate| candidate.site == *site);
            if !matching_site {
                return Some(out_of_bounds_runtime_slot_site_diagnostic(
                    node_id,
                    *plan,
                    site.0,
                    slot_plan.slot_sites.len(),
                ));
            }
        }

        // Text, DynamicExpression, AggregateOutput, Slot, and LoopControl have
        // no node-ID references to validate.
        TemplateIrNodeKind::Text { .. }
        | TemplateIrNodeKind::DynamicExpression { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::Slot { .. }
        | TemplateIrNodeKind::LoopControl { .. } => {}
    }

    None
}

// -------------------------
//  Cycle Detection
// -------------------------

/// Validates that the node tree rooted at `root_id` contains no cycles.
///
/// WHAT: performs a depth-first traversal with a visited set, stopping at
/// `MAX_CYCLE_DEPTH` to prevent unbounded recursion.
/// WHY: cycles in TIR would cause infinite loops during folding, formatting,
/// or HIR lowering.
fn validate_no_cycles(
    store: &TemplateIrStore,
    root_id: TemplateIrNodeId,
    template_index: usize,
) -> Option<CompilerDiagnostic> {
    // Use a simple visited bitset indexed by node ID.
    let mut visited = vec![false; store.nodes.len()];
    let mut depth = 0usize;

    check_node_for_cycles(store, root_id, &mut visited, &mut depth, template_index)
}

/// Recursively checks a node and its children for cycles.
fn check_node_for_cycles(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
    visited: &mut [bool],
    depth: &mut usize,
    template_index: usize,
) -> Option<CompilerDiagnostic> {
    if node_id.index() >= store.nodes.len() {
        // Out-of-bounds will be caught by reference validation.
        return None;
    }

    if *depth > MAX_CYCLE_DEPTH {
        return Some(cycle_depth_diagnostic(template_index, node_id));
    }

    if visited[node_id.index()] {
        return Some(cycle_detected_diagnostic(template_index, node_id));
    }

    visited[node_id.index()] = true;
    *depth += 1;

    let node = match store.get_node(node_id) {
        Some(node) => node,
        None => {
            *depth -= 1;
            visited[node_id.index()] = false;
            return None;
        }
    };

    let result = match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            let mut found = None;
            for &child_id in children {
                if let Some(diagnostic) =
                    check_node_for_cycles(store, child_id, visited, depth, template_index)
                {
                    found = Some(diagnostic);
                    break;
                }
            }
            found
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            let mut found = None;
            for branch in branches {
                if let Some(diagnostic) =
                    check_node_for_cycles(store, branch.body, visited, depth, template_index)
                {
                    found = Some(diagnostic);
                    break;
                }
            }
            if found.is_none()
                && let Some(fallback_id) = fallback
            {
                found = check_node_for_cycles(store, *fallback_id, visited, depth, template_index);
            }
            found
        }

        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            if let Some(diagnostic) =
                check_node_for_cycles(store, *body, visited, depth, template_index)
            {
                return Some(diagnostic);
            }
            if let Some(agg_id) = aggregate_wrapper {
                return check_node_for_cycles(store, *agg_id, visited, depth, template_index);
            }
            None
        }

        // Leaf nodes — no children to traverse.
        TemplateIrNodeKind::Text { .. }
        | TemplateIrNodeKind::DynamicExpression { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::ChildTemplate { .. }
        | TemplateIrNodeKind::InsertContribution { .. }
        | TemplateIrNodeKind::Slot { .. }
        | TemplateIrNodeKind::LoopControl { .. }
        | TemplateIrNodeKind::RuntimeSlotSite { .. } => None,
    };

    *depth -= 1;
    visited[node_id.index()] = false;

    result
}

// -------------------------
//  Diagnostic Constructors
// -------------------------

/// Creates an infrastructure diagnostic for internal TIR validation failures.
///
/// WHAT: wraps a descriptive message in the infrastructure diagnostic kind
/// so the project's diagnostic system handles it consistently.
/// WHY: ad-hoc error types would break the unified diagnostic model.
fn tir_validation_diagnostic(msg: String) -> CompilerDiagnostic {
    CompilerDiagnostic::with_severity(
        DiagnosticKind::Infrastructure(InfrastructureDiagnosticKind::InfrastructureFailure),
        DiagnosticSeverity::Error,
        SourceLocation::default(),
        DiagnosticPayload::InfrastructureError {
            msg,
            error_type: ErrorType::Compiler,
            metadata: std::collections::HashMap::new(),
        },
    )
}

fn invalid_root_diagnostic(
    template_index: usize,
    root_id: TemplateIrNodeId,
    node_count: usize,
) -> CompilerDiagnostic {
    tir_validation_diagnostic(format!(
        "TIR validation: template {} has root node {} that is out of bounds (store has {} nodes)",
        template_index, root_id, node_count
    ))
}

fn missing_node_diagnostic(node_id: TemplateIrNodeId) -> CompilerDiagnostic {
    tir_validation_diagnostic(format!(
        "TIR validation: node {} not found in store during reference check",
        node_id
    ))
}

fn out_of_bounds_node_ref_diagnostic(
    parent_id: TemplateIrNodeId,
    child_id: TemplateIrNodeId,
) -> CompilerDiagnostic {
    tir_validation_diagnostic(format!(
        "TIR validation: node {} references child node {} which is out of bounds",
        parent_id, child_id
    ))
}

fn out_of_bounds_template_ref_diagnostic(
    node_id: TemplateIrNodeId,
    template_id: TemplateIrId,
) -> CompilerDiagnostic {
    tir_validation_diagnostic(format!(
        "TIR validation: node {} references template {} which is out of bounds",
        node_id, template_id
    ))
}

fn out_of_bounds_wrapper_template_ref_diagnostic(
    wrapper_set_id: TemplateWrapperSetId,
    reference: &TemplateWrapperReference,
    template_count: usize,
) -> CompilerDiagnostic {
    tir_validation_diagnostic(format!(
        "TIR validation: wrapper set {} references template {} which is out of bounds (store has {} templates)",
        wrapper_set_id, reference, template_count
    ))
}

fn out_of_bounds_wrapper_set_ref_diagnostic(
    template_id: TemplateIrId,
    wrapper_set_id: TemplateWrapperSetId,
    wrapper_set_count: usize,
) -> CompilerDiagnostic {
    tir_validation_diagnostic(format!(
        "TIR validation: template {} references wrapper set {} which is out of bounds (store has {} wrapper sets)",
        template_id, wrapper_set_id, wrapper_set_count
    ))
}

fn out_of_bounds_slot_plan_ref_diagnostic(
    template_id: TemplateIrId,
    slot_plan_id: TemplateSlotPlanId,
    slot_plan_count: usize,
) -> CompilerDiagnostic {
    tir_validation_diagnostic(format!(
        "TIR validation: template {} references slot plan {} which is out of bounds (store has {} slot plans)",
        template_id, slot_plan_id, slot_plan_count
    ))
}

fn out_of_bounds_runtime_slot_site_plan_diagnostic(
    node_id: TemplateIrNodeId,
    slot_plan_id: TemplateSlotPlanId,
    slot_plan_count: usize,
) -> CompilerDiagnostic {
    tir_validation_diagnostic(format!(
        "TIR validation: runtime slot site node {} references slot plan {} which is out of bounds (store has {} slot plans)",
        node_id, slot_plan_id, slot_plan_count
    ))
}

fn out_of_bounds_runtime_slot_site_diagnostic(
    node_id: TemplateIrNodeId,
    slot_plan_id: TemplateSlotPlanId,
    site_index: usize,
    site_count: usize,
) -> CompilerDiagnostic {
    tir_validation_diagnostic(format!(
        "TIR validation: runtime slot site node {} references site {} in slot plan {} but the plan has {} sites",
        node_id, site_index, slot_plan_id, site_count
    ))
}

fn slot_plan_side_table_diagnostic(
    slot_plan_id: TemplateSlotPlanId,
    detail: String,
) -> CompilerDiagnostic {
    tir_validation_diagnostic(format!(
        "TIR validation: slot plan {} side table {}",
        slot_plan_id, detail
    ))
}

fn node_reactive_subscription_diagnostic(detail: String) -> CompilerDiagnostic {
    tir_validation_diagnostic(format!(
        "TIR validation: node reactive subscription side table {}",
        detail
    ))
}

fn cycle_detected_diagnostic(
    template_index: usize,
    node_id: TemplateIrNodeId,
) -> CompilerDiagnostic {
    tir_validation_diagnostic(format!(
        "TIR validation: cycle detected at node {} in template {}",
        node_id, template_index
    ))
}

fn cycle_depth_diagnostic(template_index: usize, node_id: TemplateIrNodeId) -> CompilerDiagnostic {
    tir_validation_diagnostic(format!(
        "TIR validation: tree depth exceeded {} at node {} in template {} — possible cycle",
        MAX_CYCLE_DEPTH, node_id, template_index
    ))
}

fn duplicate_slot_occurrence_diagnostic(
    template_id: TemplateIrId,
    node_id: TemplateIrNodeId,
    occurrence_id: SlotOccurrenceId,
) -> CompilerDiagnostic {
    tir_validation_diagnostic(format!(
        "TIR validation: duplicate slot occurrence {} at node {} in template {} — slot occurrence IDs must be unique within each reachable template root",
        occurrence_id, node_id, template_id
    ))
}

fn duplicate_child_template_occurrence_diagnostic(
    template_id: TemplateIrId,
    node_id: TemplateIrNodeId,
    occurrence_id: ChildTemplateOccurrenceId,
) -> CompilerDiagnostic {
    tir_validation_diagnostic(format!(
        "TIR validation: duplicate child-template occurrence {} at node {} in template {} — child-template occurrence IDs must be unique within each reachable template root",
        occurrence_id, node_id, template_id
    ))
}

fn duplicate_expression_site_diagnostic(
    template_id: TemplateIrId,
    node_id: TemplateIrNodeId,
    site_id: ExpressionSiteId,
) -> CompilerDiagnostic {
    tir_validation_diagnostic(format!(
        "TIR validation: duplicate expression site {} at node {} in template {} — expression site IDs must be unique across dynamic-expression, branch-selector, and loop-header sites within each reachable template root",
        site_id, node_id, template_id
    ))
}
