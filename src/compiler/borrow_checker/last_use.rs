//! Last-Use Analysis
//!
//! This module implements last-use analysis for the borrow checker. Last-use analysis
//! determines when a place is used for the final time in the program, which is essential
//! for:
//! - Determining when borrows end (lifetime inference)
//! - Converting candidate moves to actual moves
//! - Inserting Drop nodes at the correct locations
//!
//! The analysis uses a two-pass approach:
//! 1. Forward pass: Record all usages of each place
//! 2. Backward pass: Determine which usages are final (no subsequent uses on any path)
//!
//! The analysis is path-sensitive, considering all possible execution paths through
//! the control flow graph.

use crate::compiler::borrow_checker::types::{BorrowChecker, ControlFlowGraph};
use crate::compiler::hir::nodes::{HirExprKind, HirKind, HirNode, HirNodeId};
use crate::compiler::hir::place::Place;
use std::collections::{HashMap, HashSet};

/// Result of last-use analysis for a single place
#[derive(Debug, Clone)]
pub struct PlaceUsageInfo {
    /// All nodes where this place is used
    pub usage_points: Vec<HirNodeId>,
    
    /// The last use point(s) for this place
    /// Multiple points possible due to different control flow paths
    pub last_use_points: HashSet<HirNodeId>,
    
    /// Whether this place is used after each node (for path-sensitive analysis)
    pub used_after: HashMap<HirNodeId, bool>,
}

impl Default for PlaceUsageInfo {
    fn default() -> Self {
        Self {
            usage_points: Vec::new(),
            last_use_points: HashSet::new(),
            used_after: HashMap::new(),
        }
    }
}

/// Complete last-use analysis results
#[derive(Debug, Clone, Default)]
pub struct LastUseAnalysis {
    /// Usage information for each place
    pub place_usages: HashMap<Place, PlaceUsageInfo>,
    
    /// Mapping from node ID to the places used at that node
    pub node_to_places: HashMap<HirNodeId, Vec<Place>>,
    
    /// Set of nodes that are last-use points for any place
    pub last_use_nodes: HashSet<HirNodeId>,
}

impl LastUseAnalysis {
    /// Create a new empty last-use analysis
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Check if a specific usage of a place is its last use
    pub fn is_last_use(&self, place: &Place, node_id: HirNodeId) -> bool {
        if let Some(usage_info) = self.place_usages.get(place) {
            usage_info.last_use_points.contains(&node_id)
        } else {
            false
        }
    }
    
    /// Get all places that have their last use at a given node
    pub fn places_with_last_use_at(&self, node_id: HirNodeId) -> Vec<&Place> {
        self.place_usages
            .iter()
            .filter(|(_, info)| info.last_use_points.contains(&node_id))
            .map(|(place, _)| place)
            .collect()
    }
    
    /// Check if a place is used after a given node on any path
    pub fn is_used_after(&self, place: &Place, node_id: HirNodeId) -> bool {
        if let Some(usage_info) = self.place_usages.get(place) {
            usage_info.used_after.get(&node_id).copied().unwrap_or(false)
        } else {
            false
        }
    }
}

/// Perform last-use analysis on HIR nodes
///
/// This function analyzes the HIR to determine the last use point for each place.
/// It uses a two-pass approach:
/// 1. Forward pass: Collect all usages of each place
/// 2. Backward pass: Determine which usages are final
pub fn analyze_last_uses(
    _checker: &BorrowChecker,
    cfg: &ControlFlowGraph,
    hir_nodes: &[HirNode],
) -> LastUseAnalysis {
    let mut analysis = LastUseAnalysis::new();
    
    // Phase 1: Forward pass - collect all place usages
    collect_place_usages(&mut analysis, hir_nodes);
    
    // Phase 2: Backward pass - determine last uses using CFG
    determine_last_uses(&mut analysis, cfg, hir_nodes);
    
    analysis
}

/// Forward pass: Collect all usages of each place in the HIR
fn collect_place_usages(analysis: &mut LastUseAnalysis, hir_nodes: &[HirNode]) {
    for node in hir_nodes {
        collect_node_usages(analysis, node);
    }
}

