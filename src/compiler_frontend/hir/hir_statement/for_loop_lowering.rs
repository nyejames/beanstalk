use super::*;
use crate::compiler_frontend::ast::ast_nodes::RangeEndKind;
use crate::return_hir_transformation_error;

impl<'a> HirBuilder<'a> {
    pub(super) fn lower_for_statement_impl(
        &mut self,
        binding: &Declaration,
        range: &ForLoopRange,
        body: &[AstNode],
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        // Build an explicit CFG pipeline so runtime range semantics are deterministic:
        // zero-step guard -> step normalization -> direction dispatch -> bounds checks.
        let pre_header_block = self.current_block_id_or_error(location)?;
        let parent_region = self.current_region_or_error(location)?;

        let step_zero_check_block =
            self.create_block(parent_region, location, "for-step-zero-check")?;
        let step_zero_panic_block =
            self.create_block(parent_region, location, "for-step-zero-panic")?;
        let step_abs_check_block =
            self.create_block(parent_region, location, "for-step-abs-check")?;
        let step_abs_negate_block =
            self.create_block(parent_region, location, "for-step-abs-negate")?;
        let direction_check_block =
            self.create_block(parent_region, location, "for-direction-check")?;
        let descending_negate_block =
            self.create_block(parent_region, location, "for-desc-negate")?;
        let header_selector_block =
            self.create_block(parent_region, location, "for-header-selector")?;
        let header_ascending_block =
            self.create_block(parent_region, location, "for-header-ascending")?;
        let header_descending_block =
            self.create_block(parent_region, location, "for-header-descending")?;
        let body_region = self.create_child_region(parent_region);
        let body_block = self.create_block(body_region, location, "for-body")?;
        let step_block = self.create_block(parent_region, location, "for-step")?;
        let exit_block = self.create_block(parent_region, location, "for-exit")?;

        let binding_type = self.lower_data_type(&binding.value.data_type, location)?;
        if !matches!(
            self.type_context.get(binding_type).kind,
            HirTypeKind::Int | HirTypeKind::Float
        ) {
            return_hir_transformation_error!(
                "For-loop binding must be Int or Float",
                self.hir_error_location(location)
            );
        }

        let bool_ty = self.intern_type_kind(HirTypeKind::Bool);
        let string_ty = self.intern_type_kind(HirTypeKind::String);

        let current_local = self.allocate_temp_local(binding_type, Some(location.clone()))?;
        let end_local = self.allocate_temp_local(binding_type, Some(location.clone()))?;
        let step_local = self.allocate_temp_local(binding_type, Some(location.clone()))?;
        let ascending_local = self.allocate_temp_local(bool_ty, Some(location.clone()))?;

        let lowered_start = self.lower_expression(&range.start)?;
        for prelude in lowered_start.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }
        self.emit_statement_kind(
            HirStatementKind::Assign {
                target: crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(current_local),
                value: lowered_start.value,
            },
            location,
        )?;

