//! Operator-specific HIR expression lowering helpers.
//!
//! WHAT: lowers unary and binary AST operators into explicit HIR expression nodes.
//! WHY: keeping operator handling separate makes the core expression lowering loop easier to follow.

use crate::compiler_frontend::ast::expressions::expression::Operator;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_datatypes::{HirTypeKind, TypeId};
use crate::compiler_frontend::hir::hir_nodes::{HirBinOp, HirUnaryOp};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_hir_transformation_error;

impl<'a> HirBuilder<'a> {
    pub(super) fn lower_bin_op(
        &self,
        op: &Operator,
        location: &SourceLocation,
    ) -> Result<HirBinOp, CompilerError> {
        match op {
            Operator::Add => Ok(HirBinOp::Add),
            Operator::Subtract => Ok(HirBinOp::Sub),
            Operator::Multiply => Ok(HirBinOp::Mul),
            Operator::Divide => Ok(HirBinOp::Div),
            Operator::IntDivide => Ok(HirBinOp::IntDiv),
            Operator::Modulus => Ok(HirBinOp::Mod),
            Operator::Exponent => Ok(HirBinOp::Exponent),
            Operator::And => Ok(HirBinOp::And),
            Operator::Or => Ok(HirBinOp::Or),
            Operator::GreaterThan => Ok(HirBinOp::Gt),
            Operator::GreaterThanOrEqual => Ok(HirBinOp::Ge),
            Operator::LessThan => Ok(HirBinOp::Lt),
            Operator::LessThanOrEqual => Ok(HirBinOp::Le),
            Operator::Equality => Ok(HirBinOp::Eq),
            Operator::NotEqual => Ok(HirBinOp::Ne),
            Operator::Not => {
                return_hir_transformation_error!(
                    "'not' cannot be lowered as a binary operator",
                    self.hir_error_location(location)
                )
            }
            Operator::Range => {
                return_hir_transformation_error!(
                    "Range operator is lowered as HirExpressionKind::Range",
                    self.hir_error_location(location)
                )
            }
        }
    }

    pub(super) fn lower_unary_op(
        &self,
        op: &Operator,
        location: &SourceLocation,
    ) -> Result<HirUnaryOp, CompilerError> {
        match op {
            Operator::Not => Ok(HirUnaryOp::Not),
            Operator::Subtract => Ok(HirUnaryOp::Neg),
            _ => {
                return_hir_transformation_error!(
                    format!("Unsupported unary operator: {:?}", op),
                    self.hir_error_location(location)
                )
            }
        }
    }

    // WHAT: Infers binary-op result kinds for lowered runtime expressions.
    // WHY: Runtime RPN lowering needs a final type for each expression node.
    pub(super) fn infer_binop_result_type(
        &mut self,
        left: TypeId,
        right: TypeId,
        op: HirBinOp,
    ) -> TypeId {
        match op {
            HirBinOp::Eq
            | HirBinOp::Ne
            | HirBinOp::Lt
            | HirBinOp::Le
            | HirBinOp::Gt
            | HirBinOp::Ge
            | HirBinOp::And
            | HirBinOp::Or => self.intern_type_kind(HirTypeKind::Bool),

            HirBinOp::Add | HirBinOp::Sub | HirBinOp::Mul | HirBinOp::Mod | HirBinOp::Exponent => {
                let left_kind = self.type_context.get(left).kind.clone();
                let right_kind = self.type_context.get(right).kind.clone();

                if matches!(left_kind, HirTypeKind::Float)
                    || matches!(right_kind, HirTypeKind::Float)
                {
                    self.intern_type_kind(HirTypeKind::Float)
                } else if matches!(left_kind, HirTypeKind::Decimal)
                    || matches!(right_kind, HirTypeKind::Decimal)
                {
                    self.intern_type_kind(HirTypeKind::Decimal)
                } else if matches!(left_kind, HirTypeKind::String)
                    || matches!(right_kind, HirTypeKind::String)
                {
                    self.intern_type_kind(HirTypeKind::String)
                } else {
                    left
                }
            }

            HirBinOp::Div => {
                let left_kind = self.type_context.get(left).kind.clone();
                let right_kind = self.type_context.get(right).kind.clone();

                if matches!(left_kind, HirTypeKind::Decimal)
                    || matches!(right_kind, HirTypeKind::Decimal)
                {
                    self.intern_type_kind(HirTypeKind::Decimal)
                } else {
                    self.intern_type_kind(HirTypeKind::Float)
                }
            }

            HirBinOp::IntDiv => self.intern_type_kind(HirTypeKind::Int),
        }
    }

    pub(super) fn extract_return_types_from_datatype(&self, data_type: &DataType) -> Vec<DataType> {
        match data_type {
            DataType::Returns(returns) => returns.clone(),
            DataType::None => vec![],
            other => vec![other.clone()],
        }
    }
}