/// Collect place usages from a single HIR node
fn collect_node_usages(analysis: &mut LastUseAnalysis, node: &HirNode) {
    let node_id = node.id;
    
    match &node.kind {
        HirKind::Assign { place, value } => {
            // The assigned-to place is being written, not read
            // But we still track it for Drop insertion purposes
            record_place_definition(analysis, place, node_id);
            
            // Collect usages from the value expression
            collect_expression_usages(analysis, value, node_id);
        }
        
        HirKind::Borrow { place, target, .. } => {
            // The borrowed place is being read
            record_place_usage(analysis, place, node_id);
            // The target is being written
            record_place_definition(analysis, target, node_id);
        }
        
        HirKind::If { condition, then_block, else_block } => {
            // Condition is read
            record_place_usage(analysis, condition, node_id);
            
            // Recursively process blocks
            for then_node in then_block {
                collect_node_usages(analysis, then_node);
            }
            if let Some(else_nodes) = else_block {
                for else_node in else_nodes {
                    collect_node_usages(analysis, else_node);
                }
            }
        }
        
        HirKind::Match { scrutinee, arms, default } => {
            // Scrutinee is read
            record_place_usage(analysis, scrutinee, node_id);
            
            // Process match arms
            for arm in arms {
                for arm_node in &arm.body {
                    collect_node_usages(analysis, arm_node);
                }
            }
            if let Some(default_nodes) = default {
                for default_node in default_nodes {
                    collect_node_usages(analysis, default_node);
                }
            }
        }
        
        HirKind::Loop { iterator, body, .. } => {
            // Iterator is read
            record_place_usage(analysis, iterator, node_id);
            
            // Process loop body
            for body_node in body {
                collect_node_usages(analysis, body_node);
            }
        }
        
        HirKind::Call { args, returns, .. } | HirKind::HostCall { args, returns, .. } => {
            // Arguments are read
            for arg in args {
                record_place_usage(analysis, arg, node_id);
            }
            // Returns are written
            for ret in returns {
                record_place_definition(analysis, ret, node_id);
            }
        }
        
        HirKind::Return(places) => {
            // Return values are read
            for place in places {
                record_place_usage(analysis, place, node_id);
            }
        }
        
        HirKind::ReturnError(place) => {
            record_place_usage(analysis, place, node_id);
        }
        
        HirKind::Drop(place) => {
            // Drop reads the place (to deallocate it)
            record_place_usage(analysis, place, node_id);
        }
        
        HirKind::ExprStmt(place) => {
            record_place_usage(analysis, place, node_id);
        }
        
        HirKind::TryCall { call, error_handler, .. } => {
            // Process the call
            collect_node_usages(analysis, call);
            
            // Process error handler
            for handler_node in error_handler {
                collect_node_usages(analysis, handler_node);
            }
        }
        
        HirKind::OptionUnwrap { expr, default_value } => {
            collect_expression_usages(analysis, expr, node_id);
            if let Some(default) = default_value {
                collect_expression_usages(analysis, default, node_id);
            }
        }
        
        HirKind::RuntimeTemplateCall { captures, .. } => {
            for capture in captures {
                collect_expression_usages(analysis, capture, node_id);
            }
        }
        
        HirKind::TemplateFn { body, .. } | HirKind::FunctionDef { body, .. } => {
            for body_node in body {
                collect_node_usages(analysis, body_node);
            }
        }
        
        HirKind::StructDef { .. } | HirKind::Break | HirKind::Continue => {
            // No place usages
        }
    }
}

/// Collect place usages from an expression
fn collect_expression_usages(
    analysis: &mut LastUseAnalysis,
    expr: &crate::compiler::hir::nodes::HirExpr,
    node_id: HirNodeId,
) {
    match &expr.kind {
        HirExprKind::Load(place) => {
            record_place_usage(analysis, place, node_id);
        }
        
        HirExprKind::SharedBorrow(place) | HirExprKind::MutableBorrow(place) => {
            record_place_usage(analysis, place, node_id);
        }
        
        HirExprKind::CandidateMove(place) => {
            // Candidate moves are usages that might become moves
            record_place_usage(analysis, place, node_id);
        }
        
        HirExprKind::BinOp { left, right, .. } => {
            record_place_usage(analysis, left, node_id);
            record_place_usage(analysis, right, node_id);
        }
        
        HirExprKind::UnaryOp { operand, .. } => {
            record_place_usage(analysis, operand, node_id);
        }
        
        HirExprKind::Call { args, .. } => {
            for arg in args {
                record_place_usage(analysis, arg, node_id);
            }
        }
        
        HirExprKind::MethodCall { receiver, args, .. } => {
            record_place_usage(analysis, receiver, node_id);
            for arg in args {
                record_place_usage(analysis, arg, node_id);
            }
        }
        
        HirExprKind::StructConstruct { fields, .. } => {
            for (_, field_place) in fields {
                record_place_usage(analysis, field_place, node_id);
            }
        }
        
        HirExprKind::Collection(places) => {
            for place in places {
                record_place_usage(analysis, place, node_id);
            }
        }
        
        HirExprKind::Range { start, end } => {
            record_place_usage(analysis, start, node_id);
            record_place_usage(analysis, end, node_id);
        }
        
        // Literals don't use places
        HirExprKind::Int(_)
        | HirExprKind::Float(_)
        | HirExprKind::Bool(_)
        | HirExprKind::StringLiteral(_)
        | HirExprKind::Char(_) => {}
    }
}

