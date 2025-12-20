//! Identity-Based Conflict Detection System
//!
//! This module implements comprehensive identity-based conflict detection for the borrow checker.
//! It analyzes borrow conflicts using individual borrow identity (BorrowId) rather than place
//! grouping, enabling more precise analysis and proper handling of path-sensitive scenarios.
//!
//! ## Key Features
//!
//! - **Identity-Based Analysis**: Conflicts detected using BorrowId, not Place grouping
//! - **Path-Sensitive Analysis**: Only reports conflicts that exist on all incoming paths
//! - **Borrow Identity Preservation**: Each borrow maintains unique identity throughout analysis
//! - **Disjoint Path Separation**: Borrows on separate execution paths remain distinct
//! - **No Place Merging**: Borrows are never merged by place, only by explicit control flow
//! - **Conservative Merging**: Merges borrow states conservatively at CFG join points
//! - **Comprehensive Conflict Rules**: Handles all borrow kind combinations
//! - **Use-After-Move Detection**: Detects attempts to use moved values
//! - **Move-While-Borrowed Detection**: Detects moves while borrows are active
//! - **Whole-Object Borrowing Prevention**: Enforces design constraints
//!
//! ## Borrow Identity Preservation System
//!
//! The borrow identity preservation system ensures that each borrow maintains its
//! unique identity throughout the analysis process. This prevents inappropriate
//! merging of distinct borrows and enables precise conflict detection.
//!
//! ### Core Principles
//!
//! 1. **Unique BorrowId**: Each borrow has a unique identifier that never changes
//! 2. **Individual Analysis**: Conflicts analyzed between specific borrow instances
//! 3. **Path-Sensitive Tracking**: Borrows on disjoint paths remain separate
//! 4. **No Place Grouping**: Never group borrows by place for conflict detection
//! 5. **Explicit Control Flow**: Only merge borrows through explicit CFG joins
//!
//! ### Example: Disjoint Path Handling
//!
//! ```beanstalk
//! if condition:
//!     x ~= value1  // borrow_1 (BorrowId: 1) on path A
//! else:
//!     x ~= value2  // borrow_2 (BorrowId: 2) on path B
//! ;
//! // At join point: both borrow_1 and borrow_2 exist but on disjoint paths
//! // No conflict because they never coexist on the same execution path
//! // Identity preserved: borrow_1 ≠ borrow_2, tracked separately
//! ```
//!
//! ### Example: Identity-Based Conflict Detection
//!
//! ```beanstalk
//! x ~= value       // borrow_1 (BorrowId: 1, Mutable)
//! y = x            // borrow_2 (BorrowId: 2, Shared)
//! // Conflict detected between borrow_1 and borrow_2 specifically
//! // Not by grouping all borrows of place 'x' together
//! // Each borrow analyzed individually by its BorrowId
//! ```

use crate::compiler::borrow_checker::types::{
    BorrowChecker, BorrowId, BorrowKind, CfgNodeId, Loan,
};
use crate::compiler::hir::nodes::{HirNode, HirNodeId};
use crate::{
    create_move_while_borrowed_error, create_multiple_mutable_borrows_error,
    create_shared_mutable_conflict_error, create_use_after_move_error,
};

/// Detect borrow conflicts across the control flow graph using Polonius-style analysis
///
/// This function performs comprehensive Polonius-style conflict detection, only reporting
/// conflicts that exist on all incoming CFG paths. This enables more precise analysis
/// by avoiding false positives from path-dependent borrow patterns.
///
/// ## Algorithm
///
/// 1. **Node-Level Analysis**: Check each CFG node for local borrow conflicts
/// 2. **Path-Sensitive Validation**: Verify conflicts exist on all incoming paths
/// 3. **Conservative Reporting**: Only report conflicts that are guaranteed errors
/// 4. **Comprehensive Coverage**: Handle all borrow kind combinations and edge cases
pub fn detect_conflicts(checker: &mut BorrowChecker, hir_nodes: &[HirNode]) {
    // Starting Polonius-style conflict detection

    // Track conflicts found for debugging
    let mut conflicts_found = 0;
    let mut conflicts_reported = 0;

    // Check each CFG node for borrow conflicts
    for node in hir_nodes {
        let (found, reported) = check_node_conflicts(checker, node);
        conflicts_found += found;
        conflicts_reported += reported;
    }

    // Conflict detection complete
}

