//! Runtime RPN expression lowering helpers.
//!
//! WHAT: lowers AST runtime expression stacks into explicit HIR expression graphs.
//! WHY: AST already normalized precedence into RPN, so HIR can reuse that stack order directly
//! without rebuilding parser-era operator rules.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::Operator::Range;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_datatypes::HirTypeKind;
use crate::compiler_frontend::hir::hir_nodes::{
    HirExpression, HirExpressionKind, HirUnaryOp, ValueKind,
};
use crate::compiler_frontend::host_functions::CallTarget;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_hir_transformation_error;

use super::LoweredExpression;

impl<'a> HirBuilder<'a> {
    // WHAT: evaluates AST runtime expressions stored in RPN order into HIR values.
    // WHY: AST already normalized runtime operator precedence into stack order, so HIR lowering
    //      can preserve that sequencing without rebuilding precedence logic.
    pub(crate) fn lower_runtime_rpn_expression(
        &mut self,
        nodes: &[AstNode],
        location: &SourceLocation,
        expr_type: &DataType,
    ) -> Result<LoweredExpression, CompilerError> {
        let mut prelude = Vec::new();
        let mut stack: Vec<HirExpression> = Vec::with_capacity(nodes.len());

        for node in nodes {
            match &node.kind {
                NodeKind::Rvalue(sub_expr) => {
                    let lowered = self.lower_expression(sub_expr)?;
                    prelude.extend(lowered.prelude);
                    stack.push(lowered.value);
                    self.log_rpn_step("push-rvalue", node, &stack);
                }

                NodeKind::FunctionCall {
                    name,
                    args,
                    result_types,
                    location,
                } => {
                    let function_id = self.resolve_function_id_or_error(name, location)?;
                    let lowered = self.lower_call_expression(
                        CallTarget::UserFunction(function_id),
                        args,
                        result_types,
                        location,
                    )?;
                    prelude.extend(lowered.prelude);
                    stack.push(lowered.value);
                    self.log_rpn_step("push-call", node, &stack);
                }

                NodeKind::HostFunctionCall {
                    name: host_function_id,
                    args,
                    result_types,
                    location,
                } => {
                    let lowered = self.lower_call_expression(
                        CallTarget::HostFunction(host_function_id.to_owned()),
                        args,
                        result_types,
                        location,
                    )?;
                    prelude.extend(lowered.prelude);
                    stack.push(lowered.value);
                    self.log_rpn_step("push-host-call", node, &stack);
                }

                NodeKind::FieldAccess { .. } => {
                    let lowered = self.lower_ast_node_as_expression(node)?;
                    prelude.extend(lowered.prelude);
                    stack.push(lowered.value);
                    self.log_rpn_step("push-field", node, &stack);
                }

                NodeKind::Operator(op) => {
                    let region = self.current_region_or_error(location)?;
                    match op.required_values() {
                        1 => {
                            let Some(operand) = stack.pop() else {
                                return_hir_transformation_error!(
                                    format!("RPN stack underflow for unary operator {:?}", op),
                                    self.hir_error_location(location)
                                );
                            };

                            let hir_op = self.lower_unary_op(op, &node.location)?;
                            let result_ty = match hir_op {
                                HirUnaryOp::Not => self.intern_type_kind(HirTypeKind::Bool),
                                HirUnaryOp::Neg => operand.ty,
                            };

                            stack.push(self.make_expression(
                                &node.location,
                                HirExpressionKind::UnaryOp {
                                    op: hir_op,
                                    operand: Box::new(operand),
                                },
                                result_ty,
                                ValueKind::RValue,
                                region,
                            ));
                            self.log_rpn_step("unary", node, &stack);
                        }

                        2 => {
                            let Some(right) = stack.pop() else {
                                return_hir_transformation_error!(
                                    format!(
                                        "RPN stack underflow for operator {:?} (missing rhs)",
                                        op
                                    ),
                                    self.hir_error_location(location)
                                );
                            };
                            let Some(left) = stack.pop() else {
                                return_hir_transformation_error!(
                                    format!(
                                        "RPN stack underflow for operator {:?} (missing lhs)",
                                        op
                                    ),
                                    self.hir_error_location(location)
                                );
                            };

                            if matches!(op, Range) {
                                let range_ty = self.intern_type_kind(HirTypeKind::Range);
                                stack.push(self.make_expression(
                                    &node.location,
                                    HirExpressionKind::Range {
                                        start: Box::new(left),
                                        end: Box::new(right),
                                    },
                                    range_ty,
                                    ValueKind::RValue,
                                    region,
                                ));
                                self.log_rpn_step("range", node, &stack);
                                continue;
                            }

                            let hir_op = self.lower_bin_op(op, &node.location)?;
                            let result_ty = self.infer_binop_result_type(left.ty, right.ty, hir_op);

                            stack.push(self.make_expression(
                                &node.location,
                                HirExpressionKind::BinOp {
                                    left: Box::new(left),
                                    op: hir_op,
                                    right: Box::new(right),
                                },
                                result_ty,
                                ValueKind::RValue,
                                region,
                            ));
                            self.log_rpn_step("binary", node, &stack);
                        }

                        _ => {
                            return_hir_transformation_error!(
                                format!("Unsupported operator arity for {:?}", op),
                                self.hir_error_location(location)
                            )
                        }
                    }
                }

                _ => {
                    return_hir_transformation_error!(
                        format!(
                            "Unsupported AST node in runtime RPN expression: {:?}",
                            node.kind
                        ),
                        self.hir_error_location(&node.location)
                    )
                }
            }
        }

        if stack.len() != 1 {
            return_hir_transformation_error!(
                format!(
                    "Malformed runtime RPN expression: expected one value on stack, got {}",
                    stack.len()
                ),
                self.hir_error_location(location)
            );
        }

        let mut value = stack
            .pop()
            .expect("validated runtime RPN expression should leave exactly one value on the stack");
        let expected_ty = self.lower_data_type(expr_type, location)?;
        value.ty = expected_ty;

        Ok(LoweredExpression { prelude, value })
    }
}
