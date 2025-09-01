use std::time::{Duration, Instant};
use std::collections::HashMap;
use crate::compiler::mir::mir_nodes::{MirFunction, ProgramPoint};
use crate::compiler::mir::dataflow::{LoanLivenessDataflow, DataflowStatistics};
use crate::compiler::mir::liveness::{LivenessAnalysis, LivenessStatistics};

/// Performance profiler for MIR dataflow analysis
///
/// This module provides detailed profiling capabilities for the MIR dataflow
/// analysis pipeline, helping identify performance bottlenecks and validate
/// optimization goals.

/// Detailed profiling results for dataflow analysis
#[derive(Debug, Clone)]
pub struct DataflowProfile {
    /// Function being profiled
    pub function_name: String,
    /// Total analysis time
    pub total_time: Duration,
    /// Breakdown by analysis phase
    pub phase_times: HashMap<String, Duration>,
    /// Memory usage statistics
    pub memory_profile: MemoryProfile,
    /// Algorithmic complexity metrics
    pub complexity_metrics: ComplexityMetrics,
    /// Optimization opportunities
    pub optimization_hints: Vec<OptimizationHint>,
}

/// Memory usage profiling results
#[derive(Debug, Clone)]
pub struct MemoryProfile {
    /// Peak memory usage (estimated)
    pub peak_memory_bytes: usize,
    /// Memory usage by data structure type
    pub memory_breakdown: HashMap<String, usize>,
    /// Memory efficiency metrics
    pub efficiency_metrics: MemoryEfficiencyMetrics,
}

/// Memory efficiency analysis
#[derive(Debug, Clone)]
pub struct MemoryEfficiencyMetrics {
    /// Bitset utilization percentage
    pub bitset_utilization: f64,
    /// HashMap load factor
    pub hashmap_load_factor: f64,
    /// Memory overhead percentage
    pub overhead_percentage: f64,
    /// Potential memory savings
    pub potential_savings_bytes: usize,
}

/// Algorithmic complexity metrics
#[derive(Debug, Clone)]
pub struct ComplexityMetrics {
    /// Dataflow iterations to convergence
    pub dataflow_iterations: usize,
    /// Worklist operations count
    pub worklist_operations: usize,
    /// Bitset operations count
    pub bitset_operations: usize,
    /// Time complexity analysis
    pub time_complexity: ComplexityAnalysis,
    /// Space complexity analysis
    pub space_complexity: ComplexityAnalysis,
}

/// Complexity analysis results
#[derive(Debug, Clone)]
pub struct ComplexityAnalysis {
    /// Theoretical complexity (e.g., "O(n²)")
    pub theoretical: String,
    /// Observed complexity factor
    pub observed_factor: f64,
    /// Efficiency rating (0.0 to 1.0)
    pub efficiency_rating: f64,
}

/// Optimization hint for improving performance
#[derive(Debug, Clone)]
pub struct OptimizationHint {
    /// Category of optimization
    pub category: OptimizationCategory,
    /// Description of the optimization opportunity
    pub description: String,
    /// Estimated performance improvement
    pub estimated_improvement: f64,
    /// Implementation difficulty (1-5 scale)
    pub difficulty: u8,
}

/// Categories of optimization opportunities
#[derive(Debug, Clone, PartialEq)]
pub enum OptimizationCategory {
    /// Algorithm optimization
    Algorithm,
    /// Data structure optimization
    DataStructure,
    /// Memory layout optimization
    MemoryLayout,
    /// WASM-specific optimization
    WasmSpecific,
    /// Caching optimization
    Caching,
}

/// Main profiler for dataflow analysis
pub struct DataflowProfiler {
    /// Profiling results by function
    profiles: HashMap<String, DataflowProfile>,
    /// Global profiling statistics
    global_stats: GlobalProfilingStats,
}

