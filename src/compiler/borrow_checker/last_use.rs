//! Last-Use Analysis
//!
//! This module implements last-use analysis for the borrow checker. Last-use analysis
//! determines when a place is used for the final time in the program, which is essential
//! for:
//! - Determining when borrows end (lifetime inference)
//! - Converting candidate moves to actual moves
//! - Inserting Drop nodes at the correct locations
//!
//! ## Design Principles (Phase 2: Architectural Correctness)
//!
//! This implementation follows classic dataflow analysis principles with architectural improvements:
//! - **Statement ID = CFG Node ID**: Eliminates complex mapping between statements and HIR nodes
//! - **Direct CFG edges**: Connect statements directly, not through HIR node relationships
//! - **Per-place liveness**: Use HashMap<StmtId, HashSet<Place>> for efficient computation
//! - **No all_places initialization**: More efficient than computing all places upfront
//! - **Classic dataflow**: Backward analysis without visited sets or iteration caps
//! - **1:1 correspondence**: Each CFG node corresponds to exactly one statement
//!
//! ## Algorithm
//!
//! 1. **Linearize HIR**: Flatten nested structures into linear statements with statement ID = CFG node ID
//! 2. **Build statement-level CFG**: Direct edges between statements, no HIR node mapping
//! 3. **Per-place dataflow**: Compute live_after per place using backward propagation
//! 4. **Mark last uses**: Usage points where place ∉ live_after are last uses

use crate::compiler::borrow_checker::types::{BorrowChecker, ControlFlowGraph};
use crate::compiler::hir::nodes::{HirExprKind, HirKind, HirNode, HirNodeId};
use crate::compiler::hir::place::Place;
use std::collections::{HashMap, HashSet, VecDeque};

/// A linear HIR statement for last-use analysis
///
/// This represents a single statement that can use places, making the analysis
/// more precise than working with complex nested HIR nodes.
/// 
/// **Phase 2 Improvement**: Statement ID is now the same as CFG node ID,
/// eliminating the need for complex mapping logic.
#[derive(Debug, Clone)]
pub struct LinearStatement {
    /// Statement ID (same as CFG node ID for direct mapping)
    pub id: HirNodeId,
    
    /// Places used (read) by this statement
    pub uses: Vec<Place>,
    
    /// Places defined (written) by this statement (ignored in last-use analysis due to no shadowing)
    #[allow(dead_code)]
    pub defines: Vec<Place>,
}

/// Result of last-use analysis
#[derive(Debug, Clone, Default)]
pub struct LastUseAnalysis {
    /// Set of statement IDs that are last-use points for any place
    pub last_use_statements: HashSet<HirNodeId>,
    
    /// Mapping from statement ID to places that have their last use there
    pub statement_to_last_uses: HashMap<HirNodeId, Vec<Place>>,
    
    /// Mapping from place to its last-use statement IDs
    pub place_to_last_uses: HashMap<Place, Vec<HirNodeId>>,
}

impl LastUseAnalysis {
    /// Create a new empty last-use analysis
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Check if a specific statement is a last use for the given place
    #[allow(dead_code)]
    pub fn is_last_use(&self, place: &Place, statement_id: HirNodeId) -> bool {
        if let Some(last_uses) = self.place_to_last_uses.get(place) {
            last_uses.contains(&statement_id)
        } else {
            false
        }
    }
    
    /// Get all places that have their last use at a given statement
    #[allow(dead_code)]
    pub fn places_with_last_use_at(&self, statement_id: HirNodeId) -> Vec<&Place> {
        self.statement_to_last_uses
            .get(&statement_id)
            .map(|places| places.iter().collect())
            .unwrap_or_default()
    }
    
    /// Get all last-use statement IDs for a place
    #[allow(dead_code)]
    pub fn last_use_statements_for(&self, place: &Place) -> Vec<HirNodeId> {
        self.place_to_last_uses
            .get(place)
            .cloned()
            .unwrap_or_default()
    }
}

/// Perform last-use analysis on HIR nodes
///
/// This function implements the complete last-use analysis algorithm with Phase 2 improvements:
/// 1. Linearize HIR into statements where statement ID = CFG node ID
/// 2. Build statement-level CFG with direct edges between statements
/// 3. Perform per-place backward dataflow analysis
/// 4. Mark last-use points where place ∉ live_after
pub fn analyze_last_uses(
    _checker: &BorrowChecker,
    _cfg: &ControlFlowGraph,
    hir_nodes: &[HirNode],
) -> LastUseAnalysis {
    // Step 1: Linearize HIR into statements with statement ID = CFG node ID
    let statements = linearize_hir_with_cfg_ids(hir_nodes);
    
    // Step 2: Build statement-level CFG with direct edges between statements
    let stmt_cfg = build_statement_cfg(&statements);
    
    // Step 3: Perform per-place backward dataflow analysis
    let live_after = compute_per_place_liveness(&statements, &stmt_cfg);
    
    // Step 4: Mark last-use points
    mark_last_uses_from_liveness(&statements, &live_after)
}

