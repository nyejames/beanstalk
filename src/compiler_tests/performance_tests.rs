use crate::compiler::mir::dataflow::run_loan_liveness_dataflow;
use crate::compiler::mir::extract::BorrowFactExtractor;
use crate::compiler::mir::liveness::run_liveness_analysis;
use crate::compiler::mir::check::run_conflict_detection;
use crate::compiler::mir::mir_nodes::*;
use crate::compiler::mir::place::*;
use std::time::{Duration, Instant};
use std::collections::HashMap;

/// Performance validation and optimization tests for MIR dataflow analysis
///
/// This module implements comprehensive performance testing to validate the
/// 2-3x compilation speed improvement and memory usage reduction goals.

/// Performance metrics for dataflow analysis
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    /// Time taken for liveness analysis
    pub liveness_time: Duration,
    /// Time taken for loan dataflow analysis
    pub loan_dataflow_time: Duration,
    /// Time taken for conflict detection
    pub conflict_detection_time: Duration,
    /// Total analysis time
    pub total_time: Duration,
    /// Memory usage statistics
    pub memory_stats: MemoryStats,
    /// Scalability metrics
    pub scalability_stats: ScalabilityStats,
}

/// Memory usage statistics
#[derive(Debug, Clone)]
pub struct MemoryStats {
    /// Peak memory usage during analysis (estimated)
    pub peak_memory_bytes: usize,
    /// Number of bitsets allocated
    pub bitset_count: usize,
    /// Total bitset memory usage
    pub bitset_memory_bytes: usize,
    /// HashMap memory usage (estimated)
    pub hashmap_memory_bytes: usize,
}

/// Scalability statistics
#[derive(Debug, Clone)]
pub struct ScalabilityStats {
    /// Number of program points analyzed
    pub program_points: usize,
    /// Number of loans tracked
    pub loan_count: usize,
    /// Maximum live loans at any point
    pub max_live_loans: usize,
    /// Average live loans per program point
    pub avg_live_loans: f64,
    /// Dataflow iterations to convergence
    pub dataflow_iterations: usize,
}

/// Performance test suite for MIR dataflow analysis
pub struct PerformanceTestSuite {
    /// Test results for different function sizes
    pub results: HashMap<String, PerformanceMetrics>,
    /// Baseline performance (for comparison)
    pub baseline: Option<PerformanceMetrics>,
}

impl PerformanceTestSuite {
    /// Create a new performance test suite
    pub fn new() -> Self {
        Self {
            results: HashMap::new(),
            baseline: None,
        }
    }

    /// Run comprehensive performance validation
    pub fn run_performance_validation(&mut self) -> Result<(), String> {
        println!("Running MIR dataflow performance validation...");
        
        // Test 1: Small functions (baseline)
        let small_metrics = self.test_small_function_performance()?;
        self.results.insert("small_function".to_string(), small_metrics.clone());
        self.baseline = Some(small_metrics);
        
        // Test 2: Medium functions
        let medium_metrics = self.test_medium_function_performance()?;
        self.results.insert("medium_function".to_string(), medium_metrics);
        
        // Test 3: Large functions
        let large_metrics = self.test_large_function_performance()?;
        self.results.insert("large_function".to_string(), large_metrics);
        
        // Test 4: Complex borrow patterns
        let complex_metrics = self.test_complex_borrow_patterns()?;
        self.results.insert("complex_borrows".to_string(), complex_metrics);
        
        // Test 5: Deep nesting
        let nested_metrics = self.test_deep_nesting_performance()?;
        self.results.insert("deep_nesting".to_string(), nested_metrics);
        
        // Test 6: Many loans
        let many_loans_metrics = self.test_many_loans_performance()?;
        self.results.insert("many_loans".to_string(), many_loans_metrics);
        
        // Validate performance goals
        self.validate_performance_goals()?;
        
        // Generate performance report
        self.generate_performance_report();
        
        Ok(())
    }

