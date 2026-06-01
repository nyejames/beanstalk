//! JS emitter — lowers HIR into executable JavaScript.
//!
//! WHAT: converts HIR control flow/expressions into executable JS and symbol maps.
//! WHY: JS is the stable near-term backend and needs deterministic lowering output.

use crate::backends::js::JsModule;
use crate::backends::js::{JsFunctionEmissionPolicy, JsLoweringConfig};
use crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::hir::blocks::HirBlock;
use crate::compiler_frontend::hir::functions::HirFunction;
use crate::compiler_frontend::hir::ids::{BlockId, FieldId, FunctionId, LocalId};
use crate::compiler_frontend::hir::module::HirModule;
use crate::compiler_frontend::hir::reachability::{
    HirReachabilityInput, collect_hir_reachability, collect_reachability_from_start,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::traits::ids::TraitEvidenceId;
use std::collections::{BTreeSet, HashMap, HashSet};

/// Lower one validated HIR module into JavaScript source.
pub fn lower_hir_to_js(
    hir: &HirModule,
    borrow_analysis: &BorrowCheckReport,
    string_table: &StringTable,
    config: JsLoweringConfig,
    type_environment: &TypeEnvironment,
) -> Result<JsModule, CompilerError> {
    let mut emitter = JsEmitter::new(hir, borrow_analysis, string_table, config, type_environment);
    emitter.lower_module()
}

pub(crate) struct JsEmitter<'hir> {
    pub(crate) hir: &'hir HirModule,
    pub(crate) borrow_analysis: &'hir BorrowCheckReport,
    pub(crate) string_table: &'hir StringTable,
    pub(crate) config: JsLoweringConfig,
    pub(crate) type_environment: &'hir TypeEnvironment,
    pub(crate) out: String,
    pub(crate) indent: usize,
    pub(crate) blocks_by_id: HashMap<BlockId, &'hir HirBlock>,
    pub(crate) function_name_by_id: HashMap<FunctionId, String>,
    pub(crate) local_name_by_id: HashMap<LocalId, String>,
    pub(crate) field_name_by_id: HashMap<FieldId, String>,
    pub(crate) current_function: Option<FunctionId>,
    pub(crate) used_identifiers: HashSet<String>,
    pub(crate) temp_counter: usize,
    /// Set of external function IDs referenced while lowering emitted JS functions.
    /// Used to conditionally emit runtime helpers.
    pub(crate) referenced_external_functions:
        HashSet<crate::compiler_frontend::external_packages::ExternalFunctionId>,
    /// Whether choice equality was lowered, requiring the runtime helper.
    pub(crate) used_choice_equality: bool,
    /// Evidence tables referenced by emitted dynamic trait wrapper construction.
    pub(crate) used_dynamic_trait_tables: BTreeSet<TraitEvidenceId>,
    /// Whether emitted code constructs dynamic trait wrappers.
    pub(crate) used_dynamic_trait_constructor: bool,
    /// Whether emitted code dispatches through a dynamic trait wrapper.
    pub(crate) used_dynamic_trait_dispatch: bool,
}

impl<'hir> JsEmitter<'hir> {
    pub(crate) fn new(
        hir: &'hir HirModule,
        borrow_analysis: &'hir BorrowCheckReport,
        string_table: &'hir StringTable,
        config: JsLoweringConfig,
        type_environment: &'hir TypeEnvironment,
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
            type_environment,
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
            used_choice_equality: false,
            used_dynamic_trait_tables: BTreeSet::new(),
            used_dynamic_trait_constructor: false,
            used_dynamic_trait_dispatch: false,
        }
    }

    fn lower_module(&mut self) -> Result<JsModule, CompilerError> {
        self.build_symbol_maps();
        self.emit_runtime_prelude();

        let functions = self.functions_to_emit()?;

        for (index, function) in functions.into_iter().enumerate() {
            if index > 0 {
                self.emit_line("");
            }

            self.emit_function(function)?;
        }

        self.emit_core_library_helpers();

        if self.used_choice_equality {
            self.emit_runtime_choice_helpers();
        }

        self.emit_dynamic_trait_runtime()?;

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
            referenced_external_functions: self.referenced_external_functions.clone(),
        })
    }

    fn functions_to_emit(&self) -> Result<Vec<&'hir HirFunction>, CompilerError> {
        let reachable_functions = match self.config.function_emission_policy {
            JsFunctionEmissionPolicy::AllFunctions => None,
            JsFunctionEmissionPolicy::ReachableFromStart => {
                Some(self.collect_js_reachable_functions()?)
            }
        };

        let mut functions = self
            .hir
            .functions
            .iter()
            .filter(|function| {
                let Some(reachable) = reachable_functions.as_ref() else {
                    return true;
                };

                reachable.contains(&function.id)
            })
            .collect::<Vec<_>>();
        functions.sort_by_key(|function| function.id.0);

        Ok(functions)
    }

    fn collect_js_reachable_functions(
        &self,
    ) -> Result<rustc_hash::FxHashSet<FunctionId>, CompilerError> {
        let mut reachability = collect_reachability_from_start(self.hir)?;

        loop {
            let dynamic_method_roots = self.dynamic_trait_method_roots_for_operations(
                &reachability.reachable_dynamic_trait_operations,
            )?;
            let mut roots = reachability
                .reachable_functions
                .iter()
                .copied()
                .collect::<Vec<_>>();
            let original_function_count = roots.len();

            for function_id in dynamic_method_roots {
                if !reachability.reachable_functions.contains(&function_id) {
                    roots.push(function_id);
                }
            }

            if roots.len() == original_function_count {
                return Ok(reachability.reachable_functions);
            }

            roots.sort_by_key(|function_id| function_id.0);
            roots.dedup_by_key(|function_id| function_id.0);

            reachability = collect_hir_reachability(HirReachabilityInput {
                hir: self.hir,
                root_functions: roots,
            })?;
        }
    }
}
