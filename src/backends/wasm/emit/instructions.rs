//! Instruction lowering from Wasm LIR to wasm-encoder instructions.

use crate::backends::wasm::emit::sections::WasmEmitPlan;
use crate::backends::wasm::lir::instructions::{WasmCalleeRef, WasmLirStmt, WasmLirTerminator};
use crate::backends::wasm::lir::types::{
    WasmAbiType, WasmLirBlockId, WasmLirFunctionId, WasmLirLocalId,
};
use crate::backends::wasm::runtime::strings::WasmRuntimeHelper;
use crate::compiler_frontend::compiler_messages::compiler_errors::{CompilerError, ErrorType};
use rustc_hash::FxHashMap;
use wasm_encoder::{BlockType, Function, Instruction};

pub(crate) struct LirBodyEmitContext<'a> {
    pub function_id: WasmLirFunctionId,
    pub local_index_by_id: &'a FxHashMap<WasmLirLocalId, u32>,
    pub local_type_by_id: &'a FxHashMap<WasmLirLocalId, WasmAbiType>,
    pub block_index_by_id: &'a FxHashMap<WasmLirBlockId, u32>,
    pub dispatch_local_index: u32,
}

pub(crate) fn emit_statement(
    function: &mut Function,
    statement: &WasmLirStmt,
    context: &LirBodyEmitContext<'_>,
    plan: &WasmEmitPlan,
) -> Result<(), CompilerError> {
    // WHAT: lower each LIR statement into explicit Wasm stack-machine instructions.
    // WHY: statement lowering is the only place that maps semantic LIR ops to concrete opcodes.
    match statement {
        WasmLirStmt::ConstI32 { dst, value } => {
            function.instruction(&Instruction::I32Const(*value));
            function.instruction(&Instruction::LocalSet(local_index(*dst, context)?));
        }
        WasmLirStmt::ConstI64 { dst, value } => {
            function.instruction(&Instruction::I64Const(*value));
            function.instruction(&Instruction::LocalSet(local_index(*dst, context)?));
        }
        WasmLirStmt::ConstF32 { dst, value } => {
            function.instruction(&Instruction::F32Const((*value).into()));
            function.instruction(&Instruction::LocalSet(local_index(*dst, context)?));
        }
        WasmLirStmt::ConstF64 { dst, value } => {
            function.instruction(&Instruction::F64Const((*value).into()));
            function.instruction(&Instruction::LocalSet(local_index(*dst, context)?));
        }
        WasmLirStmt::ConstStaticPtr { dst, data } => {
            let offset = plan.data_offsets.get(data).copied().ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "Wasm emission missing offset for static data {:?} in function {:?}",
                    data, context.function_id
                ))
                .with_error_type(ErrorType::WasmGeneration)
            })?;
            function.instruction(&Instruction::I32Const(offset as i32));
            function.instruction(&Instruction::LocalSet(local_index(*dst, context)?));
        }
        WasmLirStmt::ConstLength { dst, value } => {
            if *value > i32::MAX as u32 {
                return Err(CompilerError::compiler_error(format!(
                    "Wasm emission cannot lower length {} > i32::MAX in function {:?}",
                    value, context.function_id
                ))
                .with_error_type(ErrorType::WasmGeneration));
            }
            function.instruction(&Instruction::I32Const(*value as i32));
            function.instruction(&Instruction::LocalSet(local_index(*dst, context)?));
        }
        WasmLirStmt::Copy { dst, src } | WasmLirStmt::Move { dst, src } => {
            // WHAT: phase-2 models copy/move as local assignment in emitted Wasm.
            // WHY: ownership specialization remains in runtime/helper behavior for now.
            function.instruction(&Instruction::LocalGet(local_index(*src, context)?));
            function.instruction(&Instruction::LocalSet(local_index(*dst, context)?));
        }
        WasmLirStmt::Call { dst, callee, args } => {
            for arg in args {
                function.instruction(&Instruction::LocalGet(local_index(*arg, context)?));
            }

            let callee_index = match callee {
                WasmCalleeRef::Function(function_id) => plan
                    .function_indices
                    .get(function_id)
                    .copied()
                    .ok_or_else(|| {
                        CompilerError::compiler_error(format!(
                            "Wasm emission missing callee function index for {:?} in {:?}",
                            function_id, context.function_id
                        ))
                        .with_error_type(ErrorType::WasmGeneration)
                    })?,
                WasmCalleeRef::Import(import_id) => plan
                    .import_function_indices
                    .get(import_id)
                    .copied()
                    .ok_or_else(|| {
                        CompilerError::compiler_error(format!(
                            "Wasm emission missing import function index for {:?} in {:?}",
                            import_id, context.function_id
                        ))
                        .with_error_type(ErrorType::WasmGeneration)
                    })?,
            };
            function.instruction(&Instruction::Call(callee_index));

            if let Some(dst) = dst {
                function.instruction(&Instruction::LocalSet(local_index(*dst, context)?));
            }
        }
        WasmLirStmt::StringNewBuffer { dst } => {
            function.instruction(&Instruction::Call(helper_index(
                plan,
                WasmRuntimeHelper::StringNewBuffer,
            )?));
            function.instruction(&Instruction::LocalSet(local_index(*dst, context)?));
        }
        WasmLirStmt::StringPushLiteral { buffer, data } => {
            let ptr = plan.data_offsets.get(data).copied().ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "Wasm emission missing static pointer for {:?} in {:?}",
                    data, context.function_id
                ))
                .with_error_type(ErrorType::WasmGeneration)
            })?;
            let len = plan.data_lengths.get(data).copied().ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "Wasm emission missing static length for {:?} in {:?}",
                    data, context.function_id
                ))
                .with_error_type(ErrorType::WasmGeneration)
            })?;
            function.instruction(&Instruction::LocalGet(local_index(*buffer, context)?));
            function.instruction(&Instruction::I32Const(ptr as i32));
            function.instruction(&Instruction::I32Const(len as i32));
            function.instruction(&Instruction::Call(helper_index(
                plan,
                WasmRuntimeHelper::StringPushLiteral,
            )?));
        }
        WasmLirStmt::StringPushHandle { buffer, handle } => {
            function.instruction(&Instruction::LocalGet(local_index(*buffer, context)?));
            function.instruction(&Instruction::LocalGet(local_index(*handle, context)?));
            function.instruction(&Instruction::Call(helper_index(
                plan,
                WasmRuntimeHelper::StringPushHandle,
            )?));
        }
        WasmLirStmt::StringFinish { dst, buffer } => {
            function.instruction(&Instruction::LocalGet(local_index(*buffer, context)?));
            function.instruction(&Instruction::Call(helper_index(
                plan,
                WasmRuntimeHelper::StringFinish,
            )?));
            function.instruction(&Instruction::LocalSet(local_index(*dst, context)?));
        }
        WasmLirStmt::DropIfOwned { value } => {
            function.instruction(&Instruction::LocalGet(local_index(*value, context)?));
            function.instruction(&Instruction::Call(helper_index(
                plan,
                WasmRuntimeHelper::DropIfOwned,
            )?));
        }
        WasmLirStmt::RetainHandle { .. } => {
            // WHAT: retain is currently a no-op at codegen level.
            // WHY: GC-first semantics and conservative helper runtime make extra retain
            // unnecessary in phase-2.
        }
        WasmLirStmt::IntEq { dst, lhs, rhs } => {
            emit_compare(function, *lhs, *rhs, context, true)?;
            function.instruction(&Instruction::LocalSet(local_index(*dst, context)?));
        }
        WasmLirStmt::IntNe { dst, lhs, rhs } => {
            emit_compare(function, *lhs, *rhs, context, false)?;
            function.instruction(&Instruction::LocalSet(local_index(*dst, context)?));
        }
    }

    Ok(())
}

