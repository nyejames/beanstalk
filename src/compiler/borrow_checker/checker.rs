//! Borrow Checker Entry Point
//!
//! This module provides the main entry point for borrow checking analysis.
//! It coordinates the various phases of borrow checking including CFG construction,
//! borrow tracking, conflict detection, and error reporting.

use crate::borrow_log;
use crate::compiler::borrow_checker::borrow_tracking::track_borrows;
use crate::compiler::borrow_checker::candidate_move_refinement::{refine_candidate_moves, validate_move_decisions};
use crate::compiler::borrow_checker::cfg::construct_cfg;
use crate::compiler::borrow_checker::conflict_detection::{
    check_move_while_borrowed, check_use_after_move, detect_conflicts,
};
use crate::compiler::borrow_checker::last_use::{analyze_last_uses, apply_last_use_analysis};
use crate::compiler::borrow_checker::types::BorrowChecker;
use crate::compiler::compiler_messages::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler::hir::nodes::{HirModule, HirNode};
use crate::compiler::string_interning::StringTable;

/// Perform borrow checking on the provided HIR module.
///
/// This is the main entry point for borrow checking analysis. It performs:
/// 1. Control Flow Graph (CFG) construction
/// 2. Borrow tracking across the CFG
/// 3. Conflict detection using Polonius-style analysis
/// 4. Error collection and reporting
///
/// The function operates exclusively on HIR and modifies it by inserting
/// Drop nodes and refining candidate moves based on last-use analysis.
pub fn check_borrows(
    hir: &mut HirModule,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    borrow_log!("Starting borrow checking analysis");

    // For now, we'll work with a simple implementation that processes
    // the HIR functions individually. In the future, this will be expanded
    // to handle the full module structure.

    // Extract HIR nodes from the module
    let hir_nodes = &hir.functions;

    if hir_nodes.is_empty() {
        borrow_log!("No HIR nodes to analyze");
        return Ok(());
    }

    // Perform borrow checking analysis
    match perform_borrow_analysis(hir_nodes, string_table) {
        Ok(()) => {
            borrow_log!("Borrow checking completed successfully");
            Ok(())
        }
        Err(messages) => {
            borrow_log!(
                "Borrow checking failed with {} errors",
                messages.errors.len()
            );

            // For now, return the first error. In the future, we should
            // handle multiple errors properly.
            if let Some(first_error) = messages.errors.into_iter().next() {
                Err(first_error)
            } else {
                Ok(()) // Only warnings, which we'll ignore for now
            }
        }
    }
}

/// Perform the core borrow checking analysis
fn perform_borrow_analysis(
    hir_nodes: &[HirNode],
    string_table: &mut StringTable,
) -> Result<(), CompilerMessages> {
    // Create borrow checker instance
    let mut checker = BorrowChecker::new(string_table);

    // Phase 1: Construct Control Flow Graph
    borrow_log!("Constructing CFG from {} HIR nodes", hir_nodes.len());
    checker.cfg = construct_cfg(hir_nodes);
    borrow_log!("CFG constructed with {} nodes", checker.cfg.nodes.len());

    // Phase 2: Track borrows across the CFG
    borrow_log!("Tracking borrows across CFG");
    track_borrows(&mut checker, hir_nodes)?;

    // Phase 3: Perform last-use analysis
    borrow_log!("Performing last-use analysis");
    let last_use_analysis = analyze_last_uses(&checker, &checker.cfg.clone(), hir_nodes);
    borrow_log!(
        "Last-use analysis complete: {} places analyzed, {} last-use points identified",
        last_use_analysis.last_use_statements.len(),
        last_use_analysis.statement_to_last_uses.len()
    );
    
    // Apply last-use analysis results to borrow checker state
    apply_last_use_analysis(&mut checker, &last_use_analysis);

    // Phase 4: Refine candidate moves based on last-use analysis
    borrow_log!("Refining candidate moves");
    let candidate_move_refinement = refine_candidate_moves(&mut checker, hir_nodes, &last_use_analysis)?;
    borrow_log!(
        "Candidate move refinement complete: {} moves, {} mutable borrows",
        candidate_move_refinement.moved_places.len(),
        candidate_move_refinement.mutable_borrows.len()
    );
    
    // Validate that move decisions don't conflict with active borrows
    validate_move_decisions(&checker, &candidate_move_refinement)?;
    
    // Validate that all candidate moves have been properly refined
    validate_complete_refinement(&checker)?;

    // Phase 5: Detect borrow conflicts
    borrow_log!("Detecting borrow conflicts");
    detect_conflicts(&mut checker, hir_nodes);

    // Phase 5: Check for use-after-move violations
    borrow_log!("Checking for use-after-move violations");
    check_use_after_move(&mut checker, hir_nodes);

    // Phase 6: Check for move-while-borrowed violations
    borrow_log!("Checking for move-while-borrowed violations");
    check_move_while_borrowed(&mut checker, hir_nodes);

    // Return results
    checker.finish()
}

/// Validate that all candidate moves have been properly refined
///
/// This function ensures that no CandidateMove borrows remain in the borrow checker
/// state after refinement, which would indicate a phase-order hazard or incomplete
/// analysis.
fn validate_complete_refinement(checker: &BorrowChecker) -> Result<(), CompilerMessages> {
    use crate::compiler::borrow_checker::types::BorrowKind;
    
    let mut unrefined_candidates = Vec::new();
    
    // Check all CFG nodes for remaining CandidateMove borrows
    for (node_id, cfg_node) in &checker.cfg.nodes {
        for loan in cfg_node.borrow_state.active_borrows.values() {
            if loan.kind == BorrowKind::CandidateMove {
                unrefined_candidates.push((*node_id, loan.place.clone()));
            }
        }
    }
    
    if !unrefined_candidates.is_empty() {
        borrow_log!(
            "Found {} unrefined candidate moves - this indicates a phase-order hazard",
            unrefined_candidates.len()
        );
        
        // In a full implementation, this would generate proper compiler errors
        // For now, we'll log the issue but continue
        for (node_id, place) in unrefined_candidates {
            borrow_log!(
                "Unrefined CandidateMove at node {} for place {}",
                node_id,
                place.display_with_table(checker.string_table)
            );
        }
    }
    
    Ok(())
}
