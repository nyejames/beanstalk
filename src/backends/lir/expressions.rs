//! Expression Lowering
//!
//! This module handles lowering HIR expressions to LIR instructions,
//! including literals, binary operations, and unary operations.

use crate::backends::lir::nodes::{LirInst, LirType};
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{BinOp, HirExpr, HirExprKind, HirPlace, UnaryOp};

use super::context::LoweringContext;
use super::types::hir_expr_to_lir_type;

impl LoweringContext {
    /// Lowers a HIR expression to a sequence of LIR instructions.
    pub fn lower_expr(&mut self, expr: &HirExpr) -> Result<Vec<LirInst>, CompilerError> {
        match &expr.kind {
            // Literals
            HirExprKind::Int(val) => self.lower_int_literal(*val),
            HirExprKind::Float(val) => self.lower_float_literal(*val),
            HirExprKind::Bool(val) => self.lower_bool_literal(*val),
            HirExprKind::Char(val) => self.lower_char_literal(*val),

            // Variable access
            HirExprKind::Load(place) => self.lower_place_load(place),

            // Binary operations
            HirExprKind::BinOp { left, op, right } => self.lower_binary_op(left, *op, right, expr),

            // Unary operations
            HirExprKind::UnaryOp { op, operand } => self.lower_unary_op(*op, operand, expr),

            // String literals
            HirExprKind::StringLiteral(_) | HirExprKind::HeapString(_) => Err(
                CompilerError::lir_transformation("String literal lowering not yet implemented"),
            ),

            // Field access
            HirExprKind::Field { base, field } => {
                let place = HirPlace::Field {
                    base: Box::new(HirPlace::Var(*base)),
                    field: *field,
                };
                self.lower_place_load(&place)
            }

            // Move
            HirExprKind::Move(place) => self.lower_place_load(place),

            // Function calls
            HirExprKind::Call { target, args } => self.lower_call_expr(target, args),

            HirExprKind::MethodCall {
                receiver,
                method,
                args,
            } => self.lower_method_call(receiver, *method, args),

            // Constructors
            HirExprKind::StructConstruct { type_name, .. } => {
                Err(CompilerError::lir_transformation(format!(
                    "Struct construction lowering not yet implemented: {}",
                    type_name
                )))
            }

            HirExprKind::Collection(_) => Err(CompilerError::lir_transformation(
                "Collection construction lowering not yet implemented",
            )),

            HirExprKind::Range { .. } => Err(CompilerError::lir_transformation(
                "Range construction lowering not yet implemented",
            )),
        }
    }

    // ========================================================================
    // Literal Lowering
    // ========================================================================

    fn lower_int_literal(&self, value: i64) -> Result<Vec<LirInst>, CompilerError> {
        Ok(vec![LirInst::I64Const(value)])
    }

    fn lower_float_literal(&self, value: f64) -> Result<Vec<LirInst>, CompilerError> {
        Ok(vec![LirInst::F64Const(value)])
    }

    fn lower_bool_literal(&self, value: bool) -> Result<Vec<LirInst>, CompilerError> {
        let int_value = if value { 1 } else { 0 };
        Ok(vec![LirInst::I32Const(int_value)])
    }

    fn lower_char_literal(&self, value: char) -> Result<Vec<LirInst>, CompilerError> {
        Ok(vec![LirInst::I32Const(value as i32)])
    }

    // ========================================================================
    // Binary Operation Lowering
    // ========================================================================

    fn lower_binary_op(
        &mut self,
        left: &HirExpr,
        op: BinOp,
        right: &HirExpr,
        _result_expr: &HirExpr,
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Lower left operand
        insts.extend(self.lower_expr(left)?);

        // Lower right operand
        insts.extend(self.lower_expr(right)?);

        // Emit the operation instruction
        let op_inst = self.lower_binop_instruction(op, left)?;
        insts.push(op_inst);

        Ok(insts)
    }

