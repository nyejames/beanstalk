//! Borrow Checker Entry Point
//!
//! This module provides the main entry point for borrow checking analysis.
//! It coordinates the various phases of borrow checking including CFG construction,
//! borrow tracking, conflict detection, and error reporting.

use crate::compiler::borrow_checker::borrow_tracking::track_borrows;
use crate::compiler::borrow_checker::candidate_move_refinement::validate_move_decisions;
use crate::compiler::borrow_checker::cfg::construct_cfg;
use crate::compiler::borrow_checker::conflict_detection::{
    analyze_control_flow_divergence, check_move_while_borrowed, check_use_after_move,
    detect_conflicts, merge_control_flow_states, propagate_borrow_states,
};
use crate::compiler::borrow_checker::last_use::{analyze_last_uses, apply_last_use_analysis};
use crate::compiler::borrow_checker::lifetime_inference::{
    apply_lifetime_inference, infer_lifetimes,
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
    // Starting borrow checking analysis

    // For now, we'll work with a simple implementation that processes
    // the HIR functions individually. In the future, this will be expanded
    // to handle the full module structure.

    // Extract HIR nodes from the module
    let hir_nodes = &hir.functions;

    if hir_nodes.is_empty() {
        // No HIR nodes to analyze
        return Ok(());
    }

    // Perform borrow checking analysis
    match perform_borrow_analysis(hir_nodes, string_table) {
        Ok(()) => {
            // Borrow checking completed successfully
            Ok(())
        }
        Err(messages) => {
            // Borrow checking failed with errors

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
    // Constructing CFG from HIR nodes
    checker.cfg = construct_cfg(hir_nodes);
    // CFG constructed

    // Phase 2: Track borrows across the CFG
    // Tracking borrows across CFG
    track_borrows(&mut checker, hir_nodes)?;

    // Phase 3: Perform last-use analysis
    // Performing last-use analysis
    let last_use_analysis = analyze_last_uses(&checker, &checker.cfg.clone(), hir_nodes);
    // Last-use analysis complete

    // Apply last-use analysis results to borrow checker state
    apply_last_use_analysis(&mut checker, &last_use_analysis);

    // Phase 4: Infer lifetimes for all borrows
    // Inferring lifetimes
    let lifetime_inference = infer_lifetimes(&checker, hir_nodes)?;
    // Lifetime inference complete

    // Apply lifetime inference results to borrow checker
    // This updates CFG nodes with accurate live borrow information for conflict detection
    apply_lifetime_inference(&mut checker, &lifetime_inference)?;

    // Update borrow state with precise lifetime spans for accurate error reporting
    update_borrow_state_with_lifetime_spans(&mut checker, &lifetime_inference)?;

    // Phase 5: Refine candidate moves using corrected lifetime inference
    // Refining candidate moves with corrected lifetime inference
    let candidate_move_refinement = crate::compiler::borrow_checker::candidate_move_refinement::refine_candidate_moves_with_lifetime_inference(
        &mut checker,
        hir_nodes,
        &lifetime_inference
    )?;
    // Candidate move refinement complete with CFG-based temporal analysis

    // Validate that move decisions don't conflict with active borrows
    validate_move_decisions(&checker, &candidate_move_refinement)?;

    // Validate that all candidate moves have been properly refined
    validate_complete_refinement(&checker)?;

    // Phase 6: Perform Polonius-style control flow analysis
    // Performing Polonius-style control flow analysis

    // 6a: Analyze control flow divergence points
    analyze_control_flow_divergence(&mut checker);

    // 6b: Propagate borrow states along CFG edges
    propagate_borrow_states(&mut checker);

    // 6c: Merge borrow states at CFG join points
    merge_control_flow_states(&mut checker);

    // Phase 7: Detect borrow conflicts using Polonius-style analysis
    // Detecting borrow conflicts with Polonius-style analysis
    detect_conflicts(&mut checker, hir_nodes);

    // Phase 8: Check for use-after-move violations
    // Checking for use-after-move violations
    check_use_after_move(&mut checker, hir_nodes);

    // Phase 9: Check for move-while-borrowed violations
    // Checking for move-while-borrowed violations
    check_move_while_borrowed(&mut checker, hir_nodes);

    // Return results
    checker.finish()
}

/// Update borrow state with precise lifetime spans for accurate error reporting
///
/// This function integrates the corrected lifetime inference results with the borrow
/// checker state to ensure that conflict detection and drop insertion use accurate
/// lifetime information instead of the previous over-conservative approach.
fn update_borrow_state_with_lifetime_spans(
    checker: &mut BorrowChecker,
    lifetime_inference: &crate::compiler::borrow_checker::lifetime_inference::LifetimeInferenceResult,
) -> Result<(), CompilerMessages> {
    // Update each CFG node with precise lifetime information
    for (node_id, live_set) in lifetime_inference.live_sets.all_live_sets() {
        if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
            // Update the borrow state with accurate live set information
            cfg_node.borrow_state.update_from_live_set(live_set);

            // Update last use points for precise drop insertion
            for &borrow_id in live_set {
                if let Some(kill_point) = lifetime_inference.live_sets.kill_point(borrow_id) {
                    // Record the precise kill point for this borrow
                    if let Some(loan) = cfg_node.borrow_state.active_borrows.get_mut(&borrow_id) {
                        loan.last_use_point = Some(kill_point);
                    }

                    // Record last use in the borrow state for move refinement integration
                    if let Some(place) = lifetime_inference.live_sets.borrow_place(borrow_id) {
                        cfg_node
                            .borrow_state
                            .record_last_use(place.clone(), kill_point);
                    }
                }
            }
        }
    }

    // Validate that lifetime spans are consistent with CFG structure
    validate_lifetime_spans_consistency(checker, lifetime_inference)?;

    Ok(())
}

/// Validate that lifetime spans are consistent with CFG structure
///
/// This ensures that the corrected lifetime inference produces results that are
/// consistent with the control flow graph structure and borrow checker state.
fn validate_lifetime_spans_consistency(
    checker: &BorrowChecker,
    lifetime_inference: &crate::compiler::borrow_checker::lifetime_inference::LifetimeInferenceResult,
) -> Result<(), CompilerMessages> {
    let mut errors = Vec::new();

    // Check that all borrows with kill points have corresponding CFG nodes
    for (borrow_id, kill_point) in lifetime_inference.live_sets.all_kill_points() {
        if !checker.cfg.nodes.contains_key(&kill_point) {
            let error = crate::compiler::compiler_messages::compiler_errors::CompilerError {
                msg: format!(
                    "Lifetime inference produced kill point {} for borrow {} that doesn't exist in CFG",
                    kill_point, borrow_id
                ),
                location:
                    crate::compiler::compiler_messages::compiler_errors::ErrorLocation::default(),
                error_type:
                    crate::compiler::compiler_messages::compiler_errors::ErrorType::Compiler,
                metadata: std::collections::HashMap::new(),
            };
            errors.push(error);
        }
    }

    // Check that creation points exist in CFG
    for (borrow_id, creation_point) in lifetime_inference.live_sets.creation_points() {
        if !checker.cfg.nodes.contains_key(&creation_point) {
            let error = crate::compiler::compiler_messages::compiler_errors::CompilerError {
                msg: format!(
                    "Lifetime inference produced creation point {} for borrow {} that doesn't exist in CFG",
                    creation_point, borrow_id
                ),
                location:
                    crate::compiler::compiler_messages::compiler_errors::ErrorLocation::default(),
                error_type:
                    crate::compiler::compiler_messages::compiler_errors::ErrorType::Compiler,
                metadata: std::collections::HashMap::new(),
            };
            errors.push(error);
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(
            crate::compiler::compiler_messages::compiler_errors::CompilerMessages {
                errors,
                warnings: Vec::new(),
            },
        )
    }
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
        // This is a soundness violation - unrefined candidate moves indicate
        // a phase-order hazard that could lead to incorrect borrow checking
        let mut metadata = std::collections::HashMap::new();
        metadata.insert(
            crate::compiler::compiler_messages::compiler_errors::ErrorMetaDataKey::CompilationStage,
            "Borrow Checking",
        );
        metadata.insert(crate::compiler::compiler_messages::compiler_errors::ErrorMetaDataKey::PrimarySuggestion, "This is a compiler bug - the move refinement phase failed to process all candidate moves");

        let error = CompilerError {
            msg: format!(
                "Soundness violation: Found {} unrefined candidate moves, indicating a phase-order hazard in borrow checking",
                unrefined_candidates.len()
            ),
            location: crate::compiler::compiler_messages::compiler_errors::ErrorLocation::default(),
            error_type: crate::compiler::compiler_messages::compiler_errors::ErrorType::Compiler,
            metadata,
        };

        let mut messages = CompilerMessages::new();
        messages.errors.push(error);
        return Err(messages);
    }

    Ok(())
}
