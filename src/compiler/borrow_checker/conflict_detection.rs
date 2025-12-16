//! Conflict Detection System
//!
//! This module implements Polonius-style conflict detection for the borrow checker.
//! It analyzes borrow conflicts using place overlap analysis and path-sensitive
//! reasoning to detect memory safety violations.

use crate::compiler::borrow_checker::types::{BorrowChecker, BorrowKind, Loan};
use crate::compiler::hir::nodes::{HirNode, HirNodeId};
use crate::compiler::hir::place::Place;
use crate::{
    create_move_while_borrowed_error, create_multiple_mutable_borrows_error,
    create_shared_mutable_conflict_error, create_use_after_move_error,
};

/// Detect borrow conflicts across the control flow graph
///
/// This function performs Polonius-style conflict detection, only reporting
/// conflicts that exist on all incoming CFG paths to ensure path-sensitive
/// analysis.
pub fn detect_conflicts(checker: &mut BorrowChecker, hir_nodes: &[HirNode]) {
    // Check each CFG node for borrow conflicts
    for node in hir_nodes {
        check_node_conflicts(checker, node);
    }
}

/// Check a single CFG node for borrow conflicts
fn check_node_conflicts(checker: &mut BorrowChecker, node: &HirNode) {
    // Get the borrow state for this node
    let borrow_state = if let Some(cfg_node) = checker.cfg.nodes.get(&node.id) {
        cfg_node.borrow_state.clone()
    } else {
        return;
    };

    // Check all pairs of active borrows for conflicts
    let active_borrows: Vec<&Loan> = borrow_state.active_borrows.values().collect();
    let mut errors = Vec::new();

    for (i, loan1) in active_borrows.iter().enumerate() {
        for loan2 in active_borrows.iter().skip(i + 1) {
            // Use enhanced place conflict detection
            if loan1
                .place
                .conflicts_with(&loan2.place, loan1.kind, loan2.kind)
            {
                // Check if this conflict exists on all incoming paths (Polonius-style)
                if conflict_exists_on_all_paths(checker, node.id, &loan1.place, &loan2.place) {
                    if let Some(error) =
                        report_borrow_conflict(checker.string_table, loan1, loan2, &node.location)
                    {
                        errors.push(error);
                    }
                }
            }
        }
    }

    // Check for whole-object borrowing violations
    check_whole_object_borrowing_violations(checker, &borrow_state, node, &mut errors);

    // Add all collected errors
    for error in errors {
        checker.add_error(error);
    }
}

/// Check if a conflict exists on all incoming CFG paths (Polonius-style analysis)
fn conflict_exists_on_all_paths(
    checker: &BorrowChecker,
    node_id: HirNodeId,
    place1: &Place,
    place2: &Place,
) -> bool {
    let predecessors = checker.cfg.predecessors(node_id);

    // If no predecessors, conflict exists trivially
    if predecessors.is_empty() {
        return true;
    }

    // Check if conflict exists on all predecessor paths
    for &pred_id in &predecessors {
        if let Some(pred_node) = checker.cfg.nodes.get(&pred_id) {
            let has_conflicting_borrows =
                pred_node.borrow_state.active_borrows.values().any(|loan1| {
                    pred_node.borrow_state.active_borrows.values().any(|loan2| {
                        loan1.id != loan2.id
                            && loan1.place.overlaps_with(place1)
                            && loan2.place.overlaps_with(place2)
                            && loan1.conflicts_with(loan2)
                    })
                });

            if !has_conflicting_borrows {
                return false; // Conflict doesn't exist on this path
            }
        } else {
            return false; // Missing predecessor means no conflict on this path
        }
    }

    true // Conflict exists on all paths
}

/// Report a borrow conflict error
fn report_borrow_conflict(
    string_table: &mut crate::compiler::string_interning::StringTable,
    loan1: &Loan,
    loan2: &Loan,
    location: &crate::compiler::parsers::tokenizer::tokens::TextLocation,
) -> Option<crate::compiler::compiler_messages::compiler_errors::CompilerError> {
    let error_location = location.clone().to_error_location(string_table);

    match (loan1.kind, loan2.kind) {
        (BorrowKind::Mutable, BorrowKind::Mutable) => Some(create_multiple_mutable_borrows_error!(
            loan2.place,
            error_location.clone(),
            error_location
        )),

        (BorrowKind::Shared, BorrowKind::Mutable) | (BorrowKind::Mutable, BorrowKind::Shared) => {
            let (existing_kind, new_kind) = if loan1.kind == BorrowKind::Mutable {
                (loan1.kind, loan2.kind)
            } else {
                (loan2.kind, loan1.kind)
            };

            Some(create_shared_mutable_conflict_error!(
                loan2.place,
                existing_kind,
                new_kind,
                error_location.clone(),
                error_location
            ))
        }

        (BorrowKind::Move, _) | (_, BorrowKind::Move) => {
            let (_, other_loan) = if loan1.kind == BorrowKind::Move {
                (loan1, loan2)
            } else {
                (loan2, loan1)
            };

            Some(create_use_after_move_error!(
                other_loan.place,
                error_location.clone(),
                error_location
            ))
        }

        // Shared borrows don't conflict with each other
        (BorrowKind::Shared, BorrowKind::Shared) => {
            // This should not happen as shared borrows don't conflict
            None
        }
    }
}

