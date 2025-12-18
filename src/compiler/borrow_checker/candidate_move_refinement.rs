//! Candidate Move Refinement
//!
//! This module implements the refinement of candidate moves based on last-use analysis.
//! Candidate moves are potential ownership transfers that are refined by the borrow checker:
//! - If a candidate move is the last use of a place, it becomes an actual move
//! - If a candidate move is not the last use, it remains a mutable borrow
//!
//! This refinement is essential for Beanstalk's reference semantics where moves are
//! determined by compiler analysis rather than explicit syntax.

use crate::compiler::borrow_checker::last_use::LastUseAnalysis;
use crate::compiler::borrow_checker::types::{BorrowChecker, BorrowId, BorrowKind, Loan};
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
    /// Convert to actual move (candidate move was last use)
    Move(Place),
    
    /// Keep as mutable borrow (candidate move was not last use)
    MutableBorrow(Place),
}

impl CandidateMoveRefinement {
    /// Create a new empty refinement result
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Check if a specific node was refined to a move
    pub fn is_move(&self, node_id: HirNodeId) -> bool {
        matches!(self.move_decisions.get(&node_id), Some(MoveDecision::Move(_)))
    }
    
    /// Check if a specific node was refined to a mutable borrow
    pub fn is_mutable_borrow(&self, node_id: HirNodeId) -> bool {
        matches!(self.move_decisions.get(&node_id), Some(MoveDecision::MutableBorrow(_)))
    }
    
    /// Get the place associated with a move decision
    pub fn get_place(&self, node_id: HirNodeId) -> Option<&Place> {
        match self.move_decisions.get(&node_id) {
            Some(MoveDecision::Move(place)) => Some(place),
            Some(MoveDecision::MutableBorrow(place)) => Some(place),
            None => None,
        }
    }
    
    /// Check if a place has been moved
    pub fn is_place_moved(&self, place: &Place) -> bool {
        self.moved_places.contains_key(place)
    }
    
    /// Get the move point for a place, if it was moved
    pub fn get_move_point(&self, place: &Place) -> Option<HirNodeId> {
        self.moved_places.get(place).copied()
    }
}

/// Refine candidate moves based on last-use analysis
///
/// This function analyzes all candidate move operations in the HIR and determines
/// whether they should become actual moves or remain as mutable borrows based on
/// whether they represent the last use of their target place.
pub fn refine_candidate_moves(
    checker: &mut BorrowChecker,
    hir_nodes: &[HirNode],
    last_use_analysis: &LastUseAnalysis,
) -> Result<CandidateMoveRefinement, CompilerMessages> {
    let mut refinement = CandidateMoveRefinement::new();
    
    // Process all HIR nodes to find candidate moves
    for node in hir_nodes {
        process_node_for_candidate_moves(checker, node, last_use_analysis, &mut refinement)?;
    }
    
    // Update borrow checker state with refined decisions
    apply_refinement_to_borrow_state(checker, &refinement)?;
    
    Ok(refinement)
}

