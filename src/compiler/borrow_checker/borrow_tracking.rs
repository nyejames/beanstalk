//! Borrow tracking system for the borrow checker.
//!
//! Handles creation, propagation, and management of borrows across the control
//! flow graph for precise lifetime analysis and conflict detection.

use crate::compiler::borrow_checker::types::{BorrowChecker, BorrowKind, Loan, BorrowState};
use crate::compiler::compiler_messages::compiler_errors::CompilerMessages;
use crate::compiler::hir::nodes::{HirExprKind, HirKind, HirNode, HirNodeId, HirMatchArm};
use std::collections::{HashSet, VecDeque};

/// Check if a place contains a heap-allocated string.
/// 
/// Used to determine if special ownership handling is needed for
/// function parameter passing and assignment operations.
fn is_heap_string_place(place: &crate::compiler::hir::place::Place, checker: &BorrowChecker) -> bool {
    // For now, use a simple heuristic based on place name
    // In the future, this should use proper type information
    match &place.root {
        crate::compiler::hir::place::PlaceRoot::Local(name) => {
            let name_str = checker.string_table.resolve(*name);
            // Check if this looks like a heap string variable
            name_str.contains("template") || name_str.contains("heap") || name_str.starts_with("_temp_")
        }
        _ => false,
    }
}

/// Track borrows across the control flow graph.
///
/// Performs main borrow tracking analysis, creating borrows for Load and
/// CandidateMove operations and propagating borrow state through the CFG.
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
            process_assign_node(checker, node, value)
        }

        HirKind::Borrow { place, kind, target } => {
            process_explicit_borrow(checker, node, place, kind, target)
        }

        HirKind::Call { args, .. } | HirKind::HostCall { args, .. } => {
            process_function_call(checker, node, args)
        }

        HirKind::If { condition, then_block, else_block } => {
            process_if_statement(checker, node, condition, then_block, else_block)
        }

        HirKind::Match { scrutinee, arms, default } => {
            process_match_statement(checker, node, scrutinee, arms, default)
        }

        HirKind::Loop { iterator, body, .. } => {
            process_loop_statement(checker, node, iterator, body)
        }

        HirKind::Return(places) => {
            process_return_statement(checker, node, places)
        }

        HirKind::ReturnError(place) => {
            process_return_error(checker, node, place)
        }

        HirKind::ExprStmt(place) => {
            process_expression_statement(checker, node, place)
        }

        // Other node types don't create borrows directly
        _ => Ok(()),
    }
}

/// Process assignment node for borrow creation
fn process_assign_node(
    checker: &mut BorrowChecker,
    node: &HirNode,
    value: &crate::compiler::hir::nodes::HirExpr,
) -> Result<(), CompilerMessages> {
    // Create borrows based on the value expression
    // For heap strings, this represents the creation or transfer of ownership
    process_expression_for_borrows(checker, value, node.id)?;
    
    // Special handling for heap string assignments
    if let HirExprKind::HeapString(_) = &value.kind {
        // This is the creation of a new heap-allocated string
        // No additional borrow tracking needed here since this is the creation point
        // The place will be tracked for Drop insertion later
    }

    Ok(())
}

/// Process explicit borrow creation
fn process_explicit_borrow(
    checker: &mut BorrowChecker,
    node: &HirNode,
    place: &crate::compiler::hir::place::Place,
    kind: &crate::compiler::hir::nodes::BorrowKind,
    target: &crate::compiler::hir::place::Place,
) -> Result<(), CompilerMessages> {
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

    Ok(())
}

/// Process function call for argument borrows
fn process_function_call(
    checker: &mut BorrowChecker,
    node: &HirNode,
    args: &[crate::compiler::hir::place::Place],
) -> Result<(), CompilerMessages> {
    // Create borrows for function arguments
    // For heap strings, this represents ownership transfer validation
    for arg_place in args {
        let borrow_id = checker.next_borrow_id();
        
        // Check if this argument is a heap-allocated string
        let borrow_kind = if is_heap_string_place(arg_place, checker) {
            // Heap strings passed to functions need careful ownership analysis
            // This will be refined by candidate move analysis
            BorrowKind::Mutable
        } else {
            BorrowKind::Shared
        };
        
        let loan = Loan::new(borrow_id, arg_place.clone(), borrow_kind, node.id);

        if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node.id) {
            cfg_node.borrow_state.add_borrow(loan);
        }
    }

    Ok(())
}