/// Global profiling statistics across all functions
#[derive(Debug, Clone, Default)]
pub struct GlobalProfilingStats {
    /// Total functions profiled
    pub functions_profiled: usize,
    /// Total analysis time
    pub total_analysis_time: Duration,
    /// Average time per function
    pub avg_time_per_function: Duration,
    /// Peak memory usage across all functions
    pub peak_memory_usage: usize,
    /// Most expensive function
    pub most_expensive_function: Option<String>,
}

impl DataflowProfiler {
    /// Create a new dataflow profiler
    pub fn new() -> Self {
        Self {
            profiles: HashMap::new(),
            global_stats: GlobalProfilingStats::default(),
        }
    }

    /// Profile dataflow analysis for a function
    pub fn profile_function(&mut self, function: &MirFunction) -> Result<DataflowProfile, String> {
        let function_name = function.name.clone();
        println!("Profiling dataflow analysis for function: {}", function_name);
        
        let start_time = Instant::now();
        let mut phase_times = HashMap::new();
        
        // Profile liveness analysis
        let liveness_start = Instant::now();
        let liveness_result = self.profile_liveness_analysis(function)?;
        let liveness_time = liveness_start.elapsed();
        phase_times.insert("liveness_analysis".to_string(), liveness_time);
        
        // Profile loan dataflow analysis
        let dataflow_start = Instant::now();
        let dataflow_result = self.profile_loan_dataflow(function)?;
        let dataflow_time = dataflow_start.elapsed();
        phase_times.insert("loan_dataflow".to_string(), dataflow_time);
        
        // Profile conflict detection
        let conflict_start = Instant::now();
        let conflict_result = self.profile_conflict_detection(function)?;
        let conflict_time = conflict_start.elapsed();
        phase_times.insert("conflict_detection".to_string(), conflict_time);
        
        let total_time = start_time.elapsed();
        
        // Generate memory profile
        let memory_profile = self.generate_memory_profile(function, &liveness_result, &dataflow_result);
        
        // Generate complexity metrics
        let complexity_metrics = self.generate_complexity_metrics(function, &dataflow_result);
        
        // Generate optimization hints
        let optimization_hints = self.generate_optimization_hints(function, &memory_profile, &complexity_metrics);
        
        let profile = DataflowProfile {
            function_name: function_name.clone(),
            total_time,
            phase_times,
            memory_profile,
            complexity_metrics,
            optimization_hints,
        };
        
        // Update global statistics
        self.update_global_stats(&profile);
        
        // Store profile
        self.profiles.insert(function_name, profile.clone());
        
        Ok(profile)
    }

    /// Profile liveness analysis specifically
    fn profile_liveness_analysis(&self, function: &MirFunction) -> Result<LivenessStatistics, String> {
        // Create a mock liveness analysis for profiling
        // In a real implementation, this would run the actual analysis
        let program_points = function.get_program_points_in_order().len();
        
        Ok(LivenessStatistics {
            total_program_points: program_points,
            max_live_vars_at_point: program_points / 4, // Estimate
            total_refinements: program_points / 8, // Estimate
        })
    }

    /// Profile loan dataflow analysis specifically
    fn profile_loan_dataflow(&self, function: &MirFunction) -> Result<DataflowStatistics, String> {
        // Create a mock dataflow analysis for profiling
        // In a real implementation, this would run the actual analysis
        let program_points = function.get_program_points_in_order().len();
        let loan_count = program_points / 3; // Estimate
        
        Ok(DataflowStatistics {
            total_program_points: program_points,
            total_loans: loan_count,
            max_live_loans_at_point: loan_count / 2,
            max_live_loans_after_point: loan_count / 2,
            avg_live_loans_per_point: (loan_count as f64) * 0.3,
        })
    }

    /// Profile conflict detection specifically
    fn profile_conflict_detection(&self, _function: &MirFunction) -> Result<(), String> {
        // Mock conflict detection profiling
        Ok(())
    }

