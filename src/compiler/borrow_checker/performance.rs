//! Performance monitoring and benchmarking for borrow checker.
//!
//! Provides performance metrics tracking to monitor and prevent regressions
//! in borrow checker performance.

use std::time::{Duration, Instant};
use std::collections::HashMap;

/// Performance metrics for borrow checker operations.
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    /// Total time spent in borrow checking
    pub total_time: Duration,
    
    /// Time spent in CFG construction
    pub cfg_construction_time: Duration,
    
    /// Time spent in borrow tracking
    pub borrow_tracking_time: Duration,
    
    /// Time spent in last-use analysis
    pub last_use_analysis_time: Duration,
    
    /// Time spent in conflict detection
    pub conflict_detection_time: Duration,
    
    /// Time spent in lifetime inference
    pub lifetime_inference_time: Duration,
    
    /// Number of CFG nodes processed
    pub cfg_nodes_count: usize,
    
    /// Number of borrows tracked
    pub borrows_tracked_count: usize,
    
    /// Number of conflicts detected
    pub conflicts_detected_count: usize,
    
    /// Number of dataflow iterations
    pub dataflow_iterations: usize,
    
    /// Peak memory usage (estimated)
    pub peak_memory_bytes: usize,
    
    /// Custom timing measurements
    pub custom_timings: HashMap<String, Duration>,
}

impl PerformanceMetrics {
    /// Create new empty performance metrics
    pub fn new() -> Self {
        Self {
            total_time: Duration::ZERO,
            cfg_construction_time: Duration::ZERO,
            borrow_tracking_time: Duration::ZERO,
            last_use_analysis_time: Duration::ZERO,
            conflict_detection_time: Duration::ZERO,
            lifetime_inference_time: Duration::ZERO,
            cfg_nodes_count: 0,
            borrows_tracked_count: 0,
            conflicts_detected_count: 0,
            dataflow_iterations: 0,
            peak_memory_bytes: 0,
            custom_timings: HashMap::new(),
        }
    }

    /// Add a custom timing measurement
    pub fn add_custom_timing(&mut self, name: String, duration: Duration) {
        self.custom_timings.insert(name, duration);
    }

    /// Print performance report
    pub fn print_report(&self) {
        println!("\n=== Borrow Checker Performance Report ===");
        println!("Total time: {:?}", self.total_time);
        println!("  CFG construction: {:?}", self.cfg_construction_time);
        println!("  Borrow tracking: {:?}", self.borrow_tracking_time);
        println!("  Last-use analysis: {:?}", self.last_use_analysis_time);
        println!("  Conflict detection: {:?}", self.conflict_detection_time);
        println!("  Lifetime inference: {:?}", self.lifetime_inference_time);
        println!("\nStatistics:");
        println!("  CFG nodes: {}", self.cfg_nodes_count);
        println!("  Borrows tracked: {}", self.borrows_tracked_count);
        println!("  Conflicts detected: {}", self.conflicts_detected_count);
        println!("  Dataflow iterations: {}", self.dataflow_iterations);
        println!("  Peak memory (estimated): {} KB", self.peak_memory_bytes / 1024);
        
        if !self.custom_timings.is_empty() {
            println!("\nCustom timings:");
            for (name, duration) in &self.custom_timings {
                println!("  {}: {:?}", name, duration);
            }
        }
        println!("==========================================\n");
    }

    /// Check if performance is within acceptable bounds
    pub fn check_performance_bounds(&self) -> Vec<String> {
        let mut warnings = Vec::new();

        // Check if any phase takes too long relative to total time
        let total_ms = self.total_time.as_millis();
        if total_ms > 0 {
            let cfg_percent = (self.cfg_construction_time.as_millis() * 100) / total_ms;
            let borrow_percent = (self.borrow_tracking_time.as_millis() * 100) / total_ms;
            let last_use_percent = (self.last_use_analysis_time.as_millis() * 100) / total_ms;
            let conflict_percent = (self.conflict_detection_time.as_millis() * 100) / total_ms;

            if cfg_percent > 30 {
                warnings.push(format!(
                    "CFG construction taking {}% of total time (expected <30%)",
                    cfg_percent
                ));
            }
            if borrow_percent > 25 {
                warnings.push(format!(
                    "Borrow tracking taking {}% of total time (expected <25%)",
                    borrow_percent
                ));
            }
            if last_use_percent > 30 {
                warnings.push(format!(
                    "Last-use analysis taking {}% of total time (expected <30%)",
                    last_use_percent
                ));
            }
            if conflict_percent > 25 {
                warnings.push(format!(
                    "Conflict detection taking {}% of total time (expected <25%)",
                    conflict_percent
                ));
            }
        }

        // Check dataflow iterations
        if self.dataflow_iterations > self.cfg_nodes_count * 5 {
            warnings.push(format!(
                "Dataflow iterations ({}) exceeds expected bound ({})",
                self.dataflow_iterations,
                self.cfg_nodes_count * 5
            ));
        }

        warnings
    }
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Timer for measuring operation duration
pub struct Timer {
    start: Instant,
    name: String,
}

impl Timer {
    /// Start a new timer
    pub fn start(name: impl Into<String>) -> Self {
        Self {
            start: Instant::now(),
            name: name.into(),
        }
    }