/// Process if statement for condition borrow and nested blocks
fn process_if_statement(
    checker: &mut BorrowChecker,
    node: &HirNode,
    condition: &crate::compiler::hir::place::Place,
    then_block: &[HirNode],
    else_block: &Option<Vec<HirNode>>,
) -> Result<(), CompilerMessages> {
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

    Ok(())
}

/// Process match statement for scrutinee borrow and arms
fn process_match_statement(
    checker: &mut BorrowChecker,
    node: &HirNode,
    scrutinee: &crate::compiler::hir::place::Place,
    arms: &[HirMatchArm],
    default: &Option<Vec<HirNode>>,
) -> Result<(), CompilerMessages> {
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

    Ok(())
}

/// Process loop statement for iterator borrow and body
fn process_loop_statement(
    checker: &mut BorrowChecker,
    node: &HirNode,
    iterator: &crate::compiler::hir::place::Place,
    body: &[HirNode],
) -> Result<(), CompilerMessages> {
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

    Ok(())
}

/// Process return statement for return value borrows
fn process_return_statement(
    checker: &mut BorrowChecker,
    node: &HirNode,
    places: &[crate::compiler::hir::place::Place],
) -> Result<(), CompilerMessages> {
    // Create borrows for return values
    for return_place in places {
        let borrow_id = checker.next_borrow_id();
        let loan = Loan::new(borrow_id, return_place.clone(), BorrowKind::Shared, node.id);

        if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node.id) {
            cfg_node.borrow_state.add_borrow(loan);
        }
    }

    Ok(())
}

/// Process return error statement
fn process_return_error(
    checker: &mut BorrowChecker,
    node: &HirNode,
    place: &crate::compiler::hir::place::Place,
) -> Result<(), CompilerMessages> {
    // Create borrow for error return
    let borrow_id = checker.next_borrow_id();
    let loan = Loan::new(borrow_id, place.clone(), BorrowKind::Shared, node.id);

    if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node.id) {
        cfg_node.borrow_state.add_borrow(loan);
    }

    Ok(())
}

/// Process expression statement
fn process_expression_statement(
    checker: &mut BorrowChecker,
    node: &HirNode,
    place: &crate::compiler::hir::place::Place,
) -> Result<(), CompilerMessages> {
    // Create borrow for expression statement
    let borrow_id = checker.next_borrow_id();
    let loan = Loan::new(borrow_id, place.clone(), BorrowKind::Shared, node.id);

    if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node.id) {
        cfg_node.borrow_state.add_borrow(loan);
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
            create_shared_borrow(checker, place, node_id);
        }

        HirExprKind::SharedBorrow(place) => {
            create_shared_borrow(checker, place, node_id);
        }

        HirExprKind::MutableBorrow(place) => {
            create_mutable_borrow(checker, place, node_id);
        }

        HirExprKind::CandidateMove(place) => {
            create_candidate_move_borrow(checker, place, node_id);
        }

        HirExprKind::BinOp { left, right, .. } => {
            create_binary_operation_borrows(checker, left, right, node_id);
        }

        HirExprKind::UnaryOp { operand, .. } => {
            create_shared_borrow(checker, operand, node_id);
        }

        HirExprKind::Call { args, .. } => {
            create_function_call_borrows(checker, args, node_id);
        }

        HirExprKind::MethodCall { receiver, args, .. } => {
            create_method_call_borrows(checker, receiver, args, node_id);
        }

        HirExprKind::StructConstruct { fields, .. } => {
            create_struct_construct_borrows(checker, fields, node_id);
        }

        HirExprKind::Collection(places) => {
            create_collection_borrows(checker, places, node_id);
        }

        HirExprKind::Range { start, end } => {
            create_range_borrows(checker, start, end, node_id);
        }

        // Literal expressions don't create borrows, except heap strings
        HirExprKind::Int(_)
        | HirExprKind::Float(_)
        | HirExprKind::Bool(_)
        | HirExprKind::StringLiteral(_)
        | HirExprKind::Char(_) => {}

        HirExprKind::HeapString(_) => {
            // Heap-allocated strings are owned values that need tracking
            // They are created by runtime templates and need proper ownership management
            // No borrow is created here since this is the creation point
            // The string will be assigned to a place and tracked from there
        }
    }

    Ok(())
}