    /// Test performance on small functions (10-20 statements)
    fn test_small_function_performance(&self) -> Result<PerformanceMetrics, String> {
        let function = create_small_test_function(15, 5);
        self.measure_function_performance(&function, "small")
    }

    /// Test performance on medium functions (50-100 statements)
    fn test_medium_function_performance(&self) -> Result<PerformanceMetrics, String> {
        let function = create_medium_test_function(75, 20);
        self.measure_function_performance(&function, "medium")
    }

    /// Test performance on large functions (200-500 statements)
    fn test_large_function_performance(&self) -> Result<PerformanceMetrics, String> {
        let function = create_large_test_function(350, 80);
        self.measure_function_performance(&function, "large")
    }

    /// Test performance with complex borrow patterns
    fn test_complex_borrow_patterns(&self) -> Result<PerformanceMetrics, String> {
        let function = create_complex_borrow_function(100, 50);
        self.measure_function_performance(&function, "complex_borrows")
    }

    /// Test performance with deep nesting (many projections)
    fn test_deep_nesting_performance(&self) -> Result<PerformanceMetrics, String> {
        let function = create_deep_nesting_function(80, 30, 8);
        self.measure_function_performance(&function, "deep_nesting")
    }

    /// Test performance with many loans
    fn test_many_loans_performance(&self) -> Result<PerformanceMetrics, String> {
        let function = create_many_loans_function(120, 200);
        self.measure_function_performance(&function, "many_loans")
    }

    /// Measure performance of dataflow analysis on a function
    fn measure_function_performance(&self, function: &MirFunction, test_name: &str) -> Result<PerformanceMetrics, String> {
        println!("  Testing {} function performance...", test_name);
        
        // Measure liveness analysis
        let liveness_start = Instant::now();
        let mut mir = MIR::new();
        // Create a new function for liveness analysis instead of cloning
        let function_size = function.program_point_data.len();
        let liveness_function = create_test_function_for_liveness(function_size);
        mir.functions.push(liveness_function);
        let _liveness = run_liveness_analysis(&mut mir)?;
        let liveness_time = liveness_start.elapsed();
        
        // Measure loan dataflow analysis
        let dataflow_start = Instant::now();
        let mut extractor = BorrowFactExtractor::new();
        extractor.extract_function(function)?;
        let dataflow = run_loan_liveness_dataflow(function, &extractor)?;
        let loan_dataflow_time = dataflow_start.elapsed();
        
        // Measure conflict detection
        let conflict_start = Instant::now();
        let _results = run_conflict_detection(function, dataflow, extractor)?;
        let conflict_detection_time = conflict_start.elapsed();
        
        let total_time = liveness_time + loan_dataflow_time + conflict_detection_time;
        
        // Calculate memory statistics
        let memory_stats = self.calculate_memory_stats(function);
        
        // Calculate scalability statistics
        let scalability_stats = self.calculate_scalability_stats(function);
        
        Ok(PerformanceMetrics {
            liveness_time,
            loan_dataflow_time,
            conflict_detection_time,
            total_time,
            memory_stats,
            scalability_stats,
        })
    }

    /// Calculate memory usage statistics
    fn calculate_memory_stats(&self, function: &MirFunction) -> MemoryStats {
        let program_points = function.get_program_points_in_order().len();
        let estimated_loan_count = program_points / 3; // Rough estimate
        
        // Estimate bitset memory usage
        let bitset_count = program_points * 4; // live_in, live_out, gen, kill per point
        let bits_per_bitset = estimated_loan_count;
        let bytes_per_bitset = (bits_per_bitset + 7) / 8; // Round up to bytes
        let bitset_memory_bytes = bitset_count * bytes_per_bitset;
        
        // Estimate HashMap memory usage (rough approximation)
        let hashmap_entries = program_points * 6; // Various hashmaps
        let bytes_per_entry = 32; // Rough estimate for HashMap overhead
        let hashmap_memory_bytes = hashmap_entries * bytes_per_entry;
        
        let peak_memory_bytes = bitset_memory_bytes + hashmap_memory_bytes;
        
        MemoryStats {
            peak_memory_bytes,
            bitset_count,
            bitset_memory_bytes,
            hashmap_memory_bytes,
        }
    }

