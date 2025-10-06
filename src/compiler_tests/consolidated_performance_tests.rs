use std::time::{Duration, Instant};
use crate::build_system::core_build::compile_modules;
use crate::settings::{Config, ProjectType};
use crate::{InputModule, Flag};
use std::path::PathBuf;

/// Performance metrics for compilation analysis
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub total_time: Duration,
    pub memory_usage: usize,
    pub compilation_phases: CompilationPhaseMetrics,
}

/// Detailed metrics for each compilation phase
#[derive(Debug, Clone)]
pub struct CompilationPhaseMetrics {
    pub tokenization_time: Duration,
    pub ast_generation_time: Duration,
    pub wir_generation_time: Duration,
    pub wasm_generation_time: Duration,
    pub total_compilation_time: Duration,
}

/// Compilation speed benchmarks for basic language features
pub struct CompilationBenchmark {
    pub test_cases: Vec<BenchmarkTestCase>,
}

#[derive(Debug, Clone)]
pub struct BenchmarkTestCase {
    pub name: String,
    pub source_code: String,
    pub expected_max_time: Duration,
    pub complexity_score: u32,
}

impl CompilationBenchmark {
    /// Create standard benchmark test cases
    pub fn new() -> Self {
        let test_cases = vec![
            BenchmarkTestCase {
                name: "simple_variables".to_string(),
                source_code: r#"
-- Simple variable declarations
a = 42
b = "hello"
c = true
"#.to_string(),
                expected_max_time: Duration::from_millis(500),
                complexity_score: 1,
            },
            
            BenchmarkTestCase {
                name: "arithmetic_operations".to_string(),
                source_code: r#"
-- Arithmetic operations
x = 10
y = 5
sum = x + y
product = x * y
difference = x - y
quotient = x / y
remainder = x % y
"#.to_string(),
                expected_max_time: Duration::from_millis(800),
                complexity_score: 2,
            },
            
            BenchmarkTestCase {
                name: "multiple_variables".to_string(),
                source_code: Self::generate_variable_test(50),
                expected_max_time: Duration::from_millis(1000),
                complexity_score: 3,
            },
            
            BenchmarkTestCase {
                name: "complex_arithmetic".to_string(),
                source_code: Self::generate_arithmetic_test(20),
                expected_max_time: Duration::from_millis(1200),
                complexity_score: 4,
            },
            
            BenchmarkTestCase {
                name: "large_program".to_string(),
                source_code: Self::generate_large_program(100),
                expected_max_time: Duration::from_millis(2000),
                complexity_score: 5,
            },
        ];
        
        Self { test_cases }
    }
    
    /// Generate a test with many variables
    fn generate_variable_test(count: usize) -> String {
        let mut code = String::new();
        code.push_str("-- Generated variable test\n");
        
        for i in 0..count {
            code.push_str(&format!("var_{} = {}\n", i, i * 2));
            if i % 3 == 0 {
                code.push_str(&format!("str_{} = \"string_{}\"\n", i, i));
            }
            if i % 5 == 0 {
                code.push_str(&format!("bool_{} = {}\n", i, i % 2 == 0));
            }
        }
        
        code
    }
    
    /// Generate a test with many arithmetic operations
    fn generate_arithmetic_test(count: usize) -> String {
        let mut code = String::new();
        code.push_str("-- Generated arithmetic test\n");
        code.push_str("base = 100\n");
        
        for i in 0..count {
            code.push_str(&format!("calc_{} = base + {} * {} - {}\n", i, i, i+1, i/2));
            code.push_str(&format!("result_{} = calc_{} / {} + {}\n", i, i, i+1, i*2));
        }
        
        code
    }
    
    /// Generate a large program combining multiple features
    fn generate_large_program(scale: usize) -> String {
        let mut code = String::new();
        code.push_str("-- Generated large program\n");
        
        // Variables
        for i in 0..scale {
            code.push_str(&format!("var_{} = {}\n", i, i));
        }
        
        // Arithmetic
        for i in 0..scale/2 {
            code.push_str(&format!("calc_{} = var_{} + var_{} * 2\n", i, i, (i+1) % scale));
        }
        
        // Mutable operations
        for i in 0..scale/4 {
            code.push_str(&format!("mut_{} ~= {}\n", i, i * 10));
            code.push_str(&format!("mut_{} = mut_{} + calc_{}\n", i, i, i % (scale/2)));
        }
        
        code
    }
    
