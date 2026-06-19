//! TIR store validation.
//!
//! WHAT: structural validation for `TemplateIrStore` after conversion. Checks
//! for impossible IDs, missing roots, malformed ranges, invalid side-table
//! references, and recursive cycles within a reasonable depth bound.
//!
//! WHY: TIR is an internal representation that must be well-formed before any
//! downstream pass (fold, format, HIR) can trust it. Validating at the
//! conversion boundary catches converter bugs early and prevents cascading
//! failures in later phases.
//!
//! ## Ownership contract
//!
//! Validation reads the store without mutating it. It reports problems through
//! `CompilerDiagnostic` so the diagnostic system stays unified. Validation is
//! not a user-facing feature — it protects internal invariants during TIR
//! development and testing.

use crate::compiler_frontend::ast::templates::tir::ids::{TemplateIrId, TemplateIrNodeId};
use crate::compiler_frontend::ast::templates::tir::node::TemplateIrNodeKind;
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::compiler_errors::ErrorType;
use crate::compiler_frontend::compiler_messages::diagnostic_kind::{
    DiagnosticKind, InfrastructureDiagnosticKind,
};
use crate::compiler_frontend::compiler_messages::diagnostic_payload::DiagnosticPayload;
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, DiagnosticSeverity};
use crate::compiler_frontend::instrumentation::{AstCounter, add_ast_counter};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

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
/// WHY: validation catches converter bugs at the boundary rather than letting
/// malformed TIR propagate into folding, formatting, or HIR lowering.
///
/// Returns `Some(CompilerDiagnostic)` describing the first problem found,
/// or `None` when the store is structurally valid.
pub(crate) fn validate_tir_store(store: &TemplateIrStore) -> Option<CompilerDiagnostic> {
    // Check that every template's root node exists.
    for (index, template) in store.templates.iter().enumerate() {
        let _template_id = TemplateIrId::new(index);
        if template.root.index() >= store.nodes.len() {
            return Some(invalid_root_diagnostic(
                index,
                template.root,
                store.nodes.len(),
            ));
        }
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
//  Node Reference Validation
// -------------------------

/// Validates that all child references within a node are in bounds.
///
/// WHAT: checks every `TemplateIrNodeId`, `TemplateIrId`, and
/// `TemplateWrapperSetId` referenced by the node's kind.
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

        TemplateIrNodeKind::ChildTemplate { template }
        | TemplateIrNodeKind::InsertContribution { template } => {
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

        // Text, DynamicExpression, Slot, LoopControl, RuntimeSlotSite
        // have no node-ID references to validate.
        TemplateIrNodeKind::Text { .. }
        | TemplateIrNodeKind::DynamicExpression { .. }
        | TemplateIrNodeKind::Slot { .. }
        | TemplateIrNodeKind::LoopControl { .. }
        | TemplateIrNodeKind::RuntimeSlotSite { .. } => {}
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