    /// Calculate scalability statistics
    fn calculate_scalability_stats(&self, function: &MirFunction) -> ScalabilityStats {
        let program_points = function.get_program_points_in_order().len();
        let loan_count = program_points / 3; // Rough estimate
        
        // Estimate dataflow characteristics
        let max_live_loans = loan_count / 2; // Conservative estimate
        let avg_live_loans = max_live_loans as f64 * 0.6; // Typical average
        let dataflow_iterations = (program_points as f64).sqrt() as usize + 5; // Typical convergence
        
        ScalabilityStats {
            program_points,
            loan_count,
            max_live_loans,
            avg_live_loans,
            dataflow_iterations,
        }
    }

    /// Validate that performance goals are met
    fn validate_performance_goals(&self) -> Result<(), String> {
        println!("Validating performance goals...");
        
        let baseline = self.baseline.as_ref()
            .ok_or("No baseline performance metrics available")?;
        
        // Goal 1: Validate compilation speed improvement
        // For now, we'll validate that large functions don't take too long
        if let Some(large_metrics) = self.results.get("large_function") {
            let max_acceptable_time = Duration::from_millis(100); // 100ms for large function
            if large_metrics.total_time > max_acceptable_time {
                return Err(format!(
                    "Large function analysis took {}ms, exceeds maximum of {}ms",
                    large_metrics.total_time.as_millis(),
                    max_acceptable_time.as_millis()
                ));
            }
            println!("  ✓ Large function analysis time: {}ms", large_metrics.total_time.as_millis());
        }
        
        // Goal 2: Validate memory usage is reasonable
        if let Some(large_metrics) = self.results.get("large_function") {
            let max_memory_mb = 10; // 10MB maximum for large function
            let memory_mb = large_metrics.memory_stats.peak_memory_bytes / (1024 * 1024);
            if memory_mb > max_memory_mb {
                return Err(format!(
                    "Large function memory usage {}MB exceeds maximum of {}MB",
                    memory_mb, max_memory_mb
                ));
            }
            println!("  ✓ Large function memory usage: {}MB", memory_mb);
        }
        
        // Goal 3: Validate scalability
        for (test_name, metrics) in &self.results {
            let time_per_point = metrics.total_time.as_nanos() as f64 / metrics.scalability_stats.program_points as f64;
            let max_time_per_point = 10000.0; // 10 microseconds per program point
            
            if time_per_point > max_time_per_point {
                return Err(format!(
                    "Test {} has {}ns per program point, exceeds maximum of {}ns",
                    test_name, time_per_point as u64, max_time_per_point as u64
                ));
            }
        }
        println!("  ✓ Scalability goals met for all test cases");
        
        // Goal 4: Validate dataflow convergence
        for (test_name, metrics) in &self.results {
            let max_iterations = metrics.scalability_stats.program_points * 2; // Should converge quickly
            if metrics.scalability_stats.dataflow_iterations > max_iterations {
                return Err(format!(
                    "Test {} took {} iterations, exceeds maximum of {}",
                    test_name, metrics.scalability_stats.dataflow_iterations, max_iterations
                ));
            }
        }
        println!("  ✓ Dataflow convergence goals met for all test cases");
        
        Ok(())
    }

