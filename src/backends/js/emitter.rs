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
use crate::compiler_frontend::hir::expressions::{HirExpression, HirExpressionKind};
use crate::compiler_frontend::hir::functions::HirFunction;
use crate::compiler_frontend::hir::ids::{BlockId, FieldId, FunctionId, LocalId};
use crate::compiler_frontend::hir::module::HirModule;
use crate::compiler_frontend::hir::patterns::HirPattern;
use crate::compiler_frontend::hir::reachability::{
    HirReachabilityInput, collect_hir_reachability, collect_reachability_from_start,
};
use crate::compiler_frontend::hir::statements::HirStatementKind;
use crate::compiler_frontend::hir::terminators::HirTerminator;
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

        let functions = self.functions_to_emit()?;
        let emitted_code_uses_maps = self.emitted_functions_use_maps(&functions)?;
        self.emit_runtime_prelude(emitted_code_uses_maps);

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

    /// Returns true when any emitted reachable JS body can construct, store, copy, display, or
    /// operate on a Beanstalk map.
    ///
    /// WHAT: scans the same function/block subset that JS lowering will emit.
    /// WHY: map helpers are new runtime surface. Emitting them only for map-using programs avoids
    /// changing existing golden artifacts while still making map copy/string fallback paths safe.
    fn emitted_functions_use_maps(
        &self,
        functions: &[&'hir HirFunction],
    ) -> Result<bool, CompilerError> {
        for function in functions {
            if self.type_environment.is_map_type(function.return_type) {
                return Ok(true);
            }

            let reachable_blocks = self.collect_reachable_blocks(function.entry)?;
            for block_id in reachable_blocks {
                let block = self.block_by_id(block_id)?;
                for local in &block.locals {
                    if self.type_environment.is_map_type(local.ty) {
                        return Ok(true);
                    }
                }

                for statement in &block.statements {
                    if self.statement_uses_maps(&statement.kind) {
                        return Ok(true);
                    }
                }

                if self.terminator_uses_maps(&block.terminator) {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    fn statement_uses_maps(&self, statement: &HirStatementKind) -> bool {
        match statement {
            HirStatementKind::Assign { target: _, value }
            | HirStatementKind::Expr(value)
            | HirStatementKind::PushRuntimeFragment { value, .. } => {
                self.expression_uses_maps(value)
            }

            HirStatementKind::Call { args, .. } => args
                .iter()
                .any(|argument| self.expression_uses_maps(argument)),

            HirStatementKind::CallDynamicTraitMethod { receiver, args, .. } => {
                self.expression_uses_maps(receiver)
                    || args
                        .iter()
                        .any(|argument| self.expression_uses_maps(&argument.value))
            }

            HirStatementKind::MapOp { .. } => true,

            HirStatementKind::Drop(_) => false,
        }
    }

    fn terminator_uses_maps(&self, terminator: &HirTerminator) -> bool {
        match terminator {
            HirTerminator::If { condition, .. } => self.expression_uses_maps(condition),

            HirTerminator::FallibleBranch { result, .. } => self.expression_uses_maps(result),

            HirTerminator::Match { scrutinee, arms } => {
                self.expression_uses_maps(scrutinee)
                    || arms.iter().any(|arm| {
                        arm.guard
                            .as_ref()
                            .is_some_and(|guard| self.expression_uses_maps(guard))
                            || self.pattern_uses_maps(&arm.pattern)
                    })
            }

            HirTerminator::Return(value)
            | HirTerminator::ReturnSuccess(value)
            | HirTerminator::ReturnError(value) => self.expression_uses_maps(value),

            HirTerminator::Jump { .. }
            | HirTerminator::Break { .. }
            | HirTerminator::Continue { .. }
            | HirTerminator::Uninitialized
            | HirTerminator::RuntimeFailure { .. }
            | HirTerminator::AssertFailure { .. } => false,
        }
    }

    fn pattern_uses_maps(&self, pattern: &HirPattern) -> bool {
        match pattern {
            HirPattern::Literal(value)
            | HirPattern::OptionValue { value }
            | HirPattern::OptionRelational { value, .. }
            | HirPattern::Relational { value, .. } => self.expression_uses_maps(value),

            HirPattern::OptionNone
            | HirPattern::OptionPresent
            | HirPattern::Wildcard
            | HirPattern::ChoiceVariant { .. }
            | HirPattern::Capture => false,
        }
    }

    fn expression_uses_maps(&self, expression: &HirExpression) -> bool {
        if self.type_environment.is_map_type(expression.ty) {
            return true;
        }

        match &expression.kind {
            HirExpressionKind::Load(_) | HirExpressionKind::Copy(_) => false,

            HirExpressionKind::BinOp { left, right, .. } => {
                self.expression_uses_maps(left) || self.expression_uses_maps(right)
            }

            HirExpressionKind::UnaryOp { operand, .. } => self.expression_uses_maps(operand),

            HirExpressionKind::StructConstruct { fields, .. } => fields
                .iter()
                .any(|(_, field_value)| self.expression_uses_maps(field_value)),

            HirExpressionKind::Collection(elements)
            | HirExpressionKind::TupleConstruct { elements } => elements
                .iter()
                .any(|element| self.expression_uses_maps(element)),

            HirExpressionKind::MapLiteral(_) => true,

            HirExpressionKind::Range { start, end } => {
                self.expression_uses_maps(start) || self.expression_uses_maps(end)
            }

            HirExpressionKind::TupleGet { tuple, .. } => self.expression_uses_maps(tuple),

            HirExpressionKind::FallibleUnwrapSuccess { result }
            | HirExpressionKind::FallibleUnwrapError { result } => {
                self.expression_uses_maps(result)
            }

            HirExpressionKind::BuiltinCast { value, .. }
            | HirExpressionKind::ConstructDynamicTraitValue { value, .. } => {
                self.expression_uses_maps(value)
            }

            HirExpressionKind::VariantConstruct { fields, .. } => fields
                .iter()
                .any(|field| self.expression_uses_maps(&field.value)),

            HirExpressionKind::VariantPayloadGet { source, .. } => {
                self.expression_uses_maps(source)
            }

            HirExpressionKind::Int(_)
            | HirExpressionKind::Float(_)
            | HirExpressionKind::Bool(_)
            | HirExpressionKind::Char(_)
            | HirExpressionKind::StringLiteral(_) => false,
        }
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