/// Check for use-after-move violations
pub fn check_use_after_move(checker: &mut BorrowChecker, hir_nodes: &[HirNode]) {
    let mut errors = Vec::new();

    for node in hir_nodes {
        if let Some(cfg_node) = checker.cfg.nodes.get(&node.id) {
            // Check if any active borrows are moves that conflict with other uses
            for loan in cfg_node.borrow_state.active_borrows.values() {
                if loan.kind == BorrowKind::Move {
                    // Check if there are any other borrows of overlapping places
                    for other_loan in cfg_node.borrow_state.active_borrows.values() {
                        if loan.id != other_loan.id
                            && loan.place.overlaps_with(&other_loan.place)
                            && other_loan.creation_point > loan.creation_point
                        {
                            let error_location = node
                                .location
                                .clone()
                                .to_error_location(checker.string_table);

                            let error = create_use_after_move_error!(
                                other_loan.place,
                                error_location.clone(),
                                error_location
                            );
                            errors.push(error);
                        }
                    }
                }
            }
        }
    }

    // Add all collected errors
    for error in errors {
        checker.add_error(error);
    }
}

/// Check for whole-object borrowing violations
///
/// This function enforces the design constraint that prevents borrowing the whole
/// object when a part is already borrowed. For example:
/// - If `x.field` is borrowed, then borrowing `x` should be prevented
/// - If `arr[i]` is borrowed, then borrowing `arr` should be prevented
fn check_whole_object_borrowing_violations(
    checker: &mut BorrowChecker,
    borrow_state: &crate::compiler::borrow_checker::types::BorrowState,
    node: &HirNode,
    errors: &mut Vec<crate::compiler::compiler_messages::compiler_errors::CompilerError>,
) {
    let active_borrows: Vec<&Loan> = borrow_state.active_borrows.values().collect();

    for (i, loan1) in active_borrows.iter().enumerate() {
        for loan2 in active_borrows.iter().skip(i + 1) {
            // Check if one place is a prefix of another (whole-object vs part relationship)
            let (whole_loan, part_loan) = if loan1.place.is_prefix_of(&loan2.place) {
                (loan1, loan2) // loan1 is the whole object, loan2 is the part
            } else if loan2.place.is_prefix_of(&loan1.place) {
                (loan2, loan1) // loan2 is the whole object, loan1 is the part
            } else {
                continue; // No prefix relationship
            };

            // Report error: cannot borrow whole object when part is borrowed
            let error_location = node
                .location
                .clone()
                .to_error_location(checker.string_table);

            let error = crate::create_whole_object_borrow_error!(
                whole_loan.place,
                part_loan.place,
                error_location.clone(),
                error_location
            );
            errors.push(error);
        }
    }
}

/// Check for move-while-borrowed violations
pub fn check_move_while_borrowed(checker: &mut BorrowChecker, hir_nodes: &[HirNode]) {
    let mut errors = Vec::new();

    for node in hir_nodes {
        if let Some(cfg_node) = checker.cfg.nodes.get(&node.id) {
            // Check if any moves occur while there are active borrows
            for loan in cfg_node.borrow_state.active_borrows.values() {
                if loan.kind == BorrowKind::Move {
                    // Check if there are any active shared or mutable borrows of the same place
                    for other_loan in cfg_node.borrow_state.active_borrows.values() {
                        if loan.id != other_loan.id
                            && loan.place.overlaps_with(&other_loan.place)
                            && other_loan.creation_point < loan.creation_point
                            && matches!(other_loan.kind, BorrowKind::Shared | BorrowKind::Mutable)
                        {
                            let error_location = node
                                .location
                                .clone()
                                .to_error_location(checker.string_table);

                            let error = create_move_while_borrowed_error!(
                                loan.place,
                                other_loan.kind,
                                error_location.clone(),
                                error_location
                            );
                            errors.push(error);
                        }
                    }
                }
            }
        }
    }

    // Add all collected errors
    for error in errors {
        checker.add_error(error);
    }
}
