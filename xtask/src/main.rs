//! xtask - Benchmark orchestration tool for Beanstalk compiler
//!
//! This crate provides build automation and benchmark tooling for the Beanstalk
//! compiler project. It is a workspace member that runs benchmarks and generates
//! timing reports.
//!
//! # Usage
//!
//! ```text
//! cargo run --package xtask --bin xtask -- <mode>
//! ```
//!
//! Modes:
//! - `bench`                - Run the full benchmark suite and update local/public summaries
//! - `bench-check`          - Run the full benchmark suite without writing benchmark history
//! - `bench-frontend-check` - Run the focused frontend benchmark suite without writing history
//! - `bench-frontend`       - Run the focused frontend benchmark suite and record

mod bench;
mod bench_history;
mod bench_migration;
mod bench_observations;
mod bench_summary;
mod bench_system;
mod bench_time;
mod bench_types;
mod case_parser;
mod compiler_binary;
mod frontend_bench;
mod mode;
mod process_runner;

use bench::{BenchMode, BenchOptions, run_benchmarks};
use frontend_bench::{FrontendBenchMode, FrontendBenchOptions, run_frontend_benchmarks};
use mode::BenchmarkMode;
use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        eprintln!("Usage: xtask <mode>");
        eprintln!();
        eprintln!("Modes:");
        eprintln!(
            "  bench                Run the full benchmark suite and update local/public summaries"
        );
        eprintln!(
            "  bench-check          Run the full benchmark suite without writing benchmark history"
        );
        eprintln!(
            "  bench-frontend-check Run the focused frontend benchmark suite without writing history"
        );
        eprintln!("  bench-frontend       Run the focused frontend benchmark suite and record");
        process::exit(1);
    }

    let mode_str = &args[1];

    let Some(mode) = BenchmarkMode::parse(mode_str) else {
        eprintln!("Error: Unknown mode '{}'", mode_str);
        eprintln!();
        eprintln!("Valid modes: bench, bench-check, bench-frontend-check, bench-frontend");
        process::exit(1);
    };

    match mode {
        BenchmarkMode::Bench => {
            let options = BenchOptions {
                warmup_runs: 1,
                measured_iterations: 10,
                mode: BenchMode::Record,
            };

            exit_with_result(run_benchmarks(options));
        }
        BenchmarkMode::BenchCheck => {
            let options = BenchOptions {
                warmup_runs: 1,
                measured_iterations: 10,
                mode: BenchMode::Check,
            };

            exit_with_result(run_benchmarks(options));
        }
        BenchmarkMode::BenchFrontendCheck => {
            let options = FrontendBenchOptions {
                warmup_runs: 1,
                measured_iterations: 10,
                mode: FrontendBenchMode::Check,
            };

            exit_with_result(run_frontend_benchmarks(options));
        }
        BenchmarkMode::BenchFrontend => {
            let options = FrontendBenchOptions {
                warmup_runs: 1,
                measured_iterations: 10,
                mode: FrontendBenchMode::Record,
            };

            exit_with_result(run_frontend_benchmarks(options));
        }
    }
}

fn exit_with_result(result: Result<(), String>) -> ! {
    match result {
        Ok(()) => process::exit(0),
        Err(error) => {
            eprintln!("Error: {}", error);
            process::exit(1);
        }
    }
}