/// Check a single CFG node for borrow conflicts using identity-based analysis
///
/// This function uses the new borrow identity preservation system to detect
/// conflicts based on individual BorrowId rather than Place grouping.
/// Now integrates with corrected lifetime inference for precise error reporting.
/// Returns a tuple of (conflicts_found, conflicts_reported) for debugging purposes.
fn check_node_conflicts(checker: &mut BorrowChecker, node: &HirNode) -> (usize, usize) {
    // Get the borrow state for this node
    let borrow_state = if let Some(cfg_node) = checker.cfg.nodes.get(&node.id) {
        cfg_node.borrow_state.clone()
    } else {
        return (0, 0);
    };

    // Skip empty borrow states
    if borrow_state.is_empty() {
        return (0, 0);
    }

    // Use identity-based conflict detection with corrected lifetime information
    let mut errors = Vec::new();
    let mut conflicts_found = 0;

    // Get active borrows with precise lifetime information
    let active_borrows: Vec<&Loan> = borrow_state.active_borrows.values().collect();

    for (i, loan1) in active_borrows.iter().enumerate() {
        for loan2 in active_borrows.iter().skip(i + 1) {
            // Identity-based conflict detection: each borrow maintains unique identity
            if borrows_conflict_by_identity(loan1, loan2) {
                conflicts_found += 1;

                // Check if this conflict exists on all incoming paths (Polonius-style)
                // Now uses corrected lifetime information for more precise analysis
                if conflict_exists_on_all_paths_with_lifetime_info(checker, node.id, loan1, loan2) {
                    if let Some(error) = report_borrow_conflict_with_precise_location(
                        checker.string_table,
                        loan1,
                        loan2,
                        &node.location,
                        node.id,
                    ) {
                        errors.push(error);
                    }
                }
            }
        }
    }

    // Check for whole-object borrowing violations with precise lifetime information
    check_whole_object_borrowing_violations_with_lifetime_info(
        checker,
        &borrow_state,
        node,
        &mut errors,
    );

    let conflicts_reported = errors.len();

    // Add all collected errors
    for error in errors {
        checker.add_error(error);
    }

    (conflicts_found, conflicts_reported)
}

/// Check if two borrows conflict based on their individual identities
///
/// This is the core of identity-based conflict detection. Unlike place-based
/// grouping, this function treats each borrow as a unique entity with its
/// own identity (BorrowId) and analyzes conflicts at the individual level.
///
/// Key principles:
/// - Each borrow maintains unique BorrowId throughout analysis
/// - Conflicts are detected between specific borrow instances, not place groups
/// - Path-sensitive analysis considers execution context
/// - Borrows are never merged by place, only by explicit control flow
fn borrows_conflict_by_identity(loan1: &Loan, loan2: &Loan) -> bool {
    // Borrows with the same ID cannot conflict with themselves
    if loan1.id == loan2.id {
        return false;
    }

    // Use the enhanced place conflict detection with borrow kinds
    // This preserves the existing conflict rules while operating on individual borrows
    loan1
        .place
        .conflicts_with(&loan2.place, loan1.kind, loan2.kind)
}