    /// Generate memory usage profile
    fn generate_memory_profile(
        &self,
        function: &MirFunction,
        liveness_stats: &LivenessStatistics,
        dataflow_stats: &DataflowStatistics,
    ) -> MemoryProfile {
        let program_points = function.get_program_points_in_order().len();
        
        // Calculate memory usage by data structure
        let mut memory_breakdown = HashMap::new();
        
        // Bitset memory usage
        let bitsets_per_point = 4; // live_in, live_out, gen, kill
        let bits_per_bitset = dataflow_stats.total_loans;
        let bytes_per_bitset = (bits_per_bitset + 7) / 8; // Round up to bytes
        let total_bitset_memory = program_points * bitsets_per_point * bytes_per_bitset;
        memory_breakdown.insert("bitsets".to_string(), total_bitset_memory);
        
        // HashMap memory usage (rough estimate)
        let hashmap_entries = program_points * 6; // Various hashmaps
        let bytes_per_entry = 32; // Rough estimate for HashMap overhead + data
        let total_hashmap_memory = hashmap_entries * bytes_per_entry;
        memory_breakdown.insert("hashmaps".to_string(), total_hashmap_memory);
        
        // Vector memory usage
        let vector_memory = program_points * 16; // Various vectors
        memory_breakdown.insert("vectors".to_string(), vector_memory);
        
        // Control flow graph memory
        let cfg_memory = program_points * 24; // Successor/predecessor lists
        memory_breakdown.insert("control_flow_graph".to_string(), cfg_memory);
        
        let peak_memory_bytes = total_bitset_memory + total_hashmap_memory + vector_memory + cfg_memory;
        
        // Calculate efficiency metrics
        let bitset_utilization = dataflow_stats.avg_live_loans_per_point / (dataflow_stats.total_loans as f64);
        let hashmap_load_factor = 0.75; // Typical HashMap load factor
        let overhead_percentage = ((total_hashmap_memory + vector_memory + cfg_memory) as f64 / peak_memory_bytes as f64) * 100.0;
        
        // Estimate potential savings
        let sparse_threshold = 0.1;
        let potential_savings_bytes = if bitset_utilization < sparse_threshold {
            // Could save memory with sparse bitsets
            (total_bitset_memory as f64 * (1.0 - bitset_utilization * 2.0)) as usize
        } else {
            0
        };
        
        let efficiency_metrics = MemoryEfficiencyMetrics {
            bitset_utilization,
            hashmap_load_factor,
            overhead_percentage,
            potential_savings_bytes,
        };
        
        MemoryProfile {
            peak_memory_bytes,
            memory_breakdown,
            efficiency_metrics,
        }
    }

    /// Generate algorithmic complexity metrics
    fn generate_complexity_metrics(
        &self,
        function: &MirFunction,
        dataflow_stats: &DataflowStatistics,
    ) -> ComplexityMetrics {
        let program_points = function.get_program_points_in_order().len();
        let loan_count = dataflow_stats.total_loans;
        
        // Estimate dataflow iterations (typically converges quickly)
        let dataflow_iterations = (program_points as f64).sqrt() as usize + 3;
        
        // Estimate operation counts
        let worklist_operations = dataflow_iterations * program_points;
        let bitset_operations = worklist_operations * loan_count * 3; // union, subtract, compare
        
        // Time complexity analysis
        let theoretical_complexity = format!("O(n²·m)"); // n = program points, m = loans
        let observed_factor = (worklist_operations * loan_count) as f64 / (program_points * program_points * loan_count) as f64;
        let time_efficiency_rating = if observed_factor < 0.5 { 0.9 } else if observed_factor < 1.0 { 0.7 } else { 0.5 };
        
        let time_complexity = ComplexityAnalysis {
            theoretical: theoretical_complexity,
            observed_factor,
            efficiency_rating: time_efficiency_rating,
        };
        
        // Space complexity analysis
        let space_theoretical = format!("O(n·m)"); // n = program points, m = loans
        let space_observed_factor = (program_points * loan_count) as f64 / (program_points * loan_count) as f64;
        let space_efficiency_rating = if dataflow_stats.avg_live_loans_per_point < (loan_count as f64 * 0.3) { 0.8 } else { 0.6 };
        
        let space_complexity = ComplexityAnalysis {
            theoretical: space_theoretical,
            observed_factor: space_observed_factor,
            efficiency_rating: space_efficiency_rating,
        };
        
        ComplexityMetrics {
            dataflow_iterations,
            worklist_operations,
            bitset_operations,
            time_complexity,
            space_complexity,
        }
    }