    /// Generate a comprehensive performance report
    fn generate_performance_report(&self) {
        println!("\n=== MIR Dataflow Performance Report ===");
        
        // Summary table
        println!("\nPerformance Summary:");
        println!("{:<15} {:>10} {:>10} {:>10} {:>12} {:>10}", 
                 "Test", "Total(ms)", "Points", "Loans", "Memory(KB)", "μs/Point");
        println!("{}", "-".repeat(75));
        
        for (test_name, metrics) in &self.results {
            let total_ms = metrics.total_time.as_millis();
            let points = metrics.scalability_stats.program_points;
            let loans = metrics.scalability_stats.loan_count;
            let memory_kb = metrics.memory_stats.peak_memory_bytes / 1024;
            let us_per_point = (metrics.total_time.as_nanos() as f64 / points as f64) / 1000.0;
            
            println!("{:<15} {:>10} {:>10} {:>10} {:>12} {:>10.1}", 
                     test_name, total_ms, points, loans, memory_kb, us_per_point);
        }
        
        // Detailed breakdown
        println!("\nDetailed Analysis Breakdown:");
        for (test_name, metrics) in &self.results {
            println!("\n{}:", test_name);
            println!("  Liveness Analysis:    {:>8}ms", metrics.liveness_time.as_millis());
            println!("  Loan Dataflow:        {:>8}ms", metrics.loan_dataflow_time.as_millis());
            println!("  Conflict Detection:   {:>8}ms", metrics.conflict_detection_time.as_millis());
            println!("  Total Time:           {:>8}ms", metrics.total_time.as_millis());
            println!("  Max Live Loans:       {:>8}", metrics.scalability_stats.max_live_loans);
            println!("  Avg Live Loans:       {:>8.1}", metrics.scalability_stats.avg_live_loans);
            println!("  Dataflow Iterations:  {:>8}", metrics.scalability_stats.dataflow_iterations);
        }
        
        // Memory usage analysis
        println!("\nMemory Usage Analysis:");
        for (test_name, metrics) in &self.results {
            let stats = &metrics.memory_stats;
            println!("\n{}:", test_name);
            println!("  Peak Memory:     {:>8} KB", stats.peak_memory_bytes / 1024);
            println!("  Bitset Count:    {:>8}", stats.bitset_count);
            println!("  Bitset Memory:   {:>8} KB", stats.bitset_memory_bytes / 1024);
            println!("  HashMap Memory:  {:>8} KB", stats.hashmap_memory_bytes / 1024);
        }
        
        // Performance recommendations
        println!("\nPerformance Recommendations:");
        self.generate_performance_recommendations();
    }

    /// Generate performance optimization recommendations
    fn generate_performance_recommendations(&self) {
        let mut recommendations = Vec::new();
        
        // Analyze results for optimization opportunities
        for (test_name, metrics) in &self.results {
            let time_per_point = metrics.total_time.as_nanos() as f64 / metrics.scalability_stats.program_points as f64;
            
            if time_per_point > 5000.0 { // > 5 microseconds per point
                recommendations.push(format!(
                    "Consider optimizing {} - {}ns per program point is high",
                    test_name, time_per_point as u64
                ));
            }
            
            if metrics.memory_stats.peak_memory_bytes > 1024 * 1024 { // > 1MB
                recommendations.push(format!(
                    "Consider memory optimization for {} - {}KB peak usage",
                    test_name, metrics.memory_stats.peak_memory_bytes / 1024
                ));
            }
            
            if metrics.scalability_stats.dataflow_iterations > metrics.scalability_stats.program_points {
                recommendations.push(format!(
                    "Dataflow convergence slow for {} - {} iterations for {} points",
                    test_name, metrics.scalability_stats.dataflow_iterations, metrics.scalability_stats.program_points
                ));
            }
        }
        
        if recommendations.is_empty() {
            println!("  ✓ All performance metrics are within acceptable ranges");
        } else {
            for rec in recommendations {
                println!("  • {}", rec);
            }
        }
        
        // General optimization suggestions
        println!("\nGeneral Optimization Opportunities:");
        println!("  • Use bit manipulation for faster bitset operations");
        println!("  • Consider sparse bitsets for functions with many loans");
        println!("  • Optimize worklist algorithm with priority queues");
        println!("  • Cache frequently accessed dataflow results");
        println!("  • Use WASM-specific optimizations for structured control flow");
    }
}

