//! HIR Expression Lowering
//!
//! Lowers typed AST expressions into HIR expressions and statement preludes.
//! This file contains expression-specific lowering logic on `HirBuilder`.

use crate::backends::function_registry::CallTarget;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind, Var};
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, Operator,
};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorLocation};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_datatypes::{HirType, HirTypeKind, TypeId};
use crate::compiler_frontend::hir::hir_display::HirDisplayContext;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FieldId, FunctionId, HirBinOp, HirBlock, HirExpression, HirExpressionKind, HirLocal,
    HirNodeId, HirPlace, HirStatement, HirStatementKind, HirUnaryOp, LocalId, RegionId, StructId,
    ValueKind,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringId;
use crate::compiler_frontend::tokenizer::tokens::TextLocation;
use crate::hir_log;
use crate::return_hir_transformation_error;

#[derive(Debug, Clone)]
pub(crate) struct LoweredExpression {
    pub prelude: Vec<HirStatement>,
    pub value: HirExpression,
}

impl<'a> HirBuilder<'a> {
    pub(crate) fn lower_expression(
        &mut self,
        expr: &Expression,
    ) -> Result<LoweredExpression, CompilerError> {
        self.log_expression_input(expr);

        let lowered = match &expr.kind {
            ExpressionKind::Int(value) => {
                let region = self.current_region_or_error(&expr.location)?;
                let ty = self.lower_data_type(&expr.data_type, &expr.location)?;

                Ok(LoweredExpression {
                    prelude: vec![],
                    value: HirExpression {
                        kind: HirExpressionKind::Int(*value),
                        ty,
                        value_kind: ValueKind::Const,
                        region,
                    },
                })
            }

            ExpressionKind::Float(value) => {
                let region = self.current_region_or_error(&expr.location)?;
                let ty = self.lower_data_type(&expr.data_type, &expr.location)?;

                Ok(LoweredExpression {
                    prelude: vec![],
                    value: HirExpression {
                        kind: HirExpressionKind::Float(*value),
                        ty,
                        value_kind: ValueKind::Const,
                        region,
                    },
                })
            }

            ExpressionKind::Bool(value) => {
                let region = self.current_region_or_error(&expr.location)?;
                let ty = self.lower_data_type(&expr.data_type, &expr.location)?;

                Ok(LoweredExpression {
                    prelude: vec![],
                    value: HirExpression {
                        kind: HirExpressionKind::Bool(*value),
                        ty,
                        value_kind: ValueKind::Const,
                        region,
                    },
                })
            }

            ExpressionKind::Char(value) => {
                let region = self.current_region_or_error(&expr.location)?;
                let ty = self.lower_data_type(&expr.data_type, &expr.location)?;

                Ok(LoweredExpression {
                    prelude: vec![],
                    value: HirExpression {
                        kind: HirExpressionKind::Char(*value),
                        ty,
                        value_kind: ValueKind::Const,
                        region,
                    },
                })
            }

            ExpressionKind::StringSlice(value) => {
                let region = self.current_region_or_error(&expr.location)?;
                let ty = self.lower_data_type(&expr.data_type, &expr.location)?;

                Ok(LoweredExpression {
                    prelude: vec![],
                    value: HirExpression {
                        kind: HirExpressionKind::StringLiteral(
                            self.string_table.resolve(*value).to_owned(),
                        ),
                        ty,
                        value_kind: ValueKind::Const,
                        region,
                    },
                })
            }

            ExpressionKind::Reference(name) => {
                let region = self.current_region_or_error(&expr.location)?;
                let local_id = self.resolve_local_id_or_error(name, &expr.location)?;
                let ty = self.lower_data_type(&expr.data_type, &expr.location)?;

                Ok(LoweredExpression {
                    prelude: vec![],
                    value: HirExpression {
                        kind: HirExpressionKind::Load(HirPlace::Local(local_id)),
                        ty,
                        value_kind: ValueKind::Place,
                        region,
                    },
                })
            }

            ExpressionKind::Runtime(nodes) => {
                self.lower_runtime_rpn_expression(nodes, &expr.location, &expr.data_type)
            }

            ExpressionKind::FunctionCall(name, args) => self.lower_call_expression(
                CallTarget::UserFunction(name.clone()),
                args,
                &self.extract_return_types_from_datatype(&expr.data_type),
                &expr.location,
            ),

            ExpressionKind::HostFunctionCall(host_id, args) => self.lower_call_expression(
                CallTarget::HostFunction(host_id.clone()),
                args,
                &self.extract_return_types_from_datatype(&expr.data_type),
                &expr.location,
            ),

            ExpressionKind::Collection(items) => {
                let mut prelude = Vec::new();
                let mut lowered_items = Vec::with_capacity(items.len());

                for item in items {
                    let lowered_item = self.lower_expression(item)?;
                    prelude.extend(lowered_item.prelude);
                    lowered_items.push(lowered_item.value);
                }

                let region = self.current_region_or_error(&expr.location)?;
                let ty = self.lower_data_type(&expr.data_type, &expr.location)?;

                Ok(LoweredExpression {
                    prelude,
                    value: HirExpression {
                        kind: HirExpressionKind::Collection(lowered_items),
                        ty,
                        value_kind: ValueKind::RValue,
                        region,
                    },
                })
            }

            ExpressionKind::Range(start, end) => {
                let mut prelude = Vec::new();
                let lowered_start = self.lower_expression(start)?;
                let lowered_end = self.lower_expression(end)?;
                prelude.extend(lowered_start.prelude);
                prelude.extend(lowered_end.prelude);

                let region = self.current_region_or_error(&expr.location)?;
                let ty = self.lower_data_type(&expr.data_type, &expr.location)?;

                Ok(LoweredExpression {
                    prelude,
                    value: HirExpression {
                        kind: HirExpressionKind::Range {
                            start: Box::new(lowered_start.value),
                            end: Box::new(lowered_end.value),
                        },
                        ty,
                        value_kind: ValueKind::RValue,
                        region,
                    },
                })
            }

            ExpressionKind::StructInstance(args) => {
                let struct_id = self.resolve_struct_id_from_nominal_fields(args, &expr.location)?;
                let mut prelude = Vec::new();
                let mut fields = Vec::with_capacity(args.len());

                for arg in args {
                    let field_id =
                        self.resolve_field_id_or_error(struct_id, &arg.id, &expr.location)?;
                    let lowered_value = self.lower_expression(&arg.value)?;
                    prelude.extend(lowered_value.prelude);
                    fields.push((field_id, lowered_value.value));
                }

                let region = self.current_region_or_error(&expr.location)?;
                let ty = self.lower_data_type(&expr.data_type, &expr.location)?;

                Ok(LoweredExpression {
                    prelude,
                    value: HirExpression {
                        kind: HirExpressionKind::StructConstruct { struct_id, fields },
                        ty,
                        value_kind: ValueKind::RValue,
                        region,
                    },
                })
            }

            ExpressionKind::Template(_) => {
                return_hir_transformation_error!(
                    "Runtime template expressions are not lowered in this phase",
                    self.hir_error_location(&expr.location)
                )
            }

            ExpressionKind::Function(_, _) => {
                return_hir_transformation_error!(
                    "Function expressions are not lowered in this phase",
                    self.hir_error_location(&expr.location)
                )
            }

            ExpressionKind::StructDefinition(_) => {
                return_hir_transformation_error!(
                    "Struct definition expressions are not lowered in this phase",
                    self.hir_error_location(&expr.location)
                )
            }

            ExpressionKind::None => {
                let region = self.current_region_or_error(&expr.location)?;
                Ok(LoweredExpression {
                    prelude: vec![],
                    value: self.unit_expression(region),
                })
            }
        }?;

        self.log_expression_output(expr, &lowered.value);
        Ok(lowered)
    }

