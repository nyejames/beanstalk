//! Borrow Checker Module
//!
//! This module implements Beanstalk's borrow checker, which validates memory safety
//! and ownership rules on HIR (High-Level Intermediate Representation). The borrow
//! checker ensures all borrowing, ownership transfers, and memory access patterns
//! are safe before lowering to LIR.
//!
//! ## Architecture
//!
//! The borrow checker consists of several key components:
//! - **Control Flow Graph (CFG) Construction**: Builds a graph representation of program flow
//! - **Borrow Tracking**: Tracks active borrows and their lifetimes across the CFG
//! - **Last-Use Analysis**: Determines when values are used for the final time
//! - **Conflict Detection**: Detects memory safety violations using place overlap analysis
//!
//! ## Key Design Principles
//!
//! - **HIR-Only Operation**: Operates exclusively on HIR, never on AST or LIR
//! - **Automatic Lifetime Inference**: No explicit lifetime annotations required
//! - **Last-Use Move Optimization**: Candidate moves become actual moves when they are the final use
//! - **Polonius-Style Analysis**: Path-sensitive conflict detection
//! - **Place-Based Memory Model**: All memory access tracked through precise place representations
//! - **Conservative Analysis**: When in doubt, chooses the safe option

pub mod checker;
pub mod cfg;
pub mod borrow_tracking;
pub mod conflict_detection;
pub mod types;



// Re-export main entry point
pub use checker::check_borrows;

// Re-export core types
pub use types::{
    BorrowChecker, ControlFlowGraph, BorrowState, Loan, BorrowKind, CfgNodeType,
    BorrowId, CfgRegion
};