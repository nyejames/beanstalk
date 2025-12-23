//! Borrow tracking system for the borrow checker.
//! 
//! Handles creation, propagation, and management of borrows across the control
//! flow graph, enabling precise lifetime analysis and conflict detection.

use crate::compiler::borrow_checker::types::{BorrowChecker, BorrowKind, Loan};
use crate::compiler::compiler_messages::compiler_errors::CompilerMessages;
use crate::compiler::hir::nodes::{HirExprKind, HirKind, HirNode, HirNodeId};

/// Check if a place contains a heap-allocated string for special ownership handling.
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
            process_assign_node(checker, value, node.id)
        }

        HirKind::Borrow { place, kind, target } => {
            process_borrow_node(checker, place, kind, target, node.id)
        }

        HirKind::Call { args, .. } | HirKind::HostCall { args, .. } => {
            process_call_node(checker, args, node.id)
        }

        HirKind::If { condition, then_block, else_block } => {
            process_if_node(checker, condition, then_block, else_block, node.id)
        }

        HirKind::Match { scrutinee, arms, default } => {
            process_match_node(checker, scrutinee, arms, default, node.id)
        }

        HirKind::Loop { iterator, body, .. } => {
            process_loop_node(checker, iterator, body, node.id)
        }

        HirKind::Return(places) => {
            process_return_node(checker, places, node.id)
        }

        HirKind::ReturnError(place) => {
            process_return_error_node(checker, place, node.id)
        }

        HirKind::ExprStmt(place) => {
            process_expr_stmt_node(checker, place, node.id)
        }

        // Other node types don't create borrows directly
        _ => Ok(()),
    }
}

/// Process assignment node for borrow creation
fn process_assign_node(
    checker: &mut BorrowChecker,
    value: &crate::compiler::hir::nodes::HirExpr,
    node_id: HirNodeId,
) -> Result<(), CompilerMessages> {
    process_expression_for_borrows(checker, value, node_id)?;
    
    // Special handling for heap string assignments
    if let HirExprKind::HeapString(_) = &value.kind {
        // This is the creation of a new heap-allocated string
        // No additional borrow tracking needed here since this is the creation point
    }
    
    Ok(())
}

/// Process explicit borrow node
fn process_borrow_node(
    checker: &mut BorrowChecker,
    place: &crate::compiler::hir::place::Place,
    kind: &crate::compiler::hir::nodes::BorrowKind,
    _target: &crate::compiler::hir::place::Place,
    node_id: HirNodeId,
) -> Result<(), CompilerMessages> {
    let borrow_id = checker.next_borrow_id();
    let borrow_kind = match kind {
        crate::compiler::hir::nodes::BorrowKind::Shared => BorrowKind::Shared,
        crate::compiler::hir::nodes::BorrowKind::Mutable => BorrowKind::Mutable,
    };
    let loan = Loan::new(borrow_id, place.clone(), borrow_kind, node_id);

    if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
        cfg_node.borrow_state.add_borrow(loan);
    }
    
    Ok(())
}

/// Process function call node for argument borrows
fn process_call_node(
    checker: &mut BorrowChecker,
    args: &[crate::compiler::hir::place::Place],
    node_id: HirNodeId,
) -> Result<(), CompilerMessages> {
    for arg_place in args {
        let borrow_id = checker.next_borrow_id();
        
        let borrow_kind = if is_heap_string_place(arg_place, checker) {
            BorrowKind::Mutable
        } else {
            BorrowKind::Shared
        };
        
        let loan = Loan::new(borrow_id, arg_place.clone(), borrow_kind, node_id);

        if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
            cfg_node.borrow_state.add_borrow(loan);
        }
    }
    
    Ok(())
}