        let lowered_end = self.lower_expression(&range.end)?;
        for prelude in lowered_end.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }
        self.emit_statement_kind(
            HirStatementKind::Assign {
                target: crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(end_local),
                value: lowered_end.value,
            },
            location,
        )?;

        let pre_header_region = self.current_region_or_error(location)?;

        // `by` is optional for integer ranges; omitted steps default to +1 / +1.0.
        if let Some(step_expression) = &range.step {
            let lowered_step = self.lower_expression(step_expression)?;
            for prelude in lowered_step.prelude {
                self.emit_statement_to_current_block(prelude, location)?;
            }

            self.emit_statement_kind(
                HirStatementKind::Assign {
                    target: crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(step_local),
                    value: lowered_step.value,
                },
                location,
            )?;
        } else {
            let default_step =
                if matches!(self.type_context.get(binding_type).kind, HirTypeKind::Float) {
                    self.make_expression(
                        location,
                        HirExpressionKind::Float(1.0),
                        binding_type,
                        ValueKind::Const,
                        pre_header_region,
                    )
                } else {
                    self.make_expression(
                        location,
                        HirExpressionKind::Int(1),
                        binding_type,
                        ValueKind::Const,
                        pre_header_region,
                    )
                };

            self.emit_statement_kind(
                HirStatementKind::Assign {
                    target: crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(step_local),
                    value: default_step,
                },
                location,
            )?;
        }

        let ascending_current = self.make_expression(
            location,
            HirExpressionKind::Load(crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(
                current_local,
            )),
            binding_type,
            ValueKind::Place,
            pre_header_region,
        );
        let ascending_end = self.make_expression(
            location,
            HirExpressionKind::Load(crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(
                end_local,
            )),
            binding_type,
            ValueKind::Place,
            pre_header_region,
        );
        let ascending_value = self.make_expression(
            location,
            HirExpressionKind::BinOp {
                left: Box::new(ascending_current),
                op: HirBinOp::Le,
                right: Box::new(ascending_end),
            },
            bool_ty,
            ValueKind::RValue,
            pre_header_region,
        );
        self.emit_statement_kind(
            HirStatementKind::Assign {
                target: crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(ascending_local),
                value: ascending_value,
            },
            location,
        )?;

        // Dynamic `by` expressions still need a runtime zero check before entering the loop.
        self.emit_jump_to(
            pre_header_block,
            step_zero_check_block,
            location,
            "for.enter",
        )?;

        self.set_current_block(step_zero_check_block, location)?;
        let zero_check_region = self.current_region_or_error(location)?;
        let step_for_zero_check = self.make_expression(
            location,
            HirExpressionKind::Load(crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(
                step_local,
            )),
            binding_type,
            ValueKind::Place,
            zero_check_region,
        );
        let zero_literal = if matches!(self.type_context.get(binding_type).kind, HirTypeKind::Float)
        {
            self.make_expression(
                location,
                HirExpressionKind::Float(0.0),
                binding_type,
                ValueKind::Const,
                zero_check_region,
            )
        } else {
            self.make_expression(
                location,
                HirExpressionKind::Int(0),
                binding_type,
                ValueKind::Const,
                zero_check_region,
            )
        };
        let step_is_zero = self.make_expression(
            location,
            HirExpressionKind::BinOp {
                left: Box::new(step_for_zero_check),
                op: HirBinOp::Eq,
                right: Box::new(zero_literal),
            },
            bool_ty,
            ValueKind::RValue,
            zero_check_region,
        );
        self.emit_terminator(
            step_zero_check_block,
            HirTerminator::If {
                condition: step_is_zero,
                then_block: step_zero_panic_block,
                else_block: step_abs_check_block,
            },
            location,
        )?;
        self.log_control_flow_edge(
            step_zero_check_block,
            step_zero_panic_block,
            "for.step.zero",
        );
        self.log_control_flow_edge(
            step_zero_check_block,
            step_abs_check_block,
            "for.step.non_zero",
        );

        self.set_current_block(step_zero_panic_block, location)?;
        let panic_region = self.current_region_or_error(location)?;
        let panic_message = self.make_expression(
            location,
            HirExpressionKind::StringLiteral("Loop step cannot be zero".to_owned()),
            string_ty,
            ValueKind::Const,
            panic_region,
        );
        self.emit_terminator(
            step_zero_panic_block,
            HirTerminator::Panic {
                message: Some(panic_message),
            },
            location,
        )?;

        self.set_current_block(step_abs_check_block, location)?;
        let abs_check_region = self.current_region_or_error(location)?;
        let step_for_abs_check = self.make_expression(
            location,
            HirExpressionKind::Load(crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(
                step_local,
            )),
            binding_type,
            ValueKind::Place,
            abs_check_region,
        );
        let abs_zero_literal =
            if matches!(self.type_context.get(binding_type).kind, HirTypeKind::Float) {
                self.make_expression(
                    location,
                    HirExpressionKind::Float(0.0),
                    binding_type,
                    ValueKind::Const,
                    abs_check_region,
                )
            } else {
                self.make_expression(
                    location,
                    HirExpressionKind::Int(0),
                    binding_type,
                    ValueKind::Const,
                    abs_check_region,
                )
            };
        let step_is_negative = self.make_expression(
            location,
            HirExpressionKind::BinOp {
                left: Box::new(step_for_abs_check),
                op: HirBinOp::Lt,
                right: Box::new(abs_zero_literal),
            },
            bool_ty,
            ValueKind::RValue,
            abs_check_region,
        );
        self.emit_terminator(
            step_abs_check_block,
            HirTerminator::If {
                condition: step_is_negative,
                then_block: step_abs_negate_block,
                else_block: direction_check_block,
            },
            location,
        )?;
        self.log_control_flow_edge(step_abs_check_block, step_abs_negate_block, "for.step.neg");
        self.log_control_flow_edge(step_abs_check_block, direction_check_block, "for.step.pos");

        // Normalize explicit negative steps to magnitude first.
        self.set_current_block(step_abs_negate_block, location)?;
        let abs_negate_region = self.current_region_or_error(location)?;
        let abs_step_current = self.make_expression(
            location,
            HirExpressionKind::Load(crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(
                step_local,
            )),
            binding_type,
            ValueKind::Place,
            abs_negate_region,
        );
        let abs_zero = if matches!(self.type_context.get(binding_type).kind, HirTypeKind::Float) {
            self.make_expression(
                location,
                HirExpressionKind::Float(0.0),
                binding_type,
                ValueKind::Const,
                abs_negate_region,
            )
        } else {
            self.make_expression(
                location,
                HirExpressionKind::Int(0),
                binding_type,
                ValueKind::Const,
                abs_negate_region,
            )
        };
        let abs_negated = self.make_expression(
            location,
            HirExpressionKind::BinOp {
                left: Box::new(abs_zero),
                op: HirBinOp::Sub,
                right: Box::new(abs_step_current),
            },
            binding_type,
            ValueKind::RValue,
            abs_negate_region,
        );
        self.emit_statement_kind(
            HirStatementKind::Assign {
                target: crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(step_local),
                value: abs_negated,
            },
            location,
        )?;
        self.emit_jump_to(
            step_abs_negate_block,
            direction_check_block,
            location,
            "for.step.abs.done",
        )?;

        self.set_current_block(direction_check_block, location)?;
        let direction_check_region = self.current_region_or_error(location)?;
        let ascending_for_direction = self.make_expression(
            location,
            HirExpressionKind::Load(crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(
                ascending_local,
            )),
            bool_ty,
            ValueKind::Place,
            direction_check_region,
        );
        self.emit_terminator(
            direction_check_block,
            HirTerminator::If {
                condition: ascending_for_direction,
                then_block: header_selector_block,
                else_block: descending_negate_block,
            },
            location,
        )?;
        self.log_control_flow_edge(
            direction_check_block,
            header_selector_block,
            "for.direction.asc",
        );
        self.log_control_flow_edge(
            direction_check_block,
            descending_negate_block,
            "for.direction.desc",
        );

        // Apply direction after magnitude normalization so descending loops always decrement.
        self.set_current_block(descending_negate_block, location)?;
        let desc_negate_region = self.current_region_or_error(location)?;
        let desc_step_current = self.make_expression(
            location,
            HirExpressionKind::Load(crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(
                step_local,
            )),
            binding_type,
            ValueKind::Place,
            desc_negate_region,
        );
        let desc_zero = if matches!(self.type_context.get(binding_type).kind, HirTypeKind::Float) {
            self.make_expression(
                location,
                HirExpressionKind::Float(0.0),
                binding_type,
                ValueKind::Const,
                desc_negate_region,
            )
        } else {
            self.make_expression(
                location,
                HirExpressionKind::Int(0),
                binding_type,
                ValueKind::Const,
                desc_negate_region,
            )
        };
        let desc_negated = self.make_expression(
            location,
            HirExpressionKind::BinOp {
                left: Box::new(desc_zero),
                op: HirBinOp::Sub,
                right: Box::new(desc_step_current),
            },
            binding_type,
            ValueKind::RValue,
            desc_negate_region,
        );
        self.emit_statement_kind(
            HirStatementKind::Assign {
                target: crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(step_local),
                value: desc_negated,
            },
            location,
        )?;
        self.emit_jump_to(
            descending_negate_block,
            header_selector_block,
            location,
            "for.direction.done",
        )?;

        self.set_current_block(header_selector_block, location)?;
        let header_selector_region = self.current_region_or_error(location)?;
        let ascending_for_header = self.make_expression(
            location,
            HirExpressionKind::Load(crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(
                ascending_local,
            )),
            bool_ty,
            ValueKind::Place,
            header_selector_region,
        );
        self.emit_terminator(
            header_selector_block,
            HirTerminator::If {
                condition: ascending_for_header,
                then_block: header_ascending_block,
                else_block: header_descending_block,
            },
            location,
        )?;
        self.log_control_flow_edge(
            header_selector_block,
            header_ascending_block,
            "for.header.asc",
        );
        self.log_control_flow_edge(
            header_selector_block,
            header_descending_block,
            "for.header.desc",
        );

        // `to` / `upto` become strict vs inclusive comparators per direction branch.
        self.set_current_block(header_ascending_block, location)?;
        let header_ascending_region = self.current_region_or_error(location)?;
        let asc_current_value = self.make_expression(
            location,
            HirExpressionKind::Load(crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(
                current_local,
            )),
            binding_type,
            ValueKind::Place,
            header_ascending_region,
        );
        let asc_end_value = self.make_expression(
            location,
            HirExpressionKind::Load(crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(
                end_local,
            )),
            binding_type,
            ValueKind::Place,
            header_ascending_region,
        );
        let asc_comparison_op = match range.end_kind {
            RangeEndKind::Exclusive => HirBinOp::Lt,
            RangeEndKind::Inclusive => HirBinOp::Le,
        };
        let asc_condition = self.make_expression(
            location,
            HirExpressionKind::BinOp {
                left: Box::new(asc_current_value),
                op: asc_comparison_op,
                right: Box::new(asc_end_value),
            },
            bool_ty,
            ValueKind::RValue,
            header_ascending_region,
        );
        self.emit_terminator(
            header_ascending_block,
            HirTerminator::If {
                condition: asc_condition,
                then_block: body_block,
                else_block: exit_block,
            },
            location,
        )?;
        self.log_control_flow_edge(header_ascending_block, body_block, "for.asc.true");
        self.log_control_flow_edge(header_ascending_block, exit_block, "for.asc.false");

        self.set_current_block(header_descending_block, location)?;
        let header_descending_region = self.current_region_or_error(location)?;
        let desc_current_value = self.make_expression(
            location,
            HirExpressionKind::Load(crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(
                current_local,
            )),
            binding_type,
            ValueKind::Place,
            header_descending_region,
        );
        let desc_end_value = self.make_expression(
            location,
            HirExpressionKind::Load(crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(
                end_local,
            )),
            binding_type,
            ValueKind::Place,
            header_descending_region,
        );
        let desc_comparison_op = match range.end_kind {
            RangeEndKind::Exclusive => HirBinOp::Gt,
            RangeEndKind::Inclusive => HirBinOp::Ge,
        };
        let desc_condition = self.make_expression(
            location,
            HirExpressionKind::BinOp {
                left: Box::new(desc_current_value),
                op: desc_comparison_op,
                right: Box::new(desc_end_value),
            },
            bool_ty,
            ValueKind::RValue,
            header_descending_region,
        );
        self.emit_terminator(
            header_descending_block,
            HirTerminator::If {
                condition: desc_condition,
                then_block: body_block,
                else_block: exit_block,
            },
            location,
        )?;
        self.log_control_flow_edge(header_descending_block, body_block, "for.desc.true");
        self.log_control_flow_edge(header_descending_block, exit_block, "for.desc.false");

        self.set_current_block(body_block, location)?;
        let binding_local = self.allocate_named_local(
            binding.id.clone(),
            binding_type,
            true,
            Some(location.clone()),
        )?;
        let body_region_id = self.current_region_or_error(location)?;
        let body_current_value = self.make_expression(
            location,
            HirExpressionKind::Load(crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(
                current_local,
            )),
            binding_type,
            ValueKind::Place,
            body_region_id,
        );
        self.emit_statement_kind(
            HirStatementKind::Assign {
                target: crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(binding_local),
                value: body_current_value,
            },
            location,
        )?;

        self.push_loop_targets(exit_block, step_block);
        self.lower_statement_sequence(body)?;
        self.pop_loop_targets();

        if !self.block_has_explicit_terminator(body_block, location)? {
            self.emit_jump_to(body_block, step_block, location, "for.body.step")?;
        }

        self.set_current_block(step_block, location)?;
        let step_region = self.current_region_or_error(location)?;
        let step_current = self.make_expression(
            location,
            HirExpressionKind::Load(crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(
                current_local,
            )),
            binding_type,
            ValueKind::Place,
            step_region,
        );
        let step_delta = self.make_expression(
            location,
            HirExpressionKind::Load(crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(
                step_local,
            )),
            binding_type,
            ValueKind::Place,
            step_region,
        );
        let stepped = self.make_expression(
            location,
            HirExpressionKind::BinOp {
                left: Box::new(step_current),
                op: HirBinOp::Add,
                right: Box::new(step_delta),
            },
            binding_type,
            ValueKind::RValue,
            step_region,
        );

        self.emit_statement_kind(
            HirStatementKind::Assign {
                target: crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(current_local),
                value: stepped,
            },
            location,
        )?;
        self.emit_jump_to(step_block, header_selector_block, location, "for.backedge")?;

        self.set_current_block(exit_block, location)
    }
}
