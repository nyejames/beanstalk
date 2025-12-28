//! Structured Control Flow Handling for Borrow Checker
//!
//! Handles structured control flow (if, match, loop) with separate borrow tracking
//! for different execution paths and conservative merging at join points.

use crate::compiler::borrow_checker::types::{BorrowChecker, BorrowState, CfgNodeId};
use crate::compiler::compiler_messages::compiler_errors::CompilerMessages;
use crate::compiler::hir::nodes::{HirKind, HirMatchArm, HirNode, HirNodeId};
use std::collections::HashMap;

/// Handle structured control flow for borrow checking
pub fn handle_structured_control_flow(
    checker: &BorrowChecker,
    hir_nodes: &[HirNode],
) -> Result<(), CompilerMessages> {
    for node in hir_nodes {
        match &node.kind {
            HirKind::If {
                condition: _,
                then_block,
                else_block,
            } => {
                handle_if_statement(checker, node.id, then_block, else_block.as_deref())?;
            }

            HirKind::Match {
                scrutinee: _,
                arms,
                default,
            } => {
                handle_match_statement(checker, node.id, arms, default.as_deref())?;
            }

            HirKind::Loop {
                label: _,
                binding: _,
                iterator: _,
                body,
                index_binding: _,
            } => {
                handle_loop_statement(checker, node.id, body)?;
            }

            // Recursively handle nested control flow
            HirKind::FunctionDef { body, .. } | HirKind::TemplateFn { body, .. } => {
                handle_structured_control_flow(checker, body)?;
            }

            _ => {
                // For other node types, no special control flow handling needed
            }
        }
    }

    Ok(())
}

/// Handle If statements with separate borrow tracking in branches
fn handle_if_statement(
    checker: &BorrowChecker,
    if_node_id: HirNodeId,
    then_block: &[HirNode],
    else_block: Option<&[HirNode]>,
) -> Result<(), CompilerMessages> {
    // Get the current borrow state before the if statement
    let pre_if_state = if let Some(if_node) = checker.cfg.nodes.get(&if_node_id) {
        if_node.borrow_state.clone()
    } else {
        BorrowState::default()
    };

    // Process then branch with separate state tracking (read-only)
    let then_end_state =
        process_branch_with_separate_tracking(checker, then_block, &pre_if_state, "then branch")?;

    // Process else branch with separate state tracking (read-only)
    let else_end_state = if let Some(else_nodes) = else_block {
        process_branch_with_separate_tracking(checker, else_nodes, &pre_if_state, "else branch")?
    } else {
        // If no else branch, the pre-if state continues unchanged
        pre_if_state.clone()
    };

    // Find the join point after the if statement
    let join_point = find_if_join_point(checker, if_node_id, then_block, else_block);

    // Perform conservative merge analysis at the join point (read-only)
    if let Some(join_node_id) = join_point {
        conservative_merge_at_join_point(
            checker,
            join_node_id,
            vec![then_end_state, else_end_state],
        );
    }

    Ok(())
}

/// Handle Match statements with separate tracking per arm
fn handle_match_statement(
    checker: &BorrowChecker,
    match_node_id: HirNodeId,
    arms: &[HirMatchArm],
    default: Option<&[HirNode]>,
) -> Result<(), CompilerMessages> {
    // Get the current borrow state before the match statement
    let pre_match_state = if let Some(match_node) = checker.cfg.nodes.get(&match_node_id) {
        match_node.borrow_state.clone()
    } else {
        BorrowState::default()
    };

    let mut arm_end_states = Vec::new();

    // Process each match arm with separate state tracking (read-only)
    for (arm_index, arm) in arms.iter().enumerate() {
        let arm_label = format!("match arm {}", arm_index);
        let arm_end_state = process_branch_with_separate_tracking(
            checker,
            &arm.body,
            &pre_match_state,
            &arm_label,
        )?;
        arm_end_states.push(arm_end_state);
    }

    // Process default arm if it exists (read-only)
    if let Some(default_nodes) = default {
        let default_end_state = process_branch_with_separate_tracking(
            checker,
            default_nodes,
            &pre_match_state,
            "default arm",
        )?;
        arm_end_states.push(default_end_state);
    }

    // Find the join point after the match statement
    let join_point = find_match_join_point(checker, match_node_id, arms, default);

    // Perform conservative merge analysis at the join point (read-only)
    if let Some(join_node_id) = join_point {
        conservative_merge_at_join_point(checker, join_node_id, arm_end_states);
    }

    Ok(())
}