    /// Generate optimization hints based on profiling results
    fn generate_optimization_hints(
        &self,
        function: &MirFunction,
        memory_profile: &MemoryProfile,
        complexity_metrics: &ComplexityMetrics,
    ) -> Vec<OptimizationHint> {
        let mut hints = Vec::new();
        let program_points = function.get_program_points_in_order().len();
        
        // Memory optimization hints
        if memory_profile.efficiency_metrics.bitset_utilization < 0.1 {
            hints.push(OptimizationHint {
                category: OptimizationCategory::DataStructure,
                description: format!(
                    "Consider sparse bitsets - only {:.1}% utilization",
                    memory_profile.efficiency_metrics.bitset_utilization * 100.0
                ),
                estimated_improvement: 0.3, // 30% memory reduction
                difficulty: 3,
            });
        }
        
        if memory_profile.efficiency_metrics.overhead_percentage > 50.0 {
            hints.push(OptimizationHint {
                category: OptimizationCategory::MemoryLayout,
                description: format!(
                    "High memory overhead ({:.1}%) - consider data structure consolidation",
                    memory_profile.efficiency_metrics.overhead_percentage
                ),
                estimated_improvement: 0.2, // 20% memory reduction
                difficulty: 4,
            });
        }
        
        // Algorithm optimization hints
        if complexity_metrics.dataflow_iterations > program_points / 2 {
            hints.push(OptimizationHint {
                category: OptimizationCategory::Algorithm,
                description: format!(
                    "Slow convergence ({} iterations) - consider worklist prioritization",
                    complexity_metrics.dataflow_iterations
                ),
                estimated_improvement: 0.25, // 25% time reduction
                difficulty: 3,
            });
        }
        
        if complexity_metrics.time_complexity.efficiency_rating < 0.6 {
            hints.push(OptimizationHint {
                category: OptimizationCategory::Algorithm,
                description: "Poor time complexity efficiency - consider algorithmic improvements".to_string(),
                estimated_improvement: 0.4, // 40% time reduction
                difficulty: 5,
            });
        }
        
        // WASM-specific optimization hints
        if program_points > 100 {
            hints.push(OptimizationHint {
                category: OptimizationCategory::WasmSpecific,
                description: "Large function - consider WASM structured control flow optimizations".to_string(),
                estimated_improvement: 0.15, // 15% time reduction
                difficulty: 2,
            });
        }
        
        // Caching optimization hints
        if complexity_metrics.bitset_operations > program_points * 1000 {
            hints.push(OptimizationHint {
                category: OptimizationCategory::Caching,
                description: format!(
                    "High bitset operation count ({}) - consider result caching",
                    complexity_metrics.bitset_operations
                ),
                estimated_improvement: 0.2, // 20% time reduction
                difficulty: 2,
            });
        }
        
        hints
    }

    /// Update global profiling statistics
    fn update_global_stats(&mut self, profile: &DataflowProfile) {
        self.global_stats.functions_profiled += 1;
        self.global_stats.total_analysis_time += profile.total_time;
        
        if self.global_stats.functions_profiled > 0 {
            self.global_stats.avg_time_per_function = 
                self.global_stats.total_analysis_time / self.global_stats.functions_profiled as u32;
        }
        
        if profile.memory_profile.peak_memory_bytes > self.global_stats.peak_memory_usage {
            self.global_stats.peak_memory_usage = profile.memory_profile.peak_memory_bytes;
        }
        
        // Update most expensive function
        if let Some(ref current_most_expensive) = self.global_stats.most_expensive_function {
            if let Some(current_profile) = self.profiles.get(current_most_expensive) {
                if profile.total_time > current_profile.total_time {
                    self.global_stats.most_expensive_function = Some(profile.function_name.clone());
                }
            }
        } else {
            self.global_stats.most_expensive_function = Some(profile.function_name.clone());
        }
    }