    fn lower_binop_instruction(
        &self,
        op: BinOp,
        operand: &HirExpr,
    ) -> Result<LirInst, CompilerError> {
        let lir_type = hir_expr_to_lir_type(operand);

        match (op, lir_type) {
            // I64 operations
            (BinOp::Add, LirType::I64) => Ok(LirInst::I64Add),
            (BinOp::Sub, LirType::I64) => Ok(LirInst::I64Sub),
            (BinOp::Mul, LirType::I64) => Ok(LirInst::I64Mul),
            (BinOp::Div, LirType::I64) => Ok(LirInst::I64DivS),
            (BinOp::Eq, LirType::I64) => Ok(LirInst::I64Eq),
            (BinOp::Ne, LirType::I64) => Ok(LirInst::I64Ne),
            (BinOp::Lt, LirType::I64) => Ok(LirInst::I64LtS),
            (BinOp::Gt, LirType::I64) => Ok(LirInst::I64GtS),

            // I32 operations
            (BinOp::Add, LirType::I32) => Ok(LirInst::I32Add),
            (BinOp::Sub, LirType::I32) => Ok(LirInst::I32Sub),
            (BinOp::Mul, LirType::I32) => Ok(LirInst::I32Mul),
            (BinOp::Div, LirType::I32) => Ok(LirInst::I32DivS),
            (BinOp::Eq, LirType::I32) => Ok(LirInst::I32Eq),
            (BinOp::Ne, LirType::I32) => Ok(LirInst::I32Ne),
            (BinOp::Lt, LirType::I32) => Ok(LirInst::I32LtS),
            (BinOp::Gt, LirType::I32) => Ok(LirInst::I32GtS),

            // F64 operations
            (BinOp::Add, LirType::F64) => Ok(LirInst::F64Add),
            (BinOp::Sub, LirType::F64) => Ok(LirInst::F64Sub),
            (BinOp::Mul, LirType::F64) => Ok(LirInst::F64Mul),
            (BinOp::Div, LirType::F64) => Ok(LirInst::F64Div),
            (BinOp::Eq, LirType::F64) => Ok(LirInst::F64Eq),
            (BinOp::Ne, LirType::F64) => Ok(LirInst::F64Ne),

            // Logical operations (I32 booleans)
            (BinOp::And, LirType::I32) => Ok(LirInst::I32Mul),
            (BinOp::Or, LirType::I32) => Ok(LirInst::I32Add),

            // Unsupported operations
            (BinOp::Mod, _) => Err(CompilerError::lir_transformation(
                "Modulo operation not yet supported",
            )),
            (BinOp::Le, _) | (BinOp::Ge, _) => Err(CompilerError::lir_transformation(
                "Less/greater than or equal operations not yet supported",
            )),
            (BinOp::Lt, LirType::F64) | (BinOp::Gt, LirType::F64) => Err(
                CompilerError::lir_transformation("Float comparison not yet supported"),
            ),
            (BinOp::Exponent, _) | (BinOp::Root, _) => Err(CompilerError::lir_transformation(
                "Exponent/root operations not yet supported",
            )),
            _ => Err(CompilerError::lir_transformation(format!(
                "Unsupported binary operation {:?} for type {:?}",
                op, lir_type
            ))),
        }
    }

    // ========================================================================
    // Unary Operation Lowering
    // ========================================================================

    fn lower_unary_op(
        &mut self,
        op: UnaryOp,
        operand: &HirExpr,
        _result_expr: &HirExpr,
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Lower operand
        insts.extend(self.lower_expr(operand)?);

        // Emit the operation instructions
        let op_insts = self.lower_unaryop_instructions(op, operand)?;
        insts.extend(op_insts);

        Ok(insts)
    }

    fn lower_unaryop_instructions(
        &self,
        op: UnaryOp,
        operand: &HirExpr,
    ) -> Result<Vec<LirInst>, CompilerError> {
        let lir_type = hir_expr_to_lir_type(operand);

        match (op, lir_type) {
            // Negation
            (UnaryOp::Neg, LirType::I64) => Ok(vec![LirInst::I64Const(-1), LirInst::I64Mul]),
            (UnaryOp::Neg, LirType::I32) => Ok(vec![LirInst::I32Const(-1), LirInst::I32Mul]),
            (UnaryOp::Neg, LirType::F64) => Ok(vec![LirInst::F64Const(-1.0), LirInst::F64Mul]),

            // Logical NOT
            (UnaryOp::Not, LirType::I32) => Ok(vec![LirInst::I32Const(0), LirInst::I32Eq]),

            // Unsupported
            (UnaryOp::Not, _) => Err(CompilerError::lir_transformation(format!(
                "Logical NOT only supported for boolean (I32) types, got {:?}",
                lir_type
            ))),
            (UnaryOp::Neg, LirType::F32) => Err(CompilerError::lir_transformation(
                "F32 negation not yet supported",
            )),
        }
    }
}
