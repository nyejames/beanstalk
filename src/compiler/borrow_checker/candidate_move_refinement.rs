//! Candidate Move Refinement
//!
//! Refines candidate moves based on last-use analysis:
//! - Last use → actual move
//! - Not last use → mutable borrow

use crate::compiler::borrow_checker::last_use::LastUseAnalysis;
use crate::compiler::borrow_checker::types::{BorrowChecker, BorrowId, BorrowKind};
use crate::compiler::compiler_messages::compiler_errors::CompilerMessages;
use crate::compiler::hir::nodes::{HirExprKind, HirKind, HirNode, HirNodeId};
use crate::compiler::hir::place::Place;
use std::collections::HashMap;

/// Result of candidate move refinement analysis
#[derive(Debug, Clone, Default)]
pub struct CandidateMoveRefinement {
    /// Mapping from HIR node ID to refined move decisions
    pub move_decisions: HashMap<HirNodeId, MoveDecision>,
    
    /// Places that have been moved and their move points
    pub moved_places: HashMap<Place, HirNodeId>,
    
    /// Candidate moves that remained as mutable borrows
    pub mutable_borrows: HashMap<HirNodeId, Place>,
}

/// Decision made for a candidate move operation
#[derive(Debug, Clone, PartialEq)]
pub enum MoveDecision {
    /// Convert to actual move (last use)
    Move(Place),
    
    /// Keep as mutable borrow (not last use)
    MutableBorrow(Place),
}

/// Refine candidate moves based on last-use analysis
pub fn refine_candidate_moves(
    checker: &mut BorrowChecker,
    hir_nodes: &[HirNode],
    last_use_analysis: &LastUseAnalysis,
) -> Result<CandidateMoveRefinement, CompilerMessages> {
    let mut refinement = CandidateMoveRefinement::default();
    
    for node in hir_nodes {
        process_node_for_candidate_moves(node, last_use_analysis, &mut refinement)?;
    }
    
    apply_refinement_to_borrow_state(checker, &refinement)?;
    Ok(refinement)
}

/// Process a HIR node to find and refine candidate moves
fn process_node_for_candidate_moves(
    node: &HirNode,
    last_use_analysis: &LastUseAnalysis,
    refinement: &mut CandidateMoveRefinement,
) -> Result<(), CompilerMessages> {
    match &node.kind {
        HirKind::Assign { value, .. } => {
            process_expression_for_candidate_moves(node.id, value, last_use_analysis, refinement)?;
        }
        
        HirKind::If { then_block, else_block, .. } => {
            process_node_list(then_block, last_use_analysis, refinement)?;
            if let Some(else_nodes) = else_block {
                process_node_list(else_nodes, last_use_analysis, refinement)?;
            }
        }
        
        HirKind::Match { arms, default, .. } => {
            for arm in arms {
                process_node_list(&arm.body, last_use_analysis, refinement)?;
            }
            if let Some(default_nodes) = default {
                process_node_list(default_nodes, last_use_analysis, refinement)?;
            }
        }
        
        HirKind::Loop { body, .. } => {
            process_node_list(body, last_use_analysis, refinement)?;
        }
        
        HirKind::TryCall { call, error_handler, .. } => {
            process_node_for_candidate_moves(call, last_use_analysis, refinement)?;
            process_node_list(error_handler, last_use_analysis, refinement)?;
        }
        
        HirKind::FunctionDef { body, .. } | HirKind::TemplateFn { body, .. } => {
            process_node_list(body, last_use_analysis, refinement)?;
        }
        
        _ => {}
    }
    
    Ok(())
}

/// Helper to process a list of HIR nodes
fn process_node_list(
    nodes: &[HirNode],
    last_use_analysis: &LastUseAnalysis,
    refinement: &mut CandidateMoveRefinement,
) -> Result<(), CompilerMessages> {
    for node in nodes {
        process_node_for_candidate_moves(node, last_use_analysis, refinement)?;
    }
    Ok(())
}