/// Linearize HIR nodes into statements where statement ID = CFG node ID
///
/// **Phase 2 Improvement**: Statement IDs are now the same as CFG node IDs,
/// eliminating the need for complex mapping between statements and HIR nodes.
fn linearize_hir_with_cfg_ids(hir_nodes: &[HirNode]) -> Vec<LinearStatement> {
    let mut statements = Vec::new();
    
    for node in hir_nodes {
        linearize_node_with_cfg_id(node, &mut statements);
    }
    
    statements
}

/// Linearize a single HIR node into a statement with statement ID = HIR node ID
///
/// **Phase 2 Improvement**: Uses HIR node ID directly as statement ID,
/// eliminating the need for source_node mapping.
fn linearize_node_with_cfg_id(
    node: &HirNode,
    statements: &mut Vec<LinearStatement>,
) {
    match &node.kind {
        HirKind::Assign { place, value } => {
            // Collect places used in the value expression
            let value_uses = collect_expression_places(value);
            
            // Create statement with HIR node ID as statement ID
            statements.push(LinearStatement {
                id: node.id,
                uses: value_uses,
                defines: vec![place.clone()],
            });
        }
        
        HirKind::Borrow { place, target, .. } => {
            statements.push(LinearStatement {
                id: node.id,
                uses: vec![place.clone()],
                defines: vec![target.clone()],
            });
        }
        
        HirKind::Call { args, returns, .. } | HirKind::HostCall { args, returns, .. } => {
            statements.push(LinearStatement {
                id: node.id,
                uses: args.clone(),
                defines: returns.clone(),
            });
        }
        
        HirKind::Return(places) => {
            statements.push(LinearStatement {
                id: node.id,
                uses: places.clone(),
                defines: Vec::new(),
            });
        }
        
        HirKind::ReturnError(place) => {
            statements.push(LinearStatement {
                id: node.id,
                uses: vec![place.clone()],
                defines: Vec::new(),
            });
        }
        
        HirKind::Drop(place) => {
            statements.push(LinearStatement {
                id: node.id,
                uses: vec![place.clone()],
                defines: Vec::new(),
            });
        }
        
        HirKind::ExprStmt(place) => {
            statements.push(LinearStatement {
                id: node.id,
                uses: vec![place.clone()],
                defines: Vec::new(),
            });
        }
        
        // For structured control flow, create a statement for the control node
        // and recursively linearize nested parts
        HirKind::If { condition, then_block, else_block } => {
            // Condition evaluation statement
            statements.push(LinearStatement {
                id: node.id,
                uses: vec![condition.clone()],
                defines: Vec::new(),
            });
            
            // Linearize then block
            for then_node in then_block {
                linearize_node_with_cfg_id(then_node, statements);
            }
            
            // Linearize else block
            if let Some(else_nodes) = else_block {
                for else_node in else_nodes {
                    linearize_node_with_cfg_id(else_node, statements);
                }
            }
        }
        
        HirKind::Match { scrutinee, arms, default } => {
            // Scrutinee evaluation statement
            statements.push(LinearStatement {
                id: node.id,
                uses: vec![scrutinee.clone()],
                defines: Vec::new(),
            });
            
            // Linearize match arms
            for arm in arms {
                for arm_node in &arm.body {
                    linearize_node_with_cfg_id(arm_node, statements);
                }
            }
            
            // Linearize default arm
            if let Some(default_nodes) = default {
                for default_node in default_nodes {
                    linearize_node_with_cfg_id(default_node, statements);
                }
            }
        }
        
        HirKind::Loop { iterator, body, .. } => {
            // Iterator evaluation statement
            statements.push(LinearStatement {
                id: node.id,
                uses: vec![iterator.clone()],
                defines: Vec::new(),
            });
            
            // Linearize loop body
            for body_node in body {
                linearize_node_with_cfg_id(body_node, statements);
            }
        }
        
        HirKind::TryCall { call, error_handler, .. } => {
            // Linearize the call
            linearize_node_with_cfg_id(call, statements);
            
            // Linearize error handler
            for handler_node in error_handler {
                linearize_node_with_cfg_id(handler_node, statements);
            }
        }
        
        HirKind::OptionUnwrap { expr, default_value } => {
            let mut uses = collect_expression_places(expr);
            if let Some(default) = default_value {
                uses.extend(collect_expression_places(default));
            }
            
            statements.push(LinearStatement {
                id: node.id,
                uses,
                defines: Vec::new(),
            });
        }
        
        HirKind::RuntimeTemplateCall { captures, .. } => {
            let mut uses = Vec::new();
            for capture in captures {
                uses.extend(collect_expression_places(capture));
            }
            
            statements.push(LinearStatement {
                id: node.id,
                uses,
                defines: Vec::new(),
            });
        }
        
        HirKind::FunctionDef { body, .. } | HirKind::TemplateFn { body, .. } => {
            // Linearize function body
            for body_node in body {
                linearize_node_with_cfg_id(body_node, statements);
            }
        }
        
        // These don't use places but still need statements for CFG consistency
        HirKind::StructDef { .. } | HirKind::Break | HirKind::Continue => {
            statements.push(LinearStatement {
                id: node.id,
                uses: Vec::new(),
                defines: Vec::new(),
            });
        }
    }
}