// Helper functions for borrow creation and CFG node management

/// Create a shared borrow loan for the given place and node
fn create_shared_borrow_loan(
    checker: &mut BorrowChecker,
    place: &crate::compiler::hir::place::Place,
    node_id: HirNodeId,
) -> Loan {
    let borrow_id = checker.next_borrow_id();
    Loan::new(borrow_id, place.clone(), BorrowKind::Shared, node_id)
}

/// Create a mutable borrow loan for the given place and node
fn create_mutable_borrow_loan(
    checker: &mut BorrowChecker,
    place: &crate::compiler::hir::place::Place,
    node_id: HirNodeId,
) -> Loan {
    let borrow_id = checker.next_borrow_id();
    Loan::new(borrow_id, place.clone(), BorrowKind::Mutable, node_id)
}

/// Add a loan to the CFG node's borrow state
fn add_loan_to_cfg_node(
    checker: &mut BorrowChecker,
    node_id: HirNodeId,
    loan: Loan,
) {
    if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
        cfg_node.borrow_state.add_borrow(loan);
    }
}

/// Create a shared borrow for a place
fn create_shared_borrow(
    checker: &mut BorrowChecker,
    place: &crate::compiler::hir::place::Place,
    node_id: HirNodeId,
) {
    let loan = create_shared_borrow_loan(checker, place, node_id);
    add_loan_to_cfg_node(checker, node_id, loan);
}

/// Create a mutable borrow for a place
fn create_mutable_borrow(
    checker: &mut BorrowChecker,
    place: &crate::compiler::hir::place::Place,
    node_id: HirNodeId,
) {
    let loan = create_mutable_borrow_loan(checker, place, node_id);
    add_loan_to_cfg_node(checker, node_id, loan);
}

/// Create a candidate move borrow for a place
fn create_candidate_move_borrow(
    checker: &mut BorrowChecker,
    place: &crate::compiler::hir::place::Place,
    node_id: HirNodeId,
) {
    // Create candidate move borrow (will be refined by last-use analysis)
    // This is treated conservatively as mutable for conflict detection
    let borrow_id = checker.next_borrow_id();
    let loan = Loan::new(borrow_id, place.clone(), BorrowKind::CandidateMove, node_id);
    add_loan_to_cfg_node(checker, node_id, loan);
}

/// Create borrows for binary operation operands
fn create_binary_operation_borrows(
    checker: &mut BorrowChecker,
    left: &crate::compiler::hir::place::Place,
    right: &crate::compiler::hir::place::Place,
    node_id: HirNodeId,
) {
    create_shared_borrow(checker, left, node_id);
    create_shared_borrow(checker, right, node_id);
}

/// Create borrows for function call arguments
fn create_function_call_borrows(
    checker: &mut BorrowChecker,
    args: &[crate::compiler::hir::place::Place],
    node_id: HirNodeId,
) {
    for arg_place in args {
        create_shared_borrow(checker, arg_place, node_id);
    }
}

/// Create borrows for method call receiver and arguments
fn create_method_call_borrows(
    checker: &mut BorrowChecker,
    receiver: &crate::compiler::hir::place::Place,
    args: &[crate::compiler::hir::place::Place],
    node_id: HirNodeId,
) {
    create_shared_borrow(checker, receiver, node_id);
    
    for arg_place in args {
        create_shared_borrow(checker, arg_place, node_id);
    }
}

/// Create borrows for struct construction field values
fn create_struct_construct_borrows(
    checker: &mut BorrowChecker,
    fields: &[(crate::compiler::string_interning::InternedString, crate::compiler::hir::place::Place)],
    node_id: HirNodeId,
) {
    for (_, field_place) in fields {
        create_shared_borrow(checker, field_place, node_id);
    }
}

