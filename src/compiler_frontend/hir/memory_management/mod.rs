//! Memory Management Components for HIR Builder
//!
//! This module contains all components related to memory management during HIR generation,
//! including drop point insertion and ownership capability tracking.
//!
//! ## Key Components
//!
//! - **DropPointInserter**: Inserts possible_drop operations at control flow boundaries
//!
//! ## Design Philosophy
//!
//! The memory management components in HIR generation follow a conservative, structural approach:
//!
//! 1. **Structural Analysis Only**: No deep ownership analysis - that's the borrow checker's job
//! 2. **Conservative Insertion**: Insert drops where ownership *could* exist
//! 3. **Runtime Resolution**: Let the ownership flag determine actual drop behavior
//! 4. **Control Flow Boundaries**: Focus on scope exits, returns, breaks, and merges
//!
//! This approach ensures memory safety without requiring complex static analysis during HIR generation.

pub mod drop_point_inserter;

pub use drop_point_inserter::DropPointInserter;