/// Record a place usage at a specific node
fn record_place_usage(analysis: &mut LastUseAnalysis, place: &Place, node_id: HirNodeId) {
    // Add to place_usages
    let usage_info = analysis.place_usages.entry(place.clone()).or_default();
    usage_info.usage_points.push(node_id);
    
    // Add to node_to_places
    analysis.node_to_places
        .entry(node_id)
        .or_default()
        .push(place.clone());
}

/// Record a place definition (write) at a specific node
/// This is tracked separately as definitions don't count as "uses" for last-use analysis
fn record_place_definition(analysis: &mut LastUseAnalysis, place: &Place, _node_id: HirNodeId) {
    // Ensure the place exists in our tracking, but don't add as a usage point
    analysis.place_usages.entry(place.clone()).or_default();
}

/// Backward pass: Determine last uses using CFG analysis
///
/// This implements path-sensitive last-use analysis:
/// - A usage is a "last use" if there are no subsequent uses on ANY path from that point
/// - We use backward dataflow analysis to compute this
fn determine_last_uses(
    analysis: &mut LastUseAnalysis,
    cfg: &ControlFlowGraph,
    hir_nodes: &[HirNode],
) {
    // Build a mapping from node IDs to their position for ordering
    let node_order = build_node_order(hir_nodes);
    
    // For each place, determine its last use points
    let places: Vec<Place> = analysis.place_usages.keys().cloned().collect();
    
    for place in places {
        determine_place_last_uses(analysis, cfg, &place, &node_order);
    }
    
    // Build the set of all last-use nodes
    for usage_info in analysis.place_usages.values() {
        for &node_id in &usage_info.last_use_points {
            analysis.last_use_nodes.insert(node_id);
        }
    }
}

/// Build a mapping from node IDs to their order in the HIR
fn build_node_order(hir_nodes: &[HirNode]) -> HashMap<HirNodeId, usize> {
    let mut order = HashMap::new();
    let mut counter = 0;
    
    fn visit_node(node: &HirNode, order: &mut HashMap<HirNodeId, usize>, counter: &mut usize) {
        order.insert(node.id, *counter);
        *counter += 1;
        
        match &node.kind {
            HirKind::If { then_block, else_block, .. } => {
                for n in then_block {
                    visit_node(n, order, counter);
                }
                if let Some(else_nodes) = else_block {
                    for n in else_nodes {
                        visit_node(n, order, counter);
                    }
                }
            }
            HirKind::Match { arms, default, .. } => {
                for arm in arms {
                    for n in &arm.body {
                        visit_node(n, order, counter);
                    }
                }
                if let Some(default_nodes) = default {
                    for n in default_nodes {
                        visit_node(n, order, counter);
                    }
                }
            }
            HirKind::Loop { body, .. } => {
                for n in body {
                    visit_node(n, order, counter);
                }
            }
            HirKind::TryCall { call, error_handler, .. } => {
                visit_node(call, order, counter);
                for n in error_handler {
                    visit_node(n, order, counter);
                }
            }
            HirKind::FunctionDef { body, .. } | HirKind::TemplateFn { body, .. } => {
                for n in body {
                    visit_node(n, order, counter);
                }
            }
            _ => {}
        }
    }
    
    for node in hir_nodes {
        visit_node(node, &mut order, &mut counter);
    }
    
    order
}

