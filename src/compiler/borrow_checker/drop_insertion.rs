//! Drop node insertion for precise value cleanup.
//!
//! Inserts explicit Drop nodes in HIR at precise locations to ensure values
//! are cleaned up exactly when their ownership ends.

use crate::compiler::borrow_checker::last_use::LastUseAnalysis;
use crate::compiler::borrow_checker::types::{BorrowChecker, BorrowKind};
use crate::compiler::compiler_messages::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler::hir::nodes::{HirKind, HirNode, HirNodeId};
use crate::compiler::hir::place::Place;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::string_interning::StringTable;
use std::collections::{HashMap, HashSet};

/// Determine if a place needs cleanup (Drop insertion).
/// 
/// Heap-allocated strings created by templates need explicit cleanup,
/// while stack-allocated string slices do not.
fn place_needs_cleanup(place: &Place, hir_nodes: &[HirNode]) -> bool {
    // For now, we'll be conservative and assume all places might need cleanup
    // In the future, this should check the actual type and origin of the place
    
    // Check if this place was assigned a heap-allocated string
    for node in hir_nodes {
        if let HirKind::Assign { place: assign_place, value } = &node.kind {
            if assign_place == place {
                match &value.kind {
                    crate::compiler::hir::nodes::HirExprKind::HeapString(_) => {
                        // This place contains a heap-allocated string and needs cleanup
                        return true;
                    }
                    crate::compiler::hir::nodes::HirExprKind::Call { .. } |
                    crate::compiler::hir::nodes::HirExprKind::MethodCall { .. } => {
                        // Function calls might return heap-allocated strings
                        // For now, be conservative and assume cleanup is needed
                        return true;
                    }
                    _ => {}
                }
            }
        }
    }
    
    // Default to needing cleanup for safety
    true
}

/// Result of Drop insertion analysis
#[derive(Debug, Clone, Default)]
pub struct DropInsertionResult {
    /// Drop nodes to be inserted, mapped by insertion point
    pub drop_insertions: HashMap<HirNodeId, Vec<DropInsertion>>,
    
    /// Places that have been moved and should not get Drop nodes
    pub moved_places: HashSet<Place>,
    
    /// Scope exit points where Drop nodes are needed
    pub scope_exits: HashMap<HirNodeId, Vec<Place>>,
    
    /// Validation data for debugging
    pub validation_data: DropValidationData,
}

/// A single Drop node insertion
#[derive(Debug, Clone)]
pub struct DropInsertion {
    /// The place to be dropped
    pub place: Place,
    
    /// The insertion point (after which HIR node)
    pub after_node: HirNodeId,
    
    /// The reason for this Drop insertion
    pub reason: DropReason,
    
    /// New node ID for the Drop node
    pub drop_node_id: HirNodeId,
}

/// Reason for Drop insertion
#[derive(Debug, Clone, PartialEq)]
pub enum DropReason {
    /// Last use of a value
    LastUse,
    
    /// Scope exit for non-moved value
    ScopeExit,
    
    /// Explicit cleanup point
    ExplicitCleanup,
}

/// Validation data for Drop insertion
#[derive(Debug, Clone, Default)]
pub struct DropValidationData {
    /// All places analyzed for Drop insertion
    pub analyzed_places: HashSet<Place>,
    
    /// Validation errors found
    pub validation_errors: Vec<String>,
    
    /// Paths checked for Drop coverage
    pub paths_checked: Vec<PathCheck>,
}

/// Path coverage check for Drop insertion
#[derive(Debug, Clone)]
pub struct PathCheck {
    /// Description of the path
    pub description: String,
    
    /// Start node of the path
    pub start_node: HirNodeId,
    
    /// End node of the path
    pub end_node: HirNodeId,
    
    /// Whether Drop coverage is complete on this path
    pub coverage_complete: bool,
    
    /// Places that need Drop nodes on this path
    pub places_needing_drop: Vec<Place>,
}

