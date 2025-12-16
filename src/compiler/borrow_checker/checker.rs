//! Borrow Checker Entry Point
//!
//! This module provides the main entry point for borrow checking analysis.
//! It coordinates the various phases of borrow checking including CFG construction,
//! borrow tracking, conflict detection, and error reporting.

use crate::borrow_log;
use crate::compiler::borrow_checker::borrow_tracking::track_borrows;
use crate::compiler::borrow_checker::cfg::construct_cfg;
use crate::compiler::borrow_checker::conflict_detection::{
    check_move_while_borrowed, check_use_after_move, detect_conflicts,
};
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
    borrow_log!("check_borrows(): Starting borrow checking analysis");

    // For now, we'll work with a simple implementation that processes
    // the HIR functions individually. In the future, this will be expanded
    // to handle the full module structure.

    // Extract HIR nodes from the module
    let hir_nodes = &hir.functions;

    if hir_nodes.is_empty() {
        borrow_log!("check_borrows(): No HIR nodes to analyze");
        return Ok(());
    }

    // Perform borrow checking analysis
    match perform_borrow_analysis(hir_nodes, string_table) {
        Ok(()) => {
            borrow_log!("check_borrows(): Borrow checking completed successfully");
            Ok(())
        }
        Err(messages) => {
            borrow_log!(
                "check_borrows(): Borrow checking failed with {} errors",
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
    borrow_log!(
        "check_borrows(): Constructing CFG from {} HIR nodes",
        hir_nodes.len()
    );
    checker.cfg = construct_cfg(hir_nodes);
    borrow_log!(
        "check_borrows(): CFG constructed with {} nodes",
        checker.cfg.nodes.len()
    );

    // Phase 2: Track borrows across the CFG
    borrow_log!("check_borrows(): Tracking borrows across CFG");
    track_borrows(&mut checker, hir_nodes)?;

    // Phase 3: Detect borrow conflicts
    borrow_log!("check_borrows(): Detecting borrow conflicts");
    detect_conflicts(&mut checker, hir_nodes);

    // Phase 4: Check for use-after-move violations
    borrow_log!("check_borrows(): Checking for use-after-move violations");
    check_use_after_move(&mut checker, hir_nodes);

    // Phase 5: Check for move-while-borrowed violations
    borrow_log!("check_borrows(): Checking for move-while-borrowed violations");
    check_move_while_borrowed(&mut checker, hir_nodes);

    // Return results
    checker.finish()
}