/// Determine last use points for a specific place using backward analysis
fn determine_place_last_uses(
    analysis: &mut LastUseAnalysis,
    cfg: &ControlFlowGraph,
    place: &Place,
    node_order: &HashMap<HirNodeId, usize>,
) {
    let usage_info = match analysis.place_usages.get_mut(place) {
        Some(info) => info,
        None => return,
    };
    
    if usage_info.usage_points.is_empty() {
        return;
    }
    
    // Get all usage points for this place
    let usage_points: HashSet<HirNodeId> = usage_info.usage_points.iter().copied().collect();
    
    // Initialize used_after for all nodes in CFG
    // A node has used_after = true if the place is used on any path after that node
    let mut used_after: HashMap<HirNodeId, bool> = HashMap::new();
    
    // Initialize all nodes to false
    for &node_id in cfg.nodes.keys() {
        used_after.insert(node_id, false);
    }
    
    // Backward dataflow analysis using worklist algorithm
    // Start from exit points and work backwards
    let mut worklist: Vec<HirNodeId> = cfg.exit_points.clone();
    let mut visited: HashSet<HirNodeId> = HashSet::new();
    
    // Also add all nodes that have no successors (implicit exits)
    for (&node_id, successors) in &cfg.edges {
        if successors.is_empty() {
            worklist.push(node_id);
        }
    }
    
    // Add all nodes to worklist initially for complete analysis
    for &node_id in cfg.nodes.keys() {
        if !worklist.contains(&node_id) {
            worklist.push(node_id);
        }
    }
    
    // Sort worklist by reverse node order (process later nodes first)
    worklist.sort_by(|a, b| {
        let order_a = node_order.get(a).copied().unwrap_or(0);
        let order_b = node_order.get(b).copied().unwrap_or(0);
        order_b.cmp(&order_a) // Reverse order
    });
    
    // Iterate until fixed point
    let max_iterations = cfg.nodes.len() * 3;
    let mut iterations = 0;
    
    while let Some(node_id) = worklist.pop() {
        iterations += 1;
        if iterations > max_iterations {
            break; // Safety limit
        }
        
        if visited.contains(&node_id) {
            continue;
        }
        
        // Compute used_after for this node based on successors
        let successors = cfg.successors(node_id);
        let mut any_successor_uses = false;
        
        for &succ_id in successors {
            // If successor uses the place, or place is used after successor
            if usage_points.contains(&succ_id) || used_after.get(&succ_id).copied().unwrap_or(false) {
                any_successor_uses = true;
                break;
            }
        }
        
        let old_value = used_after.get(&node_id).copied().unwrap_or(false);
        let new_value = any_successor_uses;
        
        if old_value != new_value {
            used_after.insert(node_id, new_value);
            
            // Add predecessors to worklist for re-processing
            let predecessors = cfg.predecessors(node_id);
            for pred_id in predecessors {
                visited.remove(&pred_id);
                if !worklist.contains(&pred_id) {
                    worklist.push(pred_id);
                }
            }
        }
        
        visited.insert(node_id);
    }
    
    // Store used_after results
    usage_info.used_after = used_after.clone();
    
    // Determine last use points: a usage is a last use if used_after is false
    for &usage_node in &usage_info.usage_points {
        let is_used_after = used_after.get(&usage_node).copied().unwrap_or(false);
        if !is_used_after {
            usage_info.last_use_points.insert(usage_node);
        }
    }
}

/// Update borrow checker state with last-use information
///
/// This function integrates the last-use analysis results into the borrow checker's
/// state, updating the borrow state at each CFG node with last-use information.
pub fn apply_last_use_analysis(
    checker: &mut BorrowChecker,
    analysis: &LastUseAnalysis,
) {
    // Update each CFG node's borrow state with last-use information
    for (place, usage_info) in &analysis.place_usages {
        for &last_use_node in &usage_info.last_use_points {
            if let Some(cfg_node) = checker.cfg.nodes.get_mut(&last_use_node) {
                cfg_node.borrow_state.record_last_use(place.clone(), last_use_node);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_last_use_analysis_creation() {
        let analysis = LastUseAnalysis::new();
        assert!(analysis.place_usages.is_empty());
        assert!(analysis.node_to_places.is_empty());
        assert!(analysis.last_use_nodes.is_empty());
    }
    
    #[test]
    fn test_place_usage_info_default() {
        let info = PlaceUsageInfo::default();
        assert!(info.usage_points.is_empty());
        assert!(info.last_use_points.is_empty());
        assert!(info.used_after.is_empty());
    }
}
