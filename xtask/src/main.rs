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
//! - `bench`       - Full benchmark (2 warmup, 10 measured)
//! - `bench-quick` - Quick benchmark (1 warmup, 3 measured)
//! - `bench-ci`    - CI benchmark (0 warmup, 1 measured)

mod bench;
mod case_parser;
mod command;
mod report;

use bench::{BenchOptions, run_benchmarks};
use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    // Expect exactly one argument (the mode)
    // args[0] is the program name
    if args.len() != 2 {
        eprintln!("Usage: xtask <mode>");
        eprintln!();
        eprintln!("Modes:");
        eprintln!("  bench       Full benchmark (2 warmup, 10 measured)");
        eprintln!("  bench-quick Quick benchmark (1 warmup, 3 measured)");
        eprintln!("  bench-ci    CI benchmark (0 warmup, 1 measured)");
        process::exit(1);
    }

    let mode_str = &args[1];
    let options = match mode_str.as_str() {
        "bench" => BenchOptions::full(),
        "bench-quick" => BenchOptions::quick(),
        "bench-ci" => BenchOptions::ci(),
        _ => {
            eprintln!("Error: Unknown mode '{}'", mode_str);
            eprintln!();
            eprintln!("Valid modes: bench, bench-quick, bench-ci");
            process::exit(1);
        }
    };

    // Run benchmarks
    match run_benchmarks(options) {
        Ok(()) => {
            process::exit(0);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    }
}