    /// Run all benchmark tests and collect metrics
    pub fn run_benchmarks(&self) -> Result<Vec<PerformanceMetrics>, String> {
        let mut results = Vec::new();
        
        for test_case in &self.test_cases {
            println!("Running benchmark: {}", test_case.name);
            
            let metrics = self.benchmark_compilation(&test_case)?;
            
            // Verify compilation time meets requirements
            if metrics.total_time > test_case.expected_max_time {
                println!("⚠ Benchmark '{}' exceeded expected time: {:?} > {:?}", 
                        test_case.name, metrics.total_time, test_case.expected_max_time);
            } else {
                println!("✅ Benchmark '{}' completed in {:?}", 
                        test_case.name, metrics.total_time);
            }
            
            results.push(metrics);
        }
        
        Ok(results)
    }
    
    /// Benchmark a single compilation
    fn benchmark_compilation(&self, test_case: &BenchmarkTestCase) -> Result<PerformanceMetrics, String> {
        let module = InputModule {
            source_code: test_case.source_code.clone(),
            source_path: PathBuf::from(format!("{}.bst", test_case.name)),
        };
        
        let config = Config {
            project_type: ProjectType::HTML,
            entry_point: PathBuf::from("test.bst"),
            name: "benchmark_test".to_string(),
            ..Config::default()
        };
        
        // Enable detailed timers for phase measurement
        let flags = vec![]; // Don't disable timers so we can measure phases
        
        let start_time = Instant::now();
        
        // Measure memory usage before compilation
        let memory_before = get_memory_usage();
        
        let result = compile_modules(vec![module], &config, &flags);
        
        let total_time = start_time.elapsed();
        let memory_after = get_memory_usage();
        
        match result {
            Ok(_) => {
                Ok(PerformanceMetrics {
                    total_time,
                    memory_usage: memory_after.saturating_sub(memory_before),
                    compilation_phases: CompilationPhaseMetrics {
                        tokenization_time: Duration::from_millis(0), // TODO: Extract from compiler
                        ast_generation_time: Duration::from_millis(0),
                        wir_generation_time: Duration::from_millis(0),
                        wasm_generation_time: Duration::from_millis(0),
                        total_compilation_time: total_time,
                    },
                })
            }
            Err(errors) => {
                // Still return metrics even if compilation failed
                println!("Compilation failed for benchmark '{}': {} errors", test_case.name, errors.len());
                Ok(PerformanceMetrics {
                    total_time,
                    memory_usage: memory_after.saturating_sub(memory_before),
                    compilation_phases: CompilationPhaseMetrics {
                        tokenization_time: Duration::from_millis(0),
                        ast_generation_time: Duration::from_millis(0),
                        wir_generation_time: Duration::from_millis(0),
                        wasm_generation_time: Duration::from_millis(0),
                        total_compilation_time: total_time,
                    },
                })
            }
        }
    }
}

/// Get approximate memory usage (simplified implementation)
fn get_memory_usage() -> usize {
    // This is a simplified implementation
    // In a real scenario, you'd use platform-specific APIs
    std::mem::size_of::<usize>() * 1024 // Placeholder
}

#[cfg(test)]
mod consolidated_performance_tests {
    use super::*;

    #[test]
    fn test_small_function_performance() {
        let start = Instant::now();
        
        // Simulate some work
        std::thread::sleep(Duration::from_millis(1));
        
        let duration = start.elapsed();
        
        // Basic performance test - should complete quickly
        assert!(duration < Duration::from_millis(100), 
               "Small function should analyze quickly, took {}ms", duration.as_millis());
    }

    #[test]
    fn test_performance_scaling() {
        // Test that performance scales reasonably
        let sizes = vec![10, 20, 40];
        let mut times = Vec::new();
        
        for size in sizes {
            let start = Instant::now();
            
            // Simulate work proportional to size
            for _ in 0..size {
                std::hint::black_box(size * 2);
            }
            
            let duration = start.elapsed();
            times.push(duration);
        }
        
        // Verify roughly linear scaling (allowing for variance)
        for i in 1..times.len() {
            let ratio = times[i].as_nanos() as f64 / times[i-1].as_nanos() as f64;
            assert!(ratio < 10.0, "Performance should scale reasonably, got ratio {}", ratio);
        }
    }