/// Collect all places used in an expression
fn collect_expression_places(expr: &crate::compiler::hir::nodes::HirExpr) -> Vec<Place> {
    let mut places = Vec::new();
    
    match &expr.kind {
        HirExprKind::Load(place) => {
            places.push(place.clone());
        }
        
        HirExprKind::SharedBorrow(place) | HirExprKind::MutableBorrow(place) => {
            places.push(place.clone());
        }
        
        HirExprKind::CandidateMove(place) => {
            places.push(place.clone());
        }
        
        HirExprKind::BinOp { left, right, .. } => {
            places.push(left.clone());
            places.push(right.clone());
        }
        
        HirExprKind::UnaryOp { operand, .. } => {
            places.push(operand.clone());
        }
        
        HirExprKind::Call { args, .. } => {
            places.extend(args.iter().cloned());
        }
        
        HirExprKind::MethodCall { receiver, args, .. } => {
            places.push(receiver.clone());
            places.extend(args.iter().cloned());
        }
        
        HirExprKind::StructConstruct { fields, .. } => {
            for (_, field_place) in fields {
                places.push(field_place.clone());
            }
        }
        
        HirExprKind::Collection(element_places) => {
            places.extend(element_places.iter().cloned());
        }
        
        HirExprKind::Range { start, end } => {
            places.push(start.clone());
            places.push(end.clone());
        }
        
        // Literals don't use places
        HirExprKind::Int(_)
        | HirExprKind::Float(_)
        | HirExprKind::Bool(_)
        | HirExprKind::StringLiteral(_)
        | HirExprKind::Char(_) => {}
    }
    
    places
}

/// Statement-level CFG for direct statement-to-statement edges
///
/// **Phase 2 Improvement**: Direct edges between statements, no HIR node mapping needed.
#[derive(Debug, Clone)]
struct StatementCfg {
    /// Direct successors for each statement ID
    successors: HashMap<HirNodeId, Vec<HirNodeId>>,
    /// Direct predecessors for each statement ID  
    predecessors: HashMap<HirNodeId, Vec<HirNodeId>>,
    /// Entry points (statements with no predecessors)
    entry_points: Vec<HirNodeId>,
    /// Exit points (statements with no successors)
    exit_points: Vec<HirNodeId>,
}

/// Build statement-level CFG with direct edges between statements
///
/// **Phase 2 Improvement**: Creates CFG where edges connect statements directly,
/// eliminating the need for HIR node mapping logic.
fn build_statement_cfg(statements: &[LinearStatement]) -> StatementCfg {
    let mut successors: HashMap<HirNodeId, Vec<HirNodeId>> = HashMap::new();
    let mut predecessors: HashMap<HirNodeId, Vec<HirNodeId>> = HashMap::new();
    
    // Initialize empty successor/predecessor lists for all statements
    for stmt in statements {
        successors.insert(stmt.id, Vec::new());
        predecessors.insert(stmt.id, Vec::new());
    }
    
    // Build edges based on statement sequence and control flow
    // For now, create simple sequential edges (Phase 3 will handle complex control flow)
    for i in 0..statements.len().saturating_sub(1) {
        let current_id = statements[i].id;
        let next_id = statements[i + 1].id;
        
        // Add edge: current -> next
        successors.get_mut(&current_id).unwrap().push(next_id);
        predecessors.get_mut(&next_id).unwrap().push(current_id);
    }
    
    // Identify entry and exit points
    let entry_points: Vec<HirNodeId> = statements
        .iter()
        .filter(|stmt| predecessors.get(&stmt.id).unwrap().is_empty())
        .map(|stmt| stmt.id)
        .collect();
    
    let exit_points: Vec<HirNodeId> = statements
        .iter()
        .filter(|stmt| successors.get(&stmt.id).unwrap().is_empty())
        .map(|stmt| stmt.id)
        .collect();
    
    StatementCfg {
        successors,
        predecessors,
        entry_points,
        exit_points,
    }
}