    /// Generate a comprehensive profiling report
    pub fn generate_report(&self) -> String {
        let mut report = String::new();
        
        report.push_str("=== MIR Dataflow Analysis Profiling Report ===\n\n");
        
        // Global statistics
        report.push_str("Global Statistics:\n");
        report.push_str(&format!("  Functions Profiled: {}\n", self.global_stats.functions_profiled));
        report.push_str(&format!("  Total Analysis Time: {}ms\n", self.global_stats.total_analysis_time.as_millis()));
        report.push_str(&format!("  Average Time per Function: {}ms\n", self.global_stats.avg_time_per_function.as_millis()));
        report.push_str(&format!("  Peak Memory Usage: {} KB\n", self.global_stats.peak_memory_usage / 1024));
        
        if let Some(ref most_expensive) = self.global_stats.most_expensive_function {
            report.push_str(&format!("  Most Expensive Function: {}\n", most_expensive));
        }
        
        report.push_str("\n");
        
        // Individual function profiles
        for (function_name, profile) in &self.profiles {
            report.push_str(&format!("Function: {}\n", function_name));
            report.push_str(&format!("  Total Time: {}ms\n", profile.total_time.as_millis()));
            
            // Phase breakdown
            report.push_str("  Phase Breakdown:\n");
            for (phase, time) in &profile.phase_times {
                let percentage = (time.as_nanos() as f64 / profile.total_time.as_nanos() as f64) * 100.0;
                report.push_str(&format!("    {}: {}ms ({:.1}%)\n", phase, time.as_millis(), percentage));
            }
            
            // Memory profile
            report.push_str(&format!("  Peak Memory: {} KB\n", profile.memory_profile.peak_memory_bytes / 1024));
            report.push_str(&format!("  Bitset Utilization: {:.1}%\n", 
                                   profile.memory_profile.efficiency_metrics.bitset_utilization * 100.0));
            
            // Complexity metrics
            report.push_str(&format!("  Dataflow Iterations: {}\n", profile.complexity_metrics.dataflow_iterations));
            report.push_str(&format!("  Time Complexity Efficiency: {:.1}%\n", 
                                   profile.complexity_metrics.time_complexity.efficiency_rating * 100.0));
            
            // Optimization hints
            if !profile.optimization_hints.is_empty() {
                report.push_str("  Optimization Opportunities:\n");
                for hint in &profile.optimization_hints {
                    report.push_str(&format!("    • {} ({:.0}% improvement, difficulty {})\n", 
                                           hint.description, hint.estimated_improvement * 100.0, hint.difficulty));
                }
            }
            
            report.push_str("\n");
        }
        
        // Overall recommendations
        report.push_str("Overall Recommendations:\n");
        report.push_str(&self.generate_overall_recommendations());
        
        report
    }