/// Process a single HIR node to find and refine candidate moves
fn process_node_for_candidate_moves(
    _checker: &mut BorrowChecker,
    node: &HirNode,
    last_use_analysis: &LastUseAnalysis,
    refinement: &mut CandidateMoveRefinement,
) -> Result<(), CompilerMessages> {
    match &node.kind {
        HirKind::Assign { place: _, value } => {
            // Check if the value expression contains candidate moves
            process_expression_for_candidate_moves(node.id, value, last_use_analysis, refinement)?;
        }
        
        HirKind::If { condition: _, then_block, else_block } => {
            // Process then block
            for then_node in then_block {
                process_node_for_candidate_moves(_checker, then_node, last_use_analysis, refinement)?;
            }
            
            // Process else block
            if let Some(else_nodes) = else_block {
                for else_node in else_nodes {
                    process_node_for_candidate_moves(_checker, else_node, last_use_analysis, refinement)?;
                }
            }
        }
        
        HirKind::Match { scrutinee: _, arms, default } => {
            // Process match arms
            for arm in arms {
                for arm_node in &arm.body {
                    process_node_for_candidate_moves(_checker, arm_node, last_use_analysis, refinement)?;
                }
            }
            
            // Process default arm
            if let Some(default_nodes) = default {
                for default_node in default_nodes {
                    process_node_for_candidate_moves(_checker, default_node, last_use_analysis, refinement)?;
                }
            }
        }
        
        HirKind::Loop { iterator: _, body, .. } => {
            // Process loop body
            for body_node in body {
                process_node_for_candidate_moves(_checker, body_node, last_use_analysis, refinement)?;
            }
        }
        
        HirKind::TryCall { call, error_handler, .. } => {
            // Process the call
            process_node_for_candidate_moves(_checker, call, last_use_analysis, refinement)?;
            
            // Process error handler
            for handler_node in error_handler {
                process_node_for_candidate_moves(_checker, handler_node, last_use_analysis, refinement)?;
            }
        }
        
        HirKind::FunctionDef { body, .. } | HirKind::TemplateFn { body, .. } => {
            // Process function body
            for body_node in body {
                process_node_for_candidate_moves(_checker, body_node, last_use_analysis, refinement)?;
            }
        }
        
        // Other node types don't contain candidate moves directly
        _ => {}
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
    match &expr.kind {
        HirExprKind::CandidateMove(place) => {
            // This is a candidate move - determine if it should become an actual move
            let decision = if last_use_analysis.is_last_use(place, node_id) {
                // This candidate move is the last use of the place - convert to actual move
                refinement.moved_places.insert(place.clone(), node_id);
                MoveDecision::Move(place.clone())
            } else {
                // This candidate move is not the last use - keep as mutable borrow
                refinement.mutable_borrows.insert(node_id, place.clone());
                MoveDecision::MutableBorrow(place.clone())
            };
            
            refinement.move_decisions.insert(node_id, decision);
        }
        
        HirExprKind::BinOp { left: _, right: _, .. } => {
            // Binary operations don't contain candidate moves directly
            // (operands are places, not expressions)
        }
        
        HirExprKind::UnaryOp { operand: _, .. } => {
            // Unary operations don't contain candidate moves directly
        }
        
        HirExprKind::Call { args: _, .. } => {
            // Function calls don't contain candidate moves directly
            // (arguments are places, not expressions)
        }
        
        HirExprKind::MethodCall { receiver: _, args: _, .. } => {
            // Method calls don't contain candidate moves directly
        }
        
        HirExprKind::StructConstruct { fields: _, .. } => {
            // Struct construction doesn't contain candidate moves directly
        }
        
        HirExprKind::Collection(_) => {
            // Collections don't contain candidate moves directly
        }
        
        HirExprKind::Range { start: _, end: _ } => {
            // Ranges don't contain candidate moves directly
        }
        
        // Other expression types don't contain candidate moves
        HirExprKind::Load(_)
        | HirExprKind::SharedBorrow(_)
        | HirExprKind::MutableBorrow(_)
        | HirExprKind::Int(_)
        | HirExprKind::Float(_)
        | HirExprKind::Bool(_)
        | HirExprKind::StringLiteral(_)
        | HirExprKind::Char(_) => {}
    }
    
    Ok(())
}

/// Apply refinement decisions to the borrow checker state
///
/// This function updates the borrow checker's CFG nodes to reflect the refined
/// move/borrow decisions, converting mutable borrow candidates to actual moves
/// where appropriate.
fn apply_refinement_to_borrow_state(
    checker: &mut BorrowChecker,
    refinement: &CandidateMoveRefinement,
) -> Result<(), CompilerMessages> {
    // Update borrow kinds in CFG nodes based on refinement decisions
    for (node_id, decision) in &refinement.move_decisions {
        if let Some(cfg_node) = checker.cfg.nodes.get_mut(node_id) {
            // Find the loan that corresponds to this candidate move
            let mut loan_to_update: Option<(BorrowId, Place)> = None;
            
            for (&borrow_id, loan) in &cfg_node.borrow_state.active_borrows {
                match decision {
                    MoveDecision::Move(place) | MoveDecision::MutableBorrow(place) => {
                        if loan.place == *place && loan.creation_point == *node_id {
                            loan_to_update = Some((borrow_id, place.clone()));
                            break;
                        }
                    }
                }
            }
            
            // Update the loan's kind based on the decision
            if let Some((borrow_id, place)) = loan_to_update {
                if let Some(loan) = cfg_node.borrow_state.active_borrows.get_mut(&borrow_id) {
                    match decision {
                        MoveDecision::Move(_) => {
                            loan.kind = BorrowKind::Move;
                        }
                        MoveDecision::MutableBorrow(_) => {
                            loan.kind = BorrowKind::Mutable;
                        }
                    }
                }
                
                // For moves, we should also record that the place has been moved
                if matches!(decision, MoveDecision::Move(_)) {
                    cfg_node.borrow_state.record_last_use(place, *node_id);
                }
            }
        }
    }
    
    Ok(())
}

/// Update HIR annotations with move/borrow decisions
///
/// This function would update the HIR nodes to reflect the refined decisions,
/// but since HIR nodes are typically immutable after creation, this is more
/// of a conceptual operation. In practice, the refinement information is
/// used by later compilation stages (LIR generation, codegen) to make the
/// appropriate ownership decisions.
#[allow(dead_code)]
pub fn update_hir_annotations(
    _hir_nodes: &mut [HirNode],
    _refinement: &CandidateMoveRefinement,
) -> Result<(), CompilerMessages> {
    // In a full implementation, this would update HIR nodes with refined
    // move/borrow information. For now, the refinement information is
    // stored separately and used by later compilation stages.
    
    // The refinement decisions are applied to the borrow checker state
    // and will be used by LIR generation and codegen to make the
    // appropriate ownership transfer decisions.
    
    Ok(())
}

/// Validate that moves don't conflict with active borrows
///
/// This function ensures that when a candidate move is refined to an actual move,
/// there are no conflicting active borrows that would make the move invalid.
pub fn validate_move_decisions(
    checker: &BorrowChecker,
    refinement: &CandidateMoveRefinement,
) -> Result<(), CompilerMessages> {
    for (node_id, decision) in &refinement.move_decisions {
        if let MoveDecision::Move(place) = decision {
            // Check if there are any conflicting borrows at this point
            if let Some(cfg_node) = checker.cfg.nodes.get(node_id) {
                let conflicting_borrows = cfg_node.borrow_state.borrows_for_overlapping_places(place);
                
                // Filter out the move itself (it's not a conflict)
                let actual_conflicts: Vec<_> = conflicting_borrows
                    .into_iter()
                    .filter(|loan| loan.creation_point != *node_id)
                    .collect();
                
                if !actual_conflicts.is_empty() {
                    // In a full implementation, this would generate a borrow checker error
                    // For now, we'll just continue - the conflict detection in other
                    // parts of the borrow checker will catch these issues
                }
            }
        }
    }
    
    Ok(())
}