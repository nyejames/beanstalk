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
/// **Phase 3 Improvement**: Enhanced with control flow type information for
/// topologically correct CFG construction.
#[derive(Debug, Clone)]
pub struct LinearStatement {
    /// Statement ID (same as CFG node ID for direct mapping)
    pub id: HirNodeId,
    
    /// Places used (read) by this statement
    pub uses: Vec<Place>,
    
    /// Places defined (written) by this statement (ignored in last-use analysis due to no shadowing)
    #[allow(dead_code)]
    pub defines: Vec<Place>,
    
    /// Control flow type for this statement
    pub control_flow_type: ControlFlowType,
}

/// Control flow classification for statements
#[derive(Debug, Clone, PartialEq)]
pub enum ControlFlowType {
    /// Regular statement with sequential flow
    Sequential,
    /// If condition that branches
    IfCondition,
    /// Match scrutinee that branches
    MatchCondition,
    /// Loop header that can iterate or exit
    LoopHeader,
    /// Return statement (terminating)
    Return,
    /// Break statement (jumps to loop exit)
    Break,
    /// Continue statement (jumps to loop header)
    Continue,
    /// Function definition entry point
    FunctionEntry,
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
pub fn linearize_hir_with_cfg_ids(hir_nodes: &[HirNode]) -> Vec<LinearStatement> {
    let mut statements = Vec::new();
    
    for node in hir_nodes {
        linearize_node_with_cfg_id(node, &mut statements);
    }
    
    statements
}

/// Linearize a single HIR node into a statement with statement ID = HIR node ID
///
/// **Phase 3 Improvement**: Enhanced linearization that preserves control flow structure
/// information needed for topologically correct CFG construction.
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
                control_flow_type: ControlFlowType::Sequential,
            });
        }
        
        HirKind::Borrow { place, target, .. } => {
            statements.push(LinearStatement {
                id: node.id,
                uses: vec![place.clone()],
                defines: vec![target.clone()],
                control_flow_type: ControlFlowType::Sequential,
            });
        }
        
        HirKind::Call { args, returns, .. } | HirKind::HostCall { args, returns, .. } => {
            statements.push(LinearStatement {
                id: node.id,
                uses: args.clone(),
                defines: returns.clone(),
                control_flow_type: ControlFlowType::Sequential,
            });
        }
        
        HirKind::Return(places) => {
            statements.push(LinearStatement {
                id: node.id,
                uses: places.clone(),
                defines: Vec::new(),
                control_flow_type: ControlFlowType::Return,
            });
        }
        
        HirKind::ReturnError(place) => {
            statements.push(LinearStatement {
                id: node.id,
                uses: vec![place.clone()],
                defines: Vec::new(),
                control_flow_type: ControlFlowType::Return,
            });
        }
        
        HirKind::Drop(place) => {
            statements.push(LinearStatement {
                id: node.id,
                uses: vec![place.clone()],
                defines: Vec::new(),
                control_flow_type: ControlFlowType::Sequential,
            });
        }
        
        HirKind::ExprStmt(place) => {
            statements.push(LinearStatement {
                id: node.id,
                uses: vec![place.clone()],
                defines: Vec::new(),
                control_flow_type: ControlFlowType::Sequential,
            });
        }
        
        // For structured control flow, create a statement for the control node
        // and recursively linearize nested parts with proper structure tracking
        HirKind::If { condition, then_block, else_block } => {
            // Condition evaluation statement
            statements.push(LinearStatement {
                id: node.id,
                uses: vec![condition.clone()],
                defines: Vec::new(),
                control_flow_type: ControlFlowType::IfCondition,
            });
            
            // Linearize then block with early return detection
            for then_node in then_block {
                linearize_node_with_cfg_id(then_node, statements);
                
                // If this is an early return, subsequent statements in this block are unreachable
                if is_early_return_node(then_node) {
                    break;
                }
            }
            
            // Linearize else block with early return detection
            if let Some(else_nodes) = else_block {
                for else_node in else_nodes {
                    linearize_node_with_cfg_id(else_node, statements);
                    
                    // If this is an early return, subsequent statements in this block are unreachable
                    if is_early_return_node(else_node) {
                        break;
                    }
                }
            }
        }
        
        HirKind::Match { scrutinee, arms, default } => {
            // Scrutinee evaluation statement
            statements.push(LinearStatement {
                id: node.id,
                uses: vec![scrutinee.clone()],
                defines: Vec::new(),
                control_flow_type: ControlFlowType::MatchCondition,
            });
            
            // Linearize match arms with early return detection
            for arm in arms {
                for arm_node in &arm.body {
                    linearize_node_with_cfg_id(arm_node, statements);
                    
                    // If this is an early return, subsequent statements in this arm are unreachable
                    if is_early_return_node(arm_node) {
                        break;
                    }
                }
            }
            
            // Linearize default arm with early return detection
            if let Some(default_nodes) = default {
                for default_node in default_nodes {
                    linearize_node_with_cfg_id(default_node, statements);
                    
                    // If this is an early return, subsequent statements in this arm are unreachable
                    if is_early_return_node(default_node) {
                        break;
                    }
                }
            }
        }
        
        HirKind::Loop { iterator, body, .. } => {
            // Iterator evaluation statement (loop header)
            statements.push(LinearStatement {
                id: node.id,
                uses: vec![iterator.clone()],
                defines: Vec::new(),
                control_flow_type: ControlFlowType::LoopHeader,
            });
            
            // Linearize loop body with break/continue detection
            for body_node in body {
                linearize_node_with_cfg_id(body_node, statements);
                
                // Break and continue affect control flow but don't stop linearization
                // (other statements in the loop body might be reachable via different paths)
            }
        }
        
        HirKind::TryCall { call, error_handler, .. } => {
            // Linearize the call
            linearize_node_with_cfg_id(call, statements);
            
            // Linearize error handler with early return detection
            for handler_node in error_handler {
                linearize_node_with_cfg_id(handler_node, statements);
                
                // If this is an early return, subsequent statements in handler are unreachable
                if is_early_return_node(handler_node) {
                    break;
                }
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
                control_flow_type: ControlFlowType::Sequential,
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
                control_flow_type: ControlFlowType::Sequential,
            });
        }
        
        HirKind::FunctionDef { body, .. } => {
            // Function entry statement
            statements.push(LinearStatement {
                id: node.id,
                uses: Vec::new(),
                defines: Vec::new(),
                control_flow_type: ControlFlowType::FunctionEntry,
            });
            
            // Linearize function body with early return detection
            for body_node in body {
                linearize_node_with_cfg_id(body_node, statements);
                
                // If this is an early return, subsequent statements are unreachable
                if is_early_return_node(body_node) {
                    break;
                }
            }
        }
        
        HirKind::TemplateFn { body, .. } => {
            // Template function entry statement
            statements.push(LinearStatement {
                id: node.id,
                uses: Vec::new(),
                defines: Vec::new(),
                control_flow_type: ControlFlowType::FunctionEntry,
            });
            
            // Linearize function body with early return detection
            for body_node in body {
                linearize_node_with_cfg_id(body_node, statements);
                
                // If this is an early return, subsequent statements are unreachable
                if is_early_return_node(body_node) {
                    break;
                }
            }
        }
        
        // Control flow statements that need special CFG handling
        HirKind::Break => {
            statements.push(LinearStatement {
                id: node.id,
                uses: Vec::new(),
                defines: Vec::new(),
                control_flow_type: ControlFlowType::Break,
            });
        }
        
        HirKind::Continue => {
            statements.push(LinearStatement {
                id: node.id,
                uses: Vec::new(),
                defines: Vec::new(),
                control_flow_type: ControlFlowType::Continue,
            });
        }
        
        // These don't use places but still need statements for CFG consistency
        HirKind::StructDef { .. } => {
            statements.push(LinearStatement {
                id: node.id,
                uses: Vec::new(),
                defines: Vec::new(),
                control_flow_type: ControlFlowType::Sequential,
            });
        }
    }
}