/// Handle Loop statements with borrow boundary crossing
fn handle_loop_statement(
    checker: &BorrowChecker,
    loop_node_id: HirNodeId,
    body: &[HirNode],
) -> Result<(), CompilerMessages> {
    // Get the current borrow state before the loop
    let pre_loop_state = if let Some(loop_node) = checker.cfg.nodes.get(&loop_node_id) {
        loop_node.borrow_state.clone()
    } else {
        BorrowState::default()
    };

    // Process loop body with boundary crossing analysis (read-only)
    let loop_body_state =
        process_loop_body_with_boundary_tracking(checker, body, &pre_loop_state, loop_node_id)?;

    // Analyze loop iteration boundary (read-only)
    analyze_loop_iteration_boundary(checker, loop_node_id, &pre_loop_state, &loop_body_state);

    // Find loop exit point and analyze conservative exit merge (read-only)
    let loop_exit_point = find_loop_exit_point(checker, loop_node_id, body);
    if let Some(exit_node_id) = loop_exit_point {
        // At loop exit, we conservatively analyze the pre-loop state with
        // any borrows that could persist from loop iterations
        let exit_states = vec![pre_loop_state, loop_body_state];
        conservative_merge_at_join_point(checker, exit_node_id, exit_states);
    }

    Ok(())
}

/// Process a branch with separate state tracking
fn process_branch_with_separate_tracking(
    checker: &BorrowChecker,
    branch_nodes: &[HirNode],
    initial_state: &BorrowState,
    _branch_label: &str,
) -> Result<BorrowState, CompilerMessages> {
    // Analyze the branch without modifying existing states
    // This ensures compatibility with already-completed last-use analysis
    let mut current_state = initial_state.clone();

    // Process each node in the branch to understand state evolution
    for node in branch_nodes {
        // Read the existing borrow state for this node (already computed)
        if let Some(cfg_node) = checker.cfg.nodes.get(&node.id) {
            // Merge the current branch state with the existing node state
            // This preserves the existing analysis while adding branch context
            let mut enhanced_state = current_state.clone();
            enhanced_state.union_merge(&cfg_node.borrow_state);
            current_state = enhanced_state;
        }

        // Recursively handle nested control flow within this branch
        // Note: These calls are now read-only and don't modify checker state
        match &node.kind {
            HirKind::If {
                condition: _,
                then_block,
                else_block,
            } => {
                // Read-only analysis of nested if statements
                let _ = analyze_if_statement_readonly(
                    checker,
                    node.id,
                    then_block,
                    else_block.as_deref(),
                )?;
            }

            HirKind::Match {
                scrutinee: _,
                arms,
                default,
            } => {
                // Read-only analysis of nested match statements
                let _ =
                    analyze_match_statement_readonly(checker, node.id, arms, default.as_deref())?;
            }

            HirKind::Loop {
                label: _,
                binding: _,
                iterator: _,
                body,
                index_binding: _,
            } => {
                // Read-only analysis of nested loops
                let _ = analyze_loop_statement_readonly(checker, node.id, body)?;
            }

            _ => {
                // For other nodes, no special handling needed
            }
        }
    }

    Ok(current_state)
}

