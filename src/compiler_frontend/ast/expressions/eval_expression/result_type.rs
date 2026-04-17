//! Expression result-type resolution for AST evaluation.
//!
//! WHAT: mirrors final RPN execution shape with a type-only stack.
//! WHY: AST must enforce operator typing before folding/lowering so later stages never infer
//! type policy from runtime-oriented structures.

use super::operator_policy::{resolve_binary_operator_type, resolve_unary_operator_type};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::{return_compiler_error, return_syntax_error};

pub(super) fn resolve_expression_result_type(
    output_queue: &[AstNode],
    expression_location: &SourceLocation,
    string_table: &StringTable,
) -> Result<DataType, CompilerError> {
    // Mirror the final RPN evaluation shape with a type-only stack so operator diagnostics fire
    // before constant folding mutates any nodes.
    let mut stack: Vec<DataType> = Vec::with_capacity(output_queue.len());

    for node in output_queue {
        match &node.kind {
            NodeKind::Rvalue(expr) => stack.push(expr.data_type.to_owned()),
            NodeKind::FieldAccess { data_type, .. } => stack.push(data_type.to_owned()),
            NodeKind::FunctionCall { result_types, .. }
            | NodeKind::MethodCall { result_types, .. }
            | NodeKind::HostFunctionCall { result_types, .. }
            | NodeKind::ResultHandledFunctionCall { result_types, .. } => {
                stack.push(Expression::call_result_type(result_types.to_owned()));
            }
            NodeKind::Operator(op) => match op.required_values() {
                1 => {
                    let Some(operand) = stack.pop() else {
                        return_syntax_error!(
                            format!("Missing operand for unary operator '{}'.", op.to_str()),
                            node.location.clone(),
                            {
                                CompilationStage => "Expression Evaluation",
                            }
                        );
                    };
                    stack.push(resolve_unary_operator_type(
                        op,
                        &operand,
                        &node.location,
                        string_table,
                    )?);
                }
                2 => {
                    let Some(rhs) = stack.pop() else {
                        return_syntax_error!(
                            format!("Missing right-hand operand for operator '{}'.", op.to_str()),
                            node.location.clone(),
                            {
                                CompilationStage => "Expression Evaluation",
                            }
                        );
                    };
                    let Some(lhs) = stack.pop() else {
                        return_syntax_error!(
                            format!("Missing left-hand operand for operator '{}'.", op.to_str()),
                            node.location.clone(),
                            {
                                CompilationStage => "Expression Evaluation",
                            }
                        );
                    };
                    stack.push(resolve_binary_operator_type(
                        &lhs,
                        &rhs,
                        op,
                        &node.location,
                        string_table,
                    )?);
                }
                _ => {
                    return_compiler_error!(format!(
                        "Unsupported operator arity during expression typing: {:?}",
                        op
                    ));
                }
            },
            _ => {
                return_compiler_error!(format!(
                    "Unsupported AST node found in expression typing: {:?}",
                    node.kind
                ));
            }
        }
    }

    if stack.len() != 1 {
        return_syntax_error!(
            "Invalid expression shape after operator resolution.",
            expression_location.clone(),
            {
                CompilationStage => "Expression Evaluation",
                PrimarySuggestion => "Check the number of operands and operators in this expression",
            }
        );
    }

    match stack.pop() {
        Some(data_type) => Ok(data_type),
        None => return_compiler_error!(
            "Expression typing stack unexpectedly empty after shape validation."
        ),
    }
}
