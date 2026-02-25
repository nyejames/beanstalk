//! Borrow Checker Transfer Driver
//!
//! This module coordinates block transfer for the borrow checker.
//! Statement/terminator rules live in submodules so the control flow of the
//! fixed-point analysis remains easy to follow from a single entrypoint.

mod access;
mod call_semantics;
mod facts;

use crate::backends::function_registry::HostRegistry;
use crate::compiler_frontend::analysis::borrow_checker::diagnostics::BorrowDiagnostics;
use crate::compiler_frontend::analysis::borrow_checker::state::{BorrowState, FunctionLayout};
use crate::compiler_frontend::analysis::borrow_checker::types::{
    FunctionReturnAliasSummary, StatementBorrowFact, TerminatorBorrowFact, ValueBorrowFact,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{BlockId, FunctionId, HirNodeId, HirValueId};
use crate::compiler_frontend::hir::hir_nodes::{HirBlock, HirModule};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use rustc_hash::FxHashMap;

use access::{transfer_statement, transfer_terminator};
use facts::ValueFactBuffer;

pub(super) struct BorrowTransferContext<'a> {
    pub module: &'a HirModule,
    pub string_table: &'a StringTable,
    pub host_registry: &'a HostRegistry,
    pub function_by_path: &'a FxHashMap<InternedPath, FunctionId>,
    pub function_param_mutability: &'a FxHashMap<FunctionId, Vec<bool>>,
    pub function_return_alias: &'a FxHashMap<FunctionId, FunctionReturnAliasSummary>,
    pub diagnostics: BorrowDiagnostics<'a>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct BlockTransferStats {
    pub statements_analyzed: usize,
    pub terminators_analyzed: usize,
    pub conflicts_checked: usize,
    pub mutable_call_sites: usize,
    pub statement_facts: Vec<(HirNodeId, StatementBorrowFact)>,
    pub terminator_fact: Option<(BlockId, TerminatorBorrowFact)>,
    pub value_facts: Vec<(HirValueId, ValueBorrowFact)>,
}

/// Executes one forward transfer step for a single basic block.
///
/// The caller owns fixed-point scheduling. This function only applies local
/// transfer rules and emits borrow facts for the processed block.
pub(super) fn transfer_block(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    block: &HirBlock,
    state: &mut BorrowState,
) -> Result<BlockTransferStats, CompilerError> {
    let mut stats = BlockTransferStats::default();
    let mut value_fact_buffer = ValueFactBuffer::new(layout.local_count());

    for statement in &block.statements {
        transfer_statement(
            context,
            layout,
            state,
            block.id,
            statement,
            &mut stats,
            &mut value_fact_buffer,
        )?;
        stats.statements_analyzed += 1;
    }

    transfer_terminator(
        context,
        layout,
        state,
        block.id,
        &block.terminator,
        &mut stats,
        &mut value_fact_buffer,
    )?;
    stats.terminators_analyzed += 1;

    stats.value_facts = value_fact_buffer.into_serialized(layout);
    Ok(stats)
}
