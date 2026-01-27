//! # Borrow Checker Module
//!
//! The Beanstalk borrow checker enforces memory safety rules and enables optimization opportunities.
//! Unlike traditional borrow checkers that are required for correctness, Beanstalk's borrow checker
//! serves as an optimization enabler - values that pass borrow checking become eligible for non-GC
//! memory management, while those that fail remain safely managed by the garbage collector.
//!
//! ## Architecture
//!
//! The borrow checker follows a multi-phase analysis approach:
//! 1. **Place Analysis**: Extract and categorize all places (variables, fields, indices) from HIR
//! 2. **Control Flow Analysis**: Build control flow graph and identify merge points
//! 3. **Dataflow Analysis**: Perform forward and backward dataflow analysis
//! 4. **Last-Use Analysis**: Identify final usage points for ownership transfer opportunities
//! 5. **Conflict Detection**: Detect borrowing conflicts and generate errors
//! 6. **Drop Validation**: Ensure all potentially owned values reach drop sites
//! 7. **Ownership Annotation**: Add eligibility annotations to HIR nodes
//!
//! ## Memory Safety Rules
//!
//! The borrow checker enforces these core rules:
//! - **Shared References**: Multiple shared references to the same place are allowed
//! - **Mutable Access**: Only one mutable access allowed at a time, exclusive of all other access
//! - **Move Safety**: Prevent use-after-move violations
//! - **Field-Level Precision**: Track borrowing at field granularity for disjoint access
//!
//! ## Integration
//!
//! The borrow checker integrates between HIR generation and backend lowering:
//! - Input: HIR module with place-based tracking
//! - Output: Annotated HIR with ownership eligibility information
//! - Errors: Detailed borrow checking violations with actionable suggestions

pub mod borrow_state;
pub mod control_flow;
pub mod dataflow;
pub mod error_reporting;
pub mod place_registry;

use self::control_flow::ControlFlowGraph;
use self::dataflow::DataflowEngine;
use self::error_reporting::BorrowErrorReporter;
use self::place_registry::PlaceRegistry;
use crate::compiler::compiler_errors::CompilerMessages;
use crate::compiler::hir::nodes::HirModule;
use crate::compiler::string_interning::StringTable;

pub struct BorrowCheckOutcome {
    pub analysis: dataflow::AnalysisResults,
}

/// Main borrow checker that orchestrates all analysis phases
pub struct BorrowChecker;

impl BorrowChecker {
    /// Create a new borrow checker instance
    pub fn new() -> Self {
        Self
    }

    /// Check a HIR module for borrow safety violations
    ///
    /// Returns Ok(()) if the module passes borrow checking, or Err with detailed error messages
    /// if violations are found. The HIR module is annotated with ownership eligibility information
    /// when checking succeeds.
    pub fn check_module(
        &mut self,
        hir_module: &HirModule,
        string_table: &StringTable,
    ) -> Result<BorrowCheckOutcome, CompilerMessages> {
        let control_flow = ControlFlowGraph::from_hir_module(hir_module);
        let mut dataflow = DataflowEngine::new(control_flow, PlaceRegistry::new());
        let analysis = dataflow.analyze(hir_module);

        if analysis.conflicts.is_empty() {
            return Ok(BorrowCheckOutcome { analysis });
        }

        let reporter = BorrowErrorReporter::new(dataflow.place_registry().clone(), string_table);

        let errors = analysis
            .conflicts
            .iter()
            .map(|conflict| reporter.create_borrow_conflict_error(conflict))
            .collect();

        Err(CompilerMessages {
            errors,
            warnings: Vec::new(),
        })
    }
}