/// Create borrows for collection elements
fn create_collection_borrows(
    checker: &mut BorrowChecker,
    places: &[crate::compiler::hir::place::Place],
    node_id: HirNodeId,
) {
    for element_place in places {
        create_shared_borrow(checker, element_place, node_id);
    }
}

/// Create borrows for range bounds
fn create_range_borrows(
    checker: &mut BorrowChecker,
    start: &crate::compiler::hir::place::Place,
    end: &crate::compiler::hir::place::Place,
    node_id: HirNodeId,
) {
    create_shared_borrow(checker, start, node_id);
    create_shared_borrow(checker, end, node_id);
}

/// Propagate borrow state through the control flow graph.
///
/// Implements worklist-based dataflow analysis to propagate borrow state
/// through the CFG with conservative merging at join points and iterative
/// refinement until fixed point is reached.
fn propagate_borrow_state(checker: &mut BorrowChecker) -> Result<(), CompilerMessages> {
    let mut work_list = initialize_worklist(checker);
    let mut in_worklist: HashSet<HirNodeId> = checker.cfg.entry_points.iter().copied().collect();
    
    let mut iterations = 0;
    let max_iterations = checker.cfg.nodes.len() * 10; // Prevent infinite loops

    while let Some(node_id) = work_list.pop_front() {
        in_worklist.remove(&node_id);
        
        iterations += 1;
        if iterations > max_iterations {
            // Safety limit reached - this shouldn't happen with well-formed CFGs
            break;
        }

        // Merge incoming borrow states from predecessors
        merge_predecessor_states(checker, node_id)?;

        // Propagate to successors if they would change
        propagate_to_successors(checker, node_id, &mut work_list, &mut in_worklist);
    }

    Ok(())
}

/// Initialize the worklist with entry points
fn initialize_worklist(checker: &BorrowChecker) -> VecDeque<HirNodeId> {
    checker.cfg.entry_points.iter().copied().collect()
}

/// Merge borrow states from all predecessor nodes
fn merge_predecessor_states(checker: &mut BorrowChecker, node_id: HirNodeId) -> Result<(), CompilerMessages> {
    let predecessors = checker.cfg.predecessors_slice(node_id);

    if predecessors.is_empty() {
        return Ok(());
    }

    // Collect states from predecessors and current node
    let (predecessor_states, current_node_state) = collect_borrow_states(checker, node_id, predecessors);

    // Apply the appropriate merging strategy
    apply_merging_strategy(checker, node_id, predecessor_states, current_node_state);

    Ok(())
}

/// Collect borrow states from predecessors and current node
fn collect_borrow_states(
    checker: &BorrowChecker,
    node_id: HirNodeId,
    predecessors: &[HirNodeId],
) -> (Vec<BorrowState>, Option<BorrowState>) {
    let mut predecessor_states = Vec::with_capacity(predecessors.len());
    let mut current_node_state = None;
    
    // Get the current node's state
    if let Some(current_node) = checker.cfg.nodes.get(&node_id) {
        current_node_state = Some(current_node.borrow_state.clone());
    }
    
    // Collect predecessor states
    for &pred_id in predecessors {
        if let Some(pred_node) = checker.cfg.nodes.get(&pred_id) {
            predecessor_states.push(pred_node.borrow_state.clone());
        }
    }

    (predecessor_states, current_node_state)
}

/// Apply the appropriate merging strategy based on number of predecessors
fn apply_merging_strategy(
    checker: &mut BorrowChecker,
    node_id: HirNodeId,
    predecessor_states: Vec<BorrowState>,
    current_node_state: Option<BorrowState>,
) {
    if let (Some(current_node), Some(own_borrows)) = 
        (checker.cfg.nodes.get_mut(&node_id), current_node_state) {
        
        match predecessor_states.len() {
            0 => {
                // No predecessors - keep own borrows only
                current_node.borrow_state = own_borrows.clone();
            }
            1 => {
                apply_single_predecessor_merge(current_node, &own_borrows, &predecessor_states[0]);
            }
            _ => {
                apply_multiple_predecessor_merge(current_node, &own_borrows, &predecessor_states);
            }
        }

        // Re-add own borrows that were created at this node
        restore_own_borrows(current_node, &own_borrows);
    }
}

