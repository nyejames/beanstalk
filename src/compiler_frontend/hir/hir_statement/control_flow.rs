//! Control-flow lowering helpers for HIR statements.
//!
//! WHAT: lowers structured control-flow constructs into explicit CFG blocks and terminators.
//! WHY: if/match/loop lowering is the densest CFG-building logic in HIR and benefits from a
//! dedicated module boundary.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, MatchExhaustiveness};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::match_patterns::{
    MatchArm, MatchPattern, RelationalPatternOp,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::blocks::{HirBlock, HirLocal};
use crate::compiler_frontend::hir::expressions::{
    HirExpression, HirExpressionKind, HirVariantCarrier, HirVariantField, ValueKind,
};
use crate::compiler_frontend::hir::hir_builder::{HirBuilder, LoopTargets};
use crate::compiler_frontend::hir::hir_datatypes::{HirTypeKind, TypeId};
use crate::compiler_frontend::hir::ids::{BlockId, LocalId};
use crate::compiler_frontend::hir::patterns::{HirMatchArm, HirPattern, HirRelationalPatternOp};
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::hir::regions::HirRegion;
use crate::compiler_frontend::hir::statements::HirStatementKind;
use crate::compiler_frontend::hir::terminators::HirTerminator;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_hir_transformation_error;
use rustc_hash::FxHashMap;

fn lower_relational_pattern_op(op: RelationalPatternOp) -> HirRelationalPatternOp {
    match op {
        RelationalPatternOp::LessThan => HirRelationalPatternOp::LessThan,
        RelationalPatternOp::LessThanOrEqual => HirRelationalPatternOp::LessThanOrEqual,
        RelationalPatternOp::GreaterThan => HirRelationalPatternOp::GreaterThan,
        RelationalPatternOp::GreaterThanOrEqual => HirRelationalPatternOp::GreaterThanOrEqual,
    }
}

/// Recursively substitute local references in an expression with replacement expressions.
///
/// WHY: match guards are evaluated in the parent block (match terminator), before capture
/// assignments run in the arm body block. By replacing capture local references in guards
/// with direct `VariantPayloadGet` expressions on the scrutinee, guards can evaluate without
/// referencing uninitialized locals.
fn substitute_locals_in_expression(
    expr: &HirExpression,
    substitutions: &FxHashMap<LocalId, HirExpression>,
) -> HirExpression {
    let direct_match = match &expr.kind {
        HirExpressionKind::Load(HirPlace::Local(local_id))
        | HirExpressionKind::Copy(HirPlace::Local(local_id)) => {
            substitutions.get(local_id).cloned()
        }
        _ => None,
    };

    if let Some(replacement) = direct_match {
        return replacement;
    }

    let new_kind = match &expr.kind {
        HirExpressionKind::Int(_)
        | HirExpressionKind::Float(_)
        | HirExpressionKind::Bool(_)
        | HirExpressionKind::Char(_)
        | HirExpressionKind::StringLiteral(_) => expr.kind.clone(),

        HirExpressionKind::Load(place) => {
            HirExpressionKind::Load(substitute_locals_in_place(place, substitutions))
        }
        HirExpressionKind::Copy(place) => {
            HirExpressionKind::Copy(substitute_locals_in_place(place, substitutions))
        }

        HirExpressionKind::BinOp { left, op, right } => HirExpressionKind::BinOp {
            left: Box::new(substitute_locals_in_expression(left, substitutions)),
            op: *op,
            right: Box::new(substitute_locals_in_expression(right, substitutions)),
        },

        HirExpressionKind::UnaryOp { op, operand } => HirExpressionKind::UnaryOp {
            op: *op,
            operand: Box::new(substitute_locals_in_expression(operand, substitutions)),
        },

        HirExpressionKind::StructConstruct { struct_id, fields } => {
            HirExpressionKind::StructConstruct {
                struct_id: *struct_id,
                fields: fields
                    .iter()
                    .map(|(field_id, value)| {
                        (
                            *field_id,
                            substitute_locals_in_expression(value, substitutions),
                        )
                    })
                    .collect(),
            }
        }

        HirExpressionKind::Collection(elements) => HirExpressionKind::Collection(
            elements
                .iter()
                .map(|e| substitute_locals_in_expression(e, substitutions))
                .collect(),
        ),

        HirExpressionKind::Range { start, end } => HirExpressionKind::Range {
            start: Box::new(substitute_locals_in_expression(start, substitutions)),
            end: Box::new(substitute_locals_in_expression(end, substitutions)),
        },

        HirExpressionKind::TupleConstruct { elements } => HirExpressionKind::TupleConstruct {
            elements: elements
                .iter()
                .map(|e| substitute_locals_in_expression(e, substitutions))
                .collect(),
        },

        HirExpressionKind::TupleGet { tuple, index } => HirExpressionKind::TupleGet {
            tuple: Box::new(substitute_locals_in_expression(tuple, substitutions)),
            index: *index,
        },

        HirExpressionKind::ResultPropagate { result } => HirExpressionKind::ResultPropagate {
            result: Box::new(substitute_locals_in_expression(result, substitutions)),
        },

        HirExpressionKind::ResultIsOk { result } => HirExpressionKind::ResultIsOk {
            result: Box::new(substitute_locals_in_expression(result, substitutions)),
        },

        HirExpressionKind::ResultUnwrapOk { result } => HirExpressionKind::ResultUnwrapOk {
            result: Box::new(substitute_locals_in_expression(result, substitutions)),
        },

        HirExpressionKind::ResultUnwrapErr { result } => HirExpressionKind::ResultUnwrapErr {
            result: Box::new(substitute_locals_in_expression(result, substitutions)),
        },

        HirExpressionKind::BuiltinCast { kind, value } => HirExpressionKind::BuiltinCast {
            kind: *kind,
            value: Box::new(substitute_locals_in_expression(value, substitutions)),
        },

        HirExpressionKind::VariantConstruct {
            carrier,
            variant_index,
            fields,
        } => HirExpressionKind::VariantConstruct {
            carrier: carrier.clone(),
            variant_index: *variant_index,
            fields: fields
                .iter()
                .map(|field| HirVariantField {
                    name: field.name,
                    value: substitute_locals_in_expression(&field.value, substitutions),
                })
                .collect(),
        },

        HirExpressionKind::VariantPayloadGet {
            carrier,
            source,
            variant_index,
            field_index,
        } => HirExpressionKind::VariantPayloadGet {
            carrier: carrier.clone(),
            source: Box::new(substitute_locals_in_expression(source, substitutions)),
            variant_index: *variant_index,
            field_index: *field_index,
        },
    };

    HirExpression {
        id: expr.id,
        kind: new_kind,
        ty: expr.ty,
        value_kind: expr.value_kind,
        region: expr.region,
    }
}