/// Insert Drop nodes based on last-use analysis and move decisions
///
/// This is the main entry point for Drop insertion. It analyzes the HIR nodes,
/// integrates with last-use analysis and move decisions, and determines where
/// Drop nodes should be inserted.
pub fn insert_drop_nodes(
    checker: &BorrowChecker,
    hir_nodes: &mut Vec<HirNode>,
    last_use_analysis: &LastUseAnalysis,
) -> Result<DropInsertionResult, CompilerMessages> {
    // Step 1: Analyze move decisions to identify moved places
    let moved_places = analyze_moved_places(checker, hir_nodes);
    
    // Step 2: Identify Drop insertion points from last-use analysis
    let drop_insertions = identify_drop_insertion_points(
        checker,
        hir_nodes,
        last_use_analysis,
        &moved_places,
    )?;
    
    // Step 3: Identify scope exit Drop points
    let scope_exits = identify_scope_exit_drops(checker, hir_nodes, &moved_places)?;
    
    // Step 4: Validate Drop coverage on all paths
    let validation_data = validate_drop_coverage(
        checker,
        hir_nodes,
        &drop_insertions,
        &scope_exits,
        &moved_places,
    )?;
    
    // Step 5: Insert Drop nodes into HIR while preserving node ID ordering
    let mut result = DropInsertionResult {
        drop_insertions,
        moved_places,
        scope_exits,
        validation_data,
    };
    
    insert_drops_into_hir(hir_nodes, &mut result)?;
    
    Ok(result)
}

/// Analyze move decisions to identify places that have been moved
///
/// Moved places should not get Drop nodes at the move source, since ownership
/// has been transferred.
fn analyze_moved_places(checker: &BorrowChecker, hir_nodes: &[HirNode]) -> HashSet<Place> {
    let mut moved_places = HashSet::new();
    
    // Check all CFG nodes for Move borrows
    for cfg_node in checker.cfg.nodes.values() {
        for loan in cfg_node.borrow_state.active_borrows.values() {
            if loan.kind == BorrowKind::Move {
                moved_places.insert(loan.place.clone());
            }
        }
    }
    
    // Also check HIR nodes for explicit moves
    for node in hir_nodes {
        collect_moved_places_from_node(node, &mut moved_places);
    }
    
    moved_places
}

/// Recursively collect moved places from a HIR node
fn collect_moved_places_from_node(node: &HirNode, moved_places: &mut HashSet<Place>) {
    match &node.kind {
        HirKind::Assign { value, .. } => {
            if let crate::compiler::hir::nodes::HirExprKind::CandidateMove(place) = &value.kind {
                // Note: CandidateMove that becomes Move should be tracked
                // For now, we'll be conservative and not mark it as moved here
                // since the refinement might have kept it as a mutable borrow
                let _ = place; // Suppress unused warning
            }
        }
        
        HirKind::If { then_block, else_block, .. } => {
            for then_node in then_block {
                collect_moved_places_from_node(then_node, moved_places);
            }
            if let Some(else_nodes) = else_block {
                for else_node in else_nodes {
                    collect_moved_places_from_node(else_node, moved_places);
                }
            }
        }
        
        HirKind::Match { arms, default, .. } => {
            for arm in arms {
                for arm_node in &arm.body {
                    collect_moved_places_from_node(arm_node, moved_places);
                }
            }
            if let Some(default_nodes) = default {
                for default_node in default_nodes {
                    collect_moved_places_from_node(default_node, moved_places);
                }
            }
        }
        
        HirKind::Loop { body, .. } => {
            for body_node in body {
                collect_moved_places_from_node(body_node, moved_places);
            }
        }
        
        HirKind::FunctionDef { body, .. } => {
            for body_node in body {
                collect_moved_places_from_node(body_node, moved_places);
            }
        }
        
        HirKind::TryCall { call, error_handler, .. } => {
            collect_moved_places_from_node(call, moved_places);
            for handler_node in error_handler {
                collect_moved_places_from_node(handler_node, moved_places);
            }
        }
        
        HirKind::TemplateFn { body, .. } => {
            for body_node in body {
                collect_moved_places_from_node(body_node, moved_places);
            }
        }
        
        // Other node types don't contain nested nodes or moves
        _ => {}
    }
}