/// Process if statement node
fn process_if_node(
    checker: &mut BorrowChecker,
    condition: &crate::compiler::hir::place::Place,
    then_block: &[HirNode],
    else_block: &Option<Vec<HirNode>>,
    node_id: HirNodeId,
) -> Result<(), CompilerMessages> {
    // Create borrow for condition
    let borrow_id = checker.next_borrow_id();
    let loan = Loan::new(borrow_id, condition.clone(), BorrowKind::Shared, node_id);

    if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
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

/// Process match statement node
fn process_match_node(
    checker: &mut BorrowChecker,
    scrutinee: &crate::compiler::hir::place::Place,
    arms: &[crate::compiler::hir::nodes::HirMatchArm],
    default: &Option<Vec<HirNode>>,
    node_id: HirNodeId,
) -> Result<(), CompilerMessages> {
    // Create borrow for scrutinee
    let borrow_id = checker.next_borrow_id();
    let loan = Loan::new(borrow_id, scrutinee.clone(), BorrowKind::Shared, node_id);

    if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
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

/// Process loop node
fn process_loop_node(
    checker: &mut BorrowChecker,
    iterator: &crate::compiler::hir::place::Place,
    body: &[HirNode],
    node_id: HirNodeId,
) -> Result<(), CompilerMessages> {
    // Create borrow for iterator
    let borrow_id = checker.next_borrow_id();
    let loan = Loan::new(borrow_id, iterator.clone(), BorrowKind::Shared, node_id);

    if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
        cfg_node.borrow_state.add_borrow(loan);
    }

    // Process loop body
    for body_node in body {
        process_node_for_borrows(checker, body_node)?;
    }
    
    Ok(())
}

/// Process return statement node
fn process_return_node(
    checker: &mut BorrowChecker,
    places: &[crate::compiler::hir::place::Place],
    node_id: HirNodeId,
) -> Result<(), CompilerMessages> {
    for return_place in places {
        let borrow_id = checker.next_borrow_id();
        let loan = Loan::new(borrow_id, return_place.clone(), BorrowKind::Shared, node_id);

        if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
            cfg_node.borrow_state.add_borrow(loan);
        }
    }
    
    Ok(())
}

/// Process return error node
fn process_return_error_node(
    checker: &mut BorrowChecker,
    place: &crate::compiler::hir::place::Place,
    node_id: HirNodeId,
) -> Result<(), CompilerMessages> {
    let borrow_id = checker.next_borrow_id();
    let loan = Loan::new(borrow_id, place.clone(), BorrowKind::Shared, node_id);

    if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
        cfg_node.borrow_state.add_borrow(loan);
    }
    
    Ok(())
}

/// Process expression statement node
fn process_expr_stmt_node(
    checker: &mut BorrowChecker,
    place: &crate::compiler::hir::place::Place,
    node_id: HirNodeId,
) -> Result<(), CompilerMessages> {
    let borrow_id = checker.next_borrow_id();
    let loan = Loan::new(borrow_id, place.clone(), BorrowKind::Shared, node_id);

    if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
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
            create_shared_borrow(checker, place, node_id)
        }

        HirExprKind::SharedBorrow(place) => {
            create_shared_borrow(checker, place, node_id)
        }

        HirExprKind::MutableBorrow(place) => {
            create_mutable_borrow(checker, place, node_id)
        }

        HirExprKind::CandidateMove(place, borrow_id_opt) => {
            create_candidate_move_borrow(checker, place, borrow_id_opt, node_id)
        }

        HirExprKind::BinOp { left, right, .. } => {
            create_binary_op_borrows(checker, left, right, node_id)
        }

        HirExprKind::UnaryOp { operand, .. } => {
            create_shared_borrow(checker, operand, node_id)
        }

        HirExprKind::Call { args, .. } => {
            create_call_arg_borrows(checker, args, node_id)
        }

        HirExprKind::MethodCall { receiver, args, .. } => {
            create_method_call_borrows(checker, receiver, args, node_id)
        }

        HirExprKind::StructConstruct { fields, .. } => {
            create_struct_field_borrows(checker, fields, node_id)
        }

        HirExprKind::Collection(places) => {
            create_collection_element_borrows(checker, places, node_id)
        }

        HirExprKind::Range { start, end } => {
            create_range_borrows(checker, start, end, node_id)
        }

        // Literal expressions don't create borrows, except heap strings
        HirExprKind::Int(_)
        | HirExprKind::Float(_)
        | HirExprKind::Bool(_)
        | HirExprKind::StringLiteral(_)
        | HirExprKind::Char(_) => Ok(()),

        HirExprKind::HeapString(_) => {
            // Heap-allocated strings are owned values that need tracking
            // No borrow is created here since this is the creation point
            Ok(())
        }
    }
}

/// Create a shared borrow for a place
fn create_shared_borrow(
    checker: &mut BorrowChecker,
    place: &crate::compiler::hir::place::Place,
    node_id: HirNodeId,
) -> Result<(), CompilerMessages> {
    let borrow_id = checker.next_borrow_id();
    let loan = Loan::new(borrow_id, place.clone(), BorrowKind::Shared, node_id);

    if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
        cfg_node.borrow_state.add_borrow(loan);
    }
    
    Ok(())
}

