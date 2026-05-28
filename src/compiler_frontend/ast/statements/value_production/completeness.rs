//! Branch-body control-flow completeness analysis.
//!
//! WHAT: determines whether a sequence of AST nodes falls through, produces values,
//! or terminates on all reachable paths.
//! WHY: value-producing blocks require every reachable path to end with `then`,
//! `return`, `return!`, or another guaranteed terminator. This helper gives catch
//! and future value blocks one shared branch-flow vocabulary.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::statements::value_production::types::BranchFlow;
use crate::compiler_frontend::datatypes::ids::TypeId;

/// Analyzes a body to determine its value-production or termination behavior.
///
/// WHAT: scans statements in order and returns the first non-fallthrough flow found.
/// WHY: this preserves the old catch-handler behavior where a single terminating
/// statement anywhere in the body was considered sufficient to prevent fallthrough.
/// Future value-block completeness may require stricter all-paths analysis.
pub fn analyze_branch_flow(body: &[AstNode]) -> BranchFlow {
    for statement in body {
        let flow = statement_flow(statement);
        if flow != BranchFlow::FallsThrough {
            return flow;
        }
    }

    BranchFlow::FallsThrough
}

/// Determines the control-flow effect of a single statement.
///
/// WHAT: inspects the AST node kind and delegates to recursive analysis for
/// compound statements (`if`, `match`).
/// WHY: simple statements have fixed behavior, but nested control flow requires
/// recursive analysis so that every path is checked.
fn statement_flow(statement: &AstNode) -> BranchFlow {
    match &statement.kind {
        NodeKind::ThenValue(_) => BranchFlow::ProducesValue,

        NodeKind::Return(_) | NodeKind::ReturnError(_) => BranchFlow::Terminates,

        NodeKind::If(_, then_body, Some(else_body)) => combine_branch_flows(
            analyze_branch_flow(then_body),
            analyze_branch_flow(else_body),
        ),

        // Without an `else`, the false path falls through even when the true path
        // produces or terminates.
        NodeKind::If(_, _, None) => BranchFlow::FallsThrough,

        NodeKind::Match {
            arms,
            default: maybe_default_body,
            ..
        } => {
            // Match parsing already enforces exhaustiveness for supported statement matches.
            // Fold from the first arm's real flow so all-producing matches stay producing.
            let mut arm_flows = arms.iter().map(|arm| analyze_branch_flow(&arm.body));
            let all_arms_flow = match arm_flows.next() {
                Some(first_flow) => arm_flows.fold(first_flow, combine_branch_flows),
                None => BranchFlow::FallsThrough,
            };

            match maybe_default_body {
                Some(default_body) => {
                    combine_branch_flows(all_arms_flow, analyze_branch_flow(default_body))
                }
                None => all_arms_flow,
            }
        }

        _ => BranchFlow::FallsThrough,
    }
}

/// Merges the flow of two branches into a single result.
///
/// WHAT: given the flow of a left and right branch, returns the unified flow.
/// WHY: branches only agree when they are both Terminates or both ProducesValue;
/// any mismatch means control can fall through on at least one path.
fn combine_branch_flows(left: BranchFlow, right: BranchFlow) -> BranchFlow {
    match (left, right) {
        // Both paths terminate → the combined construct terminates.
        (BranchFlow::Terminates, BranchFlow::Terminates) => BranchFlow::Terminates,

        // Both paths produce values → the combined construct produces values.
        (BranchFlow::ProducesValue, BranchFlow::ProducesValue) => BranchFlow::ProducesValue,

        // Any mismatch or fallthrough means the combined construct falls through.
        _ => BranchFlow::FallsThrough,
    }
}

/// Extracts the type of a single produced value from a body, if one exists.
///
/// WHAT: recursively scans for `ThenValue` on reachable paths, handling nested `if`.
/// WHY: inferred declarations need to determine the result type from value-producing
/// block bodies before the declaration's type is known.
///
/// Returns `None` if the body terminates without producing or if no `ThenValue` is found.
/// For nested `if`, returns the type only when all producing paths agree.
pub fn extract_single_produced_type(body: &[AstNode]) -> Option<TypeId> {
    for statement in body {
        match &statement.kind {
            NodeKind::ThenValue(produced_values) => {
                if produced_values.expressions.len() == 1 {
                    return Some(produced_values.expressions[0].type_id);
                }
                return None;
            }

            NodeKind::If(_, then_body, Some(else_body)) => {
                let then_type = extract_single_produced_type(then_body);
                let else_type = extract_single_produced_type(else_body);

                return match (then_type, else_type) {
                    (Some(t), Some(e)) => {
                        if t == e {
                            Some(t)
                        } else {
                            // Type mismatch between branches.
                            // Return one so the caller can diagnose.
                            Some(t)
                        }
                    }
                    (Some(t), None) => Some(t),
                    (None, Some(e)) => Some(e),
                    (None, None) => None,
                };
            }

            NodeKind::If(_, then_body, None) => {
                return extract_single_produced_type(then_body);
            }

            NodeKind::Match { arms, default, .. } => {
                let mut found_type: Option<TypeId> = None;
                for arm in arms {
                    if let Some(arm_type) = extract_single_produced_type(&arm.body) {
                        if let Some(existing) = found_type {
                            if existing != arm_type {
                                return Some(existing);
                            }
                        } else {
                            found_type = Some(arm_type);
                        }
                    }
                }
                if let Some(default_body) = default
                    && let Some(default_type) = extract_single_produced_type(default_body)
                {
                    if let Some(existing) = found_type {
                        if existing != default_type {
                            return Some(existing);
                        }
                    } else {
                        found_type = Some(default_type);
                    }
                }
                return found_type;
            }

            NodeKind::Return(_) | NodeKind::ReturnError(_) => return None,

            _ => {}
        }
    }

    None
}
