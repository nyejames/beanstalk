//! Function Call Lowering
//!
//! This module handles lowering function calls, method calls, and host function calls
//! to LIR instructions, including ownership tagging for arguments.

use crate::compiler::compiler_messages::compiler_errors::CompilerError;
use crate::compiler::datatypes::DataType;
use crate::compiler::hir::nodes::{HirExpr, HirExprKind, HirPlace};
use crate::compiler::host_functions::registry::{CallTarget, HostFunctionId};
use crate::compiler::lir::nodes::{LirInst, LirType};
use crate::compiler::lir::types::datatype_to_lir_type;
use crate::compiler::string_interning::{InternedString, StringId};

use super::context::LoweringContext;
use super::types::hir_expr_to_lir_type;

impl LoweringContext {
    // ========================================================================
    // Regular Function Calls
    // ========================================================================

    /// Lowers a regular function call to LIR instructions.
    pub fn lower_function_call(
        &mut self,
        name: StringId,
        args: &[HirExpr],
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Lower each argument with ownership tagging
        for arg in args {
            insts.extend(self.lower_argument_with_ownership(arg)?);
        }

        // Look up the function index
        let func_idx = self.get_function_index(name).ok_or_else(|| {
            CompilerError::lir_transformation(format!("Unknown function: {}", name))
        })?;

        // Emit call instruction
        insts.push(LirInst::Call(func_idx));

        Ok(insts)
    }

    /// Lowers a function call expression (used when call result is needed).
    pub fn lower_call_expr(
        &mut self,
        target: &CallTarget,
        args: &[HirExpr],
    ) -> Result<Vec<LirInst>, CompilerError> {
        match target {
            CallTarget::UserFunction(name) => {
                self.lower_function_call(*name, args)
            }
            CallTarget::HostFunction(id) => {
                self.lower_host_call(*id, args)
            }
        }
    }

    /// Lowers a method call expression.
    pub fn lower_method_call(
        &mut self,
        receiver: &HirExpr,
        method: InternedString,
        args: &[HirExpr],
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Lower the receiver as the first argument
        insts.extend(self.lower_argument_with_ownership(receiver)?);

        // Lower remaining arguments
        for arg in args {
            insts.extend(self.lower_argument_with_ownership(arg)?);
        }

        // Look up method as a function
        let func_idx = self.get_function_index(method).ok_or_else(|| {
            CompilerError::lir_transformation(format!("Unknown method: {}", method))
        })?;

        // Emit call instruction
        insts.push(LirInst::Call(func_idx));

        Ok(insts)
    }

    /// Lowers an argument expression with ownership tagging.
    pub fn lower_argument_with_ownership(
        &mut self,
        arg: &HirExpr,
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        let needs_ownership_tagging = self.is_heap_allocated_expr(arg);

        if needs_ownership_tagging {
            match &arg.kind {
                HirExprKind::Load(HirPlace::Var(var_name)) => {
                    let local_idx = self.var_to_local.get(var_name).ok_or_else(|| {
                        CompilerError::lir_transformation(format!(
                            "Undefined variable in function argument: {}",
                            var_name
                        ))
                    })?;

                    if self.is_last_use(*var_name) {
                        insts.push(LirInst::PrepareOwnedArg(*local_idx));
                    } else {
                        insts.push(LirInst::PrepareBorrowedArg(*local_idx));
                    }
                }
                HirExprKind::Move(HirPlace::Var(var_name)) => {
                    let local_idx = self.var_to_local.get(var_name).ok_or_else(|| {
                        CompilerError::lir_transformation(format!(
                            "Undefined variable in move expression: {}",
                            var_name
                        ))
                    })?;
                    insts.push(LirInst::PrepareOwnedArg(*local_idx));
                }
                _ => {
                    // Complex expression - lower it and tag the result
                    insts.extend(self.lower_expr(arg)?);

                    let lir_type = hir_expr_to_lir_type(arg);
                    let temp_local = self.local_allocator.allocate(lir_type);
                    insts.push(LirInst::LocalTee(temp_local));
                    insts.push(LirInst::TagAsOwned(temp_local));
                    insts.push(LirInst::LocalGet(temp_local));

                    self.local_allocator.free(temp_local);
                }
            }
        } else {
            // Stack-allocated types - just lower the expression
            insts.extend(self.lower_expr(arg)?);
        }

        Ok(insts)
    }

    // ========================================================================
    // Host Function Calls
    // ========================================================================

    /// Lowers a host function call to LIR instructions.
    pub fn lower_host_call(
        &mut self,
        id: HostFunctionId,
        args: &[HirExpr],
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Lower each argument with ownership tagging
        for arg in args {
            insts.extend(self.lower_argument_with_ownership(arg)?);
        }

        // Get or register the host function index
        let host_func_idx = self.register_host_function(id);

        // Host functions use offset 0x10000 to distinguish from regular functions
        let import_call_idx = 0x10000 + host_func_idx;
        insts.push(LirInst::Call(import_call_idx));

        Ok(insts)
    }

    // ========================================================================
    // Function Parameter Handling
    // ========================================================================

    /// Lowers function parameters to WASM function parameters.
    pub fn lower_function_parameters(
        &mut self,
        params: &[(InternedString, DataType)],
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        for (param_idx, (param_name, param_type)) in params.iter().enumerate() {
            let param_local = param_idx as u32;

            if self.is_heap_allocated_type(param_type) {
                let lir_type = datatype_to_lir_type(param_type);
                let real_ptr_local = self.local_allocator.allocate(lir_type);

                insts.push(LirInst::HandleOwnedParam {
                    param_local,
                    real_ptr_local,
                });

                self.var_to_local.insert(*param_name, real_ptr_local);
            } else {
                self.var_to_local.insert(*param_name, param_local);
            }
        }

        Ok(insts)
    }

    /// Converts function signature parameters to LIR types.
    pub fn params_to_lir_types(&self, params: &[(InternedString, DataType)]) -> Vec<LirType> {
        params
            .iter()
            .map(|(_, data_type)| datatype_to_lir_type(data_type))
            .collect()
    }

    /// Converts function return types to LIR types.
    pub fn returns_to_lir_types(&self, returns: &[DataType]) -> Vec<LirType> {
        returns.iter().map(datatype_to_lir_type).collect()
    }
}
