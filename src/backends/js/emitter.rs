//! JS emitter — lowers HIR into executable JavaScript.
//!
//! WHAT: converts HIR control flow/expressions into executable JS and symbol maps.
//! WHY: JS is the stable near-term backend and needs deterministic lowering output.

use crate::backends::js::JsLoweringConfig;
use crate::backends::js::JsModule;
use crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::hir::blocks::HirBlock;
use crate::compiler_frontend::hir::ids::{BlockId, FieldId, FunctionId, LocalId};
use crate::compiler_frontend::hir::module::HirModule;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use std::collections::{HashMap, HashSet};

/// Lower one validated HIR module into JavaScript source.
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
    pub(crate) external_package_registry: ExternalPackageRegistry,
    pub(crate) out: String,
    pub(crate) indent: usize,
    pub(crate) blocks_by_id: HashMap<BlockId, &'hir HirBlock>,
    pub(crate) function_name_by_id: HashMap<FunctionId, String>,
    pub(crate) local_name_by_id: HashMap<LocalId, String>,
    pub(crate) field_name_by_id: HashMap<FieldId, String>,
    pub(crate) current_function: Option<FunctionId>,
    pub(crate) used_identifiers: HashSet<String>,
    pub(crate) temp_counter: usize,
    /// Set of external function IDs referenced during lowering.
    /// Used to conditionally emit runtime helpers.
    pub(crate) referenced_external_functions:
        HashSet<crate::compiler_frontend::external_packages::ExternalFunctionId>,
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
            external_package_registry: config.external_package_registry.clone(),
            config,
            out: String::new(),
            indent: 0,
            blocks_by_id,
            function_name_by_id: HashMap::new(),
            local_name_by_id: HashMap::new(),
            field_name_by_id: HashMap::new(),
            current_function: None,
            used_identifiers: HashSet::new(),
            temp_counter: 0,
            referenced_external_functions: HashSet::new(),
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

        self.emit_runtime_math_helpers();

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

            self.emit_line(&format!("{start_name}();"));
        }

        Ok(JsModule {
            source: self.out.clone(),
            function_name_by_id: self.function_name_by_id.clone(),
        })
    }
}
