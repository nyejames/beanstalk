use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::Operator;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::external_packages::CallTarget;
use crate::compiler_frontend::hir::expressions::{HirExpressionKind, ValueKind};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_datatypes::HirTypeKind;
use crate::compiler_frontend::hir::operators::HirUnaryOp;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_hir_transformation_error;

use super::super::LoweredExpression;
use super::RuntimeRpnTree;

impl<'a> HirBuilder<'a> {
    // WHAT: evaluates AST runtime expressions stored in RPN order into HIR values.
    // WHY: this keeps parser precedence decisions intact while enabling dedicated CFG lowering for
    //      short-circuit `and`/`or` so RHS side effects stay branch-gated.
    pub(crate) fn lower_runtime_rpn_expression(
        &mut self,
        nodes: &[AstNode],
        location: &SourceLocation,
        expr_type: &DataType,
    ) -> Result<LoweredExpression, CompilerError> {
        let tree = self.build_runtime_rpn_tree(nodes, location)?;
        let mut lowered = self.lower_runtime_tree_node(&tree, location)?;
        let expected_ty = self.lower_data_type(expr_type, location)?;
        lowered.value.ty = expected_ty;
        Ok(lowered)
    }

    pub(super) fn lower_runtime_tree_node(
        &mut self,
        node: &RuntimeRpnTree,
        location: &SourceLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        match node {
            RuntimeRpnTree::Leaf(node) => self.lower_runtime_leaf_node(node.as_ref(), location),
            RuntimeRpnTree::Unary {
                op,
                operand,
                location,
            } => {
                let lowered_operand = self.lower_runtime_tree_node(operand, location)?;
                let region = self.current_region_or_error(location)?;
                let hir_op = self.lower_unary_op(op, location)?;
                let result_ty = match hir_op {
                    HirUnaryOp::Not => self.intern_type_kind(HirTypeKind::Bool),
                    HirUnaryOp::Neg => lowered_operand.value.ty,
                };

                Ok(LoweredExpression {
                    prelude: lowered_operand.prelude,
                    value: self.make_expression(
                        location,
                        HirExpressionKind::UnaryOp {
                            op: hir_op,
                            operand: Box::new(lowered_operand.value),
                        },
                        result_ty,
                        ValueKind::RValue,
                        region,
                    ),
                })
            }
            RuntimeRpnTree::Binary {
                left,
                op,
                right,
                location,
            } => {
                if matches!(op, Operator::And | Operator::Or) {
                    return self.lower_short_circuit_binary_expression(left, op, right, location);
                }

                let lowered_left = self.lower_runtime_tree_node(left, location)?;
                let lowered_right = self.lower_runtime_tree_node(right, location)?;
                let region = self.current_region_or_error(location)?;
                let mut prelude = lowered_left.prelude;
                prelude.extend(lowered_right.prelude);

                if matches!(op, Operator::Range) {
                    let range_ty = self.intern_type_kind(HirTypeKind::Range);
                    return Ok(LoweredExpression {
                        prelude,
                        value: self.make_expression(
                            location,
                            HirExpressionKind::Range {
                                start: Box::new(lowered_left.value),
                                end: Box::new(lowered_right.value),
                            },
                            range_ty,
                            ValueKind::RValue,
                            region,
                        ),
                    });
                }

                let hir_op = self.lower_bin_op(op, location)?;
                let result_ty = self.infer_binop_result_type(
                    lowered_left.value.ty,
                    lowered_right.value.ty,
                    hir_op,
                );

                Ok(LoweredExpression {
                    prelude,
                    value: self.make_expression(
                        location,
                        HirExpressionKind::BinOp {
                            left: Box::new(lowered_left.value),
                            op: hir_op,
                            right: Box::new(lowered_right.value),
                        },
                        result_ty,
                        ValueKind::RValue,
                        region,
                    ),
                })
            }
        }
    }

    fn lower_runtime_leaf_node(
        &mut self,
        node: &AstNode,
        fallback_location: &SourceLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        match &node.kind {
            NodeKind::Rvalue(sub_expr) => self.lower_expression(sub_expr),
            NodeKind::FunctionCall {
                name,
                args,
                result_types,
                location,
            } => {
                let function_id = self.resolve_function_id_or_error(name, location)?;
                self.lower_call_expression(
                    CallTarget::UserFunction(function_id),
                    args,
                    result_types,
                    location,
                )
            }
            NodeKind::ResultHandledFunctionCall {
                name,
                args,
                result_types,
                handling,
                location,
            } => {
                let function_id = self.resolve_function_id_or_error(name, location)?;
                self.lower_result_handled_call_expression(
                    CallTarget::UserFunction(function_id),
                    args,
                    result_types,
                    handling,
                    true,
                    location,
                )
            }
            NodeKind::HostFunctionCall {
                name: host_function_id,
                args,
                result_types,
                location,
            } => self.lower_call_expression(
                CallTarget::ExternalFunction(host_function_id.to_owned()),
                args,
                result_types,
                location,
            ),
            NodeKind::FieldAccess { .. } => self.lower_ast_node_as_expression(node),
            NodeKind::MethodCall {
                receiver,
                method_path,
                builtin,
                args,
                result_types,
                location,
                ..
            } => self.lower_receiver_method_call_expression(
                method_path,
                *builtin,
                receiver,
                args,
                result_types,
                location,
            ),
            NodeKind::CollectionBuiltinCall {
                receiver,
                op,
                args,
                result_types,
                location,
            } => self.lower_collection_builtin_call_expression(
                *op,
                receiver,
                args,
                result_types,
                location,
            ),
            _ => {
                return_hir_transformation_error!(
                    format!(
                        "Unsupported AST node in runtime RPN expression: {:?}",
                        node.kind
                    ),
                    self.hir_error_location(fallback_location)
                )
            }
        }
    }
}
