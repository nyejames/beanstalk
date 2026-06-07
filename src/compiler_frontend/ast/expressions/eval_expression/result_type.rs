//! Expression result-type resolution for AST evaluation.
//!
//! WHAT: mirrors final RPN execution shape with a type-only stack.
//! WHY: AST must enforce operator typing before folding/lowering so later stages never infer
//!      type policy from runtime-oriented structures.

use super::operator_policy::{resolve_binary_operator_type, resolve_unary_operator_type};
use super::typing_error::ExpressionTypingError;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::Operator;
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, OperatorOperandPosition};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::datatypes::{DataType, diagnostic_type_spelling};
use crate::compiler_frontend::symbols::string_interning::StringTable;

/// Resolved type facts carried by the operator typing stack.
///
/// WHAT: keeps canonical `TypeId` next to the diagnostic/value spelling needed for
///      readable errors and a few value-shape restrictions such as compile-time paths.
/// WHY: operator compatibility should be decided on semantic IDs without throwing away
///      the `DataType` payloads that final AST nodes still carry for diagnostics.
#[derive(Clone)]
pub(super) struct ExpressionResultType {
    pub(super) diagnostic_type: DataType,
    pub(super) type_id: TypeId,
}

impl ExpressionResultType {
    pub(super) fn from_type_id(type_id: TypeId, type_environment: &TypeEnvironment) -> Self {
        Self {
            diagnostic_type: diagnostic_type_spelling(type_id, type_environment),
            type_id,
        }
    }
}

pub(super) fn resolve_expression_result_type(
    output_queue: &[AstNode],
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

    for node in output_queue {
        match &node.kind {
            // Values and field accesses push their pre-resolved types directly.
            NodeKind::Rvalue(expr) => stack.push(ExpressionResultType {
                diagnostic_type: expr.diagnostic_type.to_owned(),
                type_id: expr.type_id,
            }),

            NodeKind::FieldAccess {
                type_id,
                diagnostic_type,
                ..
            } => stack.push(ExpressionResultType {
                diagnostic_type: diagnostic_type.to_owned(),
                type_id: *type_id,
            }),

            // Calls may produce single or multiple results; the helper normalises them.
            NodeKind::FunctionCall {
                result_type_ids, ..
            }
            | NodeKind::MethodCall {
                result_type_ids, ..
            }
            | NodeKind::DynamicTraitMethodCall {
                result_type_ids, ..
            }
            | NodeKind::CollectionBuiltinCall {
                result_type_ids, ..
            }
            | NodeKind::MapBuiltinCall {
                result_type_ids, ..
            }
            | NodeKind::HostFunctionCall {
                result_type_ids, ..
            }
            | NodeKind::HandledFallibleHostFunctionCall {
                result_type_ids, ..
            }
            | NodeKind::HandledFallibleFunctionCall {
                result_type_ids, ..
            } => {
                stack.push(call_result_expression_type(
                    result_type_ids,
                    type_environment,
                ));
            }

            // Operators consume operand types from the stack and push the result type.
            NodeKind::Operator(op) => match op.required_values() {
                1 => {
                    let Some(operand) = stack.pop() else {
                        return Err(missing_operand_error(
                            op,
                            OperatorOperandPosition::Unary,
                            &node.location,
                            string_table,
                        ));
                    };
                    stack.push(resolve_unary_operator_type(
                        op,
                        &operand,
                        &node.location,
                        type_environment,
                    )?);
                }

                2 => {
                    let Some(rhs) = stack.pop() else {
                        return Err(missing_operand_error(
                            op,
                            OperatorOperandPosition::BinaryRight,
                            &node.location,
                            string_table,
                        ));
                    };
                    let Some(lhs) = stack.pop() else {
                        return Err(missing_operand_error(
                            op,
                            OperatorOperandPosition::BinaryLeft,
                            &node.location,
                            string_table,
                        ));
                    };
                    stack.push(resolve_binary_operator_type(
                        &lhs,
                        &rhs,
                        op,
                        &node.location,
                        type_environment,
                    )?);
                }

                _ => {
                    return Err(CompilerError::compiler_error(format!(
                        "Unsupported operator arity during expression typing: {:?}",
                        op
                    ))
                    .into());
                }
            },

            _ => {
                return Err(CompilerError::compiler_error(format!(
                    "Unsupported AST node found in expression typing: {:?}",
                    node.kind
                ))
                .into());
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

fn call_result_expression_type(
    result_type_ids: &[TypeId],
    type_environment: &TypeEnvironment,
) -> ExpressionResultType {
    let type_id = match result_type_ids {
        [] => type_environment.builtins().none,
        [single] => *single,
        // Multi-result calls are rejected by the statement/multi-bind owners.
        // Keep expression-operator diagnostics user-facing if one reaches this path.
        _ => type_environment.builtins().none,
    };

    ExpressionResultType::from_type_id(type_id, type_environment)
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
