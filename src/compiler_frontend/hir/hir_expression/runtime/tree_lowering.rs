use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{FallibleHandling, Operator};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::ids::TypeId as FrontendTypeId;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::external_packages::CallTarget;
use crate::compiler_frontend::hir::expressions::{HirExpression, HirExpressionKind, ValueKind};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::operators::HirUnaryOp;
use crate::compiler_frontend::hir::statements::HirStatement;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_hir_transformation_error;

use super::super::{
    DynamicTraitMethodCallLoweringInput, ExternalFallibleCallLoweringInput, LoweredExpression,
};
use super::RuntimeRpnTree;

impl<'a> HirBuilder<'a> {
    // WHAT: evaluates AST runtime expressions stored in RPN order into HIR values.
    // WHY: this keeps parser precedence decisions intact while enabling dedicated CFG lowering for
    //      short-circuit `and`/`or` so RHS side effects stay branch-gated.
    pub(crate) fn lower_runtime_rpn_expression(
        &mut self,
        nodes: &[AstNode],
        location: &SourceLocation,
        expr_type_id: FrontendTypeId,
    ) -> Result<LoweredExpression, CompilerError> {
        let tree = self.build_runtime_rpn_tree(nodes, location)?;
        let mut lowered = self.lower_runtime_tree_node(&tree, location)?;
        let expected_ty = self.lower_type_id(expr_type_id, location)?;
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
                let mut prelude = Vec::new();
                let lowered_operand =
                    self.lower_runtime_tree_child_for_parent(&mut prelude, operand, location)?;
                let region = self.current_region_or_error(location)?;
                let hir_op = self.lower_unary_op(op, location)?;
                let result_ty = match hir_op {
                    HirUnaryOp::Not => builtin_type_ids::BOOL,
                    HirUnaryOp::Neg => lowered_operand.ty,
                };

                Ok(LoweredExpression {
                    prelude,
                    value: self.make_expression(
                        location,
                        HirExpressionKind::UnaryOp {
                            op: hir_op,
                            operand: Box::new(lowered_operand),
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

                let mut prelude = Vec::new();
                let lowered_left =
                    self.lower_runtime_tree_child_for_parent(&mut prelude, left, location)?;
                let lowered_right =
                    self.lower_runtime_tree_child_for_parent(&mut prelude, right, location)?;
                let region = self.current_region_or_error(location)?;

                if matches!(op, Operator::Range) {
                    let range_ty = builtin_type_ids::RANGE;
                    return Ok(LoweredExpression {
                        prelude,
                        value: self.make_expression(
                            location,
                            HirExpressionKind::Range {
                                start: Box::new(lowered_left),
                                end: Box::new(lowered_right),
                            },
                            range_ty,
                            ValueKind::RValue,
                            region,
                        ),
                    });
                }

                let hir_op = self.lower_bin_op(op, location)?;
                let result_ty =
                    self.infer_binop_result_type(lowered_left.ty, lowered_right.ty, hir_op);

                Ok(LoweredExpression {
                    prelude,
                    value: self.make_expression(
                        location,
                        HirExpressionKind::BinOp {
                            left: Box::new(lowered_left),
                            op: hir_op,
                            right: Box::new(lowered_right),
                        },
                        result_ty,
                        ValueKind::RValue,
                        region,
                    ),
                })
            }
        }
    }

    pub(super) fn lower_runtime_tree_value_to_current_block(
        &mut self,
        node: &RuntimeRpnTree,
        location: &SourceLocation,
    ) -> Result<HirExpression, CompilerError> {
        let lowered = self.lower_runtime_tree_node(node, location)?;
        for prelude in lowered.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }

        Ok(lowered.value)
    }

    fn lower_runtime_tree_child_for_parent(
        &mut self,
        pending_prelude: &mut Vec<HirStatement>,
        node: &RuntimeRpnTree,
        location: &SourceLocation,
    ) -> Result<HirExpression, CompilerError> {
        if self.runtime_tree_needs_current_block_lowering(node) {
            for prelude in pending_prelude.drain(..) {
                self.emit_statement_to_current_block(prelude, location)?;
            }

            return self.lower_runtime_tree_value_to_current_block(node, location);
        }

        let lowered = self.lower_runtime_tree_node(node, location)?;
        pending_prelude.extend(lowered.prelude);
        Ok(lowered.value)
    }

    fn runtime_tree_needs_current_block_lowering(&self, node: &RuntimeRpnTree) -> bool {
        match node {
            RuntimeRpnTree::Leaf(node) => self.ast_node_needs_current_block_lowering(node),

            RuntimeRpnTree::Unary { operand, .. } => {
                self.runtime_tree_needs_current_block_lowering(operand)
            }

            RuntimeRpnTree::Binary {
                left, op, right, ..
            } => {
                matches!(op, Operator::And | Operator::Or)
                    || self.runtime_tree_needs_current_block_lowering(left)
                    || self.runtime_tree_needs_current_block_lowering(right)
            }
        }
    }

    fn lower_runtime_leaf_node(
        &mut self,
        node: &AstNode,
        fallback_location: &SourceLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        match &node.kind {
            NodeKind::Rvalue(sub_expr)
                if self.expression_needs_current_block_lowering(sub_expr) =>
            {
                let value = self.lower_expression_value_to_current_block(sub_expr)?;
                Ok(LoweredExpression {
                    prelude: vec![],
                    value,
                })
            }
            NodeKind::Rvalue(sub_expr) => self.lower_expression(sub_expr),
            NodeKind::FunctionCall {
                name,
                args,
                result_type_ids,
                location,
            } => {
                let function_id = self.resolve_function_id_or_error(name, location)?;
                self.lower_call_expression(
                    CallTarget::UserFunction(function_id),
                    args,
                    result_type_ids,
                    location,
                )
            }
            NodeKind::HandledFallibleFunctionCall {
                name,
                args,
                result_type_ids,
                handling,
                location,
            } => {
                if matches!(handling, FallibleHandling::Propagate) {
                    let function_id = self.resolve_function_id_or_error(name, location)?;
                    let value = self.lower_fallible_call_to_success_value(
                        CallTarget::UserFunction(function_id),
                        args,
                        result_type_ids,
                        location,
                    )?;
                    return Ok(LoweredExpression {
                        prelude: vec![],
                        value,
                    });
                }

                let function_id = self.resolve_function_id_or_error(name, location)?;
                self.lower_handled_fallible_call_expression(
                    CallTarget::UserFunction(function_id),
                    args,
                    result_type_ids,
                    handling,
                    true,
                    location,
                )
            }
            NodeKind::HandledFallibleHostFunctionCall {
                name: host_function_id,
                args,
                result_type_ids,
                error_type_id,
                handling,
                location,
            } => {
                if matches!(handling, FallibleHandling::Propagate) {
                    let value = self.lower_external_fallible_call_to_success_value(
                        *host_function_id,
                        args,
                        result_type_ids,
                        *error_type_id,
                        location,
                    )?;
                    return Ok(LoweredExpression {
                        prelude: vec![],
                        value,
                    });
                }

                self.lower_handled_external_fallible_call_expression(
                    ExternalFallibleCallLoweringInput {
                        id: *host_function_id,
                        args,
                        result_type_ids,
                        error_type_id: *error_type_id,
                        handling,
                        value_required: true,
                        location,
                    },
                )
            }
            NodeKind::HostFunctionCall {
                name: host_function_id,
                args,
                result_type_ids,
                location,
            } => self.lower_call_expression(
                CallTarget::ExternalFunction(*host_function_id),
                args,
                result_type_ids,
                location,
            ),
            NodeKind::FieldAccess { .. } => self.lower_ast_node_as_expression(node),
            NodeKind::MethodCall {
                receiver,
                method_path,
                args,
                result_type_ids,
                location,
                ..
            } => self.lower_receiver_method_call_expression(
                method_path,
                receiver,
                args,
                result_type_ids,
                location,
            ),
            NodeKind::DynamicTraitMethodCall {
                receiver,
                trait_id,
                requirement_id,
                receiver_requires_mutable,
                args,
                result_type_ids,
                location,
                ..
            } => self.lower_dynamic_trait_method_call_expression(
                DynamicTraitMethodCallLoweringInput {
                    receiver,
                    trait_id: *trait_id,
                    requirement_id: *requirement_id,
                    receiver_requires_mutable: *receiver_requires_mutable,
                    args,
                    result_type_ids,
                    location,
                },
            ),
            NodeKind::CollectionBuiltinCall {
                receiver,
                op,
                receiver_requires_mutable,
                args,
                result_type_ids,
                location,
            } => self.lower_collection_builtin_call_expression(
                *op,
                receiver,
                *receiver_requires_mutable,
                args,
                result_type_ids,
                location,
            ),
            NodeKind::MapBuiltinCall {
                receiver,
                op,
                receiver_requires_mutable,
                args,
                result_type_ids,
                location,
                ..
            } => self.lower_map_builtin_call_expression(
                *op,
                receiver,
                *receiver_requires_mutable,
                args,
                result_type_ids,
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