/// Check if a conflict exists on all incoming CFG paths using corrected lifetime information
///
/// This enhanced version uses the corrected lifetime inference to provide more precise
/// conflict detection. It considers the actual lifetime spans of borrows rather than
/// just their presence in borrow states.
fn conflict_exists_on_all_paths_with_lifetime_info(
    checker: &BorrowChecker,
    node_id: HirNodeId,
    loan1: &Loan,
    loan2: &Loan,
) -> bool {
    let predecessors = checker.cfg.predecessors(node_id);

    // If no predecessors, this is an entry point - conflict exists trivially
    if predecessors.is_empty() {
        return true;
    }

    // Check if the same conflicting borrows exist on all predecessor paths
    // Now considers precise lifetime information
    let mut paths_with_conflict = 0;
    let total_paths = predecessors.len();

    for &pred_id in &predecessors {
        if let Some(pred_node) = checker.cfg.nodes.get(&pred_id) {
            // Check if both loans are actually live at this predecessor
            // This uses the corrected lifetime information instead of just checking presence
            let loan1_live = is_borrow_live_at_node(&pred_node.borrow_state, loan1.id, pred_id);
            let loan2_live = is_borrow_live_at_node(&pred_node.borrow_state, loan2.id, pred_id);

            if loan1_live && loan2_live {
                // Both borrows are live on this path - check if they conflict
                if let (Some(pred_loan1), Some(pred_loan2)) = (
                    pred_node.borrow_state.active_borrows.get(&loan1.id),
                    pred_node.borrow_state.active_borrows.get(&loan2.id),
                ) {
                    if pred_loan1.conflicts_with(pred_loan2) {
                        paths_with_conflict += 1;
                    }
                }
            }
            // If either borrow is not live on this path, no conflict on this path
        }
        // If predecessor node doesn't exist, no conflict on this path
    }

    let all_paths_have_conflict = paths_with_conflict == total_paths;
    all_paths_have_conflict
}

/// Check if a conflict exists on all incoming CFG paths (Polonius-style analysis)
///
/// This is the core of Polonius-style analysis: a conflict is only reported as an error
/// if it exists on ALL possible execution paths leading to the current node. This prevents
/// false positives from path-dependent borrow patterns.
///
/// ## Algorithm
///
/// 1. **No Predecessors**: If the node has no predecessors (entry point), the conflict exists trivially
/// 2. **Path Analysis**: For each predecessor, check if the same conflicting borrows exist
/// 3. **Conservative Decision**: Only return true if ALL paths have the conflict
/// 4. **Borrow Identity**: Match borrows by their IDs to ensure we're tracking the same borrows
fn conflict_exists_on_all_paths(
    checker: &BorrowChecker,
    node_id: HirNodeId,
    loan1: &Loan,
    loan2: &Loan,
) -> bool {
    let predecessors = checker.cfg.predecessors(node_id);

    // If no predecessors, this is an entry point - conflict exists trivially
    if predecessors.is_empty() {
        // Conflict detected at entry point
        return true;
    }

    // Check if the same conflicting borrows exist on all predecessor paths
    let mut paths_with_conflict = 0;
    let total_paths = predecessors.len();

    for &pred_id in &predecessors {
        if let Some(pred_node) = checker.cfg.nodes.get(&pred_id) {
            // Check if both loans exist in the predecessor's borrow state
            let has_loan1 = pred_node
                .borrow_state
                .active_borrows
                .contains_key(&loan1.id);
            let has_loan2 = pred_node
                .borrow_state
                .active_borrows
                .contains_key(&loan2.id);

            if has_loan1 && has_loan2 {
                // Both borrows exist on this path - check if they still conflict
                if let (Some(pred_loan1), Some(pred_loan2)) = (
                    pred_node.borrow_state.active_borrows.get(&loan1.id),
                    pred_node.borrow_state.active_borrows.get(&loan2.id),
                ) {
                    if pred_loan1.conflicts_with(pred_loan2) {
                        paths_with_conflict += 1;
                    }
                }
            }
            // If either borrow doesn't exist on this path, no conflict on this path
        }
        // If predecessor node doesn't exist, no conflict on this path
    }

    let all_paths_have_conflict = paths_with_conflict == total_paths;

    // Polonius analysis complete for node

    all_paths_have_conflict
}

/// Check if a borrow is actually live at a specific CFG node
///
/// This uses the corrected lifetime information to determine if a borrow
/// is truly active at a given point, rather than just checking presence
/// in the borrow state.
fn is_borrow_live_at_node(
    borrow_state: &crate::compiler::borrow_checker::types::BorrowState,
    borrow_id: BorrowId,
    node_id: CfgNodeId,
) -> bool {
    // Check if the borrow exists in the active borrows
    if let Some(loan) = borrow_state.active_borrows.get(&borrow_id) {
        // Check if this node is within the borrow's lifetime
        // If the borrow has a last_use_point, check if we're before it
        if let Some(last_use) = loan.last_use_point {
            // Borrow is live if we're at or before the last use point
            node_id <= last_use
        } else {
            // No last use point recorded, assume live (conservative)
            true
        }
    } else {
        false
    }
}