/// Create a mutable borrow for a place
fn create_mutable_borrow(
    checker: &mut BorrowChecker,
    place: &crate::compiler::hir::place::Place,
    node_id: HirNodeId,
) -> Result<(), CompilerMessages> {
    let borrow_id = checker.next_borrow_id();
    let loan = Loan::new(borrow_id, place.clone(), BorrowKind::Mutable, node_id);

    if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
        cfg_node.borrow_state.add_borrow(loan);
    }
    
    Ok(())
}

/// Create a candidate move borrow with optional pre-allocated ID
fn create_candidate_move_borrow(
    checker: &mut BorrowChecker,
    place: &crate::compiler::hir::place::Place,
    borrow_id_opt: &Option<crate::compiler::borrow_checker::types::BorrowId>,
    node_id: HirNodeId,
) -> Result<(), CompilerMessages> {
    // Use pre-allocated BorrowId from HIR generation for O(1) refinement
    let borrow_id = if let Some(id) = borrow_id_opt {
        *id
    } else {
        // Fallback for legacy HIR nodes without pre-allocated BorrowId
        checker.next_borrow_id()
    };
    
    let loan = Loan::new(borrow_id, place.clone(), BorrowKind::CandidateMove, node_id);

    if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
        cfg_node.borrow_state.add_borrow(loan);
    }
    
    Ok(())
}

/// Create borrows for binary operation operands
fn create_binary_op_borrows(
    checker: &mut BorrowChecker,
    left: &crate::compiler::hir::place::Place,
    right: &crate::compiler::hir::place::Place,
    node_id: HirNodeId,
) -> Result<(), CompilerMessages> {
    create_shared_borrow(checker, left, node_id)?;
    create_shared_borrow(checker, right, node_id)?;
    Ok(())
}

/// Create borrows for function call arguments
fn create_call_arg_borrows(
    checker: &mut BorrowChecker,
    args: &[crate::compiler::hir::place::Place],
    node_id: HirNodeId,
) -> Result<(), CompilerMessages> {
    for arg_place in args {
        create_shared_borrow(checker, arg_place, node_id)?;
    }
    Ok(())
}

/// Create borrows for method call receiver and arguments
fn create_method_call_borrows(
    checker: &mut BorrowChecker,
    receiver: &crate::compiler::hir::place::Place,
    args: &[crate::compiler::hir::place::Place],
    node_id: HirNodeId,
) -> Result<(), CompilerMessages> {
    create_shared_borrow(checker, receiver, node_id)?;
    
    for arg_place in args {
        create_shared_borrow(checker, arg_place, node_id)?;
    }
    
    Ok(())
}

/// Create borrows for struct field values
fn create_struct_field_borrows(
    checker: &mut BorrowChecker,
    fields: &[(crate::compiler::string_interning::InternedString, crate::compiler::hir::place::Place)],
    node_id: HirNodeId,
) -> Result<(), CompilerMessages> {
    for (_, field_place) in fields {
        create_shared_borrow(checker, field_place, node_id)?;
    }
    Ok(())
}

/// Create borrows for collection elements
fn create_collection_element_borrows(
    checker: &mut BorrowChecker,
    places: &[crate::compiler::hir::place::Place],
    node_id: HirNodeId,
) -> Result<(), CompilerMessages> {
    for element_place in places {
        create_shared_borrow(checker, element_place, node_id)?;
    }
    Ok(())
}

/// Create borrows for range bounds
fn create_range_borrows(
    checker: &mut BorrowChecker,
    start: &crate::compiler::hir::place::Place,
    end: &crate::compiler::hir::place::Place,
    node_id: HirNodeId,
) -> Result<(), CompilerMessages> {
    create_shared_borrow(checker, start, node_id)?;
    create_shared_borrow(checker, end, node_id)?;
    Ok(())
}