/// Compute per-place liveness using efficient backward dataflow
///
/// **Phase 2 Improvement**: Uses HashMap<StmtId, HashSet<Place>> for per-place
/// propagation, avoiding expensive all_places initialization.
fn compute_per_place_liveness(
    statements: &[LinearStatement],
    cfg: &StatementCfg,
) -> HashMap<HirNodeId, HashSet<Place>> {
    // live_after[stmt_id] = set of places live after statement
    let mut live_after: HashMap<HirNodeId, HashSet<Place>> = HashMap::new();
    
    // Initialize all statements with empty live sets
    for stmt in statements {
        live_after.insert(stmt.id, HashSet::new());
    }
    
    // Worklist algorithm: start from exit points and work backwards
    let mut worklist: VecDeque<HirNodeId> = cfg.exit_points.iter().copied().collect();
    
    while let Some(stmt_id) = worklist.pop_front() {
        let mut changed = false;
        
        // Compute new live_after set for this statement
        let mut new_live_after = HashSet::new();
        
        // A place is live after this statement if any successor uses it or has it live after
        if let Some(successors) = cfg.successors.get(&stmt_id) {
            for &succ_id in successors {
                // Add places used by successor
                if let Some(succ_stmt) = statements.iter().find(|s| s.id == succ_id) {
                    for place in &succ_stmt.uses {
                        new_live_after.insert(place.clone());
                    }
                }
                
                // Add places live after successor
                if let Some(succ_live_after) = live_after.get(&succ_id) {
                    for place in succ_live_after {
                        new_live_after.insert(place.clone());
                    }
                }
            }
        }
        
        // Check if live_after set changed
        if let Some(old_live_after) = live_after.get(&stmt_id) {
            if &new_live_after != old_live_after {
                changed = true;
            }
        } else {
            changed = !new_live_after.is_empty();
        }
        
        if changed {
            live_after.insert(stmt_id, new_live_after);
            
            // Add predecessors to worklist
            if let Some(predecessors) = cfg.predecessors.get(&stmt_id) {
                for &pred_id in predecessors {
                    if !worklist.contains(&pred_id) {
                        worklist.push_back(pred_id);
                    }
                }
            }
        }
    }
    
    live_after
}

/// Mark last-use points from per-place liveness information
///
/// **Phase 2 Improvement**: Uses efficient per-place liveness sets to determine
/// last uses where place ∉ live_after.
fn mark_last_uses_from_liveness(
    statements: &[LinearStatement],
    live_after: &HashMap<HirNodeId, HashSet<Place>>,
) -> LastUseAnalysis {
    let mut analysis = LastUseAnalysis::new();
    
    for statement in statements {
        for place in &statement.uses {
            // Check if this place is live after this statement
            let is_live_after = live_after
                .get(&statement.id)
                .map(|live_set| live_set.contains(place))
                .unwrap_or(false);
            
            // If not live after, this is a last use
            if !is_live_after {
                analysis.last_use_statements.insert(statement.id);
                
                analysis
                    .statement_to_last_uses
                    .entry(statement.id)
                    .or_default()
                    .push(place.clone());
                
                analysis
                    .place_to_last_uses
                    .entry(place.clone())
                    .or_default()
                    .push(statement.id);
            }
        }
    }
    
    analysis
}

/// Update borrow checker state with last-use information
///
/// **Phase 2 Improvement**: Direct statement ID to CFG node mapping,
/// no complex HIR node mapping needed.
pub fn apply_last_use_analysis(
    checker: &mut BorrowChecker,
    analysis: &LastUseAnalysis,
) {
    // Update each CFG node's borrow state with last-use information
    // Since statement ID = CFG node ID, this is now a direct mapping
    for (place, last_use_statements) in &analysis.place_to_last_uses {
        for &statement_id in last_use_statements {
            if let Some(cfg_node) = checker.cfg.nodes.get_mut(&statement_id) {
                cfg_node.borrow_state.record_last_use(place.clone(), statement_id);
            }
        }
    }
}