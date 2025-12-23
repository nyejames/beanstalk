//! Lifetime Inference - Algebraic Approach
//!
//! Implements lifetime inference using algebraic set operations instead of
//! geometric path enumeration, providing linear time complexity and better precision.

pub(crate) mod borrow_live_sets;
pub(crate) mod dataflow_engine;
pub(crate) mod parameter_analysis;
pub(crate) mod temporal_analysis;

// Re-export main types and functions for easy access
pub(crate) use borrow_live_sets::BorrowLiveSets;
pub(crate) use dataflow_engine::{BorrowDataflow, DataflowResult};
pub(crate) use parameter_analysis::{ParameterAnalysis, ParameterLifetimeInfo};
pub(crate) use temporal_analysis::{DominanceInfo, TemporalAnalysis};

use crate::compiler::borrow_checker::types::BorrowChecker;
use crate::compiler::compiler_messages::compiler_errors::CompilerMessages;
use crate::compiler::hir::nodes::HirNode;

/// Complete lifetime inference result containing all computed lifetime information
///
/// This structure provides comprehensive lifetime information for all borrows
/// in the analyzed code, suitable for integration with move refinement,
/// conflict detection, and drop insertion.
#[derive(Debug, Clone)]
pub(crate) struct LifetimeInferenceResult {
    /// Active borrow sets per CFG node - the core result of algebraic analysis
    pub(crate) live_sets: BorrowLiveSets,

    /// Temporal analysis results including dominance and reachability information
    pub(crate) temporal_info: DominanceInfo,

    /// Parameter lifetime information (simplified - no reference returns)
    pub(crate) parameter_info: ParameterLifetimeInfo,

    /// Dataflow analysis convergence information for debugging
    pub(crate) dataflow_result: DataflowResult,
}

/// Main entry point for algebraic lifetime inference
pub(crate) fn infer_lifetimes(
    checker: &BorrowChecker,
    hir_nodes: &[HirNode],
) -> Result<LifetimeInferenceResult, CompilerMessages> {
    // Phase 1: Build temporal analysis infrastructure
    let temporal_analysis = TemporalAnalysis::new(&checker.cfg)?;
    let temporal_info = temporal_analysis.compute_dominance_info()?;

    // Phase 2: Initialize borrow live sets
    let mut live_sets = BorrowLiveSets::new();
    live_sets.initialize_from_cfg(checker)?;

    // Phase 3: Run fixpoint dataflow analysis
    let mut dataflow = BorrowDataflow::new(&checker.cfg, live_sets);
    let dataflow_result = dataflow.analyze_to_fixpoint()?;
    let final_live_sets = dataflow.into_live_sets();

    // Phase 4: Analyze parameter lifetimes (simplified - no reference returns)
    let mut parameter_analysis = ParameterAnalysis::new();
    let parameter_info = parameter_analysis.analyze_parameters(hir_nodes, &final_live_sets)?;

    // Phase 5: Validate soundness with hard error enforcement
    validate_lifetime_soundness(&final_live_sets, &temporal_info)?;

    Ok(LifetimeInferenceResult {
        live_sets: final_live_sets,
        temporal_info,
        parameter_info,
        dataflow_result,
    })
}

/// Validate that all inferred lifetimes are sound
fn validate_lifetime_soundness(
    live_sets: &BorrowLiveSets,
    temporal_info: &DominanceInfo,
) -> Result<(), CompilerMessages> {
    let mut errors = Vec::new();

    // Check 1: All borrow creation points must dominate their usage points
    for (borrow_id, creation_point) in live_sets.creation_points() {
        for usage_point in live_sets.usage_points(borrow_id) {
            if !temporal_info.dominates(creation_point, usage_point) {
                let error = crate::compiler::compiler_messages::compiler_errors::CompilerError {
                    msg: format!(
                        "Soundness violation: Borrow {:?} used before creation. Created at {:?}, used at {:?}",
                        borrow_id, creation_point, usage_point
                    ),
                    location:
                        crate::compiler::compiler_messages::compiler_errors::ErrorLocation::default(
                        ),
                    error_type:
                        crate::compiler::compiler_messages::compiler_errors::ErrorType::Compiler,
                    metadata: std::collections::HashMap::new(),
                };
                errors.push(error);
            }
        }
    }

    // Check 2: No unreachable borrow regions
    for (borrow_id, creation_point) in live_sets.creation_points() {
        if let Some(kill_point) = live_sets.kill_point(borrow_id)
            && !temporal_info.can_reach(creation_point, kill_point)
        {
            let error = crate::compiler::compiler_messages::compiler_errors::CompilerError {
                msg: format!(
                    "Soundness violation: Unreachable borrow region for {:?}. Created at {:?}, killed at unreachable {:?}",
                    borrow_id, creation_point, kill_point
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

    // Check 3: Fixpoint stability - all live sets should be consistent
    if !live_sets.is_stable() {
        let error = crate::compiler::compiler_messages::compiler_errors::CompilerError {
            msg: "Soundness violation: Lifetime inference did not reach a stable fixpoint"
                .to_string(),
            location: crate::compiler::compiler_messages::compiler_errors::ErrorLocation::default(),
            error_type: crate::compiler::compiler_messages::compiler_errors::ErrorType::Compiler,
            metadata: std::collections::HashMap::new(),
        };
        errors.push(error);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(CompilerMessages {
            errors,
            warnings: Vec::new(),
        })
    }
}

/// Apply lifetime inference results to the borrow checker
pub(crate) fn apply_lifetime_inference(
    checker: &mut BorrowChecker,
    inference_result: &LifetimeInferenceResult,
) -> Result<(), CompilerMessages> {
    // Update CFG nodes with accurate live borrow information
    for (node_id, live_set) in inference_result.live_sets.all_live_sets() {
        if let Some(cfg_node) = checker.cfg.nodes.get_mut(&node_id) {
            // Update active borrows with precise lifetime information
            cfg_node.borrow_state.update_from_live_set(live_set);
        }
    }

    // Provide accurate last-use information for move refinement
    for (borrow_id, kill_point) in inference_result.live_sets.all_kill_points() {
        checker.record_last_use(borrow_id, kill_point);
    }

    // Applied new lifetime inference

    Ok(())
}

/// Check if a place usage is a last use according to lifetime inference
pub(crate) fn is_last_use_according_to_lifetime_inference(
    place: &crate::compiler::hir::place::Place,
    usage_node: crate::compiler::hir::nodes::HirNodeId,
    inference_result: &LifetimeInferenceResult,
) -> bool {
    // Check if any borrow of this place has its kill point at this usage node
    for borrow_id in inference_result.live_sets.all_borrows() {
        if let Some(borrow_place) = inference_result.live_sets.borrow_place(borrow_id)
            && borrow_place == place
            && let Some(kill_point) = inference_result.live_sets.kill_point(borrow_id)
            && kill_point == usage_node
        {
            return true;
        }
    }

    false
}

// Algebraic approach uses set operations on active borrow collections
// instead of explicit path enumeration, providing linear complexity
// and better precision for lifetime inference.