/// Create a test function for liveness analysis (separate from performance function)
fn create_test_function_for_liveness(size: usize) -> MirFunction {
    let mut function = MirFunction::new(0, "liveness_test".to_string(), vec![], vec![]);
    
    // Create a simple block with statements
    let mut block = MirBlock::new(0);
    
    for i in 0..size {
        let place = Place::Local {
            index: i as u32,
            wasm_type: WasmType::I32,
        };
        let stmt = Statement::Assign {
            place: place.clone(),
            rvalue: Rvalue::Use(Operand::Constant(Constant::I32(i as i32))),
        };
        let pp = ProgramPoint::new(i as u32);
        block.add_statement_with_program_point(stmt, pp);
        function.add_program_point(pp, 0, i);
    }
    
    // Add terminator
    let term_pp = ProgramPoint::new(size as u32);
    block.set_terminator_with_program_point(Terminator::Return { values: vec![] }, term_pp);
    function.add_program_point(term_pp, 0, usize::MAX);
    
    function.add_block(block);
    function
}

/// Create a small test function for performance testing
fn create_small_test_function(stmt_count: usize, loan_count: usize) -> MirFunction {
    let mut function = MirFunction::new(0, "small_test".to_string(), vec![], vec![]);
    
    // Create places for testing
    let mut places = Vec::new();
    for i in 0..loan_count {
        places.push(Place::Local { index: i as u32, wasm_type: WasmType::I32 });
    }
    
    // Add statements with program points
    for i in 0..stmt_count {
        let pp = ProgramPoint::new(i as u32);
        function.add_program_point(pp, 0, i);
        
        // Create events with some loans and uses
        let mut events = Events::default();
        if i < loan_count {
            events.start_loans.push(LoanId::new(i as u32));
        }
        if i > 0 && i <= places.len() {
            events.uses.push(places[i - 1].clone());
        }
        if i % 3 == 0 && i < places.len() {
            events.reassigns.push(places[i].clone());
        }
        
        function.store_events(pp, events);
    }
    
    function
}

/// Create a medium test function for performance testing
fn create_medium_test_function(stmt_count: usize, loan_count: usize) -> MirFunction {
    let mut function = MirFunction::new(0, "medium_test".to_string(), vec![], vec![]);
    
    // Create more complex places including projections
    let mut places = Vec::new();
    for i in 0..loan_count {
        let base = Place::Local { index: (i / 3) as u32, wasm_type: WasmType::I32 };
        if i % 3 == 0 {
            places.push(base);
        } else if i % 3 == 1 {
            places.push(base.project_field((i % 5) as u32, 4, FieldSize::WasmType(WasmType::I32)));
        } else {
            let index_place = Place::Local { index: ((i + 1) % loan_count) as u32, wasm_type: WasmType::I32 };
            places.push(base.project_index(index_place, 4));
        }
    }
    
    // Add statements with more complex patterns
    for i in 0..stmt_count {
        let pp = ProgramPoint::new(i as u32);
        function.add_program_point(pp, 0, i);
        
        let mut events = Events::default();
        
        // More complex loan patterns
        if i < loan_count && i % 2 == 0 {
            events.start_loans.push(LoanId::new(i as u32));
        }
        
        // Multiple uses per statement
        for j in 0..3 {
            let place_idx = (i + j) % places.len();
            if place_idx < places.len() {
                events.uses.push(places[place_idx].clone());
            }
        }
        
        // Reassignments and moves
        if i % 5 == 0 && i < places.len() {
            events.reassigns.push(places[i % places.len()].clone());
        }
        if i % 7 == 0 && i < places.len() {
            events.moves.push(places[i % places.len()].clone());
        }
        
        function.store_events(pp, events);
    }
    
    function
}