fn substitute_locals_in_place(
    place: &HirPlace,
    substitutions: &FxHashMap<LocalId, HirExpression>,
) -> HirPlace {
    match place {
        HirPlace::Local(_) => place.clone(),
        HirPlace::Field { base, field } => HirPlace::Field {
            base: Box::new(substitute_locals_in_place(base, substitutions)),
            field: *field,
        },
        HirPlace::Index { base, index } => HirPlace::Index {
            base: Box::new(substitute_locals_in_place(base, substitutions)),
            index: Box::new(substitute_locals_in_expression(index, substitutions)),
        },
    }
}

impl<'a> HirBuilder<'a> {
    pub(super) fn lower_scoped_block_statement(
        &mut self,
        body: &[AstNode],
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let entry_block = self.current_block_id_or_error(location)?;
        let parent_region = self.current_region_or_error(location)?;
        let body_region = self.create_child_region(parent_region);
        let body_block = self.create_block(body_region, location, "scoped-block")?;

        self.emit_jump_to(entry_block, body_block, location, "block.enter")?;
        self.set_current_block(body_block, location)?;
        self.lower_statement_sequence(body)?;

        let body_tail_block = self.current_block_id_or_error(location)?;
        if self.block_has_explicit_terminator(body_tail_block, location)? {
            return self.set_current_block(body_tail_block, location);
        }

        let after_block = self.create_block(parent_region, location, "scoped-block-after")?;
        self.emit_jump_to(body_tail_block, after_block, location, "block.exit")?;
        self.set_current_block(after_block, location)
    }

    pub(super) fn lower_if_statement(
        &mut self,
        condition: &Expression,
        then_body: &[AstNode],
        else_body: Option<&[AstNode]>,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let condition_lowered = self.lower_expression(condition)?;

        for prelude in condition_lowered.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }
        let condition_block = self.current_block_id_or_error(location)?;