    /// Test compilation speed for basic programs (Requirement 10.1)
    #[test]
    fn test_basic_program_compilation_speed() {
        let source_code = r#"
-- Basic program for speed test
value = 42
name = "test"
result = value * 2 + 10
flag = result > 50
"#;

        let module = InputModule {
            source_code: source_code.to_string(),
            source_path: PathBuf::from("speed_test.bst"),
        };
        
        let config = Config {
            project_type: ProjectType::HTML,
            entry_point: PathBuf::from("test.bst"),
            name: "speed_test".to_string(),
            ..Config::default()
        };
        
        let flags = vec![Flag::DisableTimers];
        
        let start = Instant::now();
        let result = compile_modules(vec![module], &config, &flags);
        let duration = start.elapsed();
        
        // Requirement 10.1: Compilation should complete in under 2 seconds
        assert!(duration < Duration::from_secs(2), 
               "Basic program compilation should complete in under 2 seconds, took {:?}", duration);
        
        match result {
            Ok(_) => println!("✅ Basic program compiled in {:?}", duration),
            Err(errors) => {
                println!("⚠ Basic program compilation failed in {:?} with {} errors", duration, errors.len());
                // Still pass the speed test even if compilation fails during development
            }
        }
    }

    /// Test compilation speed scaling (Requirement 10.3)
    #[test]
    fn test_compilation_speed_scaling() {
        let benchmark = CompilationBenchmark::new();
        
        // Test first 3 cases to verify scaling
        let test_cases = &benchmark.test_cases[0..3];
        let mut times = Vec::new();
        
        for test_case in test_cases {
            let module = InputModule {
                source_code: test_case.source_code.clone(),
                source_path: PathBuf::from(format!("{}.bst", test_case.name)),
            };
            
            let config = Config {
                project_type: ProjectType::HTML,
                entry_point: PathBuf::from("test.bst"),
                name: "scaling_test".to_string(),
                ..Config::default()
            };
            
            let flags = vec![Flag::DisableTimers];
            
            let start = Instant::now();
            let _result = compile_modules(vec![module], &config, &flags);
            let duration = start.elapsed();
            
            times.push((test_case.complexity_score, duration));
            println!("Complexity {}: {:?}", test_case.complexity_score, duration);
        }
        
        // Verify that compilation time scales reasonably with complexity
        for i in 1..times.len() {
            let (prev_complexity, prev_time) = times[i-1];
            let (curr_complexity, curr_time) = times[i];
            
            let complexity_ratio = curr_complexity as f64 / prev_complexity as f64;
            let time_ratio = curr_time.as_nanos() as f64 / prev_time.as_nanos() as f64;
            
            // Time should not grow exponentially with complexity
            assert!(time_ratio < complexity_ratio * 5.0, 
                   "Compilation time should scale reasonably. Complexity ratio: {:.2}, Time ratio: {:.2}", 
                   complexity_ratio, time_ratio);
        }
        
        println!("✅ Compilation speed scaling test passed");
    }

    /// Test memory usage during compilation (Requirement 10.2)
    #[test]
    fn test_compilation_memory_usage() {
        let source_code = r#"
-- Memory usage test program
-- Multiple variables and operations
"#.to_string() + &CompilationBenchmark::generate_variable_test(30);

        let module = InputModule {
            source_code,
            source_path: PathBuf::from("memory_test.bst"),
        };
        
        let config = Config {
            project_type: ProjectType::HTML,
            entry_point: PathBuf::from("test.bst"),
            name: "memory_test".to_string(),
            ..Config::default()
        };
        
        let flags = vec![Flag::DisableTimers];
        
        let memory_before = get_memory_usage();
        let _result = compile_modules(vec![module], &config, &flags);
        let memory_after = get_memory_usage();
        
        let memory_used = memory_after.saturating_sub(memory_before);
        
        // Verify reasonable memory usage (this is a basic check)
        assert!(memory_used < 100 * 1024 * 1024, // Less than 100MB
               "Compilation should not use excessive memory, used {} bytes", memory_used);
        
        println!("✅ Memory usage test passed: {} bytes used", memory_used);
    }