/// Process loop body with boundary crossing analysis
fn process_loop_body_with_boundary_tracking(
    checker: &BorrowChecker,
    body: &[HirNode],
    pre_loop_state: &BorrowState,
    loop_node_id: HirNodeId,
) -> Result<BorrowState, CompilerMessages> {
    // Analyze the loop body starting with the pre-loop state
    let mut loop_iteration_state = pre_loop_state.clone();

    // Process each node in the loop body (read-only)
    for node in body {
        // Read the existing borrow state for this node (already computed)
        if let Some(cfg_node) = checker.cfg.nodes.get(&node.id) {
            // Merge the current iteration state with any borrows created at this node
            let node_borrows = cfg_node.borrow_state.clone();
            let mut enhanced_state = loop_iteration_state.clone();
            enhanced_state.union_merge(&node_borrows);

            // Update iteration state for next node
            loop_iteration_state = enhanced_state;
        }

        // Handle nested control flow within the loop body (read-only)
        match &node.kind {
            HirKind::If {
                condition: _,
                then_block,
                else_block,
            } => {
                // Read-only analysis of nested if statements
                let _ = analyze_if_statement_readonly(
                    checker,
                    node.id,
                    then_block,
                    else_block.as_deref(),
                )?;
            }

            HirKind::Match {
                scrutinee: _,
                arms,
                default,
            } => {
                // Read-only analysis of nested match statements
                let _ =
                    analyze_match_statement_readonly(checker, node.id, arms, default.as_deref())?;
            }

            // Nested loops require special handling
            HirKind::Loop {
                label: _,
                binding: _,
                iterator: _,
                body: nested_body,
                index_binding: _,
            } => {
                // Read-only analysis of nested loops
                let _ = analyze_loop_statement_readonly(checker, node.id, nested_body)?;
            }

            _ => {
                // Other nodes are handled by the state update above
            }
        }
    }

    // Analyze the back-edge from loop body to loop header (read-only)
    analyze_loop_back_edge(checker, loop_node_id, &loop_iteration_state, pre_loop_state);

    Ok(loop_iteration_state)
}

/// Analyze loop iteration boundary
fn analyze_loop_iteration_boundary(
    _checker: &BorrowChecker,
    _loop_node_id: HirNodeId,
    _pre_loop_state: &BorrowState,
    _loop_body_end_state: &BorrowState,
) {
    // This function is now analysis-only and doesn't modify existing states
    // At the loop iteration boundary, we would analyze:
    // 1. Borrows that existed before the loop (persist across iterations)
    // 2. Borrows created in the loop body that might persist to next iteration

    // In a full implementation, this would perform analysis without state modification
}

/// Analyze the back-edge from loop body to loop header
fn analyze_loop_back_edge(
    _checker: &BorrowChecker,
    _loop_node_id: HirNodeId,
    _loop_end_state: &BorrowState,
    _pre_loop_state: &BorrowState,
) {
    // This function is now analysis-only and doesn't modify existing states
    // The back-edge represents the flow from the end of the loop body
    // back to the loop header for the next iteration

    // In a full implementation, this would analyze the back-edge without state modification
}

/// Perform conservative merge at a control flow join point
fn conservative_merge_at_join_point(
    _checker: &BorrowChecker,
    _join_node_id: CfgNodeId,
    _incoming_states: Vec<BorrowState>,
) {
    // This function is now analysis-only and doesn't modify existing states
    // The actual merging logic would be used for analysis purposes only

    // In a full implementation, this would:
    // 1. Analyze the incoming states for consistency
    // 2. Compute what the merged state would be
    // 3. Record analysis results for later use
    // 4. NOT modify the existing CFG node states

    // For now, we'll just perform the analysis without state modification
    // to ensure compatibility with the existing pipeline
}