pub(crate) fn emit_terminator(
    function: &mut Function,
    terminator: &WasmLirTerminator,
    context: &LirBodyEmitContext<'_>,
) -> Result<(), CompilerError> {
    match terminator {
        WasmLirTerminator::Jump(target) => {
            set_dispatch_target(function, *target, context)?;
            // Branch depth 1 exits the current `if` and restarts the dispatcher loop.
            function.instruction(&Instruction::Br(1));
        }
        WasmLirTerminator::Branch {
            condition,
            then_block,
            else_block,
        } => {
            function.instruction(&Instruction::LocalGet(local_index(*condition, context)?));
            function.instruction(&Instruction::If(BlockType::Empty));
            set_dispatch_target(function, *then_block, context)?;
            function.instruction(&Instruction::Else);
            set_dispatch_target(function, *else_block, context)?;
            function.instruction(&Instruction::End);
            // Re-enter the loop with the newly selected target block.
            function.instruction(&Instruction::Br(1));
        }
        WasmLirTerminator::Return { value } => {
            if let Some(value) = value {
                function.instruction(&Instruction::LocalGet(local_index(*value, context)?));
            }
            function.instruction(&Instruction::Return);
        }
        WasmLirTerminator::Trap => {
            function.instruction(&Instruction::Unreachable);
        }
    }

    Ok(())
}