/// Identify Drop insertion points from last-use analysis
///
/// This creates Drop insertions immediately after the last use of values,
/// unless the value has been moved.
fn identify_drop_insertion_points(
    checker: &BorrowChecker,
    hir_nodes: &[HirNode],
    last_use_analysis: &LastUseAnalysis,
    moved_places: &HashSet<Place>,
) -> Result<HashMap<HirNodeId, Vec<DropInsertion>>, CompilerMessages> {
    let mut drop_insertions: HashMap<HirNodeId, Vec<DropInsertion>> = HashMap::new();
    let mut next_node_id = get_next_available_node_id(hir_nodes);
    
    // For each place with last-use information
    for (place, last_use_statements) in &last_use_analysis.place_to_last_uses {
        // Skip places that have been moved
        if moved_places.contains(place) {
            continue;
        }
        
        // Skip places that don't need cleanup (e.g., primitive types)
        if !place_needs_drop(place, checker.string_table) {
            continue;
        }
        
        // Insert Drop nodes after each last-use statement
        for &last_use_stmt in last_use_statements {
            let drop_insertion = DropInsertion {
                place: place.clone(),
                after_node: last_use_stmt,
                reason: DropReason::LastUse,
                drop_node_id: next_node_id,
            };
            
            drop_insertions
                .entry(last_use_stmt)
                .or_default()
                .push(drop_insertion);
            
            next_node_id += 1;
        }
    }
    
    Ok(drop_insertions)
}

/// Identify scope exit Drop points
///
/// This identifies places that go out of scope without being moved and need
/// Drop nodes at scope exits.
fn identify_scope_exit_drops(
    checker: &BorrowChecker,
    hir_nodes: &[HirNode],
    moved_places: &HashSet<Place>,
) -> Result<HashMap<HirNodeId, Vec<Place>>, CompilerMessages> {
    let mut scope_exits: HashMap<HirNodeId, Vec<Place>> = HashMap::new();
    
    // For now, implement a simple scope exit analysis
    // This will be enhanced as the borrow checker evolves
    
    // Identify function exit points as scope exits
    for exit_point in &checker.cfg.exit_points {
        let mut places_to_drop = Vec::new();
        
        // Find all places that are still active at this exit point
        if let Some(cfg_node) = checker.cfg.nodes.get(exit_point) {
            for loan in cfg_node.borrow_state.active_borrows.values() {
                // Skip moved places
                if moved_places.contains(&loan.place) {
                    continue;
                }
                
                // Skip places that don't need cleanup
                if !place_needs_drop(&loan.place, checker.string_table) {
                    continue;
                }
                
                // Only add places that haven't been explicitly dropped
                if !is_place_explicitly_dropped(&loan.place, hir_nodes) {
                    places_to_drop.push(loan.place.clone());
                }
            }
        }
        
        if !places_to_drop.is_empty() {
            scope_exits.insert(*exit_point, places_to_drop);
        }
    }
    
    Ok(scope_exits)
}

/// Validate Drop coverage on all execution paths
///
/// This ensures that Drop nodes are placed on all possible execution paths
/// where values need cleanup.
fn validate_drop_coverage(
    checker: &BorrowChecker,
    hir_nodes: &[HirNode],
    drop_insertions: &HashMap<HirNodeId, Vec<DropInsertion>>,
    scope_exits: &HashMap<HirNodeId, Vec<Place>>,
    moved_places: &HashSet<Place>,
) -> Result<DropValidationData, CompilerMessages> {
    let mut validation_data = DropValidationData::default();
    
    // Collect all places that need Drop analysis
    let mut all_places = HashSet::new();
    for node in hir_nodes {
        collect_places_from_node(node, &mut all_places);
    }
    
    // Remove moved places from analysis
    for moved_place in moved_places {
        all_places.remove(moved_place);
    }
    
    // Filter to only places that need Drop
    all_places.retain(|place| place_needs_drop(place, checker.string_table));
    
    validation_data.analyzed_places = all_places.clone();
    
    // Validate coverage for each path from entry to exit
    for &entry_point in &checker.cfg.entry_points {
        for &exit_point in &checker.cfg.exit_points {
            let path_check = validate_path_coverage(
                entry_point,
                exit_point,
                &all_places,
                drop_insertions,
                scope_exits,
                checker,
            );
            
            if !path_check.coverage_complete {
                validation_data.validation_errors.push(format!(
                    "Incomplete Drop coverage on path from {} to {}: missing drops for {:?}",
                    entry_point, exit_point, path_check.places_needing_drop
                ));
            }
            
            validation_data.paths_checked.push(path_check);
        }
    }
    
    Ok(validation_data)
}