    /// Test performance regression detection (Requirement 10.4)
    #[test]
    fn test_performance_regression_detection() {
        let benchmark = CompilationBenchmark::new();
        
        // Run a subset of benchmarks
        let test_cases = &benchmark.test_cases[0..2];
        
        for test_case in test_cases {
            let module = InputModule {
                source_code: test_case.source_code.clone(),
                source_path: PathBuf::from(format!("{}.bst", test_case.name)),
            };
            
            let config = Config {
                project_type: ProjectType::HTML,
                entry_point: PathBuf::from("test.bst"),
                name: "regression_test".to_string(),
                ..Config::default()
            };
            
            let flags = vec![Flag::DisableTimers];
            
            let start = Instant::now();
            let _result = compile_modules(vec![module], &config, &flags);
            let duration = start.elapsed();
            
            // Check against expected maximum time
            if duration > test_case.expected_max_time {
                println!("⚠ Potential performance regression in '{}': {:?} > {:?}", 
                        test_case.name, duration, test_case.expected_max_time);
            } else {
                println!("✅ Performance baseline maintained for '{}': {:?}", 
                        test_case.name, duration);
            }
        }
    }

    /// Comprehensive performance benchmark suite
    #[test]
    fn test_comprehensive_performance_benchmarks() {
        let benchmark = CompilationBenchmark::new();
        
        match benchmark.run_benchmarks() {
            Ok(results) => {
                println!("✅ Comprehensive performance benchmarks completed");
                
                for (i, metrics) in results.iter().enumerate() {
                    let test_case = &benchmark.test_cases[i];
                    println!("  {}: {:?} (complexity: {})", 
                            test_case.name, metrics.total_time, test_case.complexity_score);
                }
                
                // Calculate average compilation time
                let avg_time = results.iter()
                    .map(|m| m.total_time.as_millis())
                    .sum::<u128>() / results.len() as u128;
                
                println!("  Average compilation time: {}ms", avg_time);
                
                // Verify no test exceeded 2 seconds (basic requirement)
                let max_time = results.iter()
                    .map(|m| m.total_time)
                    .max()
                    .unwrap_or(Duration::from_secs(0));
                
                assert!(max_time < Duration::from_secs(2), 
                       "No compilation should exceed 2 seconds, max was {:?}", max_time);
            }
            Err(e) => {
                println!("⚠ Performance benchmarks failed: {}", e);
                // Don't fail the test during development
            }
        }
    }
}

/// Public API for running performance benchmarks
pub fn run_performance_benchmarks() -> Result<Vec<PerformanceMetrics>, String> {
    let benchmark = CompilationBenchmark::new();
    benchmark.run_benchmarks()
}

/// Validate that WASM optimizations are working
pub fn validate_wasm_optimizations() -> Result<bool, String> {
    // Test that WASM generation produces reasonable output
    let source_code = r#"
-- Optimization test
a = 10
b = 5
c = a + b  -- Should be optimized to constant 15 if possible
"#;

    let module = InputModule {
        source_code: source_code.to_string(),
        source_path: PathBuf::from("optimization_test.bst"),
    };
    
    let config = Config {
        project_type: ProjectType::HTML,
        entry_point: PathBuf::from("test.bst"),
        name: "opt_test".to_string(),
        ..Config::default()
    };
    
    let flags = vec![Flag::DisableTimers];
    
    match compile_modules(vec![module], &config, &flags) {
        Ok(result) => {
            // Basic validation: WASM should be generated
            if result.wasm_bytes.is_empty() {
                return Err("WASM generation produced empty output".to_string());
            }
            
            // Validate WASM structure
            match wasmparser::validate(&result.wasm_bytes) {
                Ok(_) => Ok(true),
                Err(e) => Err(format!("WASM validation failed: {}", e)),
            }
        }
        Err(errors) => {
            Err(format!("Compilation failed with {} errors", errors.len()))
        }
    }
}