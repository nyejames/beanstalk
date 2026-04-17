//! Operator typing policy for AST expression evaluation.
//!
//! WHAT: resolves unary/binary operator result types for natural expressions.
//! WHY: AST is the policy owner for operator typing; contextual coercion happens at explicit
//! declaration/return boundaries after parsing.

mod arithmetic;
mod comparison;
mod diagnostics;
mod logical;
mod shared;
mod unary;

use crate::compiler_frontend::ast::expressions::expression::Operator;
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::symbols::string_interning::StringTable;

pub(super) fn resolve_unary_operator_type(
    op: &Operator,
    operand: &DataType,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<DataType, CompilerError> {
    unary::resolve_unary_operator_type(op, operand, location, string_table)
}

pub(super) fn resolve_binary_operator_type(
    lhs: &DataType,
    rhs: &DataType,
    op: &Operator,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<DataType, CompilerError> {
    shared::reject_result_operands(lhs, rhs, op, location, string_table)?;

    if logical::is_logical_operator(op) {
        return logical::resolve_logical_operator_type(lhs, rhs, op, location, string_table);
    }

    if comparison::is_comparison_operator(op) {
        return comparison::resolve_comparison_operator_type(lhs, rhs, op, location, string_table);
    }

    arithmetic::resolve_arithmetic_operator_type(lhs, rhs, op, location, string_table)
}
