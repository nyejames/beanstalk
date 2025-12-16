//! Borrow Tracking System
//!
//! This module implements the core borrow tracking functionality for the borrow checker.
//! It handles the creation, propagation, and management of borrows across the control
//! flow graph, enabling precise lifetime analysis and conflict detection.

use crate::compiler::borrow_checker::types::{BorrowChecker, BorrowKind, Loan};
use crate::compiler::compiler_messages::compiler_errors::CompilerMessages;
use crate::compiler::hir::nodes::{HirExprKind, HirKind, HirNode, HirNodeId};
use std::collections::HashSet;

/// Track borrows across the control flow graph
///
/// This function performs the main borrow tracking analysis, creating borrows
/// for Load and CandidateMove operations and propagating borrow state through
/// the CFG.
pub fn track_borrows(
    checker: &mut BorrowChecker,
    hir_nodes: &[HirNode],
) -> Result<(), CompilerMessages> {
    // Process each HIR node to create borrows
    for node in hir_nodes {
        process_node_for_borrows(checker, node)?;
    }

    // Propagate borrow state through the CFG
    propagate_borrow_state(checker)?;

    Ok(())
}

/// Process a single HIR node to create appropriate borrows
fn process_node_for_borrows(
    checker: &mut BorrowChecker,
    node: &HirNode,
) -> Result<(), CompilerMessages> {
    match &node.kind {
        HirKind::Assign { place: _, value } => {
            // Create borrows based on the value expression
            process_expression_for_borrows(checker, value, node.id)?;
        }

        HirKind::Borrow {
            place,
            kind,
            target: _,
        } => {
            // Explicit borrow creation
            let borrow_id = checker.next_borrow_id();
            let borrow_kind = match kind {
                crate::compiler::hir::nodes::BorrowKind::Shared => BorrowKind::Shared,
                crate::compiler::hir::nodes::BorrowKind::Mutable => BorrowKind::Mutable,
            };
            let loan = Loan::new(borrow_id, place.clone(), borrow_kind, node.id);

            // Add to CFG node's borrow state
            if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node.id) {
                cfg_node.borrow_state.add_borrow(loan);
            }
        }

        HirKind::Call { args, .. } | HirKind::HostCall { args, .. } => {
            // Create borrows for function arguments
            for arg_place in args {
                let borrow_id = checker.next_borrow_id();
                let loan = Loan::new(borrow_id, arg_place.clone(), BorrowKind::Shared, node.id);

                if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node.id) {
                    cfg_node.borrow_state.add_borrow(loan);
                }
            }
        }

        HirKind::If {
            condition,
            then_block,
            else_block,
        } => {
            // Create borrow for condition
            let borrow_id = checker.next_borrow_id();
            let loan = Loan::new(borrow_id, condition.clone(), BorrowKind::Shared, node.id);

            if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node.id) {
                cfg_node.borrow_state.add_borrow(loan);
            }

            // Process blocks recursively
            for then_node in then_block {
                process_node_for_borrows(checker, then_node)?;
            }

            if let Some(else_nodes) = else_block {
                for else_node in else_nodes {
                    process_node_for_borrows(checker, else_node)?;
                }
            }
        }

        HirKind::Match {
            scrutinee,
            arms,
            default,
        } => {
            // Create borrow for scrutinee
            let borrow_id = checker.next_borrow_id();
            let loan = Loan::new(borrow_id, scrutinee.clone(), BorrowKind::Shared, node.id);

            if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node.id) {
                cfg_node.borrow_state.add_borrow(loan);
            }

            // Process match arms
            for arm in arms {
                for arm_node in &arm.body {
                    process_node_for_borrows(checker, arm_node)?;
                }
            }

            // Process default arm
            if let Some(default_nodes) = default {
                for default_node in default_nodes {
                    process_node_for_borrows(checker, default_node)?;
                }
            }
        }

        HirKind::Loop { iterator, body, .. } => {
            // Create borrow for iterator
            let borrow_id = checker.next_borrow_id();
            let loan = Loan::new(borrow_id, iterator.clone(), BorrowKind::Shared, node.id);

            if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node.id) {
                cfg_node.borrow_state.add_borrow(loan);
            }

            // Process loop body
            for body_node in body {
                process_node_for_borrows(checker, body_node)?;
            }
        }

        HirKind::Return(places) => {
            // Create borrows for return values
            for return_place in places {
                let borrow_id = checker.next_borrow_id();
                let loan = Loan::new(borrow_id, return_place.clone(), BorrowKind::Shared, node.id);

                if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node.id) {
                    cfg_node.borrow_state.add_borrow(loan);
                }
            }
        }

        HirKind::ReturnError(place) => {
            // Create borrow for error return
            let borrow_id = checker.next_borrow_id();
            let loan = Loan::new(borrow_id, place.clone(), BorrowKind::Shared, node.id);

            if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node.id) {
                cfg_node.borrow_state.add_borrow(loan);
            }
        }

        HirKind::ExprStmt(place) => {
            // Create borrow for expression statement
            let borrow_id = checker.next_borrow_id();
            let loan = Loan::new(borrow_id, place.clone(), BorrowKind::Shared, node.id);

            if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node.id) {
                cfg_node.borrow_state.add_borrow(loan);
            }
        }

        // Other node types don't create borrows directly
        _ => {}
    }

    Ok(())
}