/// Validate Drop coverage for a specific execution path
fn validate_path_coverage(
    start_node: HirNodeId,
    end_node: HirNodeId,
    places_needing_drop: &HashSet<Place>,
    drop_insertions: &HashMap<HirNodeId, Vec<DropInsertion>>,
    scope_exits: &HashMap<HirNodeId, Vec<Place>>,
    checker: &BorrowChecker,
) -> PathCheck {
    let mut coverage_complete = true;
    let mut places_still_needing_drop = places_needing_drop.clone();
    
    // For simplicity, assume all places get proper Drop coverage
    // This will be enhanced with actual path traversal in the future
    
    // Check if drop insertions cover the needed places
    for drop_list in drop_insertions.values() {
        for drop_insertion in drop_list {
            places_still_needing_drop.remove(&drop_insertion.place);
        }
    }
    
    // Check if scope exits cover remaining places
    for place_list in scope_exits.values() {
        for place in place_list {
            places_still_needing_drop.remove(place);
        }
    }
    
    // If any places still need Drop, coverage is incomplete
    if !places_still_needing_drop.is_empty() {
        coverage_complete = false;
    }
    
    PathCheck {
        description: format!("Path from {} to {}", start_node, end_node),
        start_node,
        end_node,
        coverage_complete,
        places_needing_drop: places_still_needing_drop.into_iter().collect(),
    }
}

/// Insert Drop nodes into HIR while preserving node ID ordering
///
/// This modifies the HIR by inserting Drop nodes at the identified locations
/// while maintaining the structural integrity and node ID ordering.
fn insert_drops_into_hir(
    hir_nodes: &mut Vec<HirNode>,
    result: &mut DropInsertionResult,
) -> Result<(), CompilerMessages> {
    // Collect all Drop nodes to insert, sorted by insertion point
    let mut insertions_by_node: Vec<(HirNodeId, Vec<DropInsertion>)> = 
        result.drop_insertions.iter()
            .map(|(&node_id, insertions)| (node_id, insertions.clone()))
            .collect();
    
    // Sort by node ID to maintain ordering
    insertions_by_node.sort_by_key(|(node_id, _)| *node_id);
    
    // Insert Drop nodes in reverse order to maintain indices
    for (after_node_id, drop_insertions) in insertions_by_node.into_iter().rev() {
        // Find the position to insert after
        if let Some(insert_pos) = find_insertion_position(hir_nodes, after_node_id) {
            // Insert Drop nodes after this position
            for (i, drop_insertion) in drop_insertions.into_iter().enumerate() {
                let drop_node = create_drop_node(drop_insertion);
                hir_nodes.insert(insert_pos + 1 + i, drop_node);
            }
        }
    }
    
    // Insert scope exit Drop nodes
    for (&exit_node_id, places) in &result.scope_exits {
        if let Some(insert_pos) = find_insertion_position(hir_nodes, exit_node_id) {
            let mut next_node_id = get_next_available_node_id(hir_nodes);
            
            for (i, place) in places.iter().enumerate() {
                let drop_insertion = DropInsertion {
                    place: place.clone(),
                    after_node: exit_node_id,
                    reason: DropReason::ScopeExit,
                    drop_node_id: next_node_id,
                };
                
                let drop_node = create_drop_node(drop_insertion);
                hir_nodes.insert(insert_pos + 1 + i, drop_node);
                next_node_id += 1;
            }
        }
    }
    
    Ok(())
}

/// Find the insertion position for a Drop node after the given HIR node
fn find_insertion_position(hir_nodes: &[HirNode], after_node_id: HirNodeId) -> Option<usize> {
    hir_nodes.iter().position(|node| node.id == after_node_id)
}

/// Create a Drop HIR node from a Drop insertion
fn create_drop_node(drop_insertion: DropInsertion) -> HirNode {
    HirNode {
        kind: HirKind::Drop(drop_insertion.place),
        location: TextLocation::default(), // Use default location for inserted nodes
        scope: crate::compiler::interned_path::InternedPath::default(),
        id: drop_insertion.drop_node_id,
    }
}

/// Get the next available HIR node ID
fn get_next_available_node_id(hir_nodes: &[HirNode]) -> HirNodeId {
    hir_nodes.iter().map(|node| node.id).max().unwrap_or(0) + 1
}