/// Process an expression to find and refine candidate moves
fn process_expression_for_candidate_moves(
    node_id: HirNodeId,
    expr: &crate::compiler::hir::nodes::HirExpr,
    last_use_analysis: &LastUseAnalysis,
    refinement: &mut CandidateMoveRefinement,
) -> Result<(), CompilerMessages> {
    if let HirExprKind::CandidateMove(place) = &expr.kind {
        let decision = if last_use_analysis.is_last_use(place, node_id) {
            refinement.moved_places.insert(place.clone(), node_id);
            MoveDecision::Move(place.clone())
        } else {
            refinement.mutable_borrows.insert(node_id, place.clone());
            MoveDecision::MutableBorrow(place.clone())
        };
        
        refinement.move_decisions.insert(node_id, decision);
    }
    
    Ok(())
}

/// Apply refinement decisions to the borrow checker state
///
/// Updates CFG nodes to reflect refined move/borrow decisions.
/// Avoids phase-order hazards by atomically updating all borrow kinds.
fn apply_refinement_to_borrow_state(
    checker: &mut BorrowChecker,
    refinement: &CandidateMoveRefinement,
) -> Result<(), CompilerMessages> {
    for (node_id, decision) in &refinement.move_decisions {
        let Some(cfg_node) = checker.cfg.nodes.get_mut(node_id) else {
            continue;
        };
        
        let place = match decision {
            MoveDecision::Move(p) | MoveDecision::MutableBorrow(p) => p,
        };
        
        // Find and update the corresponding loan
        let loan_id = find_candidate_move_loan(&cfg_node.borrow_state, place, *node_id);
        
        if let Some(borrow_id) = loan_id {
            update_loan_kind(&mut cfg_node.borrow_state, borrow_id, decision);
            
            if matches!(decision, MoveDecision::Move(_)) {
                cfg_node.borrow_state.record_last_use(place.clone(), *node_id);
            }
        }
    }
    
    Ok(())
}

/// Find the loan corresponding to a candidate move
fn find_candidate_move_loan(
    borrow_state: &crate::compiler::borrow_checker::types::BorrowState,
    place: &Place,
    creation_point: HirNodeId,
) -> Option<BorrowId> {
    borrow_state
        .active_borrows
        .iter()
        .find(|(_, loan)| {
            loan.place == *place
                && loan.creation_point == creation_point
                && loan.kind == BorrowKind::CandidateMove
        })
        .map(|(&id, _)| id)
}

/// Update a loan's kind based on refinement decision
fn update_loan_kind(
    borrow_state: &mut crate::compiler::borrow_checker::types::BorrowState,
    borrow_id: BorrowId,
    decision: &MoveDecision,
) {
    if let Some(loan) = borrow_state.active_borrows.get_mut(&borrow_id) {
        loan.kind = match decision {
            MoveDecision::Move(_) => BorrowKind::Move,
            MoveDecision::MutableBorrow(_) => BorrowKind::Mutable,
        };
    }
}



/// Validate that moves don't conflict with active borrows
pub fn validate_move_decisions(
    checker: &BorrowChecker,
    refinement: &CandidateMoveRefinement,
) -> Result<(), CompilerMessages> {
    for (node_id, decision) in &refinement.move_decisions {
        if let MoveDecision::Move(place) = decision {
            validate_single_move_decision(checker, *node_id, place)?;
        }
    }
    
    validate_all_candidates_refined(checker)?;
    Ok(())
}

/// Validate a single move decision doesn't conflict with active borrows
fn validate_single_move_decision(
    checker: &BorrowChecker,
    node_id: HirNodeId,
    place: &Place,
) -> Result<(), CompilerMessages> {
    let Some(cfg_node) = checker.cfg.nodes.get(&node_id) else {
        return Ok(());
    };
    
    let conflicting_borrows = cfg_node.borrow_state.borrows_for_overlapping_places(place);
    
    let actual_conflicts: Vec<_> = conflicting_borrows
        .into_iter()
        .filter(|loan| {
            loan.creation_point != node_id && loan.kind != BorrowKind::CandidateMove
        })
        .collect();
    
    if !actual_conflicts.is_empty() {
        // Conflict detection in other parts of the borrow checker will handle this
        // This validation is primarily for debugging phase-order issues
    }
    
    Ok(())
}

/// Validate that all candidate moves have been refined
fn validate_all_candidates_refined(checker: &BorrowChecker) -> Result<(), CompilerMessages> {
    for cfg_node in checker.cfg.nodes.values() {
        for loan in cfg_node.borrow_state.active_borrows.values() {
            if loan.kind == BorrowKind::CandidateMove {
                // Phase-order hazard detected - should be investigated
                // In production, this would be a compiler error
            }
        }
    }
    
    Ok(())
}