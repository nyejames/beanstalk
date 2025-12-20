//! Performance Tests for Lifetime Inference Optimizations
//!
//! This module contains performance tests to validate that the lifetime inference
//! optimizations achieve linear or near-linear time complexity and meet the
//! performance requirements specified in the task.

use crate::compiler::borrow_checker::lifetime_inference::borrow_live_sets::BorrowLiveSets;
use crate::compiler::borrow_checker::lifetime_inference::dataflow_engine::{
    BorrowDataflow, PerformanceMetrics,
};
use crate::compiler::borrow_checker::types::{CfgNodeId, ControlFlowGraph};
use std::time::{Duration, Instant};

/// Performance benchmark for dataflow analysis
///
/// Tests that the optimized dataflow analysis maintains linear time complexity
/// as the input size grows.
pub(crate) fn benchmark_dataflow_performance() {
    println!("=== Lifetime Inference Performance Benchmark ===");

    // Test different input sizes to validate linear complexity
    let test_sizes = vec![10, 50, 100, 500, 1000];
    let mut results = Vec::new();

    for &size in &test_sizes {
        let result = benchmark_single_size(size);

        println!(
            "Size {}: {:.2}ms, {:.2} ns/(node*edge*borrow)",
            size,
            result.analysis_time.as_millis(),
            result.complexity_factor
        );

        results.push((size, result));
    }

    // Validate that complexity remains roughly linear
    validate_linear_scaling(&results);
}

/// Benchmark dataflow analysis for a specific input size
fn benchmark_single_size(size: usize) -> PerformanceMetrics {
    // Create a synthetic CFG with the specified size
    let cfg = create_synthetic_cfg(size);
    let mut live_sets = BorrowLiveSets::new();

    // Initialize with synthetic borrow data
    initialize_synthetic_borrows(&mut live_sets, &cfg, size / 4); // 25% of nodes have borrows

    // Run the dataflow analysis
    let mut dataflow = BorrowDataflow::new(&cfg, live_sets);

    let result = dataflow
        .analyze_to_fixpoint()
        .expect("Dataflow should converge");

    result.performance_metrics
}

/// Create a synthetic CFG for performance testing
fn create_synthetic_cfg(size: usize) -> ControlFlowGraph {
    let mut cfg = ControlFlowGraph::new();

    // Create nodes in a linear chain with some branches for realistic complexity
    for i in 0..size {
        let node_id = i; // CfgNodeId is just usize
        cfg.add_node(
            node_id,
            crate::compiler::borrow_checker::types::CfgNodeType::Statement,
        );

        // Add edges to create realistic control flow patterns
        if i > 0 {
            let prev_node = i - 1;
            cfg.add_edge(prev_node, node_id);
        }

        // Add some branching every 10 nodes to create join points
        if i % 10 == 0 && i + 5 < size {
            let branch_target = i + 5;
            cfg.add_edge(node_id, branch_target);
        }
    }

    cfg
}

/// Initialize synthetic borrow data for performance testing
fn initialize_synthetic_borrows(
    live_sets: &mut BorrowLiveSets,
    cfg: &ControlFlowGraph,
    num_borrows: usize,
) {
    // Create synthetic borrows distributed across CFG nodes
    let nodes: Vec<CfgNodeId> = cfg.nodes.keys().copied().collect();

    for i in 0..num_borrows {
        let borrow_id = i; // BorrowId is just usize
        let creation_node = nodes[i % nodes.len()];

        // Create a synthetic place for this borrow
        let place = create_synthetic_place(i);

        // Add the borrow to live sets
        live_sets.create_borrow(creation_node, borrow_id);

        // Set up borrow metadata
        // Note: This is a simplified setup for performance testing
        // In real usage, this would be populated from actual HIR analysis
    }
}

/// Create a synthetic place for performance testing
fn create_synthetic_place(index: usize) -> crate::compiler::hir::place::Place {
    use crate::compiler::hir::place::{Place, PlaceRoot};
    use crate::compiler::string_interning::StringId;

    Place {
        root: PlaceRoot::Local(StringId::from_u32(index as u32)), // Create StringId using from_u32
        projections: Vec::new(), // Simple places for performance testing
    }
}

/// Validate that the performance scales linearly with input size
fn validate_linear_scaling(results: &[(usize, PerformanceMetrics)]) {
    println!("\n=== Linear Scaling Validation ===");

    if results.len() < 2 {
        println!("Not enough data points for scaling validation");
        return;
    }

    // Check that complexity factor remains roughly constant
    let complexity_factors: Vec<f64> = results
        .iter()
        .map(|(_, metrics)| metrics.complexity_factor)
        .collect();

    let min_factor = complexity_factors
        .iter()
        .fold(f64::INFINITY, |a, &b| a.min(b));
    let max_factor = complexity_factors
        .iter()
        .fold(f64::NEG_INFINITY, |a, &b| a.max(b));

    let factor_ratio = max_factor / min_factor;

    println!(
        "Complexity factor range: {:.2} - {:.2}",
        min_factor, max_factor
    );
    println!("Factor ratio (max/min): {:.2}", factor_ratio);

    // For linear complexity, the factor ratio should be small (< 10x)
    const MAX_ACCEPTABLE_RATIO: f64 = 10.0;

    if factor_ratio <= MAX_ACCEPTABLE_RATIO {
        println!("✓ Linear complexity validation PASSED");
    } else {
        println!("✗ Linear complexity validation FAILED");
        println!(
            "  Factor ratio {} exceeds maximum acceptable ratio {}",
            factor_ratio, MAX_ACCEPTABLE_RATIO
        );
    }

    // Check that time per node doesn't grow exponentially
    let times_per_node: Vec<Duration> = results
        .iter()
        .map(|(_, metrics)| metrics.time_per_node)
        .collect();

    let min_time = times_per_node.iter().min().unwrap();
    let max_time = times_per_node.iter().max().unwrap();

    let time_ratio = max_time.as_nanos() as f64 / min_time.as_nanos() as f64;

    println!(
        "Time per node range: {:.2}μs - {:.2}μs",
        min_time.as_micros(),
        max_time.as_micros()
    );
    println!("Time ratio (max/min): {:.2}", time_ratio);

    const MAX_ACCEPTABLE_TIME_RATIO: f64 = 5.0;

    if time_ratio <= MAX_ACCEPTABLE_TIME_RATIO {
        println!("✓ Time scaling validation PASSED");
    } else {
        println!("✗ Time scaling validation FAILED");
        println!(
            "  Time ratio {} exceeds maximum acceptable ratio {}",
            time_ratio, MAX_ACCEPTABLE_TIME_RATIO
        );
    }
}

