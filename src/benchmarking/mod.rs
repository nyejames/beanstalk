//! Public compiler benchmarking API for dev tooling.
//!
//! WHAT: provides in-process benchmark entry points that reuse production
//! compiler setup without duplicating project discovery or builder logic.
//! WHY: xtask and other tooling need focused compiler-stage measurements
//! without subprocess overhead.

pub mod frontend;

pub use frontend::{
    FrontendBenchmarkBuildProfile, FrontendBenchmarkCounter, FrontendBenchmarkError,
    FrontendBenchmarkOptions, FrontendBenchmarkReport, FrontendBenchmarkStage,
    run_frontend_benchmark,
};

#[cfg(test)]
mod tests;