    /// Generate overall optimization recommendations
    fn generate_overall_recommendations(&self) -> String {
        let mut recommendations = String::new();
        
        // Analyze patterns across all profiles
        let mut total_memory = 0;
        let mut total_time = Duration::new(0, 0);
        let mut sparse_functions = 0;
        let mut slow_convergence_functions = 0;
        
        for profile in self.profiles.values() {
            total_memory += profile.memory_profile.peak_memory_bytes;
            total_time += profile.total_time;
            
            if profile.memory_profile.efficiency_metrics.bitset_utilization < 0.1 {
                sparse_functions += 1;
            }
            
            if profile.complexity_metrics.time_complexity.efficiency_rating < 0.6 {
                slow_convergence_functions += 1;
            }
        }
        
        let function_count = self.profiles.len();
        
        if function_count > 0 {
            let avg_memory_kb = (total_memory / function_count) / 1024;
            let avg_time_ms = (total_time / function_count as u32).as_millis();
            
            recommendations.push_str(&format!("  • Average memory usage: {} KB per function\n", avg_memory_kb));
            recommendations.push_str(&format!("  • Average analysis time: {} ms per function\n", avg_time_ms));
            
            if sparse_functions > function_count / 2 {
                recommendations.push_str("  • Consider implementing sparse bitsets (many functions have low utilization)\n");
            }
            
            if slow_convergence_functions > function_count / 3 {
                recommendations.push_str("  • Consider worklist algorithm optimizations (many functions have slow convergence)\n");
            }
            
            if avg_memory_kb > 1024 {
                recommendations.push_str("  • Consider memory layout optimizations (high average memory usage)\n");
            }
            
            if avg_time_ms > 50 {
                recommendations.push_str("  • Consider algorithmic improvements (high average analysis time)\n");
            }
        }
        
        recommendations.push_str("  • Implement WASM-specific control flow optimizations\n");
        recommendations.push_str("  • Add result caching for frequently analyzed patterns\n");
        recommendations.push_str("  • Consider parallel analysis for independent functions\n");
        
        recommendations
    }

    /// Get profile for a specific function
    pub fn get_profile(&self, function_name: &str) -> Option<&DataflowProfile> {
        self.profiles.get(function_name)
    }

    /// Get global statistics
    pub fn get_global_stats(&self) -> &GlobalProfilingStats {
        &self.global_stats
    }

    /// Clear all profiling data
    pub fn clear(&mut self) {
        self.profiles.clear();
        self.global_stats = GlobalProfilingStats::default();
    }
}