    /// Stop the timer and return elapsed duration
    pub fn stop(self) -> (String, Duration) {
        (self.name, self.start.elapsed())
    }

    /// Stop the timer and print elapsed time
    pub fn stop_and_print(self) {
        let elapsed = self.start.elapsed();
        println!("[PERF] {}: {:?}", self.name, elapsed);
    }
}

/// Macro for timing a block of code
#[macro_export]
macro_rules! time_operation {
    ($metrics:expr, $field:ident, $block:block) => {{
        let timer = std::time::Instant::now();
        let result = $block;
        $metrics.$field = timer.elapsed();
        result
    }};
}

/// Benchmark runner for regression testing
pub struct BenchmarkRunner {
    benchmarks: Vec<Benchmark>,
}

#[derive(Clone)]
pub struct Benchmark {
    pub name: String,
    pub cfg_nodes: usize,
    pub expected_max_time_ms: u128,
}

impl BenchmarkRunner {
    /// Create a new benchmark runner
    pub fn new() -> Self {
        Self {
            benchmarks: Vec::new(),
        }
    }

    /// Add a benchmark
    pub fn add_benchmark(&mut self, name: String, cfg_nodes: usize, expected_max_time_ms: u128) {
        self.benchmarks.push(Benchmark {
            name,
            cfg_nodes,
            expected_max_time_ms,
        });
    }

    /// Run benchmarks and check for regressions
    pub fn check_regressions(&self, metrics: &PerformanceMetrics) -> Vec<String> {
        let mut regressions = Vec::new();

        for benchmark in &self.benchmarks {
            if metrics.cfg_nodes_count == benchmark.cfg_nodes {
                let actual_time_ms = metrics.total_time.as_millis();
                if actual_time_ms > benchmark.expected_max_time_ms {
                    regressions.push(format!(
                        "Regression in {}: {}ms > {}ms (expected max)",
                        benchmark.name, actual_time_ms, benchmark.expected_max_time_ms
                    ));
                }
            }
        }

        regressions
    }
}

impl Default for BenchmarkRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_performance_metrics_creation() {
        let metrics = PerformanceMetrics::new();
        assert_eq!(metrics.cfg_nodes_count, 0);
        assert_eq!(metrics.borrows_tracked_count, 0);
    }

    #[test]
    fn test_custom_timing() {
        let mut metrics = PerformanceMetrics::new();
        metrics.add_custom_timing("test_operation".to_string(), Duration::from_millis(100));
        assert!(metrics.custom_timings.contains_key("test_operation"));
    }

    #[test]
    fn test_performance_bounds() {
        let mut metrics = PerformanceMetrics::new();
        metrics.total_time = Duration::from_millis(1000);
        metrics.cfg_construction_time = Duration::from_millis(400); // 40% - should warn
        
        let warnings = metrics.check_performance_bounds();
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("CFG construction"));
    }

    #[test]
    fn test_timer() {
        let timer = Timer::start("test");
        std::thread::sleep(Duration::from_millis(10));
        let (name, duration) = timer.stop();
        assert_eq!(name, "test");
        assert!(duration.as_millis() >= 10);
    }

    #[test]
    fn test_benchmark_runner() {
        let mut runner = BenchmarkRunner::new();
        runner.add_benchmark("small_cfg".to_string(), 10, 100);
        
        let mut metrics = PerformanceMetrics::new();
        metrics.cfg_nodes_count = 10;
        metrics.total_time = Duration::from_millis(150); // Exceeds expected
        
        let regressions = runner.check_regressions(&metrics);
        assert!(!regressions.is_empty());
    }
}
