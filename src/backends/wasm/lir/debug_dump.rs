//! Debug pretty-printer for Wasm LIR modules.
//!
//! Phase-1 note:
//! this format is for developer diagnostics and tests, not a stable external format.

use crate::backends::wasm::lir::function::WasmLirFunction;
use crate::backends::wasm::lir::instructions::{WasmLirStmt, WasmLirTerminator};
use crate::backends::wasm::lir::module::WasmLirModule;
use std::fmt::Write as _;

pub(crate) fn dump_lir_module(module: &WasmLirModule) -> String {
    let mut out = String::new();

    let _ = writeln!(out, "WasmLirModule");
    let _ = writeln!(out, "  functions: {}", module.functions.len());
    let _ = writeln!(out, "  imports: {}", module.imports.len());
    let _ = writeln!(out, "  exports: {}", module.exports.len());
    let _ = writeln!(out, "  static_data: {}", module.static_data.len());
    let _ = writeln!(
        out,
        "  memory_plan: initial_pages={} max_pages={:?} static_data_base={} heap_base_strategy={:?}",
        module.memory_plan.initial_pages,
        module.memory_plan.max_pages,
        module.memory_plan.static_data_base,
        module.memory_plan.heap_base_strategy,
    );

    for function in &module.functions {
        dump_function(&mut out, function);
    }

    out
}

fn dump_function(out: &mut String, function: &WasmLirFunction) {
    let _ = writeln!(
        out,
        "\nfn {:?} {} origin={:?} linkage={:?}",
        function.id, function.debug_name, function.origin, function.linkage,
    );
    let _ = writeln!(
        out,
        "  sig params={:?} results={:?}",
        function.signature.params, function.signature.results,
    );

    for local in &function.locals {
        let _ = writeln!(
            out,
            "  local {:?} role={:?} ty={:?} name={:?}",
            local.id, local.role, local.ty, local.name,
        );
    }

    for block in &function.blocks {
        let _ = writeln!(out, "  block {:?}", block.id);
        for statement in &block.statements {
            let _ = writeln!(out, "    {}", format_stmt(statement));
        }
        let _ = writeln!(out, "    term {}", format_terminator(&block.terminator));
    }
}

fn format_stmt(statement: &WasmLirStmt) -> String {
    format!("{:?}", statement)
}

fn format_terminator(terminator: &WasmLirTerminator) -> String {
    format!("{:?}", terminator)
}
