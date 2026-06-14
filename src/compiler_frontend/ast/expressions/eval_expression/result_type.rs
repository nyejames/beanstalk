//! Expression result-type resolution for AST evaluation.
//!
//! WHAT: mirrors final RPN execution shape with a type-only stack.
//! WHY: AST must enforce operator typing before folding/lowering so later stages never infer
//! type policy from runtime-oriented structures.

use super::operator_policy::{resolve_binary_operator_type, resolve_unary_operator_type};
use super::typing_error::ExpressionTypingError;
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionValueShape, Operator, expression_value_shape_for_diagnostic_type,
};
use crate::compiler_frontend::ast::expressions::expression_rpn::ExpressionRpnItem;
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, OperatorOperandPosition};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::diagnostic_type_spelling;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::symbols::string_interning::StringTable;

/// Resolved type facts carried by the operator typing stack.
///
/// WHAT: keeps canonical `TypeId` next to the value shape needed for operator policy
///      and the diagnostic spelling needed for readable errors.
/// WHY: operator compatibility should be decided on semantic IDs and explicit value
///      shape metadata; `DataType` is retained only for diagnostics.
#[derive(Clone)]
pub(super) struct ExpressionResultType {
    pub(super) diagnostic_type: DataType,
    pub(super) type_id: TypeId,
    pub(super) value_shape: ExpressionValueShape,
}

impl ExpressionResultType {
    pub(super) fn from_type_id(type_id: TypeId, type_environment: &TypeEnvironment) -> Self {
        let diagnostic_type = diagnostic_type_spelling(type_id, type_environment);
        Self {
            diagnostic_type: diagnostic_type.to_owned(),
            type_id,
            value_shape: expression_value_shape_for_diagnostic_type(&diagnostic_type),
        }
    }

    pub(super) fn from_expression(expression: &Expression) -> Self {
        Self {
            diagnostic_type: expression.diagnostic_type.to_owned(),
            type_id: expression.type_id,
            value_shape: expression.value_shape,
        }
    }

    pub(super) fn from_type_id_with_shape(
        type_id: TypeId,
        type_environment: &TypeEnvironment,
        value_shape: ExpressionValueShape,
    ) -> Self {
        Self {
            diagnostic_type: diagnostic_type_spelling(type_id, type_environment),
            type_id,
            value_shape,
        }
    }
}

pub(super) fn resolve_expression_result_type(
    output_queue: &[ExpressionRpnItem],
    expression_location: &SourceLocation,
    string_table: &mut StringTable,
    type_environment: &TypeEnvironment,
) -> Result<ExpressionResultType, ExpressionTypingError> {
    // Mirror the final RPN evaluation shape with a type-only stack so operator diagnostics fire
    // before constant folding mutates any nodes.
    let mut stack: Vec<ExpressionResultType> = Vec::with_capacity(output_queue.len());

    // ------------------------
    //  Walk RPN output queue
    // ------------------------

    for item in output_queue {
        match item {
            // Operand expressions push their pre-resolved types directly.
            ExpressionRpnItem::Operand(expression) => {
                stack.push(ExpressionResultType::from_expression(expression));
            }

            // Operators consume operand types from the stack and push the result type.
            ExpressionRpnItem::Operator { operator, location } => {
                match operator.required_values() {
                    1 => {
                        let Some(operand) = stack.pop() else {
                            return Err(missing_operand_error(
                                operator,
                                OperatorOperandPosition::Unary,
                                location,
                                string_table,
                            ));
                        };
                        stack.push(resolve_unary_operator_type(
                            operator,
                            &operand,
                            location,
                            type_environment,
                        )?);
                    }

                    2 => {
                        let Some(rhs) = stack.pop() else {
                            return Err(missing_operand_error(
                                operator,
                                OperatorOperandPosition::BinaryRight,
                                location,
                                string_table,
                            ));
                        };
                        let Some(lhs) = stack.pop() else {
                            return Err(missing_operand_error(
                                operator,
                                OperatorOperandPosition::BinaryLeft,
                                location,
                                string_table,
                            ));
                        };
                        stack.push(resolve_binary_operator_type(
                            &lhs,
                            &rhs,
                            operator,
                            location,
                            type_environment,
                        )?);
                    }

                    _ => {
                        return Err(CompilerError::compiler_error(format!(
                            "Unsupported operator arity during expression typing: {:?}",
                            operator
                        ))
                        .into());
                    }
                }
            }
        }
    }

    // ------------------------
    //  Validate final stack shape
    // ------------------------

    if stack.len() != 1 {
        return Err(CompilerDiagnostic::invalid_expression(expression_location.clone()).into());
    }

    // ------------------------
    //  Extract resolved result
    // ------------------------

    // stack.len() == 1 guarantees pop() returns Some; the None arm guards a compiler bug.
    match stack.pop() {
        Some(resolved_type) => Ok(resolved_type),
        None => Err(CompilerError::compiler_error(
            "Expression typing stack unexpectedly empty after shape validation.",
        )
        .into()),
    }
}

/// Build a missing-operand diagnostic for the given operator and stack position.
fn missing_operand_error(
    operator: &Operator,
    position: OperatorOperandPosition,
    location: &SourceLocation,
    string_table: &mut StringTable,
) -> ExpressionTypingError {
    CompilerDiagnostic::missing_operator_operand(
        string_table.get_or_intern(operator.to_str().to_owned()),
        position,
        location.clone(),
    )
    .into()
}