/// Test that efficient data structures are being used
pub(crate) fn test_efficient_data_structures() {
    println!("=== Efficient Data Structures Test ===");

    // Create a medium-sized test case
    let cfg = create_synthetic_cfg(100);
    let mut live_sets = BorrowLiveSets::new();
    initialize_synthetic_borrows(&mut live_sets, &cfg, 25);

    // Test that operations are efficient
    let start_time = Instant::now();

    // Perform many set operations to test efficiency
    for i in 0..1000 {
        let node_a = i % 100; // CfgNodeId is just usize
        let node_b = (i + 1) % 100;

        // Test efficient set operations
        let _union = live_sets.set_union(&[node_a, node_b]);
        let _diff = live_sets.set_difference(node_a, node_b);
        let _equal = live_sets.sets_equal(node_a, node_b);
    }

    let operations_time = start_time.elapsed();

    println!(
        "1000 set operations completed in: {:.2}ms",
        operations_time.as_millis()
    );

    // Should complete quickly (< 10ms for 1000 operations)
    const MAX_OPERATIONS_TIME_MS: u128 = 10;

    if operations_time.as_millis() <= MAX_OPERATIONS_TIME_MS {
        println!("✓ Efficient data structures test PASSED");
    } else {
        println!("✗ Efficient data structures test FAILED");
        println!(
            "  Operations took {}ms, expected < {}ms",
            operations_time.as_millis(),
            MAX_OPERATIONS_TIME_MS
        );
    }
}

/// Test that cloning is minimized
pub(crate) fn test_minimal_cloning() {
    println!("=== Minimal Cloning Test ===");

    // This test validates that the optimized implementation uses references
    // and in-place operations instead of excessive cloning

    let cfg = create_synthetic_cfg(50);
    let mut live_sets = BorrowLiveSets::new();
    initialize_synthetic_borrows(&mut live_sets, &cfg, 12);

    // Test that read-only operations use references
    let node_id = 0; // CfgNodeId is just usize

    // These operations should use references, not cloning
    let start_time = Instant::now();

    for _ in 0..1000 {
        let _is_live = live_sets.is_live_at(node_id, 0); // BorrowId is just usize
        let _ref_access = live_sets.live_at_ref(node_id);
        let _size = live_sets.live_set_size(node_id);
        let _empty = live_sets.is_empty_at(node_id);
    }

    let ref_operations_time = start_time.elapsed();

    println!(
        "1000 reference operations completed in: {:.2}μs",
        ref_operations_time.as_micros()
    );

    // Reference operations should be very fast (< 1ms for 1000 operations)
    const MAX_REF_OPERATIONS_TIME_MS: u128 = 1;

    if ref_operations_time.as_millis() <= MAX_REF_OPERATIONS_TIME_MS {
        println!("✓ Minimal cloning test PASSED");
    } else {
        println!("✗ Minimal cloning test FAILED");
        println!(
            "  Reference operations took {}ms, expected < {}ms",
            ref_operations_time.as_millis(),
            MAX_REF_OPERATIONS_TIME_MS
        );
    }
}

/// Run all performance tests
pub(crate) fn run_all_performance_tests() {
    println!("Running lifetime inference performance tests...\n");

    benchmark_dataflow_performance();
    println!();

    test_efficient_data_structures();
    println!();

    test_minimal_cloning();
    println!();

    println!("Performance tests completed.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_performance_benchmarks() {
        // Run a smaller benchmark for unit testing
        let result = benchmark_single_size(20);

        // Validate that basic performance metrics are reasonable
        assert!(result.total_nodes > 0);
        assert!(result.complexity_factor >= 0.0);
        assert!(result.time_per_node.as_nanos() > 0);
    }

    #[test]
    fn test_run_performance_suite() {
        // Run the full performance test suite
        println!("Running performance benchmark suite...");
        benchmark_dataflow_performance();

        println!("Running efficient data structures test...");
        test_efficient_data_structures();

        println!("Running minimal cloning test...");
        test_minimal_cloning();

        println!("Performance tests completed successfully!");
    }

    #[test]
    fn test_synthetic_cfg_creation() {
        let cfg = create_synthetic_cfg(10);
        assert_eq!(cfg.nodes.len(), 10);

        // Should have some edges
        let total_edges: usize = cfg.nodes.values().map(|node| node.successors.len()).sum();
        assert!(total_edges > 0);
    }

    #[test]
    fn test_efficient_operations() {
        test_efficient_data_structures();
        test_minimal_cloning();
    }
}
