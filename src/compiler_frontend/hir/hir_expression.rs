//! HIR Expression Lowering
//!
//! Lowers typed AST expressions into HIR expressions and statement preludes.
//! This file contains the high-level dispatcher and shared expression utilities on `HirBuilder`.
//!
//! ## Diagnostic boundary
//!
//! `CompilerError` / `return_hir_transformation_error!` in this module means an internal
//! HIR transformation or lowering invariant failure only. Normal user-facing source failures
//! must be emitted as `CompilerDiagnostic` from AST or earlier stages.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{
    BuiltinCastKind, Expression, ExpressionKind,
    FallibleCarrierVariant as AstFallibleCarrierVariant, FallibleHandling, Operator,
};
use crate::compiler_frontend::ast::statements::value_production::types::ValueBlock;
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::datatypes::generic_identity_bridge::TypeIdentityKey;
use crate::compiler_frontend::datatypes::ids::TypeId as FrontendTypeId;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::external_packages::CallTarget;
use crate::compiler_frontend::hir::blocks::{HirBlock, HirLocal};
use crate::compiler_frontend::hir::expressions::{
    HirBuiltinCastKind, HirExpression, HirExpressionKind, HirVariantCarrier, HirVariantField,
    ValueKind,
};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_side_table::HirLocalOriginKind;
use crate::compiler_frontend::hir::ids::{LocalId, RegionId};
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::hir::statements::{HirStatement, HirStatementKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::format_compile_time_paths;
use crate::hir_log;
use crate::return_hir_transformation_error;

mod calls;
mod fallible;
mod literals;
mod operators;
mod option_propagation;
mod places;
mod runtime;
mod templates;
mod types;

pub(crate) use self::calls::DynamicTraitMethodCallLoweringInput;
pub(crate) use self::fallible::ExternalFallibleCallLoweringInput;

#[derive(Debug, Clone)]
pub(crate) struct LoweredExpression {
    // WHAT: Statements that must execute before evaluating `value`.
    // WHY: HIR requires expression side effects to be linearized into explicit statements.
    pub prelude: Vec<HirStatement>,
    pub value: HirExpression,
}

impl<'a> HirBuilder<'a> {
    // -------------------------
    //  Expression Lowering
    // -------------------------

    // WHAT: lowers one typed AST expression into a linearized HIR prelude/value pair.
    // WHY: HIR cannot keep nested side effects inside expressions, so every entry point must
    //      return both the value and any statements required to produce it.
    pub(crate) fn lower_expression(
        &mut self,
        expr: &Expression,
    ) -> Result<LoweredExpression, CompilerError> {
        self.log_expression_input(expr);

        let lowered = match &expr.kind {
            ExpressionKind::ChoiceConstruct {
                nominal_path,
                variant: _,
                tag,
                fields,
            } => {
                let choice_id = if let Some(TypeIdentityKey::GenericInstance(key)) = self
                    .type_environment
                    .type_id_to_type_identity_key(expr.type_id)
                {
                    self.resolve_or_register_generic_choice(
                        &key,
                        nominal_path,
                        expr.type_id,
                        &expr.location,
                    )?
                } else {
                    self.resolve_choice_id(nominal_path, &expr.location)?
                };
                let region = self.current_region_or_error(&expr.location)?;
                let ty = self.lower_type_id(expr.type_id, &expr.location)?;

                let mut prelude = Vec::new();
                let mut hir_fields = Vec::with_capacity(fields.len());

                for field in fields {
                    let value =
                        self.lower_child_expression_for_parent(&mut prelude, &field.value)?;
                    hir_fields.push(HirVariantField {
                        name: field.id.name(),
                        value,
                    });
                }

                let value_kind = if fields.iter().all(|f| f.value.is_compile_time_constant()) {
                    ValueKind::Const
                } else {
                    ValueKind::RValue
                };

                Ok(LoweredExpression {
                    prelude,
                    value: self.make_expression(
                        &expr.location,
                        HirExpressionKind::VariantConstruct {
                            carrier: HirVariantCarrier::Choice { choice_id },
                            variant_index: *tag,
                            fields: hir_fields,
                        },
                        ty,
                        value_kind,
                        region,
                    ),
                })
            }

            ExpressionKind::Int(value) => self.lower_literal_expression(
                &expr.location,
                expr.type_id,
                HirExpressionKind::Int(*value),
            ),

            ExpressionKind::Float(value) => self.lower_literal_expression(
                &expr.location,
                expr.type_id,
                HirExpressionKind::Float(*value),
            ),

            ExpressionKind::Bool(value) => self.lower_literal_expression(
                &expr.location,
                expr.type_id,
                HirExpressionKind::Bool(*value),
            ),

            ExpressionKind::Char(value) => self.lower_literal_expression(
                &expr.location,
                expr.type_id,
                HirExpressionKind::Char(*value),
            ),

            ExpressionKind::StringSlice(value) => self.lower_literal_expression(
                &expr.location,
                expr.type_id,
                HirExpressionKind::StringLiteral(self.string_table.resolve(*value).to_owned()),
            ),

            ExpressionKind::Path(compile_time_paths) => {
                // Compile-time path values lower to string literals in HIR.
                // Formatting applies #origin for root-based paths and trailing
                // slash for directories through the shared path formatter.
                let path_string = format_compile_time_paths(
                    compile_time_paths,
                    &self.path_format_config,
                    self.string_table,
                );

                self.lower_literal_expression(
                    &expr.location,
                    expr.type_id,
                    HirExpressionKind::StringLiteral(path_string),
                )
            }

            ExpressionKind::Reference(name) => {
                self.lower_reference_expression(name, expr.type_id, &expr.location)
            }

            ExpressionKind::Copy(place) => {
                let region = self.current_region_or_error(&expr.location)?;
                let (prelude, place) = self.lower_ast_node_to_place(place)?;
                let ty = self.lower_type_id(expr.type_id, &expr.location)?;

                Ok(LoweredExpression {
                    prelude,
                    value: self.make_expression(
                        &expr.location,
                        HirExpressionKind::Copy(place),
                        ty,
                        ValueKind::RValue,
                        region,
                    ),
                })
            }

            ExpressionKind::Runtime(nodes) => {
                self.lower_runtime_rpn_expression(nodes, &expr.location, expr.type_id)
            }

            ExpressionKind::FunctionCall {
                name,
                args,
                result_type_ids,
            } => {
                let function_id = self.resolve_function_id_or_error(name, &expr.location)?;
                self.lower_call_expression(
                    CallTarget::UserFunction(function_id),
                    args,
                    result_type_ids,
                    &expr.location,
                )
            }

            ExpressionKind::HandledFallibleFunctionCall {
                name,
                args,
                result_type_ids,
                handling,
            } => {
                let function_id = self.resolve_function_id_or_error(name, &expr.location)?;
                self.lower_handled_fallible_call_expression(
                    CallTarget::UserFunction(function_id),
                    args,
                    result_type_ids,
                    handling,
                    true,
                    &expr.location,
                )
            }

            ExpressionKind::HandledFallibleHostFunctionCall {
                id,
                args,
                result_type_ids,
                error_type_id,
                handling,
            } => self.lower_handled_external_fallible_call_expression(
                ExternalFallibleCallLoweringInput {
                    id: *id,
                    args,
                    result_type_ids,
                    error_type_id: *error_type_id,
                    handling,
                    value_required: true,
                    location: &expr.location,
                },
            ),

            ExpressionKind::BuiltinCast { kind, value } => {
                self.lower_builtin_cast_expression(*kind, value, &expr.location, expr.type_id)
            }

            ExpressionKind::FallibleCarrierConstruct { variant, value } => {
                let mut prelude = Vec::new();
                let lowered_value = self.lower_child_expression_for_parent(&mut prelude, value)?;
                let region = self.current_region_or_error(&expr.location)?;
                let ty = self.lower_type_id(expr.type_id, &expr.location)?;
                let variant_index = match variant {
                    AstFallibleCarrierVariant::Success => 0,
                    AstFallibleCarrierVariant::Error => 1,
                };
                let value_name = self.string_table.intern("value");

                Ok(LoweredExpression {
                    prelude,
                    value: self.make_expression(
                        &expr.location,
                        HirExpressionKind::VariantConstruct {
                            carrier: HirVariantCarrier::Fallible,
                            variant_index,
                            fields: vec![HirVariantField {
                                name: Some(value_name),
                                value: lowered_value,
                            }],
                        },
                        ty,
                        ValueKind::RValue,
                        region,
                    ),
                })
            }

            ExpressionKind::HandledFallibleExpression { value, handling } => self
                .lower_handled_fallible_expression(value, handling, &expr.location, expr.type_id),

            ExpressionKind::OptionPropagation { value } => {
                self.lower_option_expression_to_present_value(value, &expr.location)
            }

            ExpressionKind::HostFunctionCall {
                id: host_id,
                args,
                result_type_ids,
            } => self.lower_call_expression(
                CallTarget::ExternalFunction(*host_id),
                args,
                result_type_ids,
                &expr.location,
            ),

            ExpressionKind::Collection(items) => {
                let mut prelude = Vec::new();
                let mut lowered_items = Vec::with_capacity(items.len());

                for item in items {
                    let lowered_item =
                        self.lower_child_expression_for_parent(&mut prelude, item)?;
                    lowered_items.push(lowered_item);
                }

                let region = self.current_region_or_error(&expr.location)?;
                let ty = self.lower_type_id(expr.type_id, &expr.location)?;

                Ok(LoweredExpression {
                    prelude,
                    value: self.make_expression(
                        &expr.location,
                        HirExpressionKind::Collection(lowered_items),
                        ty,
                        ValueKind::RValue,
                        region,
                    ),
                })
            }

            ExpressionKind::Range(start, end) => {
                let mut prelude = Vec::new();
                let lowered_start = self.lower_child_expression_for_parent(&mut prelude, start)?;
                let lowered_end = self.lower_child_expression_for_parent(&mut prelude, end)?;

                let region = self.current_region_or_error(&expr.location)?;
                let ty = self.lower_type_id(expr.type_id, &expr.location)?;

                Ok(LoweredExpression {
                    prelude,
                    value: self.make_expression(
                        &expr.location,
                        HirExpressionKind::Range {
                            start: Box::new(lowered_start),
                            end: Box::new(lowered_end),
                        },
                        ty,
                        ValueKind::RValue,
                        region,
                    ),
                })
            }

            ExpressionKind::StructInstance(args) => {
                // INVARIANT: const-record runtime use should have been rejected in AST.
                // If a const record reaches HIR struct lowering, push validation earlier
                // instead of converting this into a user diagnostic here.
                if expr.is_const_record_value() {
                    return_hir_transformation_error!(
                        "HIR invariant: Const record reached runtime HIR struct lowering; field access should select a member before HIR generation",
                        self.hir_error_location(&expr.location)
                    );
                }

                let Some(nominal_path) = self.type_environment.nominal_path(expr.type_id) else {
                    return_hir_transformation_error!(
                        "Struct instance reached HIR lowering without a nominal struct identity",
                        self.hir_error_location(&expr.location)
                    );
                };
                let nominal_path = nominal_path.to_owned();
                let struct_id = if let Some(TypeIdentityKey::GenericInstance(key)) = self
                    .type_environment
                    .type_id_to_type_identity_key(expr.type_id)
                {
                    self.resolve_or_register_generic_struct(
                        &key,
                        &nominal_path,
                        expr.type_id,
                        &expr.location,
                    )?
                } else {
                    self.resolve_struct_id_from_nominal_path(&nominal_path, &expr.location)?
                };
                let mut prelude = Vec::new();
                let mut fields = Vec::with_capacity(args.len());

                for arg in args {
                    let field_id =
                        self.resolve_field_id_or_error(struct_id, &arg.id, &expr.location)?;
                    let lowered_value =
                        self.lower_child_expression_for_parent(&mut prelude, &arg.value)?;
                    fields.push((field_id, lowered_value));
                }

                let region = self.current_region_or_error(&expr.location)?;
                let ty = self.lower_type_id(expr.type_id, &expr.location)?;

                Ok(LoweredExpression {
                    prelude,
                    value: self.make_expression(
                        &expr.location,
                        HirExpressionKind::StructConstruct { struct_id, fields },
                        ty,
                        ValueKind::RValue,
                        region,
                    ),
                })
            }

            ExpressionKind::Template(template) => {
                self.lower_runtime_template_expression(template.as_ref(), &expr.location)
            }

            // Lower the inner value and override the HIR type with the declared
            // coercion target. Numeric coercions are resolved by the code generation
            // backend based on the type annotation. Option coercions materialize
            // `some(value)` here so backends see the real runtime carrier.
            ExpressionKind::Coerced { value, .. } => {
                let mut prelude = Vec::new();
                let mut lowered_value =
                    self.lower_child_expression_for_parent(&mut prelude, value)?;
                let coerced_ty = self.lower_type_id(expr.type_id, &expr.location)?;
                if self.type_environment.option_inner_type(expr.type_id) == Some(lowered_value.ty) {
                    let value_name = self.string_table.intern("value");
                    let region = lowered_value.region;
                    lowered_value = self.make_expression(
                        &expr.location,
                        HirExpressionKind::VariantConstruct {
                            carrier: HirVariantCarrier::Option,
                            variant_index: 1,
                            fields: vec![HirVariantField {
                                name: Some(value_name),
                                value: lowered_value,
                            }],
                        },
                        coerced_ty,
                        ValueKind::RValue,
                        region,
                    );
                    return Ok(LoweredExpression {
                        prelude,
                        value: lowered_value,
                    });
                }

                lowered_value.ty = coerced_ty;
                Ok(LoweredExpression {
                    prelude,
                    value: lowered_value,
                })
            }

            ExpressionKind::ConstructDynamicTraitValue { value, coercion } => {
                let mut prelude = Vec::new();
                let lowered_value = self.lower_child_expression_for_parent(&mut prelude, value)?;
                let region = self.current_region_or_error(&expr.location)?;
                let ty = self.lower_type_id(expr.type_id, &expr.location)?;

                Ok(LoweredExpression {
                    prelude,
                    value: self.make_expression(
                        &expr.location,
                        HirExpressionKind::ConstructDynamicTraitValue {
                            value: Box::new(lowered_value),
                            trait_id: coercion.target_trait_id,
                            evidence_id: coercion.selected_evidence_id,
                        },
                        ty,
                        ValueKind::RValue,
                        region,
                    ),
                })
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

            ExpressionKind::ValueBlock { block } => {
                self.lower_value_block(block, &expr.location, expr.type_id)
            }

            ExpressionKind::NoValue => {
                let region = self.current_region_or_error(&expr.location)?;
                Ok(LoweredExpression {
                    prelude: vec![],
                    value: self.unit_expression(&expr.location, region),
                })
            }

            ExpressionKind::OptionNone => {
                let region = self.current_region_or_error(&expr.location)?;
                let ty = self.lower_type_id(expr.type_id, &expr.location)?;
                Ok(LoweredExpression {
                    prelude: vec![],
                    value: self.make_expression(
                        &expr.location,
                        HirExpressionKind::VariantConstruct {
                            carrier: HirVariantCarrier::Option,
                            variant_index: 0,
                            fields: vec![],
                        },
                        ty,
                        ValueKind::RValue,
                        region,
                    ),
                })
            }
        }?;

        self.log_expression_output(expr, &lowered.value);
        Ok(lowered)
    }

    /// Lower an expression and emit its sequencing work into the active block immediately.
    ///
    /// WHAT: turns expression preludes into current-block statements and, when the expression is
    /// postfix-propagated, emits the success/error HIR edge before returning the unwrapped success
    /// payload.
    /// WHY: nested `expr!` is control flow. Compound expressions and call arguments need a value
    /// to continue with, but the error edge must be visible in the CFG instead of being hidden in
    /// an expression-only propagation helper.
    pub(crate) fn lower_expression_value_to_current_block(
        &mut self,
        expr: &Expression,
    ) -> Result<HirExpression, CompilerError> {
        if let Some(success_value) =
            self.lower_fallible_expression_to_success_value(expr, &expr.location)?
        {
            return Ok(success_value);
        }

        let lowered = self.lower_expression(expr)?;
        for prelude in lowered.prelude {
            self.emit_statement_to_current_block(prelude, &expr.location)?;
        }

        Ok(lowered.value)
    }

    /// Lower a child expression while preserving `lower_expression`'s prelude-returning contract.
    ///
    /// WHAT: ordinary children keep contributing to the parent's pending prelude, while children
    /// that need active CFG mutation first flush that pending prelude into the current block.
    /// WHY: `lower_expression` must still be usable by tests and pure expression callers as a
    /// linearization API, but `expr!` and short-circuit control flow cannot stay hidden inside a
    /// returned expression tree.
    pub(crate) fn lower_child_expression_for_parent(
        &mut self,
        pending_prelude: &mut Vec<HirStatement>,
        expr: &Expression,
    ) -> Result<HirExpression, CompilerError> {
        if self.expression_needs_current_block_lowering(expr) {
            for prelude in pending_prelude.drain(..) {
                self.emit_statement_to_current_block(prelude, &expr.location)?;
            }

            return self.lower_expression_value_to_current_block(expr);
        }

        let lowered = self.lower_expression(expr)?;
        pending_prelude.extend(lowered.prelude);
        Ok(lowered.value)
    }

    pub(crate) fn expression_needs_current_block_lowering(&self, expr: &Expression) -> bool {
        match &expr.kind {
            ExpressionKind::HandledFallibleFunctionCall { args, handling, .. }
            | ExpressionKind::HandledFallibleHostFunctionCall { args, handling, .. } => {
                matches!(handling, FallibleHandling::Propagate)
                    || args
                        .iter()
                        .any(|arg| self.expression_needs_current_block_lowering(&arg.value))
            }
            ExpressionKind::HandledFallibleExpression { value, handling } => {
                matches!(handling, FallibleHandling::Propagate)
                    || self.expression_needs_current_block_lowering(value)
            }
            ExpressionKind::OptionPropagation { .. } => true,
            ExpressionKind::FunctionCall { args, .. }
            | ExpressionKind::HostFunctionCall { args, .. } => args
                .iter()
                .any(|arg| self.expression_needs_current_block_lowering(&arg.value)),
            ExpressionKind::BuiltinCast { value, .. }
            | ExpressionKind::FallibleCarrierConstruct { value, .. }
            | ExpressionKind::Coerced { value, .. }
            | ExpressionKind::ConstructDynamicTraitValue { value, .. } => {
                self.expression_needs_current_block_lowering(value)
            }
            ExpressionKind::Copy(value) => self.ast_node_needs_current_block_lowering(value),
            ExpressionKind::Collection(items) => items
                .iter()
                .any(|item| self.expression_needs_current_block_lowering(item)),
            ExpressionKind::Range(start, end) => {
                self.expression_needs_current_block_lowering(start)
                    || self.expression_needs_current_block_lowering(end)
            }
            ExpressionKind::StructInstance(fields)
            | ExpressionKind::ChoiceConstruct { fields, .. } => fields
                .iter()
                .any(|field| self.expression_needs_current_block_lowering(&field.value)),
            ExpressionKind::Runtime(nodes) => nodes
                .iter()
                .any(|node| self.ast_node_needs_current_block_lowering(node)),
            ExpressionKind::Template(template) => {
                template.render_plan.is_some() || template.control_flow.is_some()
            }
            ExpressionKind::ValueBlock { .. } => true,

            ExpressionKind::Int(_)
            | ExpressionKind::Float(_)
            | ExpressionKind::Bool(_)
            | ExpressionKind::Char(_)
            | ExpressionKind::StringSlice(_)
            | ExpressionKind::Path(_)
            | ExpressionKind::Reference(_)
            | ExpressionKind::Function(_, _)
            | ExpressionKind::StructDefinition(_)
            | ExpressionKind::NoValue
            | ExpressionKind::OptionNone => false,
        }
    }

    // -------------------------
    //  Value-Block Lowering
    // -------------------------

    /// Lowers a value-producing control-flow block into CFG statements.
    ///
    /// WHAT: dispatches on the value-block kind (currently only `If`) and builds the
    ///       prelude statements + result value needed by the expression lowering contract.
    /// WHY: value blocks are expressions that build control flow; they must return a
    ///      `LoweredExpression` so the caller can emit their prelude and use their value.
    pub(super) fn lower_value_block(
        &mut self,
        block: &ValueBlock,
        location: &SourceLocation,
        result_type_id: TypeId,
    ) -> Result<LoweredExpression, CompilerError> {
        match block {
            ValueBlock::If(value_if) => {
                self.lower_value_block_if(value_if, location, result_type_id)
            }
            ValueBlock::Match(value_match) => {
                self.lower_value_block_match(value_match, location, result_type_id)
            }
            ValueBlock::Catch(value_catch) => self
                .lower_expression_value_to_current_block(&value_catch.handled_value)
                .map(|value| LoweredExpression {
                    prelude: vec![],
                    value,
                }),
        }
    }

    pub(crate) fn ast_node_needs_current_block_lowering(&self, node: &AstNode) -> bool {
        match &node.kind {
            NodeKind::Rvalue(expr) => self.expression_needs_current_block_lowering(expr),
            NodeKind::Operator(Operator::And | Operator::Or) => true,
            NodeKind::HandledFallibleFunctionCall { args, handling, .. }
            | NodeKind::HandledFallibleHostFunctionCall { args, handling, .. } => {
                matches!(handling, FallibleHandling::Propagate)
                    || args
                        .iter()
                        .any(|arg| self.expression_needs_current_block_lowering(&arg.value))
            }
            NodeKind::FunctionCall { args, .. } | NodeKind::HostFunctionCall { args, .. } => args
                .iter()
                .any(|arg| self.expression_needs_current_block_lowering(&arg.value)),
            NodeKind::MethodCall { receiver, args, .. }
            | NodeKind::DynamicTraitMethodCall { receiver, args, .. }
            | NodeKind::CollectionBuiltinCall { receiver, args, .. } => {
                self.ast_node_needs_current_block_lowering(receiver)
                    || args
                        .iter()
                        .any(|arg| self.expression_needs_current_block_lowering(&arg.value))
            }
            NodeKind::FieldAccess { base, .. } => self.ast_node_needs_current_block_lowering(base),
            _ => false,
        }
    }

    // -------------------------
    //  Builtin Casts
    // -------------------------

    fn lower_builtin_cast_expression(
        &mut self,
        kind: BuiltinCastKind,
        value: &Expression,
        location: &SourceLocation,
        result_type_id: FrontendTypeId,
    ) -> Result<LoweredExpression, CompilerError> {
        let mut prelude = Vec::new();
        let lowered_value = self.lower_child_expression_for_parent(&mut prelude, value)?;
        let region = self.current_region_or_error(location)?;
        let ty = self.lower_type_id(result_type_id, location)?;
        let hir_kind = match kind {
            BuiltinCastKind::Int => HirBuiltinCastKind::Int,
            BuiltinCastKind::Float => HirBuiltinCastKind::Float,
        };

        Ok(LoweredExpression {
            prelude,
            value: self.make_expression(
                location,
                HirExpressionKind::BuiltinCast {
                    kind: hir_kind,
                    value: Box::new(lowered_value),
                },
                ty,
                ValueKind::RValue,
                region,
            ),
        })
    }

    // -------------------------
    //  Statement Emission
    // -------------------------

    // WHAT: appends a prebuilt statement to the current block.
    // WHY: expression helpers sometimes manufacture statements outside the main statement
    //      dispatcher but still need to preserve explicit execution order.
    pub(crate) fn emit_statement_to_current_block(
        &mut self,
        statement: HirStatement,
        source_location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let block = self.current_block_mut_or_error(source_location)?;
        block.statements.push(statement);
        Ok(())
    }

    // WHAT: emits one `Assign(Local, value)` statement in the current block.
    // WHY: runtime short-circuit and fallible branching both need consistent temp-local
    //      assignment behavior and source mapping.
    pub(crate) fn emit_assign_local_statement(
        &mut self,
        local: LocalId,
        value: HirExpression,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
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

    // -------------------------
    //  Local Allocation
    // -------------------------

    // WHAT: allocates an unnamed temporary local in the current block.
    // WHY: complex expression lowering needs scratch storage to preserve evaluation order and
    //      explicit place/value distinctions in HIR.
    pub(crate) fn allocate_temp_local(
        &mut self,
        ty: TypeId,
        source_info: Option<SourceLocation>,
    ) -> Result<LocalId, CompilerError> {
        self.allocate_compiler_local(
            ty,
            source_info,
            HirLocalOriginKind::CompilerTemp,
            None,
            None,
        )
    }

    pub(crate) fn allocate_fresh_mutable_call_arg_local(
        &mut self,
        ty: TypeId,
        source_info: Option<SourceLocation>,
        call_location: &SourceLocation,
        argument_index: usize,
    ) -> Result<LocalId, CompilerError> {
        self.allocate_compiler_local(
            ty,
            source_info,
            HirLocalOriginKind::CompilerFreshMutableArg,
            Some(call_location),
            Some(argument_index),
        )
    }

    fn allocate_compiler_local(
        &mut self,
        ty: TypeId,
        source_info: Option<SourceLocation>,
        origin: HirLocalOriginKind,
        call_location: Option<&SourceLocation>,
        argument_index: Option<usize>,
    ) -> Result<LocalId, CompilerError> {
        let location = source_info.to_owned().unwrap_or_default();
        let region = self.current_region_or_error(&location)?;
        let block_id = self.current_block_id_or_error(&location)?;
        let local_id = self.allocate_local_id();

        let local = HirLocal {
            id: local_id,
            ty,
            mutable: true,
            region,
            source_info,
        };

        self.side_table.map_local_source(&local);
        self.register_local_in_block(block_id, local, &location)?;

        let temp_name = format!("__hir_tmp_{}", self.temp_local_counter);
        self.temp_local_counter += 1;
        let temp_name_id = InternedPath::from_single_str(&temp_name, self.string_table);

        // Compiler-introduced temporaries are intentionally excluded from AST symbol resolution.
        // They are named only for diagnostics/debug rendering via the side table.
        self.side_table.bind_local_name(local_id, temp_name_id);
        self.side_table
            .bind_local_origin(local_id, origin, call_location, argument_index);

        Ok(local_id)
    }

    // -------------------------
    //  Expression Construction
    // -------------------------

    // WHAT: returns mutable access to the active block or a structured lowering error.
    // WHY: most expression helpers need to append locals or statements, and failing early
    //      produces clearer diagnostics than assuming block state exists.
    pub(crate) fn current_block_mut_or_error(
        &mut self,
        location: &SourceLocation,
    ) -> Result<&mut HirBlock, CompilerError> {
        let block_id = self.current_block_id_or_error(location)?;
        self.block_mut_by_id_or_error(block_id, location)
    }

    // WHAT: allocates one HIR expression node with its identity and typing metadata attached.
    // WHY: centralizing expression construction keeps IDs, source mappings, and value kinds
    //      uniform across every lowering helper.
    pub(crate) fn make_expression(
        &mut self,
        location: &SourceLocation,
        kind: HirExpressionKind,
        ty: TypeId,
        value_kind: ValueKind,
        region: RegionId,
    ) -> HirExpression {
        let id = self.allocate_value_id();
        self.side_table.map_value(location, id, location);

        HirExpression {
            id,
            kind,
            ty,
            value_kind,
            region,
        }
    }

    // WHAT: creates a canonical load expression for one local.
    // WHY: runtime/result branching paths frequently reconstruct this node shape and should share
    //      one helper for readability and consistency.
    pub(crate) fn make_local_load_expression(
        &mut self,
        local: LocalId,
        ty: TypeId,
        location: &SourceLocation,
        region: RegionId,
    ) -> HirExpression {
        self.make_expression(
            location,
            HirExpressionKind::Load(HirPlace::Local(local)),
            ty,
            ValueKind::RValue,
            region,
        )
    }

    // WHAT: builds the canonical HIR representation of unit.
    // WHY: unit values should lower through the same tuple machinery every other pass expects.
    pub(crate) fn unit_expression(
        &mut self,
        location: &SourceLocation,
        region: RegionId,
    ) -> HirExpression {
        let unit_ty = self.type_environment.builtins().none;
        self.make_expression(
            location,
            HirExpressionKind::TupleConstruct { elements: vec![] },
            unit_ty,
            ValueKind::Const,
            region,
        )
    }

    // -------------------------
    //  Choice Support
    // -------------------------

    /// Register a choice declaration, allocating a stable `ChoiceId`.
    ///
    /// WHAT: called during `prepare_hir_declarations` to build the complete choice registry
    /// before any expression or statement lowering.
    /// WHY: separating registration from lookup keeps choice resolution a pure lookup
    ///      path and prevents lazy-creation ordering bugs.
    pub(crate) fn register_choice_id(
        &mut self,
        nominal_path: &InternedPath,
        location: &SourceLocation,
    ) -> Result<crate::compiler_frontend::hir::ids::ChoiceId, CompilerError> {
        use crate::compiler_frontend::hir::module::HirChoice;

        if let Some(&choice_id) = self.choices_by_name.get(nominal_path) {
            return Ok(choice_id);
        }

        let frontend_type_id = self
            .type_environment
            .nominal_id_for_path(nominal_path)
            .and_then(|nominal_id| self.type_environment.type_id_for_nominal_id(nominal_id))
            .ok_or_else(|| {
                crate::compiler_frontend::compiler_errors::CompilerError::compiler_error(format!(
                    "Choice '{}' is not registered in TypeEnvironment during HIR lowering",
                    nominal_path.to_string(self.string_table)
                ))
            })?;

        let choice_id = self.allocate_choice_id();

        // Push a placeholder BEFORE lowering variants so recursive registrations
        // preserve the invariant: ChoiceId(N) maps to module.choices[N].
        self.choices_by_name
            .insert(nominal_path.to_owned(), choice_id);
        self.side_table
            .bind_choice_name(choice_id, nominal_path.to_owned());
        let index = choice_id.0 as usize;
        debug_assert!(index == self.module.choices.len());
        self.module.choices.push(HirChoice {
            id: choice_id,
            frontend_type_id,
            variants: vec![],
        });

        let hir_variants = self.lower_choice_variants_for_type_id(frontend_type_id, location)?;
        self.module.choices[index].variants = hir_variants;

        Ok(choice_id)
    }

    /// Look up a pre-registered choice by its canonical path.
    ///
    /// WHAT: resolves a `ChoiceId` after `prepare_hir_declarations` has registered all choices.
    /// WHY: expression and statement lowering should never create new choice metadata;
    ///      missing entries indicate an AST → HIR contract violation.
    pub(crate) fn resolve_choice_id(
        &self,
        nominal_path: &InternedPath,
        location: &SourceLocation,
    ) -> Result<crate::compiler_frontend::hir::ids::ChoiceId, CompilerError> {
        let Some(choice_id) = self.choices_by_name.get(nominal_path).copied() else {
            return_hir_transformation_error!(
                format!(
                    "Choice '{}' was not pre-registered during HIR declaration preparation",
                    self.symbol_name_for_diagnostics(nominal_path)
                ),
                self.hir_error_location(location)
            );
        };
        Ok(choice_id)
    }

    // -------------------------
    //  Diagnostics & Logging
    // -------------------------

    // WHAT: converts a frontend text location into the shared compiler error-location format.
    // WHY: HIR lowering uses one helper so all transformation errors preserve consistent source metadata.
    pub(crate) fn hir_error_location(&self, location: &SourceLocation) -> SourceLocation {
        location.clone()
    }

    fn log_expression_input(&self, _expr: &Expression) {
        hir_log!(format!(
            "[HIR] Lowering expression {:?} @ {:?}",
            _expr.kind, _expr.location
        ));
    }

    fn log_expression_output(&self, _input: &Expression, _output: &HirExpression) {
        hir_log!(format!(
            "[HIR] Lowered expression {:?} -> {}",
            _input.kind,
            _output.display_with_context(
                &crate::compiler_frontend::hir::hir_display::HirDisplayContext::new(
                    self.string_table,
                )
                .with_side_table(&self.side_table)
                .with_type_environment(&self.type_environment),
            )
        ));
    }

    fn log_call_result_binding(
        &self,
        _location: &SourceLocation,
        _local: Option<LocalId>,
        _value: &HirExpression,
    ) {
        hir_log!(format!(
            "[HIR] Emitted call binding @ {:?}: result={:?}, value={}",
            _location,
            _local,
            _value.display_with_context(
                &crate::compiler_frontend::hir::hir_display::HirDisplayContext::new(
                    self.string_table,
                )
                .with_side_table(&self.side_table)
                .with_type_environment(&self.type_environment),
            )
        ));
    }
}