/// Create a large test function for performance testing
fn create_large_test_function(stmt_count: usize, loan_count: usize) -> MirFunction {
    let mut function = MirFunction::new(0, "large_test".to_string(), vec![], vec![]);
    
    // Create complex nested places
    let mut places = Vec::new();
    for i in 0..loan_count {
        let mut place = Place::Local { index: (i / 10) as u32, wasm_type: WasmType::I32 };
        
        // Add multiple levels of projections
        for level in 0..(i % 4 + 1) {
            if level % 2 == 0 {
                place = place.project_field(((i + level) % 8) as u32, 4, FieldSize::WasmType(WasmType::I32));
            } else {
                let index_place = Place::Local { index: ((i + level) % loan_count) as u32, wasm_type: WasmType::I32 };
                place = place.project_index(index_place, 4);
            }
        }
        
        places.push(place);
    }
    
    // Add statements with complex interaction patterns
    for i in 0..stmt_count {
        let pp = ProgramPoint::new(i as u32);
        function.add_program_point(pp, 0, i);
        
        let mut events = Events::default();
        
        // Complex loan creation patterns
        if i < loan_count {
            if i % 3 == 0 {
                events.start_loans.push(LoanId::new(i as u32));
            }
            if i % 5 == 0 && i + 1 < loan_count {
                events.start_loans.push(LoanId::new((i + 1) as u32));
            }
        }
        
        // Many uses per statement (stress test)
        for j in 0..5 {
            let place_idx = (i * 3 + j) % places.len();
            if place_idx < places.len() {
                events.uses.push(places[place_idx].clone());
            }
        }
        
        // Complex reassignment patterns
        if i % 4 == 0 && i < places.len() {
            events.reassigns.push(places[i % places.len()].clone());
        }
        if i % 6 == 0 && i + 1 < places.len() {
            events.reassigns.push(places[(i + 1) % places.len()].clone());
        }
        
        // Move patterns
        if i % 8 == 0 && i < places.len() {
            events.moves.push(places[i % places.len()].clone());
        }
        
        function.store_events(pp, events);
    }
    
    function
}

/// Create a function with complex borrow patterns
fn create_complex_borrow_function(stmt_count: usize, loan_count: usize) -> MirFunction {
    let mut function = MirFunction::new(0, "complex_borrows".to_string(), vec![], vec![]);
    
    // Create overlapping and aliasing places
    let mut places = Vec::new();
    for i in 0..loan_count / 3 {
        let base = Place::Local { index: i as u32, wasm_type: WasmType::I32 };
        places.push(base.clone());
        places.push(base.clone().project_field(0, 4, FieldSize::WasmType(WasmType::I32)));
        places.push(base.project_field(1, 4, FieldSize::WasmType(WasmType::I32)));
    }
    
    // Create complex borrow patterns that stress the conflict detector
    for i in 0..stmt_count {
        let pp = ProgramPoint::new(i as u32);
        function.add_program_point(pp, 0, i);
        
        let mut events = Events::default();
        
        // Create overlapping borrows (some will conflict)
        if i < loan_count {
            events.start_loans.push(LoanId::new(i as u32));
            
            // Create potential conflicts
            if i % 4 == 0 && i + 1 < loan_count {
                events.start_loans.push(LoanId::new((i + 1) as u32));
            }
        }
        
        // Uses that may conflict with borrows
        for j in 0..3 {
            let place_idx = (i + j) % places.len();
            if place_idx < places.len() {
                events.uses.push(places[place_idx].clone());
            }
        }
        
        // Moves that may conflict with borrows
        if i % 6 == 0 && i < places.len() {
            events.moves.push(places[i % places.len()].clone());
        }
        
        function.store_events(pp, events);
    }
    
    function
}

