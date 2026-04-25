//! Expression value materialization helpers for interpreter lowering.
//!
//! WHAT: turns lowered expression values into Exec IR local writes.
//! WHY: expression shape lowering must preserve Load vs Copy semantics without duplicating
//! materialization rules across statements, terminators, and operator operands.

use crate::backends::rust_interpreter::exec_ir::{
    ExecConstValue, ExecInstruction, ExecLocalId, ExecStorageType,
};
use crate::backends::rust_interpreter::lowering::context::{
    FunctionLoweringLayout, LoweringContext,
};
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;

#[derive(Debug, Clone)]
pub(crate) enum LoweredExpressionValue {
    Literal(ExecConstValue),
    LocalRead(ExecLocalId),
    LocalCopy(ExecLocalId),
}

pub(crate) fn materialize_expression_value(
    context: &mut LoweringContext<'_>,
    instructions: &mut Vec<ExecInstruction>,
    target: ExecLocalId,
    value: LoweredExpressionValue,
) -> Result<(), CompilerError> {
    match value {
        LoweredExpressionValue::Literal(const_value) => {
            let const_id = context.intern_const(const_value);
            instructions.push(ExecInstruction::LoadConst { target, const_id });
        }
        LoweredExpressionValue::LocalRead(source) => {
            instructions.push(ExecInstruction::ReadLocal { target, source });
        }
        LoweredExpressionValue::LocalCopy(source) => {
            instructions.push(ExecInstruction::CopyLocal { target, source });
        }
    }

    Ok(())
}

pub(crate) fn lower_expression_to_temporary(
    context: &mut LoweringContext<'_>,
    layout: &mut FunctionLoweringLayout,
    instructions: &mut Vec<ExecInstruction>,
    value: LoweredExpressionValue,
    storage_type: ExecStorageType,
) -> Result<ExecLocalId, CompilerError> {
    let storage_type = match &value {
        LoweredExpressionValue::Literal(const_value) => storage_type_for_const(const_value),
        _ => storage_type,
    };
    let temporary_local = layout.allocate_temporary_local(storage_type);
    materialize_expression_value(context, instructions, temporary_local, value)?;
    Ok(temporary_local)
}

pub(crate) fn local_for_expression_value(
    context: &mut LoweringContext<'_>,
    layout: &mut FunctionLoweringLayout,
    instructions: &mut Vec<ExecInstruction>,
    value: LoweredExpressionValue,
    storage_type: ExecStorageType,
) -> Result<ExecLocalId, CompilerError> {
    match value {
        LoweredExpressionValue::LocalRead(local_id) => Ok(local_id),
        other => lower_expression_to_temporary(context, layout, instructions, other, storage_type),
    }
}

pub(crate) fn storage_type_for_const(value: &ExecConstValue) -> ExecStorageType {
    match value {
        ExecConstValue::Unit => ExecStorageType::Unit,
        ExecConstValue::Bool(_) => ExecStorageType::Bool,
        ExecConstValue::Int(_) => ExecStorageType::Int,
        ExecConstValue::Float(_) => ExecStorageType::Float,
        ExecConstValue::Char(_) => ExecStorageType::Char,
        ExecConstValue::String(_) => ExecStorageType::HeapHandle,
    }
}