/// Apply merging strategy for single predecessor
fn apply_single_predecessor_merge(
    current_node: &mut crate::compiler::borrow_checker::types::CfgNode,
    own_borrows: &BorrowState,
    predecessor_state: &BorrowState,
) {
    if own_borrows.is_empty() {
        current_node.borrow_state = predecessor_state.clone();
    } else {
        current_node.borrow_state = own_borrows.clone();
        current_node.borrow_state.union_merge(predecessor_state);
    }
}

/// Apply merging strategy for multiple predecessors (Polonius-style)
fn apply_multiple_predecessor_merge(
    current_node: &mut crate::compiler::borrow_checker::types::CfgNode,
    own_borrows: &BorrowState,
    predecessor_states: &[BorrowState],
) {
    // Multiple predecessors - use conservative merge (intersection)
    // This implements Polonius-style analysis where conflicts
    // are only errors if they exist on ALL incoming paths
    if own_borrows.is_empty() && !predecessor_states.is_empty() {
        current_node.borrow_state = predecessor_states[0].clone();
        for pred_state in predecessor_states.iter().skip(1) {
            current_node.borrow_state.merge(pred_state);
        }
    } else {
        current_node.borrow_state = own_borrows.clone();
        for pred_state in predecessor_states {
            current_node.borrow_state.merge(pred_state);
        }
    }
}

/// Restore own borrows that were created at this node
fn restore_own_borrows(
    current_node: &mut crate::compiler::borrow_checker::types::CfgNode,
    own_borrows: &BorrowState,
) {
    for loan in own_borrows.active_borrows.values() {
        if !current_node.borrow_state.active_borrows.contains_key(&loan.id) {
            current_node.borrow_state.add_borrow(loan.clone());
        }
    }
}

/// Propagate current state to successors that would change
fn propagate_to_successors(
    checker: &mut BorrowChecker,
    node_id: HirNodeId,
    work_list: &mut VecDeque<HirNodeId>,
    in_worklist: &mut HashSet<HirNodeId>,
) {
    // Get current node's borrow state after merging
    let current_state = if let Some(node) = checker.cfg.nodes.get(&node_id) {
        &node.borrow_state
    } else {
        return;
    };

    // Get successors and check if any would change
    let successors = checker.cfg.successors(node_id);
    let changed_successors = find_changed_successors(checker, successors, current_state, in_worklist);

    // Add changed successors to worklist
    for successor_id in changed_successors {
        work_list.push_back(successor_id);
        in_worklist.insert(successor_id);
    }
}

/// Find successors that would change if current state is propagated
fn find_changed_successors(
    checker: &BorrowChecker,
    successors: &[HirNodeId],
    current_state: &BorrowState,
    in_worklist: &HashSet<HirNodeId>,
) -> Vec<HirNodeId> {
    let mut changed_successors = Vec::new();

    for &successor_id in successors {
        if let Some(successor_node) = checker.cfg.nodes.get(&successor_id) {
            let would_change = would_union_merge_change(&successor_node.borrow_state, current_state);
            if would_change && !in_worklist.contains(&successor_id) {
                changed_successors.push(successor_id);
            }
        }
    }

    changed_successors
}

/// Check if union merge would change the target state without actually performing it
/// **Optimization**: Avoids expensive cloning for change detection
fn would_union_merge_change(target: &BorrowState, source: &BorrowState) -> bool {
    // Check if source has any borrows that target doesn't have
    for &borrow_id in source.active_borrows.keys() {
        if !target.active_borrows.contains_key(&borrow_id) {
            return true;
        }
    }
    
    // Check if source has any last uses that would update target
    for (place, &source_node_id) in &source.last_uses {
        match target.last_uses.get(place) {
            Some(&target_node_id) => {
                if source_node_id > target_node_id {
                    return true;
                }
            }
            None => return true,
        }
    }
    
    false
}