/// Check if a place needs Drop cleanup
///
/// This determines whether a place represents a value that needs explicit
/// cleanup when it goes out of scope. Heap-allocated strings created by
/// templates need cleanup, while stack-allocated primitives do not.
fn place_needs_drop(place: &Place, string_table: &StringTable) -> bool {
    // Check if this is a primitive type that doesn't need Drop
    match &place.root {
        crate::compiler::hir::place::PlaceRoot::Local(name) => {
            // Simple heuristic: if the name suggests a primitive type, skip Drop
            let name_str = string_table.resolve(*name);
            if name_str.contains("int") || name_str.contains("float") || name_str.contains("bool") {
                return false;
            }
            
            // Temporary variables created for heap strings need cleanup
            if name_str.starts_with("_temp_") || name_str.contains("template") || name_str.contains("heap") {
                return true;
            }
        }
        _ => {}
    }
    
    // Default to needing Drop for safety - this includes heap-allocated strings
    // In the future, this should use proper type information to make precise decisions
    true
}

/// Check if a place has been explicitly dropped in the HIR
fn is_place_explicitly_dropped(place: &Place, hir_nodes: &[HirNode]) -> bool {
    for node in hir_nodes {
        if let HirKind::Drop(dropped_place) = &node.kind {
            if dropped_place == place {
                return true;
            }
        }
        
        // Check nested nodes
        if is_place_dropped_in_nested_nodes(place, node) {
            return true;
        }
    }
    
    false
}

/// Check if a place is dropped in nested HIR nodes
fn is_place_dropped_in_nested_nodes(place: &Place, node: &HirNode) -> bool {
    match &node.kind {
        HirKind::If { then_block, else_block, .. } => {
            for then_node in then_block {
                if let HirKind::Drop(dropped_place) = &then_node.kind {
                    if dropped_place == place {
                        return true;
                    }
                }
                if is_place_dropped_in_nested_nodes(place, then_node) {
                    return true;
                }
            }
            
            if let Some(else_nodes) = else_block {
                for else_node in else_nodes {
                    if let HirKind::Drop(dropped_place) = &else_node.kind {
                        if dropped_place == place {
                            return true;
                        }
                    }
                    if is_place_dropped_in_nested_nodes(place, else_node) {
                        return true;
                    }
                }
            }
        }
        
        HirKind::Match { arms, default, .. } => {
            for arm in arms {
                for arm_node in &arm.body {
                    if let HirKind::Drop(dropped_place) = &arm_node.kind {
                        if dropped_place == place {
                            return true;
                        }
                    }
                    if is_place_dropped_in_nested_nodes(place, arm_node) {
                        return true;
                    }
                }
            }
            
            if let Some(default_nodes) = default {
                for default_node in default_nodes {
                    if let HirKind::Drop(dropped_place) = &default_node.kind {
                        if dropped_place == place {
                            return true;
                        }
                    }
                    if is_place_dropped_in_nested_nodes(place, default_node) {
                        return true;
                    }
                }
            }
        }
        
        HirKind::Loop { body, .. } => {
            for body_node in body {
                if let HirKind::Drop(dropped_place) = &body_node.kind {
                    if dropped_place == place {
                        return true;
                    }
                }
                if is_place_dropped_in_nested_nodes(place, body_node) {
                    return true;
                }
            }
        }
        
        HirKind::FunctionDef { body, .. } => {
            for body_node in body {
                if let HirKind::Drop(dropped_place) = &body_node.kind {
                    if dropped_place == place {
                        return true;
                    }
                }
                if is_place_dropped_in_nested_nodes(place, body_node) {
                    return true;
                }
            }
        }
        
        HirKind::TryCall { call, error_handler, .. } => {
            if is_place_dropped_in_nested_nodes(place, call) {
                return true;
            }
            
            for handler_node in error_handler {
                if let HirKind::Drop(dropped_place) = &handler_node.kind {
                    if dropped_place == place {
                        return true;
                    }
                }
                if is_place_dropped_in_nested_nodes(place, handler_node) {
                    return true;
                }
            }
        }
        
        HirKind::TemplateFn { body, .. } => {
            for body_node in body {
                if let HirKind::Drop(dropped_place) = &body_node.kind {
                    if dropped_place == place {
                        return true;
                    }
                }
                if is_place_dropped_in_nested_nodes(place, body_node) {
                    return true;
                }
            }
        }
        
        // Other node types don't contain nested nodes
        _ => {}
    }
    
    false
}