        let parent_region = self.current_region_or_error(location)?;
        let then_region = self.create_child_region(parent_region);
        let else_region = self.create_child_region(parent_region);
        let then_block = self.create_block(then_region, location, "if-then")?;
        let else_block = self.create_block(else_region, location, "if-else")?;

        self.emit_terminator(
            condition_block,
            HirTerminator::If {
                condition: condition_lowered.value,
                then_block,
                else_block,
            },
            location,
        )?;

        self.log_control_flow_edge(condition_block, then_block, "if.true");
        self.log_control_flow_edge(condition_block, else_block, "if.false");

        let mut terminated_anchor: Option<BlockId> = None;

        self.set_current_block(then_block, location)?;
        self.lower_statement_sequence(then_body)?;
        let then_tail_block = self.current_block_id_or_error(location)?;
        let then_terminated = self.block_has_explicit_terminator(then_tail_block, location)?;
        if then_terminated {
            terminated_anchor = Some(then_tail_block);
        }

        self.set_current_block(else_block, location)?;
        if let Some(else_nodes) = else_body {
            self.lower_statement_sequence(else_nodes)?;
        }

        let else_tail_block = self.current_block_id_or_error(location)?;
        let else_terminated = self.block_has_explicit_terminator(else_tail_block, location)?;
        if else_terminated && terminated_anchor.is_none() {
            terminated_anchor = Some(else_tail_block);
        }

        if then_terminated && else_terminated {
            // No continuation path exists after this branch.
            return self.set_current_block(terminated_anchor.unwrap_or(then_block), location);
        }

        let merge_block = self.create_block(parent_region, location, "if-merge")?;
        if !then_terminated {
            self.emit_jump_to(then_tail_block, merge_block, location, "if.then.merge")?;
        }
        if !else_terminated {
            self.emit_jump_to(else_tail_block, merge_block, location, "if.else.merge")?;
        }