fn set_dispatch_target(
    function: &mut Function,
    target: WasmLirBlockId,
    context: &LirBodyEmitContext<'_>,
) -> Result<(), CompilerError> {
    // WHAT: convert target block id into dispatcher-table index.
    // WHY: branch/jump terminators communicate control flow via dispatch-local updates.
    let target_index = context
        .block_index_by_id
        .get(&target)
        .copied()
        .ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "Wasm emission missing block index for target {:?} in function {:?}",
                target, context.function_id
            ))
            .with_error_type(ErrorType::WasmGeneration)
        })?;

    function.instruction(&Instruction::I32Const(target_index as i32));
    function.instruction(&Instruction::LocalSet(context.dispatch_local_index));
    Ok(())
}

fn emit_compare(
    function: &mut Function,
    lhs: WasmLirLocalId,
    rhs: WasmLirLocalId,
    context: &LirBodyEmitContext<'_>,
    is_eq: bool,
) -> Result<(), CompilerError> {
    function.instruction(&Instruction::LocalGet(local_index(lhs, context)?));
    function.instruction(&Instruction::LocalGet(local_index(rhs, context)?));

    let lhs_type = context.local_type_by_id.get(&lhs).copied().ok_or_else(|| {
        CompilerError::compiler_error(format!(
            "Wasm emission missing local type for lhs {:?} in function {:?}",
            lhs, context.function_id
        ))
        .with_error_type(ErrorType::WasmGeneration)
    })?;
    let rhs_type = context.local_type_by_id.get(&rhs).copied().ok_or_else(|| {
        CompilerError::compiler_error(format!(
            "Wasm emission missing local type for rhs {:?} in function {:?}",
            rhs, context.function_id
        ))
        .with_error_type(ErrorType::WasmGeneration)
    })?;
    if lhs_type != rhs_type {
        return Err(CompilerError::compiler_error(format!(
            "Wasm emission type mismatch in comparison: lhs {:?} is {:?}, rhs {:?} is {:?} in {:?}",
            lhs, lhs_type, rhs, rhs_type, context.function_id
        ))
        .with_error_type(ErrorType::WasmGeneration));
    }

    match lhs_type {
        WasmAbiType::I64 => {
            function.instruction(if is_eq {
                &Instruction::I64Eq
            } else {
                &Instruction::I64Ne
            });
        }
        WasmAbiType::F32 => {
            function.instruction(if is_eq {
                &Instruction::F32Eq
            } else {
                &Instruction::F32Ne
            });
        }
        WasmAbiType::F64 => {
            function.instruction(if is_eq {
                &Instruction::F64Eq
            } else {
                &Instruction::F64Ne
            });
        }
        WasmAbiType::Void => {
            return Err(CompilerError::compiler_error(format!(
                "Wasm emission cannot compare Void-typed locals in function {:?}",
                context.function_id
            ))
            .with_error_type(ErrorType::WasmGeneration));
        }
        WasmAbiType::I32 | WasmAbiType::Handle => {
            function.instruction(if is_eq {
                &Instruction::I32Eq
            } else {
                &Instruction::I32Ne
            });
        }
    }

    Ok(())
}

fn local_index(
    local_id: WasmLirLocalId,
    context: &LirBodyEmitContext<'_>,
) -> Result<u32, CompilerError> {
    // WHAT: resolve LIR local ids to final Wasm local indices.
    // WHY: keeping this lookup centralized guarantees consistent error diagnostics.
    context
        .local_index_by_id
        .get(&local_id)
        .copied()
        .ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "Wasm emission missing local index for {:?} in function {:?}",
                local_id, context.function_id
            ))
            .with_error_type(ErrorType::WasmGeneration)
        })
}

fn helper_index(plan: &WasmEmitPlan, helper: WasmRuntimeHelper) -> Result<u32, CompilerError> {
    // WHAT: resolve synthesized helper to its planned function index.
    // WHY: helper calls are encoded as direct calls and must match plan indices exactly.
    plan.helper_indices.get(&helper).copied().ok_or_else(|| {
        CompilerError::compiler_error(format!(
            "Wasm emission missing helper function index for {}",
            crate::backends::wasm::emit::sections::helper_name(helper)
        ))
        .with_error_type(ErrorType::WasmGeneration)
    })
}