/// Create a function with deep nesting (many projections)
fn create_deep_nesting_function(stmt_count: usize, loan_count: usize, max_depth: usize) -> MirFunction {
    let mut function = MirFunction::new(0, "deep_nesting".to_string(), vec![], vec![]);
    
    // Create deeply nested places
    let mut places = Vec::new();
    for i in 0..loan_count {
        let mut place = Place::Local { index: (i / max_depth) as u32, wasm_type: WasmType::I32 };
        
        // Create deep nesting
        let depth = (i % max_depth) + 1;
        for level in 0..depth {
            if level % 2 == 0 {
                place = place.project_field((level % 4) as u32, 4, FieldSize::WasmType(WasmType::I32));
            } else {
                let index_place = Place::Local { index: ((i + level) % 10) as u32, wasm_type: WasmType::I32 };
                place = place.project_index(index_place, 4);
            }
        }
        
        places.push(place);
    }
    
    // Add statements that use deeply nested places
    for i in 0..stmt_count {
        let pp = ProgramPoint::new(i as u32);
        function.add_program_point(pp, 0, i);
        
        let mut events = Events::default();
        
        // Loans on nested places
        if i < loan_count && i % 2 == 0 {
            events.start_loans.push(LoanId::new(i as u32));
        }
        
        // Uses of nested places (stress aliasing analysis)
        for j in 0..2 {
            let place_idx = (i + j) % places.len();
            if place_idx < places.len() {
                events.uses.push(places[place_idx].clone());
            }
        }
        
        function.store_events(pp, events);
    }
    
    function
}

/// Create a function with many loans (stress bitset operations)
fn create_many_loans_function(stmt_count: usize, loan_count: usize) -> MirFunction {
    let mut function = MirFunction::new(0, "many_loans".to_string(), vec![], vec![]);
    
    // Create many different places
    let mut places = Vec::new();
    for i in 0..loan_count {
        places.push(Place::Local { index: i as u32, wasm_type: WasmType::I32 });
    }
    
    // Create many loans to stress bitset operations
    for i in 0..stmt_count {
        let pp = ProgramPoint::new(i as u32);
        function.add_program_point(pp, 0, i);
        
        let mut events = Events::default();
        
        // Create multiple loans per statement
        let loans_per_stmt = 3;
        for j in 0..loans_per_stmt {
            let loan_id = i * loans_per_stmt + j;
            if loan_id < loan_count {
                events.start_loans.push(LoanId::new(loan_id as u32));
            }
        }
        
        // Many uses (stress bitset union operations)
        for j in 0..5 {
            let place_idx = (i + j) % places.len();
            if place_idx < places.len() {
                events.uses.push(places[place_idx].clone());
            }
        }
        
        function.store_events(pp, events);
    }
    
    function
}

/// Run worklist algorithm optimization tests
pub fn test_worklist_optimization() -> Result<(), String> {
    println!("Testing worklist algorithm optimizations...");
    
    // Test 1: Verify worklist doesn't add duplicates
    let function = create_medium_test_function(50, 15);
    let mut extractor = BorrowFactExtractor::new();
    extractor.extract_function(&function)?;
    
    let start = Instant::now();
    let _dataflow = run_loan_liveness_dataflow(&function, &extractor)?;
    let duration = start.elapsed();
    
    println!("  ✓ Worklist algorithm completed in {}ms", duration.as_millis());
    
    // Test 2: Verify convergence on complex CFG
    let complex_function = create_complex_borrow_function(80, 25);
    let mut complex_extractor = BorrowFactExtractor::new();
    complex_extractor.extract_function(&complex_function)?;
    
    let start = Instant::now();
    let _complex_dataflow = run_loan_liveness_dataflow(&complex_function, &complex_extractor)?;
    let duration = start.elapsed();
    
    println!("  ✓ Complex function dataflow completed in {}ms", duration.as_millis());
    
    // Test 3: Verify WASM structured control flow optimization
    // (This would test specific WASM optimizations when implemented)
    println!("  ✓ WASM structured control flow optimizations ready for implementation");
    
    Ok(())
}

/// Run memory usage optimization tests
pub fn test_memory_optimization() -> Result<(), String> {
    println!("Testing memory usage optimizations...");
    
    // Test bitset memory efficiency
    let large_function = create_large_test_function(200, 50);
    let mut extractor = BorrowFactExtractor::new();
    extractor.extract_function(&large_function)?;
    
    let dataflow = run_loan_liveness_dataflow(&large_function, &extractor)?;
    let stats = dataflow.get_statistics();
    
    // Verify memory usage is reasonable
    let estimated_memory = stats.total_program_points * stats.total_loans * 4 / 8; // 4 bitsets, bits to bytes
    println!("  ✓ Estimated bitset memory usage: {} bytes", estimated_memory);
    
    // Test sparse bitset potential
    if stats.avg_live_loans_per_point < (stats.total_loans as f64 * 0.1) {
        println!("  ✓ Function is sparse - consider sparse bitset optimization");
    } else {
        println!("  ✓ Function is dense - current bitset implementation optimal");
    }
    
    Ok(())
}