        self.set_current_block(merge_block, location)
    }

    pub(super) fn lower_break_statement(
        &mut self,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let current_block = self.current_block_id_or_error(location)?;
        let targets = self.current_loop_targets_or_error("break", location)?;

        self.emit_terminator(
            current_block,
            HirTerminator::Break {
                target: targets.break_target,
            },
            location,
        )?;

        self.log_control_flow_edge(current_block, targets.break_target, "loop.break");
        Ok(())
    }

    pub(super) fn lower_continue_statement(
        &mut self,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let current_block = self.current_block_id_or_error(location)?;
        let targets = self.current_loop_targets_or_error("continue", location)?;

        self.emit_terminator(
            current_block,
            HirTerminator::Continue {
                target: targets.continue_target,
            },
            location,
        )?;

        self.log_control_flow_edge(current_block, targets.continue_target, "loop.continue");
        Ok(())
    }

    pub(super) fn lower_while_statement(
        &mut self,
        condition: &Expression,
        body: &[AstNode],
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let pre_header_block = self.current_block_id_or_error(location)?;
        let parent_region = self.current_region_or_error(location)?;

        let header_block = self.create_block(parent_region, location, "while-header")?;
        let body_region = self.create_child_region(parent_region);
        let body_block = self.create_block(body_region, location, "while-body")?;
        let exit_block = self.create_block(parent_region, location, "while-exit")?;

        self.emit_jump_to(pre_header_block, header_block, location, "while.enter")?;

        self.set_current_block(header_block, location)?;
        let lowered_condition = self.lower_expression(condition)?;
        for prelude in lowered_condition.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }
        let condition_block = self.current_block_id_or_error(location)?;

        self.emit_terminator(
            condition_block,
            HirTerminator::If {
                condition: lowered_condition.value,
                then_block: body_block,
                else_block: exit_block,
            },
            location,
        )?;

        self.log_control_flow_edge(condition_block, body_block, "while.true");
        self.log_control_flow_edge(condition_block, exit_block, "while.false");

        self.set_current_block(body_block, location)?;
        self.push_loop_targets(exit_block, header_block);
        self.lower_statement_sequence(body)?;
        self.pop_loop_targets();

        let body_tail_block = self.current_block_id_or_error(location)?;
        if !self.block_has_explicit_terminator(body_tail_block, location)? {
            self.emit_jump_to(body_tail_block, header_block, location, "while.backedge")?;
        }

        self.set_current_block(exit_block, location)
    }

    /// Lower an AST match statement into explicit CFG blocks and a `Match` terminator.
    ///
    /// WHAT: creates a block per arm (plus optional default and merge blocks), emits
    /// the `HirTerminator::Match`, then lowers each arm body and wires non-terminal
    /// arms to a shared merge block.
    /// WHY: HIR represents control flow as a flat block graph, so structured match
    /// syntax must be decomposed here. Lazy merge-block creation avoids empty blocks
    /// when every arm terminates explicitly.
    pub(super) fn lower_match_statement(
        &mut self,
        scrutinee: &Expression,
        arms: &[MatchArm],
        default: Option<&[AstNode]>,
        exhaustiveness: MatchExhaustiveness,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        match exhaustiveness {
            MatchExhaustiveness::HasDefault if default.is_none() => {
                return_hir_transformation_error!(
                    "Match marked as having a default arm but no default body was provided",
                    self.hir_error_location(location)
                );
            }
            MatchExhaustiveness::ExhaustiveChoice if default.is_some() => {
                return_hir_transformation_error!(
                    "Match marked as exhaustive choice but also provided a default arm",
                    self.hir_error_location(location)
                );
            }
            _ => {}
        }

        let lowered_scrutinee = self.lower_expression(scrutinee)?;
        for prelude in lowered_scrutinee.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }
        let current_block = self.current_block_id_or_error(location)?;
        let scrutinee_value = lowered_scrutinee.value.clone();

        let parent_region = self.current_region_or_error(location)?;
        let mut arm_blocks = Vec::with_capacity(arms.len());
        for _ in arms {
            let arm_region = self.create_child_region(parent_region);
            arm_blocks.push(self.create_block(arm_region, location, "match-arm")?);
        }

        // AST owns exhaustiveness validation; HIR only lowers the contract it receives.
        let default_block = match exhaustiveness {
            MatchExhaustiveness::HasDefault => {
                let default_region = self.create_child_region(parent_region);
                Some(self.create_block(default_region, location, "match-default")?)
            }
            MatchExhaustiveness::ExhaustiveChoice => None,
        };
        let mut merge_block = None;

        // Register capture locals before lowering guards so guard expressions can reference them.
        // WHY: guards are lowered into HirExpression here, but evaluated at runtime in the parent
        // block context. Capture locals are function-scoped in JS, so registering them early lets
        // variable reference lowering resolve capture names.
        let mut arm_capture_locals: Vec<Vec<LocalId>> = Vec::with_capacity(arms.len());
        for (index, arm) in arms.iter().enumerate() {
            let arm_block = arm_blocks[index];
            self.set_current_block(arm_block, location)?;
            let locals = self.register_match_arm_capture_locals(arm, scrutinee, location)?;
            arm_capture_locals.push(locals);
        }

        let mut hir_arms = Vec::with_capacity(arms.len() + 1);
        for (index, arm) in arms.iter().enumerate() {
            let arm_block = arm_blocks[index];
            self.set_current_block(arm_block, location)?;
            let lowered_pattern = self.lower_match_pattern(&arm.pattern, &scrutinee.data_type)?;
            let lowered_guard = match &arm.guard {
                Some(guard) => {
                    let guard_expr = self.lower_match_guard_expression(guard)?;
                    // If this arm has payload captures, substitute capture local references in the
                    // guard with direct VariantPayloadGet expressions on the scrutinee.
                    // WHY: guards are evaluated in the parent block (match terminator) before
                    // capture assignments run in the arm body. Without substitution, the borrow
                    // checker sees use-before-init on capture locals.
                    if let MatchPattern::ChoiceVariant {
                        captures,
                        tag,
                        nominal_path,
                        ..
                    } = &arm.pattern
                    {
                        if !captures.is_empty() && !arm_capture_locals[index].is_empty() {
                            let DataType::Choices { variants, .. } = &scrutinee.data_type else {
                                return_hir_transformation_error!(
                                    "Choice pattern capture used with non-choice scrutinee type",
                                    self.hir_error_location(location)
                                );
                            };
                            let choice_id =
                                self.resolve_or_create_choice_id(nominal_path, variants, location)?;
                            let region = self.current_region_or_error(location)?;
                            let mut substitutions = FxHashMap::default();
                            for (capture, &local_id) in
                                captures.iter().zip(arm_capture_locals[index].iter())
                            {
                                let field_ty = self.lower_capture_field_type(
                                    &capture.field_type,
                                    &capture.location,
                                )?;
                                let payload_get = self.make_expression(
                                    &capture.location,
                                    HirExpressionKind::VariantPayloadGet {
                                        carrier: HirVariantCarrier::Choice { choice_id },
                                        source: Box::new(scrutinee_value.clone()),
                                        variant_index: *tag,
                                        field_index: capture.field_index,
                                    },
                                    field_ty,
                                    ValueKind::RValue,
                                    region,
                                );
                                substitutions.insert(local_id, payload_get);
                            }
                            Some(substitute_locals_in_expression(&guard_expr, &substitutions))
                        } else {
                            Some(guard_expr)
                        }
                    } else {
                        Some(guard_expr)
                    }
                }
                None => None,
            };

            hir_arms.push(HirMatchArm {
                pattern: lowered_pattern,
                guard: lowered_guard,
                body: arm_block,
            });
        }

        if let Some(default_block_id) = default_block {
            hir_arms.push(HirMatchArm {
                pattern: HirPattern::Wildcard,
                guard: None,
                body: default_block_id,
            });
        }

        self.emit_terminator(
            current_block,
            HirTerminator::Match {
                scrutinee: scrutinee_value,
                arms: hir_arms,
            },
            location,
        )?;

        let mut terminated_anchor: Option<BlockId> = None;

        // Emit capture extraction assignments at the start of each arm block, then lower the body.
        for (index, arm) in arms.iter().enumerate() {
            let arm_block = arm_blocks[index];
            self.set_current_block(arm_block, location)?;

            // Temporarily restore this arm's capture bindings in locals_by_name so the body
            // lowering resolves capture names to the correct local IDs. When multiple arms have
            // captures with the same name, the last-registered local overwrites earlier ones;
            // restoring per-arm prevents use-before-init borrow checker errors.
            let mut restored_bindings = Vec::new();
            if let MatchPattern::ChoiceVariant { captures, .. } = &arm.pattern {
                for (capture, &local_id) in captures.iter().zip(arm_capture_locals[index].iter()) {
                    if let Some(binding_path) = &capture.binding_path {
                        let old = self.locals_by_name.remove(binding_path);
                        self.locals_by_name.insert(binding_path.clone(), local_id);
                        restored_bindings.push((binding_path.clone(), old));
                    }
                }
            }

            self.emit_match_arm_capture_assignments(
                arm,
                &arm_capture_locals[index],
                &lowered_scrutinee.value.clone(),
                scrutinee,
                location,
            )?;
            self.lower_statement_sequence(&arm.body)?;

            for (binding_path, old_local) in restored_bindings {
                self.locals_by_name.remove(&binding_path);
                if let Some(old) = old_local {
                    self.locals_by_name.insert(binding_path, old);
                }
            }

            let arm_tail_block = self.current_block_id_or_error(location)?;
            let arm_terminated = self.block_has_explicit_terminator(arm_tail_block, location)?;
            if arm_terminated {
                if terminated_anchor.is_none() {
                    terminated_anchor = Some(arm_tail_block);
                }
            } else {
                let merge_target =
                    self.ensure_match_merge_block(parent_region, location, &mut merge_block)?;
                self.emit_jump_to(arm_tail_block, merge_target, location, "match.arm.merge")?;
            }
        }

        if let (Some(default_block_id), Some(default_body)) = (default_block, default) {
            self.set_current_block(default_block_id, location)?;
            self.lower_statement_sequence(default_body)?;

            let default_tail_block = self.current_block_id_or_error(location)?;
            let default_terminated =
                self.block_has_explicit_terminator(default_tail_block, location)?;
            if default_terminated {
                if terminated_anchor.is_none() {
                    terminated_anchor = Some(default_tail_block);
                }
            } else {
                let merge_target =
                    self.ensure_match_merge_block(parent_region, location, &mut merge_block)?;
                self.emit_jump_to(
                    default_tail_block,
                    merge_target,
                    location,
                    "match.default.merge",
                )?;
            }
        }

        if let Some(merge_block_id) = merge_block {
            return self.set_current_block(merge_block_id, location);
        }

        if let Some(anchor_block) = terminated_anchor {
            return self.set_current_block(anchor_block, location);
        }

        return_hir_transformation_error!(
            "Match lowering produced no merge block and no terminated anchor block",
            self.hir_error_location(location)
        )
    }

    /// Lower a capture field type, handling unresolved `NamedType` by looking up type aliases.
    ///
    /// WHAT: choice payload field types may contain `NamedType` placeholders when the type checker
    /// has not yet resolved them (e.g. imported type aliases). `lower_data_type` panics on these,
    /// so this helper resolves them via `module_constants_by_name` or falls back to `String`.
    fn lower_capture_field_type(
        &mut self,
        field_type: &DataType,
        location: &SourceLocation,
    ) -> Result<TypeId, CompilerError> {
        match field_type {
            DataType::NamedType(type_name) => {
                let found = self
                    .module_constants_by_name
                    .iter()
                    .find(|(path, _)| path.name() == Some(*type_name))
                    .map(|(_, declaration)| declaration.value.data_type.clone());
                if let Some(data_type) = found {
                    return self.lower_data_type(&data_type, location);
                }
                Ok(self.intern_type_kind(HirTypeKind::String))
            }
            _ => self.lower_data_type(field_type, location),
        }
    }

    /// Register capture locals for one match arm so guards and bodies can reference them.
    ///
    /// WHAT: for each capture in a `ChoiceVariant` pattern, allocates a block local and registers
    /// it in `locals_by_name` so that `lower_expression` can resolve capture references.
    /// WHY: guards are lowered before body statements but may reference captures; locals must be
    /// visible during both guard and body lowering.
    fn register_match_arm_capture_locals(
        &mut self,
        arm: &MatchArm,
        scrutinee_ast: &Expression,
        location: &SourceLocation,
    ) -> Result<Vec<LocalId>, CompilerError> {
        let MatchPattern::ChoiceVariant {
            nominal_path,
            captures,
            ..
        } = &arm.pattern
        else {
            return Ok(Vec::new());
        };

        if captures.is_empty() {
            return Ok(Vec::new());
        }

        let DataType::Choices { variants, .. } = &scrutinee_ast.data_type else {
            return_hir_transformation_error!(
                "Choice pattern capture used with non-choice scrutinee type",
                self.hir_error_location(location)
            );
        };

        let _choice_id = self.resolve_or_create_choice_id(nominal_path, variants, location)?;
        let region = self.current_region_or_error(location)?;

        let mut local_ids = Vec::with_capacity(captures.len());
        for capture in captures {
            let field_ty = self.lower_capture_field_type(&capture.field_type, &capture.location)?;
            let local_id = self.allocate_local_id();

            let binding_path = match &capture.binding_path {
                Some(path) => path.clone(),
                None => {
                    return_hir_transformation_error!(
                        format!(
                            "Capture '{}' is missing its binding path; AST branching should have set it",
                            self.string_table.resolve(capture.field_name)
                        ),
                        self.hir_error_location(&capture.location)
                    );
                }
            };

            let block_id = self.current_block_id_or_error(&capture.location)?;
            self.register_local_in_block(
                block_id,
                HirLocal {
                    id: local_id,
                    ty: field_ty,
                    mutable: false,
                    region,
                    source_info: Some(capture.location.clone()),
                },
                &capture.location,
            )?;

            self.locals_by_name.insert(binding_path.clone(), local_id);
            self.side_table.bind_local_name(local_id, binding_path);
            local_ids.push(local_id);
        }

        Ok(local_ids)
    }

    /// Emit `Assign` statements that materialize choice payload captures at the start of an arm block.
    ///
    /// WHAT: for each capture, emits `Assign(local, VariantPayloadGet(...))` using the already-
    /// registered local from `register_match_arm_capture_locals`.
    /// WHY: keeping extraction in the arm block (not the JS match condition) preserves the HIR
    /// contract: the match terminator only dispatches; side effects live in blocks.
    fn emit_match_arm_capture_assignments(
        &mut self,
        arm: &MatchArm,
        capture_locals: &[LocalId],
        scrutinee_value: &HirExpression,
        scrutinee_ast: &Expression,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let MatchPattern::ChoiceVariant {
            nominal_path,
            tag,
            captures,
            ..
        } = &arm.pattern
        else {
            return Ok(());
        };

        if captures.is_empty() {
            return Ok(());
        }

        debug_assert_eq!(
            captures.len(),
            capture_locals.len(),
            "capture count must match registered local count"
        );

        let DataType::Choices { variants, .. } = &scrutinee_ast.data_type else {
            return_hir_transformation_error!(
                "Choice pattern capture used with non-choice scrutinee type",
                self.hir_error_location(location)
            );
        };

        let choice_id = self.resolve_or_create_choice_id(nominal_path, variants, location)?;
        let region = self.current_region_or_error(location)?;

        for (capture, &local_id) in captures.iter().zip(capture_locals.iter()) {
            let field_ty = self.lower_capture_field_type(&capture.field_type, &capture.location)?;
            let payload_get = self.make_expression(
                &capture.location,
                HirExpressionKind::VariantPayloadGet {
                    carrier: HirVariantCarrier::Choice { choice_id },
                    source: Box::new(scrutinee_value.clone()),
                    variant_index: *tag,
                    field_index: capture.field_index,
                },
                field_ty,
                ValueKind::RValue,
                region,
            );

            self.emit_statement_kind(
                HirStatementKind::Assign {
                    target: HirPlace::Local(local_id),
                    value: payload_get,
                },
                &capture.location,
            )?;
        }

        Ok(())
    }

    /// Validate and lower a match arm pattern, rejecting non-literal expressions.
    ///
    /// WHAT: lowers the pattern expression and verifies it has no side-effect prelude,
    /// is a compile-time constant, and is one of the supported literal kinds.
    /// WHY: match dispatch relies on constant comparison values; catching non-literals
    /// here produces clear HIR-stage errors instead of miscompilation.
    fn lower_match_literal_pattern(
        &mut self,
        condition: &Expression,
    ) -> Result<HirExpression, CompilerError> {
        let lowered_pattern = self.lower_expression(condition)?;
        if !lowered_pattern.prelude.is_empty() {
            return_hir_transformation_error!(
                "Match arm pattern lowering produced side-effect statements; only literal patterns are supported",
                self.hir_error_location(&condition.location)
            );
        }

        if lowered_pattern.value.value_kind != ValueKind::Const {
            return_hir_transformation_error!(
                "Match arm patterns must be compile-time literals",
                self.hir_error_location(&condition.location)
            );
        }

        if !matches!(
            lowered_pattern.value.kind,
            HirExpressionKind::Int(_)
                | HirExpressionKind::Float(_)
                | HirExpressionKind::Bool(_)
                | HirExpressionKind::Char(_)
                | HirExpressionKind::StringLiteral(_)
        ) {
            return_hir_transformation_error!(
                "Match arm patterns currently support only literal int/float/bool/char/string values",
                self.hir_error_location(&condition.location)
            );
        }

        Ok(lowered_pattern.value)
    }

    /// Lower an AST match pattern into its HIR counterpart.
    fn lower_match_pattern(
        &mut self,
        pattern: &MatchPattern,
        subject_type: &DataType,
    ) -> Result<HirPattern, CompilerError> {
        match pattern {
            MatchPattern::Literal(expression) => {
                let lowered = self.lower_match_literal_pattern(expression)?;
                Ok(HirPattern::Literal(lowered))
            }

            MatchPattern::Wildcard { .. } => Ok(HirPattern::Wildcard),

            MatchPattern::Relational { op, value, .. } => {
                let lowered_value = self.lower_match_literal_pattern(value)?;

                Ok(HirPattern::Relational {
                    op: lower_relational_pattern_op(*op),
                    value: lowered_value,
                })
            }
            MatchPattern::ChoiceVariant {
                nominal_path,
                tag,
                location,
                ..
            } => {
                let DataType::Choices { variants, .. } = subject_type else {
                    return_hir_transformation_error!(
                        "ChoiceVariant pattern used with non-choice scrutinee type",
                        self.hir_error_location(location)
                    );
                };
                let choice_id =
                    self.resolve_or_create_choice_id(nominal_path, variants, location)?;
                Ok(HirPattern::ChoiceVariant {
                    choice_id,
                    variant_index: *tag,
                })
            }
        }
    }

    /// Lower a match arm guard and ensure it remains a pure boolean expression.
    fn lower_match_guard_expression(
        &mut self,
        guard: &Expression,
    ) -> Result<HirExpression, CompilerError> {
        let lowered_guard = self.lower_expression(guard)?;
        if !lowered_guard.prelude.is_empty() {
            return_hir_transformation_error!(
                "Match arm guard lowering produced side-effect statements; guards must stay pure boolean expressions",
                self.hir_error_location(&guard.location)
            );
        }

        let HirTypeKind::Bool = self.type_context.get(lowered_guard.value.ty).kind else {
            return_hir_transformation_error!(
                "Match arm guards must lower to Bool expressions",
                self.hir_error_location(&guard.location)
            );
        };

        Ok(lowered_guard.value)
    }

    /// Lazily create the shared merge block on the first non-terminal arm that needs it.
    fn ensure_match_merge_block(
        &mut self,
        region: crate::compiler_frontend::hir::ids::RegionId,
        location: &SourceLocation,
        merge_block: &mut Option<BlockId>,
    ) -> Result<BlockId, CompilerError> {
        if let Some(existing) = *merge_block {
            return Ok(existing);
        }

        let created = self.create_block(region, location, "match-merge")?;
        *merge_block = Some(created);
        Ok(created)
    }

    pub(crate) fn create_child_region(
        &mut self,
        parent: crate::compiler_frontend::hir::ids::RegionId,
    ) -> crate::compiler_frontend::hir::ids::RegionId {
        let region_id = self.allocate_region_id();
        self.push_region(HirRegion::lexical(region_id, Some(parent)));
        region_id
    }

    pub(crate) fn create_block(
        &mut self,
        region: crate::compiler_frontend::hir::ids::RegionId,
        source_location: &SourceLocation,
        label: &str,
    ) -> Result<BlockId, CompilerError> {
        let block = HirBlock {
            id: self.allocate_block_id(),
            region,
            locals: vec![],
            statements: vec![],
            terminator: HirTerminator::Panic { message: None },
        };

        self.side_table.map_block(source_location, &block);
        self.log_block_created(block.id, label, source_location);

        let id = block.id;
        self.push_block(block);
        Ok(id)
    }

    pub(super) fn expression_from_return_values(
        &mut self,
        values: &[HirExpression],
        location: &SourceLocation,
    ) -> Result<HirExpression, CompilerError> {
        let region = self.current_region_or_error(location)?;

        match values {
            [] => Ok(self.unit_expression(location, region)),
            [single] => Ok(single.to_owned()),
            many => {
                let field_types = many.iter().map(|value| value.ty).collect::<Vec<_>>();
                let tuple_type = self.intern_type_kind(HirTypeKind::Tuple {
                    fields: field_types,
                });

                Ok(self.make_expression(
                    location,
                    HirExpressionKind::TupleConstruct {
                        elements: many.to_vec(),
                    },
                    tuple_type,
                    ValueKind::RValue,
                    region,
                ))
            }
        }
    }

    pub(crate) fn emit_jump_to(
        &mut self,
        from_block: BlockId,
        target: BlockId,
        location: &SourceLocation,
        edge_label: &str,
    ) -> Result<(), CompilerError> {
        self.emit_jump_with_args(from_block, target, vec![], location, edge_label)
    }

    pub(crate) fn emit_jump_with_args(
        &mut self,
        from_block: BlockId,
        target: BlockId,
        args: Vec<crate::compiler_frontend::hir::ids::LocalId>,
        location: &SourceLocation,
        edge_label: &str,
    ) -> Result<(), CompilerError> {
        self.emit_terminator(from_block, HirTerminator::Jump { target, args }, location)?;

        self.log_control_flow_edge(from_block, target, edge_label);
        Ok(())
    }

    pub(crate) fn emit_terminator(
        &mut self,
        block_id: BlockId,
        terminator: HirTerminator,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        self.log_terminator_emitted(block_id, &terminator, location);
        self.set_block_terminator(block_id, terminator, location)
    }

    pub(super) fn push_loop_targets(&mut self, break_target: BlockId, continue_target: BlockId) {
        self.loop_targets.push(LoopTargets {
            break_target,
            continue_target,
        });
    }

    pub(super) fn pop_loop_targets(&mut self) {
        let _ = self.loop_targets.pop();
    }

    pub(super) fn current_loop_targets_or_error(
        &self,
        keyword: &str,
        location: &SourceLocation,
    ) -> Result<LoopTargets, CompilerError> {
        let Some(targets) = self.loop_targets.last().copied() else {
            return_hir_transformation_error!(
                format!(
                    "'{}' reached HIR lowering without an active loop context",
                    keyword
                ),
                self.hir_error_location(location)
            );
        };

        Ok(targets)
    }

    pub(crate) fn is_unit_type(&self, ty: TypeId) -> bool {
        matches!(self.type_context.get(ty).kind, HirTypeKind::Unit)
    }
}