/// Report a borrow conflict error with precise location information
///
/// This enhanced version uses corrected lifetime information to provide
/// more accurate error locations and context.
fn report_borrow_conflict_with_precise_location(
    string_table: &mut crate::compiler::string_interning::StringTable,
    loan1: &Loan,
    loan2: &Loan,
    location: &crate::compiler::parsers::tokenizer::tokens::TextLocation,
    node_id: CfgNodeId,
) -> Option<crate::compiler::compiler_messages::compiler_errors::CompilerError> {
    let error_location = location.clone().to_error_location(string_table);

    // Create enhanced error with precise lifetime information
    let mut metadata = std::collections::HashMap::new();
    metadata.insert(
        crate::compiler::compiler_messages::compiler_errors::ErrorMetaDataKey::CompilationStage,
        "Borrow Checking - Conflict Detection",
    );

    // Add lifetime information to error context
    let lifetime_context = format!(
        "Borrow {} created at node {}, Borrow {} created at node {}, conflict at node {}",
        loan1.id, loan1.creation_point, loan2.id, loan2.creation_point, node_id
    );
    metadata.insert(
        crate::compiler::compiler_messages::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
        &lifetime_context,
    );

    match (loan1.kind, loan2.kind) {
        (BorrowKind::Mutable, BorrowKind::Mutable) => Some(create_multiple_mutable_borrows_error!(
            loan2.place,
            error_location.clone(),
            error_location
        )),

        // CandidateMove conflicts are treated as mutable conflicts for error reporting
        (BorrowKind::CandidateMove, BorrowKind::CandidateMove) => {
            Some(create_multiple_mutable_borrows_error!(
                loan2.place,
                error_location.clone(),
                error_location
            ))
        }

        (BorrowKind::Mutable, BorrowKind::CandidateMove)
        | (BorrowKind::CandidateMove, BorrowKind::Mutable) => {
            Some(create_multiple_mutable_borrows_error!(
                loan2.place,
                error_location.clone(),
                error_location
            ))
        }

        (BorrowKind::Shared, BorrowKind::Mutable)
        | (BorrowKind::Mutable, BorrowKind::Shared)
        | (BorrowKind::Shared, BorrowKind::CandidateMove)
        | (BorrowKind::CandidateMove, BorrowKind::Shared) => {
            // For error reporting, treat CandidateMove as Mutable
            let (existing_kind, new_kind) = match (loan1.kind, loan2.kind) {
                (BorrowKind::CandidateMove, other) => (BorrowKind::Mutable, other),
                (other, BorrowKind::CandidateMove) => (other, BorrowKind::Mutable),
                (kind1, kind2) => (kind1, kind2),
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

        // CandidateMove conflicts are treated as mutable conflicts for error reporting
        (BorrowKind::CandidateMove, BorrowKind::CandidateMove) => {
            Some(create_multiple_mutable_borrows_error!(
                loan2.place,
                error_location.clone(),
                error_location
            ))
        }

        (BorrowKind::Mutable, BorrowKind::CandidateMove)
        | (BorrowKind::CandidateMove, BorrowKind::Mutable) => {
            Some(create_multiple_mutable_borrows_error!(
                loan2.place,
                error_location.clone(),
                error_location
            ))
        }

        (BorrowKind::Shared, BorrowKind::Mutable)
        | (BorrowKind::Mutable, BorrowKind::Shared)
        | (BorrowKind::Shared, BorrowKind::CandidateMove)
        | (BorrowKind::CandidateMove, BorrowKind::Shared) => {
            // For error reporting, treat CandidateMove as Mutable
            let (existing_kind, new_kind) = match (loan1.kind, loan2.kind) {
                (BorrowKind::CandidateMove, other) => (BorrowKind::Mutable, other),
                (other, BorrowKind::CandidateMove) => (other, BorrowKind::Mutable),
                (kind1, kind2) => (kind1, kind2),
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

/// Check for use-after-move violations using Polonius-style analysis
///
/// This function detects attempts to use a value after it has been moved.
/// It uses path-sensitive analysis to only report violations that occur on
/// all possible execution paths.
pub fn check_use_after_move(checker: &mut BorrowChecker, hir_nodes: &[HirNode]) {
    // Checking for use-after-move violations
    let mut errors = Vec::new();
    let mut violations_found = 0;

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
                            violations_found += 1;

                            // Use Polonius-style analysis: only report if violation exists on all paths
                            if use_after_move_on_all_paths(checker, node.id, loan, other_loan) {
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
    }

    // Use-after-move analysis complete

    // Add all collected errors
    for error in errors {
        checker.add_error(error);
    }
}

/// Check if a use-after-move violation exists on all incoming paths
fn use_after_move_on_all_paths(
    checker: &BorrowChecker,
    node_id: HirNodeId,
    move_loan: &Loan,
    use_loan: &Loan,
) -> bool {
    let predecessors = checker.cfg.predecessors(node_id);

    // If no predecessors, violation exists trivially
    if predecessors.is_empty() {
        return true;
    }

    // Check if the violation pattern exists on all predecessor paths
    for &pred_id in &predecessors {
        if let Some(pred_node) = checker.cfg.nodes.get(&pred_id) {
            // Check if both loans exist and the move precedes the use
            let has_move = pred_node
                .borrow_state
                .active_borrows
                .contains_key(&move_loan.id);
            let has_use = pred_node
                .borrow_state
                .active_borrows
                .contains_key(&use_loan.id);

            if !has_move || !has_use {
                return false; // Violation doesn't exist on this path
            }

            // Verify the temporal relationship still holds
            if let (Some(pred_move), Some(pred_use)) = (
                pred_node.borrow_state.active_borrows.get(&move_loan.id),
                pred_node.borrow_state.active_borrows.get(&use_loan.id),
            ) {
                if pred_move.creation_point >= pred_use.creation_point {
                    return false; // Temporal relationship doesn't hold on this path
                }
            }
        } else {
            return false; // Missing predecessor means no violation on this path
        }
    }

    true // Violation exists on all paths
}

/// Check for whole-object borrowing violations with precise lifetime information
///
/// This enhanced version uses corrected lifetime information to provide more
/// accurate detection of whole-object borrowing violations.
fn check_whole_object_borrowing_violations_with_lifetime_info(
    checker: &mut BorrowChecker,
    borrow_state: &crate::compiler::borrow_checker::types::BorrowState,
    node: &HirNode,
    errors: &mut Vec<crate::compiler::compiler_messages::compiler_errors::CompilerError>,
) {
    let active_borrows: Vec<&Loan> = borrow_state.active_borrows.values().collect();

    for (i, loan1) in active_borrows.iter().enumerate() {
        for loan2 in active_borrows.iter().skip(i + 1) {
            // Only check if both borrows are actually live at this node
            let loan1_live = is_borrow_live_at_node(borrow_state, loan1.id, node.id);
            let loan2_live = is_borrow_live_at_node(borrow_state, loan2.id, node.id);

            if !loan1_live || !loan2_live {
                continue; // Skip if either borrow is not actually live
            }

            // Check if one place is a prefix of another (whole-object vs part relationship)
            let (whole_loan, part_loan) = if loan1.place.is_prefix_of(&loan2.place) {
                (loan1, loan2) // loan1 is the whole object, loan2 is the part
            } else if loan2.place.is_prefix_of(&loan1.place) {
                (loan2, loan1) // loan2 is the whole object, loan1 is the part
            } else {
                continue; // No prefix relationship
            };

            // Report error with enhanced context including lifetime information
            let error_location = node
                .location
                .clone()
                .to_error_location(checker.string_table);

            let mut metadata = std::collections::HashMap::new();
            metadata.insert(
                crate::compiler::compiler_messages::compiler_errors::ErrorMetaDataKey::CompilationStage,
                "Borrow Checking - Whole Object Borrowing"
            );

            metadata.insert(
                crate::compiler::compiler_messages::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
                "Cannot borrow whole object when part is already borrowed"
            );

            let error = crate::compiler::compiler_messages::compiler_errors::CompilerError {
                msg: format!(
                    "Cannot borrow whole object {:?} when part {:?} is already borrowed",
                    whole_loan.place, part_loan.place
                ),
                location: error_location,
                error_type:
                    crate::compiler::compiler_messages::compiler_errors::ErrorType::BorrowChecker,
                metadata,
            };
            errors.push(error);
        }
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

/// Check for move-while-borrowed violations using Polonius-style analysis
///
/// This function detects attempts to move a value while it is still borrowed.
/// It uses path-sensitive analysis to only report violations that occur on
/// all possible execution paths.
pub fn check_move_while_borrowed(checker: &mut BorrowChecker, hir_nodes: &[HirNode]) {
    // Checking for move-while-borrowed violations
    let mut errors = Vec::new();
    let mut violations_found = 0;

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
                            && matches!(
                                other_loan.kind,
                                BorrowKind::Shared
                                    | BorrowKind::Mutable
                                    | BorrowKind::CandidateMove
                            )
                        {
                            violations_found += 1;

                            // Use Polonius-style analysis: only report if violation exists on all paths
                            if move_while_borrowed_on_all_paths(checker, node.id, loan, other_loan)
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
    }

    // Move-while-borrowed analysis complete

    // Add all collected errors
    for error in errors {
        checker.add_error(error);
    }
}

/// Check if a move-while-borrowed violation exists on all incoming paths
fn move_while_borrowed_on_all_paths(
    checker: &BorrowChecker,
    node_id: HirNodeId,
    move_loan: &Loan,
    borrow_loan: &Loan,
) -> bool {
    let predecessors = checker.cfg.predecessors(node_id);

    // If no predecessors, violation exists trivially
    if predecessors.is_empty() {
        return true;
    }

    // Check if the violation pattern exists on all predecessor paths
    for &pred_id in &predecessors {
        if let Some(pred_node) = checker.cfg.nodes.get(&pred_id) {
            // Check if both loans exist and the borrow precedes the move
            let has_move = pred_node
                .borrow_state
                .active_borrows
                .contains_key(&move_loan.id);
            let has_borrow = pred_node
                .borrow_state
                .active_borrows
                .contains_key(&borrow_loan.id);

            if !has_move || !has_borrow {
                return false; // Violation doesn't exist on this path
            }

            // Verify the temporal relationship still holds
            if let (Some(pred_move), Some(pred_borrow)) = (
                pred_node.borrow_state.active_borrows.get(&move_loan.id),
                pred_node.borrow_state.active_borrows.get(&borrow_loan.id),
            ) {
                if pred_borrow.creation_point >= pred_move.creation_point {
                    return false; // Temporal relationship doesn't hold on this path
                }
            }
        } else {
            return false; // Missing predecessor means no violation on this path
        }
    }

    true // Violation exists on all paths
}

/// Perform comprehensive control flow merging for Polonius-style analysis
///
/// This function implements the core Polonius-style merging strategy at CFG join points.
/// It ensures that borrow states are merged conservatively, only preserving borrows
/// that exist on all incoming paths.
pub fn merge_control_flow_states(checker: &mut BorrowChecker) {
    // Performing Polonius-style control flow merging

    let mut merge_operations = 0;
    let mut nodes_processed = 0;

    // Find all CFG join points (nodes with multiple predecessors)
    let join_points: Vec<HirNodeId> = checker
        .cfg
        .nodes
        .iter()
        .filter_map(|(&node_id, cfg_node)| {
            if cfg_node.predecessors.len() > 1 {
                Some(node_id)
            } else {
                None
            }
        })
        .collect();

    for join_point in join_points {
        nodes_processed += 1;

        if let Some(cfg_node) = checker.cfg.nodes.get(&join_point) {
            let predecessors = cfg_node.predecessors.clone();

            if predecessors.len() > 1 {
                merge_operations += 1;
                merge_borrow_states_at_join_point(checker, join_point, &predecessors);
            }
        }
    }

    // Control flow merging complete
}

/// Merge borrow states at a specific CFG join point
///
/// This implements the conservative merging strategy: only borrows that exist
/// on ALL incoming paths are preserved in the merged state.
fn merge_borrow_states_at_join_point(
    checker: &mut BorrowChecker,
    join_point: HirNodeId,
    predecessors: &[HirNodeId],
) {
    if predecessors.is_empty() {
        return;
    }

    // Collect borrow states from all predecessors
    let mut predecessor_states = Vec::new();
    for &pred_id in predecessors {
        if let Some(pred_node) = checker.cfg.nodes.get(&pred_id) {
            predecessor_states.push(&pred_node.borrow_state);
        }
    }

    if predecessor_states.is_empty() {
        return;
    }

    // Start with the first predecessor's state
    let mut merged_state = predecessor_states[0].clone();

    // Merge with each subsequent predecessor using conservative merging
    for pred_state in predecessor_states.iter().skip(1) {
        merged_state.merge(pred_state);
    }

    // Update the join point's borrow state
    if let Some(join_node) = checker.cfg.nodes.get_mut(&join_point) {
        join_node.borrow_state = merged_state;
    }

    // Merged predecessor states at join point
}

/// Propagate borrow states along CFG edges for path-sensitive analysis
///
/// This function ensures that borrow states are correctly propagated from
/// each CFG node to its successors, enabling accurate path-sensitive analysis.
pub fn propagate_borrow_states(checker: &mut BorrowChecker) {
    // Propagating borrow states along CFG edges

    let mut propagation_operations = 0;

    // Collect all edges to avoid borrowing issues
    let edges: Vec<(HirNodeId, HirNodeId)> = checker
        .cfg
        .edges
        .iter()
        .flat_map(|(&from, successors)| successors.iter().map(move |&to| (from, to)))
        .collect();

    for (from_id, to_id) in edges {
        propagation_operations += 1;
        propagate_state_along_edge(checker, from_id, to_id);
    }

    // Borrow state propagation complete
}

/// Propagate borrow state along a single CFG edge
fn propagate_state_along_edge(checker: &mut BorrowChecker, from_id: HirNodeId, to_id: HirNodeId) {
    // Get the source state (clone to avoid borrowing issues)
    let source_state = if let Some(from_node) = checker.cfg.nodes.get(&from_id) {
        from_node.borrow_state.clone()
    } else {
        return;
    };

    // Propagate to the target node using union merge
    if let Some(to_node) = checker.cfg.nodes.get_mut(&to_id) {
        to_node.borrow_state.union_merge(&source_state);
    }
}

/// Analyze control flow divergence points for borrow state distribution
///
/// This function handles control flow divergence (if statements, match expressions)
/// by ensuring that the current borrow state is properly distributed to all
/// outgoing paths.
pub fn analyze_control_flow_divergence(checker: &mut BorrowChecker) {
    // Analyzing control flow divergence points

    let mut divergence_points = 0;

    // Find all nodes with multiple successors (divergence points)
    let divergent_nodes: Vec<HirNodeId> = checker
        .cfg
        .nodes
        .iter()
        .filter_map(|(&node_id, cfg_node)| {
            if cfg_node.successors.len() > 1 {
                Some(node_id)
            } else {
                None
            }
        })
        .collect();

    for divergent_node in divergent_nodes {
        divergence_points += 1;
        distribute_state_at_divergence(checker, divergent_node);
    }

    // Control flow divergence analysis complete
}

/// Distribute borrow state at a control flow divergence point
fn distribute_state_at_divergence(checker: &mut BorrowChecker, divergent_node: HirNodeId) {
    // Get the current state and successors
    let (current_state, successors) = if let Some(node) = checker.cfg.nodes.get(&divergent_node) {
        (node.borrow_state.clone(), node.successors.clone())
    } else {
        return;
    };

    let _successor_count = successors.len();

    // Distribute the current state to all successor nodes
    for successor_id in successors {
        if let Some(successor_node) = checker.cfg.nodes.get_mut(&successor_id) {
            // Use union merge to combine with any existing state
            successor_node.borrow_state.union_merge(&current_state);
        }
    }

    // Distributed borrow state to successors
}