/// Process an expression to create appropriate borrows
fn process_expression_for_borrows(
    checker: &mut BorrowChecker,
    expr: &crate::compiler::hir::nodes::HirExpr,
    node_id: HirNodeId,
) -> Result<(), CompilerMessages> {
    match &expr.kind {
        HirExprKind::Load(place) => {
            // Create shared borrow for load
            let borrow_id = checker.next_borrow_id();
            let loan = Loan::new(borrow_id, place.clone(), BorrowKind::Shared, node_id);

            if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
                cfg_node.borrow_state.add_borrow(loan);
            }
        }

        HirExprKind::SharedBorrow(place) => {
            // Create shared borrow
            let borrow_id = checker.next_borrow_id();
            let loan = Loan::new(borrow_id, place.clone(), BorrowKind::Shared, node_id);

            if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
                cfg_node.borrow_state.add_borrow(loan);
            }
        }

        HirExprKind::MutableBorrow(place) => {
            // Create mutable borrow
            let borrow_id = checker.next_borrow_id();
            let loan = Loan::new(borrow_id, place.clone(), BorrowKind::Mutable, node_id);

            if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
                cfg_node.borrow_state.add_borrow(loan);
            }
        }

        HirExprKind::CandidateMove(place) => {
            // Create mutable borrow candidate (will be refined by last-use analysis)
            let borrow_id = checker.next_borrow_id();
            let loan = Loan::new(borrow_id, place.clone(), BorrowKind::Mutable, node_id);

            if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
                cfg_node.borrow_state.add_borrow(loan);
            }
        }

        HirExprKind::BinOp { left, right, .. } => {
            // Create borrows for binary operation operands
            let left_borrow_id = checker.next_borrow_id();
            let left_loan = Loan::new(left_borrow_id, left.clone(), BorrowKind::Shared, node_id);

            let right_borrow_id = checker.next_borrow_id();
            let right_loan = Loan::new(right_borrow_id, right.clone(), BorrowKind::Shared, node_id);

            if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
                cfg_node.borrow_state.add_borrow(left_loan);
                cfg_node.borrow_state.add_borrow(right_loan);
            }
        }

        HirExprKind::UnaryOp { operand, .. } => {
            // Create borrow for unary operation operand
            let borrow_id = checker.next_borrow_id();
            let loan = Loan::new(borrow_id, operand.clone(), BorrowKind::Shared, node_id);

            if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
                cfg_node.borrow_state.add_borrow(loan);
            }
        }

        HirExprKind::Call { args, .. } => {
            // Create borrows for function call arguments
            for arg_place in args {
                let borrow_id = checker.next_borrow_id();
                let loan = Loan::new(borrow_id, arg_place.clone(), BorrowKind::Shared, node_id);

                if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
                    cfg_node.borrow_state.add_borrow(loan);
                }
            }
        }

        HirExprKind::MethodCall { receiver, args, .. } => {
            // Create borrow for receiver
            let receiver_borrow_id = checker.next_borrow_id();
            let receiver_loan = Loan::new(
                receiver_borrow_id,
                receiver.clone(),
                BorrowKind::Shared,
                node_id,
            );

            if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
                cfg_node.borrow_state.add_borrow(receiver_loan);
            }

            // Create borrows for arguments
            for arg_place in args {
                let borrow_id = checker.next_borrow_id();
                let loan = Loan::new(borrow_id, arg_place.clone(), BorrowKind::Shared, node_id);

                if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
                    cfg_node.borrow_state.add_borrow(loan);
                }
            }
        }

        HirExprKind::StructConstruct { fields, .. } => {
            // Create borrows for struct field values
            for (_, field_place) in fields {
                let borrow_id = checker.next_borrow_id();
                let loan = Loan::new(borrow_id, field_place.clone(), BorrowKind::Shared, node_id);

                if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
                    cfg_node.borrow_state.add_borrow(loan);
                }
            }
        }

        HirExprKind::Collection(places) => {
            // Create borrows for collection elements
            for element_place in places {
                let borrow_id = checker.next_borrow_id();
                let loan = Loan::new(
                    borrow_id,
                    element_place.clone(),
                    BorrowKind::Shared,
                    node_id,
                );

                if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
                    cfg_node.borrow_state.add_borrow(loan);
                }
            }
        }

        HirExprKind::Range { start, end } => {
            // Create borrows for range bounds
            let start_borrow_id = checker.next_borrow_id();
            let start_loan = Loan::new(start_borrow_id, start.clone(), BorrowKind::Shared, node_id);

            let end_borrow_id = checker.next_borrow_id();
            let end_loan = Loan::new(end_borrow_id, end.clone(), BorrowKind::Shared, node_id);

            if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
                cfg_node.borrow_state.add_borrow(start_loan);
                cfg_node.borrow_state.add_borrow(end_loan);
            }
        }

        // Literal expressions don't create borrows
        HirExprKind::Int(_)
        | HirExprKind::Float(_)
        | HirExprKind::Bool(_)
        | HirExprKind::StringLiteral(_)
        | HirExprKind::Char(_) => {}
    }

    Ok(())
}