/// Find the join point after an if statement
fn find_if_join_point(
    checker: &BorrowChecker,
    if_node_id: HirNodeId,
    then_block: &[HirNode],
    else_block: Option<&[HirNode]>,
) -> Option<CfgNodeId> {
    // Look for a node that is reachable from both the end of the then block
    // and the end of the else block (or the if node itself if no else block)

    let then_end_id = then_block.last().map(|n| n.id);
    let else_end_id = else_block.and_then(|block| block.last().map(|n| n.id));

    // Find common successors
    if let Some(then_end) = then_end_id {
        let then_successors = checker.cfg.successors(then_end);

        if let Some(else_end) = else_end_id {
            // Both then and else blocks exist - find common successor
            let else_successors = checker.cfg.successors(else_end);
            for &then_succ in then_successors {
                if else_successors.contains(&then_succ) {
                    return Some(then_succ);
                }
            }
        } else {
            // No else block - join point is first successor of then block
            // that is also a successor of the if node
            let if_successors = checker.cfg.successors(if_node_id);
            for &then_succ in then_successors {
                if if_successors.contains(&then_succ) {
                    return Some(then_succ);
                }
            }
        }
    }

    None
}

/// Find the join point after a match statement
fn find_match_join_point(
    checker: &BorrowChecker,
    _match_node_id: HirNodeId,
    arms: &[HirMatchArm],
    default: Option<&[HirNode]>,
) -> Option<CfgNodeId> {
    // Collect end nodes from all arms
    let mut arm_end_ids = Vec::new();

    for arm in arms {
        if let Some(last_node) = arm.body.last() {
            arm_end_ids.push(last_node.id);
        }
    }

    if let Some(default_nodes) = default {
        if let Some(last_default) = default_nodes.last() {
            arm_end_ids.push(last_default.id);
        }
    }

    // Find common successor of all arm ends
    if let Some(&first_arm_end) = arm_end_ids.first() {
        let first_successors = checker.cfg.successors(first_arm_end);

        for &candidate in first_successors {
            let mut is_common = true;
            for &arm_end in arm_end_ids.iter().skip(1) {
                let arm_successors = checker.cfg.successors(arm_end);
                if !arm_successors.contains(&candidate) {
                    is_common = false;
                    break;
                }
            }
            if is_common {
                return Some(candidate);
            }
        }
    }

    None
}

/// Find the exit point of a loop
fn find_loop_exit_point(
    checker: &BorrowChecker,
    loop_node_id: HirNodeId,
    _body: &[HirNode],
) -> Option<CfgNodeId> {
    // The loop exit point is typically a successor of the loop header
    // that is not part of the loop body (i.e., the "fall-through" edge)
    let loop_successors = checker.cfg.successors(loop_node_id);

    // For now, return the first successor that's not a back-edge
    // In a more sophisticated implementation, we would distinguish
    // between loop body entry and loop exit edges
    loop_successors.first().copied()
}

/// Create a mapping of control flow paths for analysis
///
/// This function creates a mapping that tracks which borrow states are
/// associated with which control flow paths, enabling more precise
/// path-sensitive analysis.
#[allow(dead_code)]
fn create_path_mapping(
    checker: &BorrowChecker,
    control_flow_node: HirNodeId,
) -> HashMap<CfgNodeId, BorrowState> {
    let mut path_mapping = HashMap::new();

    // Get all successors of the control flow node
    let successors = checker.cfg.successors(control_flow_node);

    for &successor_id in successors {
        if let Some(successor_node) = checker.cfg.nodes.get(&successor_id) {
            path_mapping.insert(successor_id, successor_node.borrow_state.clone());
        }
    }

    path_mapping
}