    pub(crate) fn lower_expression_and_emit(
        &mut self,
        expr: &Expression,
    ) -> Result<HirExpression, CompilerError> {
        let lowered = self.lower_expression(expr)?;

        for statement in lowered.prelude {
            self.emit_statement_to_current_block(statement, &expr.location)?;
        }

        Ok(lowered.value)
    }

    pub(crate) fn lower_runtime_rpn_expression(
        &mut self,
        nodes: &[AstNode],
        location: &TextLocation,
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
                    returns,
                    location,
                } => {
                    let lowered = self.lower_call_expression(
                        CallTarget::UserFunction(name.clone()),
                        args,
                        returns,
                        location,
                    )?;
                    prelude.extend(lowered.prelude);
                    stack.push(lowered.value);
                    self.log_rpn_step("push-call", node, &stack);
                }

                NodeKind::HostFunctionCall {
                    name: host_function_id,
                    args,
                    returns,
                    location,
                } => {
                    let lowered = self.lower_call_expression(
                        CallTarget::HostFunction(host_function_id.clone()),
                        args,
                        returns,
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

                            stack.push(HirExpression {
                                kind: HirExpressionKind::UnaryOp {
                                    op: hir_op,
                                    operand: Box::new(operand),
                                },
                                ty: result_ty,
                                value_kind: ValueKind::RValue,
                                region,
                            });
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

                            if matches!(op, Operator::Range) {
                                stack.push(HirExpression {
                                    kind: HirExpressionKind::Range {
                                        start: Box::new(left),
                                        end: Box::new(right),
                                    },
                                    ty: self.intern_type_kind(HirTypeKind::Range),
                                    value_kind: ValueKind::RValue,
                                    region,
                                });
                                self.log_rpn_step("range", node, &stack);
                                continue;
                            }

                            let hir_op = self.lower_bin_op(op, &node.location)?;
                            let result_ty = self.infer_binop_result_type(left.ty, right.ty, hir_op);

                            stack.push(HirExpression {
                                kind: HirExpressionKind::BinOp {
                                    left: Box::new(left),
                                    op: hir_op,
                                    right: Box::new(right),
                                },
                                ty: result_ty,
                                value_kind: ValueKind::RValue,
                                region,
                            });
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

        let value = stack.pop().expect("checked above");
        let expected_ty = self.lower_data_type(expr_type, location)?;

        let value = HirExpression {
            ty: expected_ty,
            ..value
        };

        Ok(LoweredExpression { prelude, value })
    }

    pub(crate) fn lower_ast_node_as_expression(
        &mut self,
        node: &AstNode,
    ) -> Result<LoweredExpression, CompilerError> {
        match &node.kind {
            NodeKind::Rvalue(expr) => self.lower_expression(expr),

            NodeKind::FunctionCall {
                name,
                args,
                returns,
                location,
            } => self.lower_call_expression(
                CallTarget::UserFunction(name.clone()),
                args,
                returns,
                location,
            ),

            NodeKind::HostFunctionCall {
                name: host_function_id,
                args,
                returns,
                location,
            } => self.lower_call_expression(
                CallTarget::HostFunction(host_function_id.clone()),
                args,
                returns,
                location,
            ),

            NodeKind::FieldAccess {
                base: _,
                field: _,
                data_type,
                ..
            } => {
                let region = self.current_region_or_error(&node.location)?;
                let (prelude, place) = self.lower_ast_node_to_place(node)?;
                let ty = self.lower_data_type(data_type, &node.location)?;

                Ok(LoweredExpression {
                    prelude,
                    value: HirExpression {
                        kind: HirExpressionKind::Load(place),
                        ty,
                        value_kind: ValueKind::Place,
                        region,
                    },
                })
            }

            _ => {
                return_hir_transformation_error!(
                    format!("AST node is not an expression: {:?}", node.kind),
                    self.hir_error_location(&node.location)
                )
            }
        }
    }

    pub(crate) fn lower_ast_node_to_place(
        &mut self,
        node: &AstNode,
    ) -> Result<(Vec<HirStatement>, HirPlace), CompilerError> {
        match &node.kind {
            NodeKind::Rvalue(expr) => match &expr.kind {
                ExpressionKind::Reference(name) => {
                    let local = self.resolve_local_id_or_error(name, &node.location)?;
                    Ok((vec![], HirPlace::Local(local)))
                }

                _ => {
                    let lowered = self.lower_expression(expr)?;
                    let place = self.place_from_expression(&lowered.value, &node.location)?;
                    Ok((lowered.prelude, place))
                }
            },

            NodeKind::FunctionCall {
                name,
                args,
                returns,
                location,
            } => {
                let lowered = self.lower_call_expression(
                    CallTarget::UserFunction(name.clone()),
                    args,
                    returns,
                    location,
                )?;
                let place = self.place_from_expression(&lowered.value, &node.location)?;
                Ok((lowered.prelude, place))
            }

            NodeKind::HostFunctionCall {
                name: host_function_id,
                args,
                returns,
                location,
            } => {
                let lowered = self.lower_call_expression(
                    CallTarget::HostFunction(host_function_id.clone()),
                    args,
                    returns,
                    location,
                )?;
                let place = self.place_from_expression(&lowered.value, &node.location)?;
                Ok((lowered.prelude, place))
            }

            NodeKind::FieldAccess { base, field, .. } => {
                let (prelude, base_place) = self.lower_ast_node_to_place(base)?;
                let field_id = self.resolve_field_id_for_base_place_or_error(
                    &base_place,
                    *field,
                    &node.location,
                )?;

                Ok((
                    prelude,
                    HirPlace::Field {
                        base: Box::new(base_place),
                        field: field_id,
                    },
                ))
            }

            _ => {
                return_hir_transformation_error!(
                    format!("Cannot lower AST node to HIR place: {:?}", node.kind),
                    self.hir_error_location(&node.location)
                )
            }
        }
    }

    pub(crate) fn lower_call_expression(
        &mut self,
        target: CallTarget,
        args: &[Expression],
        returns: &[DataType],
        location: &TextLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        if let CallTarget::UserFunction(name) = &target {
            let _ = self.resolve_function_id_or_error(name, location)?;
        }

        let mut prelude = Vec::new();
        let mut lowered_args = Vec::with_capacity(args.len());

        for arg in args {
            let lowered = self.lower_expression(arg)?;
            prelude.extend(lowered.prelude);
            lowered_args.push(lowered.value);
        }

        let no_return =
            returns.is_empty() || (returns.len() == 1 && matches!(returns[0], DataType::None));
        let statement_id = self.allocate_node_id();
        let region = self.current_region_or_error(location)?;

        if no_return {
            let statement = HirStatement {
                id: statement_id,
                kind: HirStatementKind::Call {
                    target,
                    args: lowered_args,
                    result: None,
                },
                location: location.clone(),
            };

            self.side_table.map_statement(location, &statement);
            prelude.push(statement);

            let value = self.unit_expression(region);
            self.log_call_result_binding(location, None, &value);
            return Ok(LoweredExpression { prelude, value });
        }

        let call_result_type = if returns.len() == 1 {
            self.lower_data_type(&returns[0], location)?
        } else {
            let field_types = returns
                .iter()
                .map(|ret| self.lower_data_type(ret, location))
                .collect::<Result<Vec<_>, _>>()?;
            self.intern_type_kind(HirTypeKind::Tuple {
                fields: field_types,
            })
        };

        let temp_local = self.allocate_temp_local(call_result_type, Some(location.clone()))?;

        let statement = HirStatement {
            id: statement_id,
            kind: HirStatementKind::Call {
                target,
                args: lowered_args,
                result: Some(temp_local),
            },
            location: location.clone(),
        };

        self.side_table.map_statement(location, &statement);
        prelude.push(statement);

        let value = HirExpression {
            kind: HirExpressionKind::Load(HirPlace::Local(temp_local)),
            ty: call_result_type,
            value_kind: ValueKind::Place,
            region,
        };

        self.log_call_result_binding(location, Some(temp_local), &value);

        Ok(LoweredExpression { prelude, value })
    }

    pub(crate) fn lower_data_type(
        &mut self,
        data_type: &DataType,
        location: &TextLocation,
    ) -> Result<TypeId, CompilerError> {
        let kind = match data_type {
            DataType::Inferred => {
                return_hir_transformation_error!(
                    "DataType::Inferred reached HIR lowering",
                    self.hir_error_location(location)
                )
            }

            DataType::Reference(inner) => return self.lower_data_type(inner, location),

            DataType::Bool | DataType::True | DataType::False => HirTypeKind::Bool,
            DataType::Int => HirTypeKind::Int,
            DataType::Float => HirTypeKind::Float,
            DataType::Decimal => HirTypeKind::Decimal,
            DataType::Char => HirTypeKind::Char,
            DataType::String | DataType::CoerceToString | DataType::Template => HirTypeKind::String,
            DataType::Range => HirTypeKind::Range,
            DataType::None => HirTypeKind::Unit,

            DataType::Collection(inner, _) => HirTypeKind::Collection {
                element: self.lower_data_type(inner, location)?,
            },

            DataType::Returns(values) => {
                if values.is_empty() {
                    HirTypeKind::Unit
                } else if values.len() == 1 {
                    return self.lower_data_type(&values[0], location);
                } else {
                    let fields = values
                        .iter()
                        .map(|ty| self.lower_data_type(ty, location))
                        .collect::<Result<Vec<_>, _>>()?;
                    HirTypeKind::Tuple { fields }
                }
            }

            DataType::Function(receiver, signature) => {
                let receiver = receiver
                    .as_ref()
                    .as_ref()
                    .map(|ty| self.lower_data_type(ty, location))
                    .transpose()?;

                let params = signature
                    .parameters
                    .iter()
                    .map(|param| self.lower_data_type(&param.value.data_type, location))
                    .collect::<Result<Vec<_>, _>>()?;

                let returns = signature
                    .returns
                    .iter()
                    .map(|ret| self.lower_data_type(ret, location))
                    .collect::<Result<Vec<_>, _>>()?;

                HirTypeKind::Function {
                    receiver,
                    params,
                    returns,
                }
            }

            DataType::Option(inner) => HirTypeKind::Option {
                inner: self.lower_data_type(inner, location)?,
            },

            DataType::Choices(variants) => {
                let variant_types = variants
                    .iter()
                    .map(|variant| self.lower_data_type(&variant.value.data_type, location))
                    .collect::<Result<Vec<_>, _>>()?;
                HirTypeKind::Union {
                    variants: variant_types,
                }
            }

            DataType::Parameters(fields) | DataType::Struct(fields, _) => {
                let struct_id = self.resolve_struct_id_from_nominal_fields(fields, location)?;
                HirTypeKind::Struct { struct_id }
            }
        };

        Ok(self.intern_type_kind(kind))
    }

    pub(crate) fn intern_type_kind(&mut self, kind: HirTypeKind) -> TypeId {
        if let Some(existing) = self.type_interner.get(&kind) {
            return *existing;
        }

        let id = self.type_context.insert(HirType { kind: kind.clone() });
        self.type_interner.insert(kind, id);
        id
    }

    pub(crate) fn emit_statement_to_current_block(
        &mut self,
        statement: HirStatement,
        source_location: &TextLocation,
    ) -> Result<(), CompilerError> {
        self.statement_locations.push(source_location.clone());
        let block = self.current_block_mut_or_error(source_location)?;
        block.statements.push(statement);
        Ok(())
    }

    pub(crate) fn allocate_node_id(&mut self) -> HirNodeId {
        let id = HirNodeId(self.next_node_id);
        self.next_node_id += 1;
        id
    }

    pub(crate) fn allocate_temp_local(
        &mut self,
        ty: TypeId,
        source_info: Option<TextLocation>,
    ) -> Result<LocalId, CompilerError> {
        let location = source_info.clone().unwrap_or_default();
        let region = self.current_region_or_error(&location)?;

        let local_id = LocalId(self.next_local_id);
        self.next_local_id += 1;

        let local = HirLocal {
            id: local_id,
            ty,
            mutable: true,
            region,
            source_info,
        };

        {
            let block = self.current_block_mut_or_error(&location)?;
            block.locals.push(local.clone());
        }

        let temp_name = format!("__hir_tmp_{}", self.temp_local_counter);
        self.temp_local_counter += 1;
        let temp_name_id = InternedPath::from_single_str(&temp_name, self.string_table);

        // Compiler-introduced temporaries are intentionally excluded from AST symbol resolution.
        // They are named only for diagnostics/debug rendering via the side table.
        self.side_table.bind_local_name(local_id, temp_name_id);
        self.side_table.map_local_source(&local);

        Ok(local_id)
    }

    pub(crate) fn current_block_mut_or_error(
        &mut self,
        location: &TextLocation,
    ) -> Result<&mut HirBlock, CompilerError> {
        let block_id = self.current_block_id_or_error(location)?;
        self.block_mut_by_id_or_error(block_id, location)
    }

    pub(crate) fn current_region_or_error(
        &self,
        location: &TextLocation,
    ) -> Result<RegionId, CompilerError> {
        let Some(region) = self.current_region else {
            return_hir_transformation_error!(
                "No current HIR region is active",
                self.hir_error_location(location)
            );
        };

        Ok(region)
    }

    // AST enforces module-wide unique InternedPath symbols and disallows shadowing.
    // HIR therefore resolves locals/functions by full path identity, not leaf names.
    pub(crate) fn resolve_local_id_or_error(
        &self,
        name: &InternedPath,
        location: &TextLocation,
    ) -> Result<LocalId, CompilerError> {
        let Some(local_id) = self.locals_by_name.get(name).copied() else {
            return_hir_transformation_error!(
                format!(
                    "Unresolved local '{}' during HIR expression lowering",
                    self.symbol_name_for_diagnostics(name)
                ),
                self.hir_error_location(location)
            );
        };

        Ok(local_id)
    }

    pub(crate) fn resolve_function_id_or_error(
        &self,
        name: &InternedPath,
        location: &TextLocation,
    ) -> Result<FunctionId, CompilerError> {
        let Some(function_id) = self.functions_by_name.get(name).copied() else {
            return_hir_transformation_error!(
                format!(
                    "Unresolved function '{}' during HIR expression lowering",
                    self.symbol_name_for_diagnostics(name)
                ),
                self.hir_error_location(location)
            );
        };

        Ok(function_id)
    }

    pub(crate) fn resolve_field_id_or_error(
        &self,
        struct_id: StructId,
        field_name: &InternedPath,
        location: &TextLocation,
    ) -> Result<FieldId, CompilerError> {
        let Some(field_id) = self
            .fields_by_struct_and_name
            .get(&(struct_id, field_name.clone()))
            .copied()
        else {
            return_hir_transformation_error!(
                format!(
                    "Field '{}' is not registered for struct {:?}",
                    self.symbol_name_for_diagnostics(field_name),
                    struct_id
                ),
                self.hir_error_location(location)
            );
        };

        Ok(field_id)
    }

    fn resolve_struct_id_from_nominal_fields(
        &self,
        fields: &[Var],
        location: &TextLocation,
    ) -> Result<StructId, CompilerError> {
        let Some(first_field) = fields.first() else {
            return_hir_transformation_error!(
                "Cannot lower struct from empty field list",
                self.hir_error_location(location)
            );
        };

        let Some(struct_path) = first_field.id.parent() else {
            return_hir_transformation_error!(
                format!(
                    "Field '{}' has no parent struct path during HIR lowering",
                    self.symbol_name_for_diagnostics(&first_field.id)
                ),
                self.hir_error_location(location)
            );
        };

        let Some(struct_id) = self.structs_by_name.get(&struct_path).copied() else {
            return_hir_transformation_error!(
                format!(
                    "Unresolved struct '{}' during HIR lowering",
                    self.symbol_name_for_diagnostics(&struct_path)
                ),
                self.hir_error_location(location)
            );
        };

        for field in fields {
            let Some(parent) = field.id.parent() else {
                return_hir_transformation_error!(
                    format!(
                        "Field '{}' has no parent struct path during HIR lowering",
                        self.symbol_name_for_diagnostics(&field.id)
                    ),
                    self.hir_error_location(location)
                );
            };

            if parent != struct_path {
                return_hir_transformation_error!(
                    format!(
                        "Field '{}' does not belong to struct '{}'",
                        self.symbol_name_for_diagnostics(&field.id),
                        self.symbol_name_for_diagnostics(&struct_path)
                    ),
                    self.hir_error_location(location)
                );
            }

            if !self
                .fields_by_struct_and_name
                .contains_key(&(struct_id, field.id.clone()))
            {
                return_hir_transformation_error!(
                    format!(
                        "Field '{}' is not registered for struct '{}'",
                        self.symbol_name_for_diagnostics(&field.id),
                        self.symbol_name_for_diagnostics(&struct_path)
                    ),
                    self.hir_error_location(location)
                );
            }
        }

        Ok(struct_id)
    }

    fn resolve_field_id_for_base_place_or_error(
        &self,
        base_place: &HirPlace,
        field_name: StringId,
        location: &TextLocation,
    ) -> Result<FieldId, CompilerError> {
        let struct_id = self.resolve_struct_id_for_place_or_error(base_place, location)?;
        let mut matches = self
            .fields_by_struct_and_name
            .iter()
            .filter_map(|((candidate_struct_id, candidate_name), field_id)| {
                if *candidate_struct_id == struct_id && candidate_name.name() == Some(field_name) {
                    Some(*field_id)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        matches.sort_by_key(|id| id.0);
        matches.dedup_by_key(|id| id.0);

        match matches.as_slice() {
            [single] => Ok(*single),
            [] => {
                return_hir_transformation_error!(
                    format!(
                        "Field '{}' is not defined on struct {:?}",
                        self.string_table.resolve(field_name),
                        struct_id
                    ),
                    self.hir_error_location(location)
                )
            }
            _ => {
                return_hir_transformation_error!(
                    format!(
                        "Ambiguous field '{}' on struct {:?}",
                        self.string_table.resolve(field_name),
                        struct_id
                    ),
                    self.hir_error_location(location)
                )
            }
        }
    }

    fn resolve_struct_id_for_place_or_error(
        &self,
        place: &HirPlace,
        location: &TextLocation,
    ) -> Result<StructId, CompilerError> {
        let ty = self.resolve_place_type_id_or_error(place, location)?;
        match &self.type_context.get(ty).kind {
            HirTypeKind::Struct { struct_id } => Ok(*struct_id),
            _ => {
                return_hir_transformation_error!(
                    "Field access base does not resolve to a struct value in this HIR phase",
                    self.hir_error_location(location)
                )
            }
        }
    }

    fn resolve_place_type_id_or_error(
        &self,
        place: &HirPlace,
        location: &TextLocation,
    ) -> Result<TypeId, CompilerError> {
        match place {
            HirPlace::Local(local_id) => self.resolve_local_type_id_or_error(*local_id, location),
            HirPlace::Field { field, .. } => self.resolve_field_type_id_or_error(*field, location),
            HirPlace::Index { base, .. } => {
                let base_type = self.resolve_place_type_id_or_error(base, location)?;
                match &self.type_context.get(base_type).kind {
                    HirTypeKind::Collection { element } => Ok(*element),
                    _ => {
                        return_hir_transformation_error!(
                            "Index access base is not a collection type",
                            self.hir_error_location(location)
                        )
                    }
                }
            }
        }
    }

    fn resolve_local_type_id_or_error(
        &self,
        local_id: LocalId,
        location: &TextLocation,
    ) -> Result<TypeId, CompilerError> {
        for block in &self.module.blocks {
            if let Some(local) = block.locals.iter().find(|local| local.id == local_id) {
                return Ok(local.ty);
            }
        }

        return_hir_transformation_error!(
            format!("Local {:?} is not registered in HIR blocks", local_id),
            self.hir_error_location(location)
        )
    }

    fn resolve_field_type_id_or_error(
        &self,
        field_id: FieldId,
        location: &TextLocation,
    ) -> Result<TypeId, CompilerError> {
        for hir_struct in &self.module.structs {
            if let Some(field) = hir_struct.fields.iter().find(|field| field.id == field_id) {
                return Ok(field.ty);
            }
        }

        return_hir_transformation_error!(
            format!("Field {:?} is not registered in HIR structs", field_id),
            self.hir_error_location(location)
        )
    }

    fn place_from_expression(
        &self,
        value: &HirExpression,
        location: &TextLocation,
    ) -> Result<HirPlace, CompilerError> {
        let HirExpressionKind::Load(place) = &value.kind else {
            return_hir_transformation_error!(
                "Expected a place-producing expression while lowering place",
                self.hir_error_location(location)
            );
        };

        Ok(place.clone())
    }

    fn lower_bin_op(
        &self,
        op: &Operator,
        location: &TextLocation,
    ) -> Result<HirBinOp, CompilerError> {
        match op {
            Operator::Add => Ok(HirBinOp::Add),
            Operator::Subtract => Ok(HirBinOp::Sub),
            Operator::Multiply => Ok(HirBinOp::Mul),
            Operator::Divide => Ok(HirBinOp::Div),
            Operator::Modulus => Ok(HirBinOp::Mod),
            Operator::Root => Ok(HirBinOp::Root),
            Operator::Exponent => Ok(HirBinOp::Exponent),
            Operator::And => Ok(HirBinOp::And),
            Operator::Or => Ok(HirBinOp::Or),
            Operator::GreaterThan => Ok(HirBinOp::Gt),
            Operator::GreaterThanOrEqual => Ok(HirBinOp::Ge),
            Operator::LessThan => Ok(HirBinOp::Lt),
            Operator::LessThanOrEqual => Ok(HirBinOp::Le),
            Operator::Equality => Ok(HirBinOp::Eq),
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

    fn lower_unary_op(
        &self,
        op: &Operator,
        location: &TextLocation,
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

    fn infer_binop_result_type(&mut self, left: TypeId, right: TypeId, op: HirBinOp) -> TypeId {
        match op {
            HirBinOp::Eq
            | HirBinOp::Ne
            | HirBinOp::Lt
            | HirBinOp::Le
            | HirBinOp::Gt
            | HirBinOp::Ge
            | HirBinOp::And
            | HirBinOp::Or => self.intern_type_kind(HirTypeKind::Bool),

            HirBinOp::Add
            | HirBinOp::Sub
            | HirBinOp::Mul
            | HirBinOp::Div
            | HirBinOp::Mod
            | HirBinOp::Root
            | HirBinOp::Exponent => {
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
        }
    }

    fn extract_return_types_from_datatype(&self, data_type: &DataType) -> Vec<DataType> {
        match data_type {
            DataType::Returns(returns) => returns.clone(),
            DataType::None => vec![],
            other => vec![other.clone()],
        }
    }

    pub(crate) fn unit_expression(&mut self, region: RegionId) -> HirExpression {
        HirExpression {
            kind: HirExpressionKind::TupleConstruct { elements: vec![] },
            ty: self.intern_type_kind(HirTypeKind::Unit),
            value_kind: ValueKind::Const,
            region,
        }
    }

    pub(crate) fn hir_error_location(&self, location: &TextLocation) -> ErrorLocation {
        location.to_error_location(self.string_table)
    }

    #[cfg(feature = "show_hir")]
    fn log_expression_input(&self, expr: &Expression) {
        hir_log!(format!(
            "[HIR] Lowering expression {:?} @ {:?}",
            expr.kind, expr.location
        ));
    }

    #[cfg(not(feature = "show_hir"))]
    fn log_expression_input(&self, _expr: &Expression) {}

    #[cfg(feature = "show_hir")]
    fn log_expression_output(&self, input: &Expression, output: &HirExpression) {
        let display = HirDisplayContext::new(self.string_table)
            .with_side_table(&self.side_table)
            .with_type_context(&self.type_context);

        let rendered = output.display_with_context(&display);
        hir_log!(format!(
            "[HIR] Lowered expression {:?} -> {}",
            input.kind, rendered
        ));
    }

    #[cfg(not(feature = "show_hir"))]
    fn log_expression_output(&self, _input: &Expression, _output: &HirExpression) {}

    #[cfg(feature = "show_hir")]
    fn log_rpn_step(&self, stage: &str, node: &AstNode, stack: &[HirExpression]) {
        let display = HirDisplayContext::new(self.string_table)
            .with_side_table(&self.side_table)
            .with_type_context(&self.type_context);

        let rendered_stack = stack
            .iter()
            .map(|expr| expr.display_with_context(&display))
            .collect::<Vec<_>>()
            .join(" | ");

        hir_log!(format!(
            "[HIR][RPN] {} node={:?} stack=[{}]",
            stage, node.kind, rendered_stack
        ));
    }

    #[cfg(not(feature = "show_hir"))]
    fn log_rpn_step(&self, _stage: &str, _node: &AstNode, _stack: &[HirExpression]) {}

    #[cfg(feature = "show_hir")]
    fn log_call_result_binding(
        &self,
        location: &TextLocation,
        local: Option<LocalId>,
        value: &HirExpression,
    ) {
        let display = HirDisplayContext::new(self.string_table)
            .with_side_table(&self.side_table)
            .with_type_context(&self.type_context);

        let value_string = value.display_with_context(&display);
        hir_log!(format!(
            "[HIR] Emitted call binding @ {:?}: result={:?}, value={}",
            location, local, value_string
        ));
    }

    #[cfg(not(feature = "show_hir"))]
    fn log_call_result_binding(
        &self,
        _location: &TextLocation,
        _local: Option<LocalId>,
        _value: &HirExpression,
    ) {
    }
}
