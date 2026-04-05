//! JavaScript backend for Beanstalk.
//!
//! This backend lowers HIR into readable JavaScript using GC semantics.
//! Borrowing and ownership are optimization concerns and therefore ignored here.

mod js_expr;
mod js_function;
mod js_host_functions;
mod js_statement;
mod prelude;
mod symbols;
mod utils;

#[cfg(test)]
pub(crate) mod test_symbol_helpers;
#[cfg(test)]
mod tests;

use crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FieldId, FunctionId, HirBlock, HirModule, LocalId,
};
use crate::compiler_frontend::string_interning::StringTable;
use std::collections::{HashMap, HashSet};

/// Configuration for JS lowering.
///
/// WHAT: selects formatting/debug emission behavior for generated JS modules.
/// WHY: build targets need one explicit knob set for dev vs release codegen output.
#[derive(Debug, Clone)]
pub struct JsLoweringConfig {
    /// Emit human-readable formatting.
    pub pretty: bool,

    /// Emit source location comments.
    pub emit_locations: bool,

    /// Automatically invoke the module start function.
    pub auto_invoke_start: bool,
}

impl JsLoweringConfig {
    /// Standard HTML builder lowering config.
    ///
    /// WHY: both JS-only and Wasm builder paths use the same JS lowering settings. Centralising
    /// this avoids the settings drifting independently across call sites.
    pub fn standard_html(release_build: bool) -> Self {
        JsLoweringConfig {
            pretty: !release_build,
            emit_locations: false,
            auto_invoke_start: false,
        }
    }
}

/// Result of lowering a HIR module to JavaScript.
///
/// WHAT: emitted source plus the stable HIR-function-id -> JS-name mapping.
/// WHY: builders and tests need the name map for start calls and artifact assertions.
#[derive(Debug, Clone)]
pub struct JsModule {
    /// Complete JS source code.
    pub source: String,
    pub function_name_by_id: HashMap<FunctionId, String>,
}

/// Lower one validated HIR module into JavaScript source.
///
/// WHAT: converts HIR control flow/expressions into executable JS and symbol maps.
/// WHY: JS is the stable near-term backend and needs deterministic lowering output.
pub fn lower_hir_to_js(
    hir: &HirModule,
    borrow_analysis: &BorrowCheckReport,
    string_table: &StringTable,
    config: JsLoweringConfig,
) -> Result<JsModule, CompilerError> {
    let mut emitter = JsEmitter::new(hir, borrow_analysis, string_table, config);
    emitter.lower_module()
}

pub(crate) struct JsEmitter<'hir> {
    pub(crate) hir: &'hir HirModule,
    pub(crate) borrow_analysis: &'hir BorrowCheckReport,
    pub(crate) string_table: &'hir StringTable,
    pub(crate) config: JsLoweringConfig,

    pub(crate) out: String,
    pub(crate) indent: usize,

    pub(crate) blocks_by_id: HashMap<BlockId, &'hir HirBlock>,

    pub(crate) function_name_by_id: HashMap<FunctionId, String>,
    pub(crate) local_name_by_id: HashMap<LocalId, String>,
    pub(crate) field_name_by_id: HashMap<FieldId, String>,

    used_identifiers: HashSet<String>,
    temp_counter: usize,
}

impl<'hir> JsEmitter<'hir> {
    pub(crate) fn new(
        hir: &'hir HirModule,
        borrow_analysis: &'hir BorrowCheckReport,
        string_table: &'hir StringTable,
        config: JsLoweringConfig,
    ) -> Self {
        let blocks_by_id = hir
            .blocks
            .iter()
            .map(|block| (block.id, block))
            .collect::<HashMap<_, _>>();

        Self {
            hir,
            borrow_analysis,
            string_table,
            config,
            out: String::new(),
            indent: 0,
            blocks_by_id,
            function_name_by_id: HashMap::new(),
            local_name_by_id: HashMap::new(),
            field_name_by_id: HashMap::new(),
            used_identifiers: HashSet::new(),
            temp_counter: 0,
        }
    }

    fn lower_module(&mut self) -> Result<JsModule, CompilerError> {
        self.build_symbol_maps();
        self.emit_runtime_prelude();

        let mut functions = self.hir.functions.iter().collect::<Vec<_>>();
        functions.sort_by_key(|function| function.id.0);

        for (index, function) in functions.into_iter().enumerate() {
            if index > 0 {
                self.emit_line("");
            }

            self.emit_function(function)?;
        }

        if self.config.auto_invoke_start {
            let Some(start_name) = self
                .function_name_by_id
                .get(&self.hir.start_function)
                .cloned()
            else {
                return Err(CompilerError::compiler_error(format!(
                    "JavaScript backend: start function {:?} has no generated JS name",
                    self.hir.start_function
                )));
            };

            if !self.out.is_empty() {
                self.emit_line("");
            }

            self.emit_line(&format!("{}();", start_name));
        }

        Ok(JsModule {
            source: self.out.clone(),
            function_name_by_id: self.function_name_by_id.clone(),
        })
    }
}