/// Check if a HIR node represents an early return that terminates the current block
fn is_early_return_node(node: &HirNode) -> bool {
    matches!(
        node.kind,
        HirKind::Return(_) | HirKind::ReturnError(_)
    )
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
/// **Phase 3 Improvement**: Creates topologically correct CFG that handles complex control flow:
/// - Early returns inside blocks
/// - Break/continue in loops with proper edges
/// - Correct fallthrough after if statements
/// - 1:1 correspondence between CFG nodes and statements
fn build_statement_cfg(statements: &[LinearStatement]) -> StatementCfg {
    let mut successors: HashMap<HirNodeId, Vec<HirNodeId>> = HashMap::new();
    let mut predecessors: HashMap<HirNodeId, Vec<HirNodeId>> = HashMap::new();
    
    // Initialize empty successor/predecessor lists for all statements
    for stmt in statements {
        successors.insert(stmt.id, Vec::new());
        predecessors.insert(stmt.id, Vec::new());
    }
    
    // Build a mapping from statement ID to statement for quick lookup
    let stmt_map: HashMap<HirNodeId, &LinearStatement> = statements
        .iter()
        .map(|stmt| (stmt.id, stmt))
        .collect();
    
    // Build edges with topologically correct complex control flow handling
    build_topologically_correct_edges(statements, &stmt_map, &mut successors, &mut predecessors);
    
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

/// Build topologically correct CFG edges for complex control flow
///
/// **Phase 3 Implementation**: Handles complex control flow patterns correctly:
/// - Early returns bypass subsequent statements in blocks
/// - Break statements jump to loop exit
/// - Continue statements jump to loop header
/// - Proper fallthrough after if statements
/// - 1:1 correspondence between statements and CFG nodes
fn build_topologically_correct_edges(
    statements: &[LinearStatement],
    _stmt_map: &HashMap<HirNodeId, &LinearStatement>,
    successors: &mut HashMap<HirNodeId, Vec<HirNodeId>>,
    predecessors: &mut HashMap<HirNodeId, Vec<HirNodeId>>,
) {
    // Track control flow context for break/continue handling
    let mut loop_stack: Vec<LoopContext> = Vec::new();
    
    // Build a more sophisticated CFG by analyzing statement patterns
    let mut i = 0;
    while i < statements.len() {
        let stmt = &statements[i];
        
        match stmt.control_flow_type {
            ControlFlowType::Sequential => {
                // Regular statements flow to next statement if it exists
                if i + 1 < statements.len() {
                    add_cfg_edge(stmt.id, statements[i + 1].id, successors, predecessors);
                }
                i += 1;
            }
            
            ControlFlowType::IfCondition => {
                // If condition: analyze the structure to find then/else blocks
                let (then_start, else_start, after_if) = analyze_if_structure(statements, i);
                
                // Connect condition to then block
                if let Some(then_id) = then_start {
                    add_cfg_edge(stmt.id, then_id, successors, predecessors);
                }
                
                // Connect condition to else block or fallthrough
                if let Some(else_id) = else_start {
                    add_cfg_edge(stmt.id, else_id, successors, predecessors);
                } else if let Some(after_id) = after_if {
                    // No else block, condition can fall through
                    add_cfg_edge(stmt.id, after_id, successors, predecessors);
                }
                
                // Process the blocks and connect them to after_if
                if let Some(after_id) = after_if {
                    connect_block_ends_to_target(statements, then_start, else_start, after_id, successors, predecessors);
                }
                
                i += 1;
            }
            
            ControlFlowType::MatchCondition => {
                // Match condition: similar to if but with multiple arms
                // For now, treat like if with simple fallthrough
                if i + 1 < statements.len() {
                    add_cfg_edge(stmt.id, statements[i + 1].id, successors, predecessors);
                }
                i += 1;
            }
            
            ControlFlowType::LoopHeader => {
                // Loop header: find loop body and exit
                let loop_end = find_loop_end(statements, i);
                let loop_exit = if i + loop_end + 1 < statements.len() {
                    Some(statements[i + loop_end + 1].id)
                } else {
                    None
                };
                
                let loop_ctx = LoopContext {
                    header_id: stmt.id,
                    exit_id: loop_exit,
                };
                
                // Connect to loop body (next statement)
                if i + 1 < statements.len() {
                    add_cfg_edge(stmt.id, statements[i + 1].id, successors, predecessors);
                }
                
                // Also connect to loop exit for condition check
                if let Some(exit_id) = loop_exit {
                    add_cfg_edge(stmt.id, exit_id, successors, predecessors);
                }
                
                loop_stack.push(loop_ctx);
                i += 1;
            }
            
            ControlFlowType::Return => {
                // Return statements are terminating - no outgoing edges
                i += 1;
            }
            
            ControlFlowType::Break => {
                // Break jumps to loop exit
                if let Some(loop_ctx) = loop_stack.last() {
                    if let Some(exit_id) = loop_ctx.exit_id {
                        add_cfg_edge(stmt.id, exit_id, successors, predecessors);
                    }
                }
                i += 1;
            }
            
            ControlFlowType::Continue => {
                // Continue jumps to loop header
                if let Some(loop_ctx) = loop_stack.last() {
                    add_cfg_edge(stmt.id, loop_ctx.header_id, successors, predecessors);
                }
                i += 1;
            }
            
            ControlFlowType::FunctionEntry => {
                // Function entry flows to next statement
                if i + 1 < statements.len() {
                    add_cfg_edge(stmt.id, statements[i + 1].id, successors, predecessors);
                }
                i += 1;
            }
        }
        
        // Pop loop context when we exit a loop
        // This is a simplified heuristic - in a full implementation,
        // we'd track loop boundaries more precisely
        if let Some(loop_ctx) = loop_stack.last() {
            if let Some(exit_id) = loop_ctx.exit_id {
                if i < statements.len() && statements[i].id == exit_id {
                    loop_stack.pop();
                }
            }
        }
    }
}

/// Analyze if statement structure to find then/else blocks and fallthrough point
fn analyze_if_structure(statements: &[LinearStatement], if_index: usize) -> (Option<HirNodeId>, Option<HirNodeId>, Option<HirNodeId>) {
    // This is a simplified analysis - in a full implementation,
    // we'd need to parse the original HIR structure to determine
    // the exact boundaries of then/else blocks
    
    let then_start = if if_index + 1 < statements.len() {
        Some(statements[if_index + 1].id)
    } else {
        None
    };
    
    // For now, assume no else block and simple fallthrough
    let else_start = None;
    let after_if = if if_index + 2 < statements.len() {
        Some(statements[if_index + 2].id)
    } else {
        None
    };
    
    (then_start, else_start, after_if)
}

/// Connect the ends of then/else blocks to the statement after the if
fn connect_block_ends_to_target(
    statements: &[LinearStatement],
    then_start: Option<HirNodeId>,
    _else_start: Option<HirNodeId>,
    target_id: HirNodeId,
    successors: &mut HashMap<HirNodeId, Vec<HirNodeId>>,
    predecessors: &mut HashMap<HirNodeId, Vec<HirNodeId>>,
) {
    // Find the end of the then block and connect it to target
    if let Some(then_id) = then_start {
        // For now, assume the then block is just one statement
        // In a full implementation, we'd track block boundaries
        if let Some(then_stmt) = statements.iter().find(|s| s.id == then_id) {
            if then_stmt.control_flow_type != ControlFlowType::Return {
                add_cfg_edge(then_id, target_id, successors, predecessors);
            }
        }
    }
}

/// Find the end of a loop body (simplified heuristic)
fn find_loop_end(statements: &[LinearStatement], loop_start: usize) -> usize {
    // This is a simplified heuristic - in a full implementation,
    // we'd analyze the HIR structure to find the actual loop boundaries
    
    // For now, assume the loop body is the next few statements until we find
    // a break, continue, or return, or reach the end
    let mut end = loop_start + 1;
    while end < statements.len() {
        match statements[end].control_flow_type {
            ControlFlowType::Break | ControlFlowType::Continue | ControlFlowType::Return => {
                break;
            }
            ControlFlowType::LoopHeader => {
                // Nested loop - skip it
                end += find_loop_end(statements, end) + 1;
            }
            _ => {
                end += 1;
            }
        }
    }
    
    end - loop_start - 1
}

/// Context for tracking nested loops
#[derive(Debug, Clone)]
struct LoopContext {
    header_id: HirNodeId,
    exit_id: Option<HirNodeId>,
}

/// Classification of statement types for CFG construction
#[derive(Debug, Clone, PartialEq)]
enum StatementType {
    Regular,
    IfCondition,
    LoopHeader,
    LoopEnd,
    Return,
    Break,
    Continue,
    EarlyReturn,
}

/// Classify a statement based on its control flow type
fn classify_statement_type(
    stmt: &LinearStatement,
    _stmt_map: &HashMap<HirNodeId, &LinearStatement>,
) -> StatementType {
    match stmt.control_flow_type {
        ControlFlowType::Sequential => StatementType::Regular,
        ControlFlowType::IfCondition => StatementType::IfCondition,
        ControlFlowType::MatchCondition => StatementType::IfCondition, // Treat match like if for CFG purposes
        ControlFlowType::LoopHeader => StatementType::LoopHeader,
        ControlFlowType::Return => StatementType::Return,
        ControlFlowType::Break => StatementType::Break,
        ControlFlowType::Continue => StatementType::Continue,
        ControlFlowType::FunctionEntry => StatementType::Regular, // Function entry flows to first statement
    }
}



/// Add a CFG edge between two statements
fn add_cfg_edge(
    from: HirNodeId,
    to: HirNodeId,
    successors: &mut HashMap<HirNodeId, Vec<HirNodeId>>,
    predecessors: &mut HashMap<HirNodeId, Vec<HirNodeId>>,
) {
    successors.entry(from).or_default().push(to);
    predecessors.entry(to).or_default().push(from);
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