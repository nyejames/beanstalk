//! Expression lowering for the interpreter backend.
//!
//! WHAT: lowers a restricted subset of HIR expressions into Exec IR instructions.
//! WHY: phase 1 needs a tiny executable core before broader language support is added.

use crate::backends::rust_interpreter::exec_ir::{ExecConstValue, ExecInstruction, ExecLocalId};
use crate::backends::rust_interpreter::lowering::context::{
    FunctionLoweringLayout, LoweringContext,
};
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{HirExpression, HirExpressionKind, HirPlace};

pub(crate) fn lower_expression_into(
    context: &mut LoweringContext<'_>,
    layout: &FunctionLoweringLayout,
    instructions: &mut Vec<ExecInstruction>,
    target: ExecLocalId,
    expression: &HirExpression,
) -> Result<(), CompilerError> {
    match &expression.kind {
        HirExpressionKind::Int(value) => {
            let const_id = context.intern_const(ExecConstValue::Int(*value));
            instructions.push(ExecInstruction::LoadConst { target, const_id });
            Ok(())
        }

        HirExpressionKind::Float(value) => {
            let const_id = context.intern_const(ExecConstValue::Float(*value));
            instructions.push(ExecInstruction::LoadConst { target, const_id });
            Ok(())
        }

        HirExpressionKind::Bool(value) => {
            let const_id = context.intern_const(ExecConstValue::Bool(*value));
            instructions.push(ExecInstruction::LoadConst { target, const_id });
            Ok(())
        }

        HirExpressionKind::Char(value) => {
            let const_id = context.intern_const(ExecConstValue::Char(*value));
            instructions.push(ExecInstruction::LoadConst { target, const_id });
            Ok(())
        }

        HirExpressionKind::StringLiteral(text) => {
            let const_id = context.intern_const(ExecConstValue::String(text.to_owned()));
            instructions.push(ExecInstruction::LoadConst { target, const_id });
            Ok(())
        }

        HirExpressionKind::Load(HirPlace::Local(local_id)) => {
            let Some(source) = layout.exec_local_by_hir_local.get(local_id).copied() else {
                return Err(CompilerError::compiler_error(format!(
                    "Rust interpreter lowering could not resolve local {:?} for load expression",
                    local_id
                )));
            };

            instructions.push(ExecInstruction::ReadLocal { target, source });
            Ok(())
        }

        HirExpressionKind::Copy(HirPlace::Local(local_id)) => {
            let Some(source) = layout.exec_local_by_hir_local.get(local_id).copied() else {
                return Err(CompilerError::compiler_error(format!(
                    "Rust interpreter lowering could not resolve local {:?} for copy expression",
                    local_id
                )));
            };

            instructions.push(ExecInstruction::CopyLocal { target, source });
            Ok(())
        }

        HirExpressionKind::Load(place) => Err(CompilerError::compiler_error(format!(
            "Rust interpreter lowering does not support non-local load places yet: {place:?}"
        ))),

        HirExpressionKind::Copy(place) => Err(CompilerError::compiler_error(format!(
            "Rust interpreter lowering does not support non-local copy places yet: {place:?}"
        ))),

        HirExpressionKind::TupleConstruct { elements } if elements.is_empty() => {
            let const_id = context.intern_const(ExecConstValue::Unit);
            instructions.push(ExecInstruction::LoadConst { target, const_id });
            Ok(())
        }

        _ => Err(CompilerError::compiler_error(format!(
            "Rust interpreter lowering does not support HIR expression kind yet: {:?}",
            expression.kind
        ))),
    }
}