/// Propagate borrow state through the control flow graph
/// 
/// This function implements a worklist-based dataflow analysis to propagate
/// borrow state through the CFG. It handles:
/// - Forward propagation along CFG edges
/// - Conservative merging at join points (multiple predecessors)
/// - Iterative refinement until a fixed point is reached
fn propagate_borrow_state(checker: &mut BorrowChecker) -> Result<(), CompilerMessages> {
    // Use a worklist algorithm for dataflow analysis
    // This handles cycles in the CFG (from loops) correctly
    let mut work_list: Vec<HirNodeId> = checker.cfg.entry_points.clone();
    let mut iterations = 0;
    let max_iterations = checker.cfg.nodes.len() * 10; // Prevent infinite loops

    while let Some(node_id) = work_list.pop() {
        iterations += 1;
        if iterations > max_iterations {
            // Safety limit reached - this shouldn't happen with well-formed CFGs
            break;
        }

        // First, merge incoming borrow states from all predecessors
        let predecessors = checker.cfg.predecessors(node_id);
        
        if !predecessors.is_empty() {
            // Collect predecessor states
            let predecessor_states: Vec<_> = predecessors
                .iter()
                .filter_map(|&pred_id| {
                    checker.cfg.nodes.get(&pred_id).map(|n| n.borrow_state.clone())
                })
                .collect();

            // Merge all predecessor states into the current node
            if let Some(current_node) = checker.cfg.nodes.get_mut(&node_id) {
                // Start with the node's own borrows (created at this node)
                let own_borrows = current_node.borrow_state.clone();
                
                // Merge incoming states from predecessors
                for (i, pred_state) in predecessor_states.iter().enumerate() {
                    if i == 0 && own_borrows.is_empty() {
                        // First predecessor and no own borrows - use union merge
                        current_node.borrow_state.union_merge(pred_state);
                    } else {
                        // Multiple predecessors - use conservative merge (intersection)
                        // This implements Polonius-style analysis where conflicts
                        // are only errors if they exist on ALL incoming paths
                        current_node.borrow_state.merge(pred_state);
                    }
                }
                
                // Re-add own borrows that were created at this node
                for loan in own_borrows.active_borrows.values() {
                    if !current_node.borrow_state.active_borrows.contains_key(&loan.id) {
                        current_node.borrow_state.add_borrow(loan.clone());
                    }
                }
            }
        }

        // Get current node's borrow state after merging
        let current_state = if let Some(node) = checker.cfg.nodes.get(&node_id) {
            node.borrow_state.clone()
        } else {
            continue;
        };

        // Get successors first to avoid borrowing conflicts
        let successors: Vec<HirNodeId> = checker.cfg.successors(node_id).to_vec();

        // Propagate to successors
        for successor_id in successors {
            // Check if the successor's state would change
            let state_changed = if let Some(successor_node) = checker.cfg.nodes.get(&successor_id) {
                // Check if merging would change the state
                let old_count = successor_node.borrow_state.active_borrows.len();
                let mut test_state = successor_node.borrow_state.clone();
                test_state.union_merge(&current_state);
                test_state.active_borrows.len() != old_count
            } else {
                false
            };

            // If state would change, add to worklist for re-processing
            if state_changed && !work_list.contains(&successor_id) {
                work_list.push(successor_id);
            }
        }
    }

    Ok(())
}
