//! Borrow Checker Driver
//!
//! This module orchestrates borrow checking for a complete HIR module.
//! It builds function metadata, runs a forward fixed-point dataflow analysis
//! per function, and stores snapshots/facts for downstream phases.

mod diagnostics;
mod engine;
mod metadata;
mod state;
mod transfer;
mod types;

pub(crate) use types::{
    BorrowAnalysis, BorrowCheckReport, BorrowDropSiteKind, LocalMode,
};

#[cfg(test)]
pub(crate) use types::{BorrowStateSnapshot, LocalBorrowSnapshot, BorrowDropSite};
pub(crate) type BorrowFacts = BorrowAnalysis;

use crate::compiler_frontend::analysis::borrow_checker::engine::BorrowChecker;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::HirModule;
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;

pub(crate) fn check_borrows(
    module: &HirModule,
    host_registry: &HostRegistry,
    string_table: &StringTable,
) -> Result<BorrowCheckReport, CompilerError> {
    BorrowChecker::new(module, host_registry, string_table).run()
}

#[cfg(test)]
mod tests;