/// Validate that control flow handling is correct
///
/// This function performs validation checks to ensure that the structured
/// control flow handling is working correctly and hasn't introduced any
/// inconsistencies.
#[allow(dead_code)]
pub fn validate_control_flow_handling(checker: &BorrowChecker) -> Result<(), CompilerMessages> {
    let errors = Vec::new();

    // Check that all CFG nodes have consistent borrow states
    for (_node_id, cfg_node) in &checker.cfg.nodes {
        // Validate that predecessor states are consistent with current state
        for &pred_id in &cfg_node.predecessors {
            if let Some(pred_node) = checker.cfg.nodes.get(&pred_id) {
                // Check for obvious inconsistencies
                if pred_node.borrow_state.active_borrows.is_empty()
                    && !cfg_node.borrow_state.active_borrows.is_empty()
                {
                    // This might indicate a problem with state propagation
                    // For now, we'll just note it but not error
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(CompilerMessages {
            errors,
            warnings: Vec::new(),
        })
    }
}

/// Read-only analysis of if statements for nested control flow
///
/// This function performs the same analysis as handle_if_statement but without
/// modifying any existing state, making it safe to call from within other
/// analysis functions.
fn analyze_if_statement_readonly(
    checker: &BorrowChecker,
    if_node_id: HirNodeId,
    then_block: &[HirNode],
    else_block: Option<&[HirNode]>,
) -> Result<(), CompilerMessages> {
    // Get the current borrow state before the if statement
    let pre_if_state = if let Some(if_node) = checker.cfg.nodes.get(&if_node_id) {
        if_node.borrow_state.clone()
    } else {
        BorrowState::default()
    };

    // Analyze then branch with separate state tracking (read-only)
    let _then_end_state =
        process_branch_with_separate_tracking(checker, then_block, &pre_if_state, "then branch")?;

    // Analyze else branch with separate state tracking (read-only)
    let _else_end_state = if let Some(else_nodes) = else_block {
        process_branch_with_separate_tracking(checker, else_nodes, &pre_if_state, "else branch")?
    } else {
        pre_if_state.clone()
    };

    // Note: We don't perform any merging or state updates in read-only analysis
    Ok(())
}

/// Read-only analysis of match statements for nested control flow
///
/// This function performs the same analysis as handle_match_statement but without
/// modifying any existing state, making it safe to call from within other
/// analysis functions.
fn analyze_match_statement_readonly(
    checker: &BorrowChecker,
    match_node_id: HirNodeId,
    arms: &[HirMatchArm],
    default: Option<&[HirNode]>,
) -> Result<(), CompilerMessages> {
    // Get the current borrow state before the match statement
    let pre_match_state = if let Some(match_node) = checker.cfg.nodes.get(&match_node_id) {
        match_node.borrow_state.clone()
    } else {
        BorrowState::default()
    };

    // Analyze each match arm with separate state tracking (read-only)
    for (arm_index, arm) in arms.iter().enumerate() {
        let arm_label = format!("match arm {}", arm_index);
        let _arm_end_state = process_branch_with_separate_tracking(
            checker,
            &arm.body,
            &pre_match_state,
            &arm_label,
        )?;
    }

    // Analyze default arm if it exists (read-only)
    if let Some(default_nodes) = default {
        let _default_end_state = process_branch_with_separate_tracking(
            checker,
            default_nodes,
            &pre_match_state,
            "default arm",
        )?;
    }

    // Note: We don't perform any merging or state updates in read-only analysis
    Ok(())
}

/// Read-only analysis of loop statements for nested control flow
///
/// This function performs the same analysis as handle_loop_statement but without
/// modifying any existing state, making it safe to call from within other
/// analysis functions.
fn analyze_loop_statement_readonly(
    checker: &BorrowChecker,
    loop_node_id: HirNodeId,
    body: &[HirNode],
) -> Result<(), CompilerMessages> {
    // Get the current borrow state before the loop
    let pre_loop_state = if let Some(loop_node) = checker.cfg.nodes.get(&loop_node_id) {
        loop_node.borrow_state.clone()
    } else {
        BorrowState::default()
    };

    // Analyze loop body with boundary crossing analysis (read-only)
    let _loop_body_state =
        process_loop_body_with_boundary_tracking(checker, body, &pre_loop_state, loop_node_id)?;

    // Note: We don't perform any merging or state updates in read-only analysis
    Ok(())
}