/// Collect all places from a HIR node and its nested nodes
fn collect_places_from_node(node: &HirNode, places: &mut HashSet<Place>) {
    match &node.kind {
        HirKind::Assign { place, value } => {
            places.insert(place.clone());
            collect_places_from_expression(value, places);
        }
        
        HirKind::Borrow { place, target, .. } => {
            places.insert(place.clone());
            places.insert(target.clone());
        }
        
        HirKind::Call { args, returns, .. } | HirKind::HostCall { args, returns, .. } => {
            for arg in args {
                places.insert(arg.clone());
            }
            for ret in returns {
                places.insert(ret.clone());
            }
        }
        
        HirKind::Return(return_places) => {
            for place in return_places {
                places.insert(place.clone());
            }
        }
        
        HirKind::ReturnError(place) => {
            places.insert(place.clone());
        }
        
        HirKind::Drop(place) => {
            places.insert(place.clone());
        }
        
        HirKind::ExprStmt(place) => {
            places.insert(place.clone());
        }
        
        HirKind::If { condition, then_block, else_block } => {
            places.insert(condition.clone());
            
            for then_node in then_block {
                collect_places_from_node(then_node, places);
            }
            
            if let Some(else_nodes) = else_block {
                for else_node in else_nodes {
                    collect_places_from_node(else_node, places);
                }
            }
        }
        
        HirKind::Match { scrutinee, arms, default } => {
            places.insert(scrutinee.clone());
            
            for arm in arms {
                for arm_node in &arm.body {
                    collect_places_from_node(arm_node, places);
                }
            }
            
            if let Some(default_nodes) = default {
                for default_node in default_nodes {
                    collect_places_from_node(default_node, places);
                }
            }
        }
        
        HirKind::Loop { iterator, body, .. } => {
            places.insert(iterator.clone());
            
            for body_node in body {
                collect_places_from_node(body_node, places);
            }
        }
        
        HirKind::FunctionDef { body, .. } => {
            for body_node in body {
                collect_places_from_node(body_node, places);
            }
        }
        
        HirKind::TryCall { call, error_handler, .. } => {
            collect_places_from_node(call, places);
            
            for handler_node in error_handler {
                collect_places_from_node(handler_node, places);
            }
        }
        
        HirKind::TemplateFn { body, .. } => {
            for body_node in body {
                collect_places_from_node(body_node, places);
            }
        }
        
        // Other node types don't use places directly
        _ => {}
    }
}

/// Collect places from a HIR expression
fn collect_places_from_expression(
    expr: &crate::compiler::hir::nodes::HirExpr,
    places: &mut HashSet<Place>,
) {
    use crate::compiler::hir::nodes::HirExprKind;
    
    match &expr.kind {
        HirExprKind::Load(place) => {
            places.insert(place.clone());
        }
        
        HirExprKind::SharedBorrow(place) | HirExprKind::MutableBorrow(place) => {
            places.insert(place.clone());
        }
        
        HirExprKind::CandidateMove(place) => {
            places.insert(place.clone());
        }
        
        HirExprKind::BinOp { left, right, .. } => {
            places.insert(left.clone());
            places.insert(right.clone());
        }
        
        HirExprKind::UnaryOp { operand, .. } => {
            places.insert(operand.clone());
        }
        
        HirExprKind::Call { args, .. } => {
            for arg in args {
                places.insert(arg.clone());
            }
        }
        
        HirExprKind::MethodCall { receiver, args, .. } => {
            places.insert(receiver.clone());
            for arg in args {
                places.insert(arg.clone());
            }
        }
        
        HirExprKind::StructConstruct { fields, .. } => {
            for (_, field_place) in fields {
                places.insert(field_place.clone());
            }
        }
        
        HirExprKind::Collection(element_places) => {
            for place in element_places {
                places.insert(place.clone());
            }
        }
        
        HirExprKind::Range { start, end } => {
            places.insert(start.clone());
            places.insert(end.clone());
        }
        
        // Literals don't use places, except heap strings which are owned
        HirExprKind::Int(_)
        | HirExprKind::Float(_)
        | HirExprKind::Bool(_)
        | HirExprKind::StringLiteral(_)
        | HirExprKind::Char(_) => {}

        HirExprKind::HeapString(_) => {
            // Heap-allocated strings are owned values that will be assigned to places
            // The place they're assigned to will need Drop insertion when it goes out of scope
            // No direct place collection here since this is the creation expression
        }
    }
}