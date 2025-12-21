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
use crate::compiler::borrow_checker::drop_insertion::insert_drop_nodes;
use crate::compiler::borrow_checker::last_use::{analyze_last_uses, apply_last_use_analysis};
use crate::compiler::borrow_checker::lifetime_inference::{
    apply_lifetime_inference, infer_lifetimes,
};
use crate::compiler::borrow_checker::structured_control_flow::handle_structured_control_flow;
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
    let hir_nodes = &mut hir.functions;

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
///
/// This function implements comprehensive error reporting by collecting multiple errors
/// in a single analysis pass and providing detailed error messages with actionable
/// suggestions for fixing violations.
fn perform_borrow_analysis(
    hir_nodes: &mut Vec<HirNode>,
    string_table: &mut StringTable,
) -> Result<(), CompilerMessages> {
    // Create borrow checker instance with comprehensive error collection
    let mut checker = BorrowChecker::new(string_table);

    // Phase 1: Construct Control Flow Graph
    // Constructing CFG from HIR nodes
    checker.cfg = construct_cfg(hir_nodes);
    // CFG constructed

    // Phase 2: Track borrows across the CFG
    // Tracking borrows across CFG
    if let Err(mut messages) = track_borrows(&mut checker, hir_nodes) {
        // Collect borrow tracking errors but continue analysis to find more errors
        checker.errors.append(&mut messages.errors);
        checker.warnings.append(&mut messages.warnings);
    }

    // Phase 3: Perform last-use analysis
    // Performing last-use analysis
    let last_use_analysis = analyze_last_uses(&checker, &checker.cfg.clone(), hir_nodes);
    // Last-use analysis complete

    // Apply last-use analysis results to borrow checker state
    apply_last_use_analysis(&mut checker, &last_use_analysis);

    // Phase 4: Infer lifetimes for all borrows
    // Inferring lifetimes
    let lifetime_inference = match infer_lifetimes(&checker, hir_nodes) {
        Ok(inference) => inference,
        Err(mut messages) => {
            // Collect lifetime inference errors but continue with default inference
            checker.errors.append(&mut messages.errors);
            checker.warnings.append(&mut messages.warnings);
            
            // Create a default lifetime inference to allow continued analysis
            crate::compiler::borrow_checker::lifetime_inference::LifetimeInferenceResult {
                live_sets: crate::compiler::borrow_checker::lifetime_inference::borrow_live_sets::BorrowLiveSets::new(),
                temporal_info: crate::compiler::borrow_checker::lifetime_inference::temporal_analysis::DominanceInfo::new(),
                parameter_info: crate::compiler::borrow_checker::lifetime_inference::parameter_analysis::ParameterLifetimeInfo::new(),
                dataflow_result: crate::compiler::borrow_checker::lifetime_inference::dataflow_engine::DataflowResult::new(),
            }
        }
    };
    // Lifetime inference complete

    // Apply lifetime inference results to borrow checker
    // This updates CFG nodes with accurate live borrow information for conflict detection
    if let Err(mut messages) = apply_lifetime_inference(&mut checker, &lifetime_inference) {
        checker.errors.append(&mut messages.errors);
        checker.warnings.append(&mut messages.warnings);
    }

    // Update borrow state with precise lifetime spans for accurate error reporting
    if let Err(mut messages) = update_borrow_state_with_lifetime_spans(&mut checker, &lifetime_inference) {
        checker.errors.append(&mut messages.errors);
        checker.warnings.append(&mut messages.warnings);
    }

    // Phase 5: Refine candidate moves using corrected lifetime inference
    // Refining candidate moves with corrected lifetime inference
    let candidate_move_refinement = match crate::compiler::borrow_checker::candidate_move_refinement::refine_candidate_moves_with_lifetime_inference(
        &mut checker,
        hir_nodes,
        &lifetime_inference
    ) {
        Ok(refinement) => refinement,
        Err(mut messages) => {
            // Collect move refinement errors but continue with empty refinement
            checker.errors.append(&mut messages.errors);
            checker.warnings.append(&mut messages.warnings);
            
            // Create empty refinement to allow continued analysis
            crate::compiler::borrow_checker::candidate_move_refinement::CandidateMoveRefinement::default()
        }
    };
    // Candidate move refinement complete with CFG-based temporal analysis

    // Validate that move decisions don't conflict with active borrows
    if let Err(mut messages) = validate_move_decisions(&checker, &candidate_move_refinement) {
        checker.errors.append(&mut messages.errors);
        checker.warnings.append(&mut messages.warnings);
    }

    // Validate that all candidate moves have been properly refined
    if let Err(mut messages) = validate_complete_refinement(&checker) {
        checker.errors.append(&mut messages.errors);
        checker.warnings.append(&mut messages.warnings);
    }

    // Phase 6: Perform Polonius-style control flow analysis
    // Performing Polonius-style control flow analysis

    // 6a: Analyze control flow divergence points
    analyze_control_flow_divergence(&mut checker);

    // 6b: Propagate borrow states along CFG edges
    propagate_borrow_states(&mut checker);

    // 6c: Merge borrow states at CFG join points
    merge_control_flow_states(&mut checker);

    // 6d: Handle structured control flow with enhanced separate branch tracking
    // This provides additional structured control flow analysis on top of the
    // existing Polonius-style analysis
    // Handling structured control flow
    if let Err(mut messages) = handle_structured_control_flow(&checker, hir_nodes) {
        checker.errors.append(&mut messages.errors);
        checker.warnings.append(&mut messages.warnings);
    }
    // Structured control flow handling complete

    // Phase 7: Detect borrow conflicts using Polonius-style analysis
    // Detecting borrow conflicts with Polonius-style analysis
    detect_conflicts(&mut checker, hir_nodes);

    // Phase 8: Check for use-after-move violations
    // Checking for use-after-move violations
    check_use_after_move(&mut checker, hir_nodes);

    // Phase 9: Check for move-while-borrowed violations
    // Checking for move-while-borrowed violations
    check_move_while_borrowed(&mut checker, hir_nodes);

    // Phase 10: Insert Drop nodes based on last-use analysis and move decisions
    // Inserting Drop nodes
    if let Err(mut messages) = insert_drop_nodes(&checker, hir_nodes, &last_use_analysis) {
        // Collect drop insertion errors
        checker.errors.append(&mut messages.errors);
        checker.warnings.append(&mut messages.warnings);
    }
    // Drop insertion complete

    // Generate comprehensive error summary if errors were found
    if !checker.errors.is_empty() {
        generate_error_summary(&mut checker);
    }

    // Return comprehensive results with all collected errors and warnings
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
/// Generate a comprehensive error summary for multiple borrow checker violations
///
/// This function creates a summary error that provides an overview of all detected
/// violations and actionable suggestions for fixing them. It helps users understand
/// the overall pattern of errors and prioritize fixes.
fn generate_error_summary(checker: &mut BorrowChecker) {
    let total_errors = checker.errors.len();
    
    if total_errors == 0 {
        return;
    }

    // Categorize errors by type for better reporting
    let mut conflict_errors = 0;
    let mut use_after_move_errors = 0;
    let mut move_while_borrowed_errors = 0;
    let mut validation_errors = 0;
    let mut other_errors = 0;

    for error in &checker.errors {
        match error.error_type {
            crate::compiler::compiler_messages::compiler_errors::ErrorType::BorrowChecker => {
                if error.msg.contains("conflict") || error.msg.contains("multiple") {
                    conflict_errors += 1;
                } else if error.msg.contains("use after move") || error.msg.contains("moved") {
                    use_after_move_errors += 1;
                } else if error.msg.contains("move while borrowed") {
                    move_while_borrowed_errors += 1;
                } else {
                    other_errors += 1;
                }
            }
            crate::compiler::compiler_messages::compiler_errors::ErrorType::Compiler => {
                validation_errors += 1;
            }
            _ => {
                other_errors += 1;
            }
        }
    }

    // Create comprehensive summary with actionable suggestions
    let mut summary_msg = format!(
        "Borrow checker found {} memory safety violation{}",
        total_errors,
        if total_errors == 1 { "" } else { "s" }
    );

    let mut suggestions = Vec::new();

    if conflict_errors > 0 {
        summary_msg.push_str(&format!("\n  - {} borrow conflict{}", conflict_errors, if conflict_errors == 1 { "" } else { "s" }));
        suggestions.push("Consider using borrows sequentially rather than simultaneously");
        suggestions.push("Ensure mutable and immutable borrows don't overlap in time");
    }

    if use_after_move_errors > 0 {
        summary_msg.push_str(&format!("\n  - {} use-after-move violation{}", use_after_move_errors, if use_after_move_errors == 1 { "" } else { "s" }));
        suggestions.push("Avoid using values after they have been moved");
        suggestions.push("Consider borrowing instead of moving if you need to use the value later");
    }

    if move_while_borrowed_errors > 0 {
        summary_msg.push_str(&format!("\n  - {} move-while-borrowed violation{}", move_while_borrowed_errors, if move_while_borrowed_errors == 1 { "" } else { "s" }));
        suggestions.push("Ensure all borrows are finished before moving values");
        suggestions.push("Consider restructuring code to avoid overlapping borrows and moves");
    }

    if validation_errors > 0 {
        summary_msg.push_str(&format!("\n  - {} compiler validation error{}", validation_errors, if validation_errors == 1 { "" } else { "s" }));
        suggestions.push("These indicate compiler bugs - please report them");
    }

    if other_errors > 0 {
        summary_msg.push_str(&format!("\n  - {} other error{}", other_errors, if other_errors == 1 { "" } else { "s" }));
    }

    // Add actionable suggestions
    if !suggestions.is_empty() {
        summary_msg.push_str("\n\nSuggestions for fixing these issues:");
        for (i, suggestion) in suggestions.iter().enumerate() {
            summary_msg.push_str(&format!("\n  {}. {}", i + 1, suggestion));
        }
    }

    // Add general guidance
    summary_msg.push_str("\n\nFor more information about Beanstalk's memory management, see the language documentation.");

    // Create summary error with comprehensive metadata
    let mut metadata = std::collections::HashMap::new();
    metadata.insert(
        crate::compiler::compiler_messages::compiler_errors::ErrorMetaDataKey::CompilationStage,
        "Borrow Checking - Summary",
    );
    metadata.insert(
        crate::compiler::compiler_messages::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
        "Fix the individual errors listed above to resolve all memory safety violations",
    );

    let summary_error = crate::compiler::compiler_messages::compiler_errors::CompilerError {
        msg: summary_msg,
        location: crate::compiler::compiler_messages::compiler_errors::ErrorLocation::default(),
        error_type: crate::compiler::compiler_messages::compiler_errors::ErrorType::BorrowChecker,
        metadata,
    };

    // Add summary as the first error for better visibility
    checker.errors.insert(0, summary_error);
}