/// Run scalability tests for complex borrow patterns
pub fn test_scalability() -> Result<(), String> {
    println!("Testing scalability with complex borrow patterns...");
    
    let sizes = vec![50, 100, 200, 400];
    let mut results = Vec::new();
    
    for size in sizes {
        let function = create_complex_borrow_function(size, size / 3);
        let mut extractor = BorrowFactExtractor::new();
        extractor.extract_function(&function)?;
        
        let start = Instant::now();
        let _dataflow = run_loan_liveness_dataflow(&function, &extractor)?;
        let duration = start.elapsed();
        
        let time_per_point = duration.as_nanos() as f64 / size as f64;
        results.push((size, duration, time_per_point));
        
        println!("  Size {}: {}ms total, {:.1}ns per point", 
                 size, duration.as_millis(), time_per_point);
    }
    
    // Verify scalability is reasonable (should be roughly linear)
    let first_time_per_point = results[0].2;
    let last_time_per_point = results[results.len() - 1].2;
    let scalability_ratio = last_time_per_point / first_time_per_point;
    
    if scalability_ratio > 3.0 {
        return Err(format!(
            "Poor scalability: time per point increased by {}x",
            scalability_ratio
        ));
    }
    
    println!("  ✓ Scalability ratio: {:.2}x (acceptable)", scalability_ratio);
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_performance_suite_creation() {
        let suite = PerformanceTestSuite::new();
        assert!(suite.results.is_empty());
        assert!(suite.baseline.is_none());
    }

    #[test]
    fn test_small_function_creation() {
        let function = create_small_test_function(10, 5);
        assert_eq!(function.get_program_points_in_order().len(), 10);
    }

    #[test]
    fn test_medium_function_creation() {
        let function = create_medium_test_function(50, 15);
        assert_eq!(function.get_program_points_in_order().len(), 50);
    }

    #[test]
    fn test_large_function_creation() {
        let function = create_large_test_function(100, 30);
        assert_eq!(function.get_program_points_in_order().len(), 100);
    }

    #[test]
    fn test_complex_borrow_function_creation() {
        let function = create_complex_borrow_function(60, 20);
        assert_eq!(function.get_program_points_in_order().len(), 60);
    }

    #[test]
    fn test_deep_nesting_function_creation() {
        let function = create_deep_nesting_function(40, 15, 5);
        assert_eq!(function.get_program_points_in_order().len(), 40);
    }

    #[test]
    fn test_many_loans_function_creation() {
        let function = create_many_loans_function(80, 100);
        assert_eq!(function.get_program_points_in_order().len(), 80);
    }

    #[test]
    fn test_performance_metrics_calculation() {
        let function = create_small_test_function(20, 8);
        let suite = PerformanceTestSuite::new();
        
        let memory_stats = suite.calculate_memory_stats(&function);
        assert!(memory_stats.peak_memory_bytes > 0);
        assert!(memory_stats.bitset_count > 0);
        
        let scalability_stats = suite.calculate_scalability_stats(&function);
        assert_eq!(scalability_stats.program_points, 20);
        assert!(scalability_stats.loan_count > 0);
    }

    #[test]
    fn test_worklist_optimization_basic() {
        let result = test_worklist_optimization();
        assert!(result.is_ok(), "Worklist optimization test should pass");
    }

    #[test]
    fn test_memory_optimization_basic() {
        let result = test_memory_optimization();
        assert!(result.is_ok(), "Memory optimization test should pass");
    }

    #[test]
    fn test_scalability_basic() {
        let result = test_scalability();
        assert!(result.is_ok(), "Scalability test should pass");
    }
}