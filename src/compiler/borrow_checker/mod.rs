//! Borrow checker for validating memory safety and ownership rules on HIR.

pub mod borrow_tracking;
pub mod candidate_move_refinement;
pub mod cfg;
pub mod checker;
pub mod conflict_detection;
pub mod drop_insertion;
pub mod last_use;
pub mod lifetime_inference;
pub mod performance;
pub mod structured_control_flow;
pub mod types;

// Re-export commonly used types
pub use checker::check_borrows;
pub use types::{BorrowChecker, BorrowKind, BorrowState, ControlFlowGraph, Loan};
pub use performance::{PerformanceMetrics, Timer, BenchmarkRunner};