/// Propagate borrow state through the control flow graph using worklist-based dataflow analysis.
fn propagate_borrow_state(checker: &mut BorrowChecker) -> Result<(), CompilerMessages> {
    let mut work_list: Vec<HirNodeId> = checker.cfg.entry_points.clone();
    let mut iterations = 0;
    let max_iterations = checker.cfg.nodes.len() * 10; // Prevent infinite loops

    while let Some(node_id) = work_list.pop() {
        iterations += 1;
        if iterations > max_iterations {
            break; // Safety limit reached
        }

        // Process current node and propagate to successors
        if process_node_propagation(checker, node_id) {
            // State changed, add successors to worklist
            add_successors_to_worklist(checker, node_id, &mut work_list);
        }
    }

    Ok(())
}

/// Process borrow state propagation for a single node
/// Returns true if any successor state changed
fn process_node_propagation(checker: &mut BorrowChecker, node_id: HirNodeId) -> bool {
    // Merge incoming states from predecessors
    merge_predecessor_states(checker, node_id);

    // Check if successors would change
    check_successor_state_changes(checker, node_id)
}

/// Merge borrow states from all predecessor nodes
fn merge_predecessor_states(checker: &mut BorrowChecker, node_id: HirNodeId) {
    let predecessors = checker.cfg.predecessors(node_id);
    
    if predecessors.is_empty() {
        return;
    }

    let predecessor_states = collect_predecessor_states(checker, &predecessors);
    apply_merged_state(checker, node_id, &predecessor_states);
}

/// Collect borrow states from predecessor nodes
fn collect_predecessor_states(
    checker: &BorrowChecker,
    predecessors: &[HirNodeId],
) -> Vec<crate::compiler::borrow_checker::types::BorrowState> {
    predecessors
        .iter()
        .filter_map(|&pred_id| {
            checker
                .cfg
                .nodes
                .get(&pred_id)
                .map(|n| n.borrow_state.clone())
        })
        .collect()
}

/// Apply merged predecessor states to current node
fn apply_merged_state(
    checker: &mut BorrowChecker,
    node_id: HirNodeId,
    predecessor_states: &[crate::compiler::borrow_checker::types::BorrowState],
) {
    if let Some(current_node) = checker.cfg.nodes.get_mut(&node_id) {
        let own_borrows = current_node.borrow_state.clone();

        // Merge incoming states from predecessors
        for (i, pred_state) in predecessor_states.iter().enumerate() {
            if i == 0 && own_borrows.is_empty() {
                current_node.borrow_state.union_merge(pred_state);
            } else {
                current_node.borrow_state.merge(pred_state);
            }
        }

        // Re-add own borrows created at this node
        restore_own_borrows(&mut current_node.borrow_state, &own_borrows);
    }
}

/// Restore borrows that were created at the current node
fn restore_own_borrows(
    current_state: &mut crate::compiler::borrow_checker::types::BorrowState,
    own_borrows: &crate::compiler::borrow_checker::types::BorrowState,
) {
    for loan in own_borrows.active_borrows.values() {
        if !current_state.active_borrows.contains_key(&loan.id) {
            current_state.add_borrow(loan.clone());
        }
    }
}

/// Check if propagating to successors would change their states
fn check_successor_state_changes(checker: &BorrowChecker, node_id: HirNodeId) -> bool {
    let current_state = if let Some(node) = checker.cfg.nodes.get(&node_id) {
        node.borrow_state.clone()
    } else {
        return false;
    };

    let successors: Vec<HirNodeId> = checker.cfg.successors(node_id).to_vec();
    
    successors.iter().any(|&successor_id| {
        would_successor_state_change(checker, successor_id, &current_state)
    })
}

/// Check if a successor's state would change with new input
fn would_successor_state_change(
    checker: &BorrowChecker,
    successor_id: HirNodeId,
    current_state: &crate::compiler::borrow_checker::types::BorrowState,
) -> bool {
    if let Some(successor_node) = checker.cfg.nodes.get(&successor_id) {
        let old_count = successor_node.borrow_state.active_borrows.len();
        let mut test_state = successor_node.borrow_state.clone();
        test_state.union_merge(current_state);
        test_state.active_borrows.len() != old_count
    } else {
        false
    }
}

/// Add successors to worklist if they're not already present
fn add_successors_to_worklist(
    checker: &BorrowChecker,
    node_id: HirNodeId,
    work_list: &mut Vec<HirNodeId>,
) {
    let successors: Vec<HirNodeId> = checker.cfg.successors(node_id).to_vec();
    
    for successor_id in successors {
        if !work_list.contains(&successor_id) {
            work_list.push(successor_id);
        }
    }
}