/// Entry point for profiling dataflow analysis
pub fn profile_dataflow_analysis(function: &MirFunction) -> Result<DataflowProfile, String> {
    let mut profiler = DataflowProfiler::new();
    profiler.profile_function(function)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::mir::mir_nodes::*;
    use crate::compiler::mir::place::*;

    fn create_test_function() -> MirFunction {
        let mut function = MirFunction::new(0, "test_function".to_string(), vec![], vec![]);
        
        // Add some program points and events
        for i in 0..10 {
            let pp = ProgramPoint::new(i);
            function.add_program_point(pp, 0, i);
            
            let mut events = Events::default();
            if i % 2 == 0 {
                events.start_loans.push(LoanId::new(i));
            }
            if i > 0 {
                let place = Place::Local { index: i - 1, wasm_type: WasmType::I32 };
                events.uses.push(place);
            }
            
            function.store_events(pp, events);
        }
        
        function
    }

    #[test]
    fn test_profiler_creation() {
        let profiler = DataflowProfiler::new();
        assert_eq!(profiler.profiles.len(), 0);
        assert_eq!(profiler.global_stats.functions_profiled, 0);
    }

    #[test]
    fn test_function_profiling() {
        let mut profiler = DataflowProfiler::new();
        let function = create_test_function();
        
        let result = profiler.profile_function(&function);
        assert!(result.is_ok(), "Function profiling should succeed");
        
        let profile = result.unwrap();
        assert_eq!(profile.function_name, "test_function");
        assert!(profile.total_time > Duration::new(0, 0));
        assert!(!profile.phase_times.is_empty());
    }

    #[test]
    fn test_memory_profile_generation() {
        let profiler = DataflowProfiler::new();
        let function = create_test_function();
        
        let liveness_stats = LivenessStatistics {
            total_program_points: 10,
            max_live_vars_at_point: 5,
            total_refinements: 3,
        };
        
        let dataflow_stats = DataflowStatistics {
            total_program_points: 10,
            total_loans: 5,
            max_live_loans_at_point: 3,
            max_live_loans_after_point: 3,
            avg_live_loans_per_point: 2.0,
        };
        
        let memory_profile = profiler.generate_memory_profile(&function, &liveness_stats, &dataflow_stats);
        
        assert!(memory_profile.peak_memory_bytes > 0);
        assert!(!memory_profile.memory_breakdown.is_empty());
        assert!(memory_profile.efficiency_metrics.bitset_utilization >= 0.0);
        assert!(memory_profile.efficiency_metrics.bitset_utilization <= 1.0);
    }

    #[test]
    fn test_complexity_metrics_generation() {
        let profiler = DataflowProfiler::new();
        let function = create_test_function();
        
        let dataflow_stats = DataflowStatistics {
            total_program_points: 10,
            total_loans: 5,
            max_live_loans_at_point: 3,
            max_live_loans_after_point: 3,
            avg_live_loans_per_point: 2.0,
        };
        
        let complexity_metrics = profiler.generate_complexity_metrics(&function, &dataflow_stats);
        
        assert!(complexity_metrics.dataflow_iterations > 0);
        assert!(complexity_metrics.worklist_operations > 0);
        assert!(complexity_metrics.bitset_operations > 0);
        assert!(!complexity_metrics.time_complexity.theoretical.is_empty());
        assert!(!complexity_metrics.space_complexity.theoretical.is_empty());
    }

    #[test]
    fn test_optimization_hints_generation() {
        let profiler = DataflowProfiler::new();
        let function = create_test_function();
        
        // Create memory profile with low utilization to trigger hints
        let memory_profile = MemoryProfile {
            peak_memory_bytes: 1024,
            memory_breakdown: HashMap::new(),
            efficiency_metrics: MemoryEfficiencyMetrics {
                bitset_utilization: 0.05, // Low utilization
                hashmap_load_factor: 0.75,
                overhead_percentage: 60.0, // High overhead
                potential_savings_bytes: 300,
            },
        };
        
        let complexity_metrics = ComplexityMetrics {
            dataflow_iterations: 20, // High iterations
            worklist_operations: 1000,
            bitset_operations: 50000, // High bitset operations
            time_complexity: ComplexityAnalysis {
                theoretical: "O(n²)".to_string(),
                observed_factor: 1.2,
                efficiency_rating: 0.5, // Low efficiency
            },
            space_complexity: ComplexityAnalysis {
                theoretical: "O(n)".to_string(),
                observed_factor: 1.0,
                efficiency_rating: 0.8,
            },
        };
        
        let hints = profiler.generate_optimization_hints(&function, &memory_profile, &complexity_metrics);
        
        // Should generate multiple hints due to the poor metrics
        assert!(!hints.is_empty());
        
        // Check that we get the expected hint categories
        let categories: Vec<_> = hints.iter().map(|h| &h.category).collect();
        assert!(categories.contains(&&OptimizationCategory::DataStructure));
        assert!(categories.contains(&&OptimizationCategory::MemoryLayout));
    }

    #[test]
    fn test_global_stats_update() {
        let mut profiler = DataflowProfiler::new();
        let function = create_test_function();
        
        // Profile the function
        profiler.profile_function(&function).unwrap();
        
        let stats = profiler.get_global_stats();
        assert_eq!(stats.functions_profiled, 1);
        assert!(stats.total_analysis_time > Duration::new(0, 0));
        assert!(stats.avg_time_per_function > Duration::new(0, 0));
        assert_eq!(stats.most_expensive_function, Some("test_function".to_string()));
    }

    #[test]
    fn test_report_generation() {
        let mut profiler = DataflowProfiler::new();
        let function = create_test_function();
        
        profiler.profile_function(&function).unwrap();
        
        let report = profiler.generate_report();
        assert!(!report.is_empty());
        assert!(report.contains("MIR Dataflow Analysis Profiling Report"));
        assert!(report.contains("test_function"));
        assert!(report.contains("Global Statistics"));
        assert!(report.contains("Overall Recommendations"));
    }

    #[test]
    fn test_entry_point_function() {
        let function = create_test_function();
        
        let result = profile_dataflow_analysis(&function);
        assert!(result.is_ok(), "Entry point profiling should succeed");
        
        let profile = result.unwrap();
        assert_eq!(profile.function_name, "test_function");
    }
}