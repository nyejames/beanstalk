//! Runtime RPN expression lowering helpers.
//!
//! WHAT: lowers AST runtime expression stacks into explicit HIR expression graphs.
//! WHY: AST already normalized precedence into RPN, so HIR can reuse that ordering while still
//! enforcing runtime short-circuit semantics for logical operators.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::call_argument::normalize_call_argument_values;
use crate::compiler_frontend::ast::expressions::expression::Operator;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_datatypes::HirTypeKind;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, HirExpression, HirExpressionKind, HirPlace, HirStatement, HirStatementKind,
    HirTerminator, HirUnaryOp, LocalId, ValueKind,
};
use crate::compiler_frontend::host_functions::CallTarget;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_hir_transformation_error;

use super::LoweredExpression;

#[derive(Debug, Clone)]
enum RuntimeRpnTree {
    Leaf(Box<AstNode>),
    Unary {
        op: Operator,
        operand: Box<RuntimeRpnTree>,
        location: SourceLocation,
    },
    Binary {
        left: Box<RuntimeRpnTree>,
        op: Operator,
        right: Box<RuntimeRpnTree>,
        location: SourceLocation,
    },
}

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

    fn build_runtime_rpn_tree(
        &self,
        nodes: &[AstNode],
        location: &SourceLocation,
    ) -> Result<RuntimeRpnTree, CompilerError> {
        let mut stack: Vec<RuntimeRpnTree> = Vec::with_capacity(nodes.len());

        for node in nodes {
            match &node.kind {
                NodeKind::Operator(op) => match op.required_values() {
                    1 => {
                        let Some(operand) = stack.pop() else {
                            return_hir_transformation_error!(
                                format!("RPN stack underflow for unary operator {:?}", op),
                                self.hir_error_location(&node.location)
                            );
                        };

                        stack.push(RuntimeRpnTree::Unary {
                            op: op.to_owned(),
                            operand: Box::new(operand),
                            location: node.location.clone(),
                        });
                    }
                    2 => {
                        let Some(right) = stack.pop() else {
                            return_hir_transformation_error!(
                                format!("RPN stack underflow for operator {:?} (missing rhs)", op),
                                self.hir_error_location(&node.location)
                            );
                        };
                        let Some(left) = stack.pop() else {
                            return_hir_transformation_error!(
                                format!("RPN stack underflow for operator {:?} (missing lhs)", op),
                                self.hir_error_location(&node.location)
                            );
                        };

                        stack.push(RuntimeRpnTree::Binary {
                            left: Box::new(left),
                            op: op.to_owned(),
                            right: Box::new(right),
                            location: node.location.clone(),
                        });
                    }
                    _ => {
                        return_hir_transformation_error!(
                            format!("Unsupported operator arity for {:?}", op),
                            self.hir_error_location(&node.location)
                        );
                    }
                },
                _ => stack.push(RuntimeRpnTree::Leaf(Box::new(node.to_owned()))),
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

        Ok(stack
            .pop()
            .expect("validated runtime RPN expression should leave exactly one tree node"))
    }

    fn lower_runtime_tree_node(
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
                    &normalize_call_argument_values(args),
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
                    &normalize_call_argument_values(args),
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
                CallTarget::HostFunction(host_function_id.to_owned()),
                &normalize_call_argument_values(args),
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
                &normalize_call_argument_values(args),
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

    fn lower_short_circuit_binary_expression(
        &mut self,
        left: &RuntimeRpnTree,
        op: &Operator,
        right: &RuntimeRpnTree,
        location: &SourceLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        let lowered_left = self.lower_runtime_tree_node(left, location)?;
        for statement in lowered_left.prelude {
            self.emit_statement_to_current_block(statement, location)?;
        }

        let condition_block = self.current_block_id_or_error(location)?;
        let parent_region = self.current_region_or_error(location)?;
        let bool_ty = self.intern_type_kind(HirTypeKind::Bool);
        let result_local = self.allocate_temp_local(bool_ty, Some(location.to_owned()))?;

        let rhs_region = self.create_child_region(parent_region);
        let short_region = self.create_child_region(parent_region);
        let rhs_label = if matches!(op, Operator::And) {
            "logical-and-rhs"
        } else {
            "logical-or-rhs"
        };
        let short_label = if matches!(op, Operator::And) {
            "logical-and-short"
        } else {
            "logical-or-short"
        };
        let merge_label = if matches!(op, Operator::And) {
            "logical-and-merge"
        } else {
            "logical-or-merge"
        };

        let rhs_block = self.create_block(rhs_region, location, rhs_label)?;
        let short_block = self.create_block(short_region, location, short_label)?;
        let merge_block = self.create_block(parent_region, location, merge_label)?;

        let (then_block, else_block, short_value, rhs_edge, short_edge) =
            if matches!(op, Operator::And) {
                (
                    rhs_block,
                    short_block,
                    false,
                    "logical.and.rhs",
                    "logical.and.short",
                )
            } else {
                (
                    short_block,
                    rhs_block,
                    true,
                    "logical.or.short",
                    "logical.or.rhs",
                )
            };

        self.emit_terminator(
            condition_block,
            HirTerminator::If {
                condition: lowered_left.value,
                then_block,
                else_block,
            },
            location,
        )?;

        self.emit_short_circuit_rhs_branch(
            rhs_block,
            merge_block,
            result_local,
            right,
            location,
            rhs_edge,
        )?;
        self.emit_short_circuit_constant_branch(
            (short_block, merge_block),
            result_local,
            short_value,
            bool_ty,
            location,
            short_edge,
        )?;

        self.set_current_block(merge_block, location)?;
        let merge_region = self.current_region_or_error(location)?;
        let value = self.make_expression(
            location,
            HirExpressionKind::Load(HirPlace::Local(result_local)),
            bool_ty,
            ValueKind::RValue,
            merge_region,
        );

        Ok(LoweredExpression {
            prelude: vec![],
            value,
        })
    }

    fn emit_short_circuit_rhs_branch(
        &mut self,
        rhs_block: BlockId,
        merge_block: BlockId,
        result_local: LocalId,
        rhs: &RuntimeRpnTree,
        location: &SourceLocation,
        edge_label: &str,
    ) -> Result<(), CompilerError> {
        self.set_current_block(rhs_block, location)?;

        let lowered_rhs = self.lower_runtime_tree_node(rhs, location)?;
        for statement in lowered_rhs.prelude {
            self.emit_statement_to_current_block(statement, location)?;
        }
        self.emit_runtime_assign_local_statement(result_local, lowered_rhs.value, location)?;

        let rhs_tail = self.current_block_id_or_error(location)?;
        if !self.block_has_explicit_terminator(rhs_tail, location)? {
            self.emit_jump_to(rhs_tail, merge_block, location, edge_label)?;
        }

        Ok(())
    }

    fn emit_short_circuit_constant_branch(
        &mut self,
        branch_blocks: (BlockId, BlockId),
        result_local: LocalId,
        short_value: bool,
        bool_ty: crate::compiler_frontend::hir::hir_datatypes::TypeId,
        location: &SourceLocation,
        edge_label: &str,
    ) -> Result<(), CompilerError> {
        let (short_block, merge_block) = branch_blocks;
        self.set_current_block(short_block, location)?;
        let short_region = self.current_region_or_error(location)?;
        let short_value_expression = self.make_expression(
            location,
            HirExpressionKind::Bool(short_value),
            bool_ty,
            ValueKind::Const,
            short_region,
        );
        self.emit_runtime_assign_local_statement(result_local, short_value_expression, location)?;
        self.emit_jump_to(short_block, merge_block, location, edge_label)
    }

    fn emit_runtime_assign_local_statement(
        &mut self,
        local: LocalId,
        value: HirExpression,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let value = self.materialize_short_circuit_assignment_value(value, location);
        let assign_statement = HirStatement {
            id: self.allocate_node_id(),
            kind: HirStatementKind::Assign {
                target: HirPlace::Local(local),
                value,
            },
            location: location.to_owned(),
        };

        self.side_table.map_statement(location, &assign_statement);
        self.emit_statement_to_current_block(assign_statement, location)
    }

    fn materialize_short_circuit_assignment_value(
        &mut self,
        value: HirExpression,
        location: &SourceLocation,
    ) -> HirExpression {
        // Assigning a place expression directly into the branch-merge temp can preserve aliasing
        // edges to user locals. Materialize as a copied value so branch-local temps stay detached.
        if let HirExpressionKind::Load(place) = value.kind {
            return self.make_expression(
                location,
                HirExpressionKind::Copy(place),
                value.ty,
                ValueKind::RValue,
                value.region,
            );
        }

        value
    }
}
