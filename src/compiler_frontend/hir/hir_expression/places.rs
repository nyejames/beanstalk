//! Reference and place-lowering helpers for HIR expressions.
//!
//! WHAT: lowers AST nodes that identify storage locations, field paths, and module constants.
//! WHY: HIR must distinguish assignable places from value expressions before later alias and
//! mutation analysis can reason about them.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::call_argument::CallArgument;
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, FallibleHandling,
};
use crate::compiler_frontend::builtins::CollectionBuiltinOp;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::definitions::TypeDefinition;
use crate::compiler_frontend::datatypes::generic_identity_bridge::TypeIdentityKey;
use crate::compiler_frontend::datatypes::ids::TypeId as FrontendTypeId;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::external_packages::CallTarget;
use crate::compiler_frontend::hir::expressions::{HirExpression, HirExpressionKind, ValueKind};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::ids::{FieldId, FunctionId, LocalId, StructId};
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::hir::statements::{HirStatement, HirStatementKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_hir_transformation_error;
use rustc_hash::FxHashSet;

use super::{
    DynamicTraitMethodCallLoweringInput, ExternalFallibleCallLoweringInput, LoweredExpression,
};

impl<'a> HirBuilder<'a> {
    // WHAT: converts an AST node that semantically yields a value into HIR expression form.
    // WHY: some runtime AST containers store expressions as general nodes, and HIR still needs a
    //      single value-producing lowering path for them.
    pub(crate) fn lower_ast_node_as_expression(
        &mut self,
        node: &AstNode,
    ) -> Result<LoweredExpression, CompilerError> {
        match &node.kind {
            NodeKind::Rvalue(expr) => {
                if self.expression_needs_current_block_lowering(expr) {
                    let value = self.lower_expression_value_to_current_block(expr)?;
                    return Ok(LoweredExpression {
                        prelude: vec![],
                        value,
                    });
                }

                self.lower_expression(expr)
            }

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

            NodeKind::FieldAccess {
                base: _,
                field: _,
                type_id,
                ..
            } => {
                if let Some(lowered) = self.try_lower_const_record_field_access(node)? {
                    return Ok(lowered);
                }

                let region = self.current_region_or_error(&node.location)?;
                let (prelude, place) = self.lower_ast_node_to_place(node)?;
                let ty = self.lower_type_id(*type_id, &node.location)?;

                Ok(LoweredExpression {
                    prelude,
                    value: self.make_expression(
                        &node.location,
                        HirExpressionKind::Load(place),
                        ty,
                        ValueKind::Place,
                        region,
                    ),
                })
            }

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

            _ => {
                return_hir_transformation_error!(
                    format!("AST node is not an expression: {:?}", node.kind),
                    self.hir_error_location(&node.location)
                )
            }
        }
    }

    // WHAT: resolves an AST node into a concrete HIR place for loads, stores, and copies.
    // WHY: place lowering must distinguish between value-producing expressions and assignable
    //      storage locations before later borrow and mutation analysis runs.
    pub(crate) fn lower_ast_node_to_place(
        &mut self,
        node: &AstNode,
    ) -> Result<(Vec<HirStatement>, HirPlace), CompilerError> {
        match &node.kind {
            NodeKind::Rvalue(expr) => match &expr.kind {
                ExpressionKind::Reference(name) => {
                    if let Some(local) = self.locals_by_name.get(name).copied() {
                        return Ok((vec![], HirPlace::Local(local)));
                    }

                    // Field/index lowering requires a place. Module constants are lowered as
                    // rvalues, so materialize them into a temporary local when referenced in
                    // place-position expressions (for example `format.center`).
                    let lowered =
                        self.lower_reference_expression(name, expr.type_id, &node.location)?;
                    if let HirExpressionKind::Load(place) = &lowered.value.kind {
                        return Ok((lowered.prelude, place.to_owned()));
                    }

                    let temp_local =
                        self.allocate_temp_local(lowered.value.ty, Some(node.location.to_owned()))?;
                    let assign_statement = HirStatement {
                        id: self.allocate_node_id(),
                        kind: HirStatementKind::Assign {
                            target: HirPlace::Local(temp_local),
                            value: lowered.value,
                        },
                        location: node.location.to_owned(),
                    };

                    self.side_table
                        .map_statement(&node.location, &assign_statement);

                    let mut prelude = lowered.prelude;
                    prelude.push(assign_statement);
                    Ok((prelude, HirPlace::Local(temp_local)))
                }

                _ => {
                    let lowered = if self.expression_needs_current_block_lowering(expr) {
                        LoweredExpression {
                            prelude: vec![],
                            value: self.lower_expression_value_to_current_block(expr)?,
                        }
                    } else {
                        self.lower_expression(expr)?
                    };

                    if let HirExpressionKind::Load(place) = &lowered.value.kind {
                        return Ok((lowered.prelude, place.to_owned()));
                    }

                    let temp_local =
                        self.allocate_temp_local(lowered.value.ty, Some(node.location.to_owned()))?;
                    let assign_statement = HirStatement {
                        id: self.allocate_node_id(),
                        kind: HirStatementKind::Assign {
                            target: HirPlace::Local(temp_local),
                            value: lowered.value,
                        },
                        location: node.location.to_owned(),
                    };
                    self.side_table
                        .map_statement(&node.location, &assign_statement);

                    let mut prelude = lowered.prelude;
                    prelude.push(assign_statement);
                    Ok((prelude, HirPlace::Local(temp_local)))
                }
            },

            NodeKind::FunctionCall {
                name,
                args,
                result_type_ids,
                location,
            } => {
                let function_id = self.resolve_function_id_or_error(name, location)?;
                let lowered = self.lower_call_expression(
                    CallTarget::UserFunction(function_id),
                    args,
                    result_type_ids,
                    location,
                )?;
                let place = self.place_from_expression(&lowered.value, &node.location)?;
                Ok((lowered.prelude, place))
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
                    let temp_local =
                        self.allocate_temp_local(value.ty, Some(node.location.to_owned()))?;
                    self.emit_assign_local_statement(temp_local, value, &node.location)?;
                    return Ok((vec![], HirPlace::Local(temp_local)));
                }

                let function_id = self.resolve_function_id_or_error(name, location)?;
                let lowered = self.lower_handled_fallible_call_expression(
                    CallTarget::UserFunction(function_id),
                    args,
                    result_type_ids,
                    handling,
                    true,
                    location,
                )?;
                let place = self.place_from_expression(&lowered.value, &node.location)?;
                Ok((lowered.prelude, place))
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
                    let temp_local =
                        self.allocate_temp_local(value.ty, Some(node.location.to_owned()))?;
                    self.emit_assign_local_statement(temp_local, value, &node.location)?;
                    return Ok((vec![], HirPlace::Local(temp_local)));
                }

                let lowered = self.lower_handled_external_fallible_call_expression(
                    ExternalFallibleCallLoweringInput {
                        id: *host_function_id,
                        args,
                        result_type_ids,
                        error_type_id: *error_type_id,
                        handling,
                        value_required: true,
                        location,
                    },
                )?;
                let place = self.place_from_expression(&lowered.value, &node.location)?;
                Ok((lowered.prelude, place))
            }

            NodeKind::HostFunctionCall {
                name: host_function_id,
                args,
                result_type_ids,
                location,
            } => {
                let lowered = self.lower_call_expression(
                    CallTarget::ExternalFunction(*host_function_id),
                    args,
                    result_type_ids,
                    location,
                )?;
                let place = self.place_from_expression(&lowered.value, &node.location)?;
                Ok((lowered.prelude, place))
            }

            NodeKind::MethodCall {
                receiver,
                method_path,
                args,
                result_type_ids,
                location,
                ..
            } => {
                let lowered = self.lower_receiver_method_call_expression(
                    method_path,
                    receiver,
                    args,
                    result_type_ids,
                    location,
                )?;
                let place = self.place_from_expression(&lowered.value, &node.location)?;
                Ok((lowered.prelude, place))
            }

            NodeKind::DynamicTraitMethodCall {
                receiver,
                trait_id,
                requirement_id,
                receiver_requires_mutable,
                args,
                result_type_ids,
                location,
                ..
            } => {
                let lowered = self.lower_dynamic_trait_method_call_expression(
                    DynamicTraitMethodCallLoweringInput {
                        receiver,
                        trait_id: *trait_id,
                        requirement_id: *requirement_id,
                        receiver_requires_mutable: *receiver_requires_mutable,
                        args,
                        result_type_ids,
                        location,
                    },
                )?;
                let place = self.place_from_expression(&lowered.value, &node.location)?;
                Ok((lowered.prelude, place))
            }

            NodeKind::CollectionBuiltinCall {
                receiver,
                op,
                receiver_requires_mutable,
                args,
                result_type_ids,
                location,
            } => {
                if *op == CollectionBuiltinOp::Get {
                    return self.lower_collection_get_place(receiver, args, location);
                }

                let lowered = self.lower_collection_builtin_call_expression(
                    *op,
                    receiver,
                    *receiver_requires_mutable,
                    args,
                    result_type_ids,
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

    fn lower_collection_get_place(
        &mut self,
        receiver: &AstNode,
        args: &[CallArgument],
        location: &SourceLocation,
    ) -> Result<(Vec<HirStatement>, HirPlace), CompilerError> {
        if args.len() != 1 {
            return_hir_transformation_error!(
                format!(
                    "Collection get-place lowering expected 1 index argument, found {}",
                    args.len()
                ),
                self.hir_error_location(location)
            );
        }

        let (receiver_prelude, receiver_place) = self.lower_ast_node_to_place(receiver)?;
        for prelude in receiver_prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }
        let lowered_index = self.lower_expression_value_to_current_block(&args[0].value)?;

        Ok((
            vec![],
            HirPlace::Index {
                base: Box::new(receiver_place),
                index: Box::new(lowered_index),
            },
        ))
    }

    pub(super) fn lower_reference_expression(
        &mut self,
        name: &InternedPath,
        type_id: FrontendTypeId,
        location: &SourceLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        let region = self.current_region_or_error(location)?;
        let ty = self.lower_type_id(type_id, location)?;

        if let Some(local_id) = self.locals_by_name.get(name).copied() {
            return Ok(LoweredExpression {
                prelude: vec![],
                value: self.make_expression(
                    location,
                    HirExpressionKind::Load(HirPlace::Local(local_id)),
                    ty,
                    ValueKind::Place,
                    region,
                ),
            });
        }

        if let Some(mut constant_value) =
            self.try_lower_module_constant_reference(name, location)?
        {
            // Preserve the type expected by the AST reference expression while reusing
            // the constant's lowered value shape.
            constant_value.ty = ty;
            constant_value.region = region;

            return Ok(LoweredExpression {
                prelude: vec![],
                value: constant_value,
            });
        }

        return_hir_transformation_error!(
            format!(
                "Unresolved local '{}' during HIR expression lowering",
                self.symbol_name_for_diagnostics(name)
            ),
            self.hir_error_location(location)
        )
    }

    fn try_lower_module_constant_reference(
        &mut self,
        name: &InternedPath,
        location: &SourceLocation,
    ) -> Result<Option<HirExpression>, CompilerError> {
        let Some(constant_declaration) = self.module_constants_by_name.get(name).cloned() else {
            return Ok(None);
        };

        // INVARIANT: template constants should have been materialized into string literals
        // or runtime expressions by AST template folding before HIR lowering.
        if matches!(constant_declaration.value.kind, ExpressionKind::Template(_)) {
            return_hir_transformation_error!(
                format!(
                    "HIR invariant: template constant '{}' reached HIR expression lowering before AST materialized it.",
                    self.symbol_name_for_diagnostics(name)
                ),
                self.hir_error_location(location)
            );
        }

        // INVARIANT: const-record runtime use should have been rejected in AST.
        // If a const record reaches HIR reference lowering, push validation earlier
        // instead of converting this into a user diagnostic here.
        if constant_declaration.value.is_const_record_value() {
            return_hir_transformation_error!(
                format!(
                    "HIR invariant: const record '{}' reached HIR reference lowering without field access",
                    self.symbol_name_for_diagnostics(name)
                ),
                self.hir_error_location(location)
            );
        }

        if !self.currently_lowering_constants.insert(name.to_owned()) {
            return_hir_transformation_error!(
                format!(
                    "Cyclic module constant dependency detected while lowering '{}'",
                    self.symbol_name_for_diagnostics(name)
                ),
                self.hir_error_location(location)
            );
        }

        let lowered_constant = self.lower_expression(&constant_declaration.value);
        self.currently_lowering_constants.remove(name);
        let lowered_constant = lowered_constant?;

        if !lowered_constant.prelude.is_empty() {
            return_hir_transformation_error!(
                format!(
                    "Module constant '{}' unexpectedly emitted runtime statements during HIR lowering",
                    self.symbol_name_for_diagnostics(name)
                ),
                self.hir_error_location(location)
            );
        }

        Ok(Some(lowered_constant.value))
    }

    fn try_lower_const_record_field_access(
        &mut self,
        node: &AstNode,
    ) -> Result<Option<LoweredExpression>, CompilerError> {
        let NodeKind::FieldAccess {
            base,
            field,
            type_id,
            ..
        } = &node.kind
        else {
            return Ok(None);
        };

        let mut visited_records = FxHashSet::default();
        let Some(field_expression) = self.resolve_const_record_field_expression(
            base,
            *field,
            &node.location,
            &mut visited_records,
        )?
        else {
            return Ok(None);
        };

        // INVARIANT: const-record runtime use should have been rejected in AST.
        // If a nested const-record field access still yields a record value in HIR,
        // push validation earlier instead of converting this into a user diagnostic here.
        if field_expression.is_const_record_value() {
            return_hir_transformation_error!(
                "HIR invariant: const-record field access reached HIR while still producing a record value",
                self.hir_error_location(&node.location)
            );
        }

        let mut lowered = self.lower_expression(&field_expression)?;
        let region = self.current_region_or_error(&node.location)?;
        let ty = self.lower_type_id(*type_id, &node.location)?;

        lowered.value.ty = ty;
        lowered.value.region = region;

        Ok(Some(lowered))
    }

    fn resolve_const_record_field_expression(
        &self,
        base: &AstNode,
        field: StringId,
        location: &SourceLocation,
        visited_records: &mut FxHashSet<InternedPath>,
    ) -> Result<Option<Expression>, CompilerError> {
        let Some(record_expression) =
            self.const_record_expression_for_node(base, location, visited_records)?
        else {
            return Ok(None);
        };

        let ExpressionKind::StructInstance(fields) = &record_expression.kind else {
            return_hir_transformation_error!(
                "Const record reached HIR field lowering without struct field data",
                self.hir_error_location(location)
            );
        };

        let Some(field_declaration) = fields
            .iter()
            .find(|field_declaration| field_declaration.id.name() == Some(field))
        else {
            return_hir_transformation_error!(
                format!(
                    "Const record field '{}' was not present during HIR field lowering",
                    self.string_table.resolve(field)
                ),
                self.hir_error_location(location)
            );
        };

        Ok(Some(field_declaration.value.to_owned()))
    }

    fn const_record_expression_for_node(
        &self,
        node: &AstNode,
        location: &SourceLocation,
        visited_records: &mut FxHashSet<InternedPath>,
    ) -> Result<Option<Expression>, CompilerError> {
        match &node.kind {
            NodeKind::Rvalue(expression) => {
                self.const_record_expression_for_expression(expression, location, visited_records)
            }

            NodeKind::FieldAccess { base, field, .. } => {
                let Some(field_expression) = self.resolve_const_record_field_expression(
                    base,
                    *field,
                    location,
                    visited_records,
                )?
                else {
                    return Ok(None);
                };

                self.const_record_expression_for_expression(
                    &field_expression,
                    location,
                    visited_records,
                )
            }

            NodeKind::VariableDeclaration(declaration) => self
                .const_record_expression_for_expression(
                    &declaration.value,
                    location,
                    visited_records,
                ),

            _ => Ok(None),
        }
    }

    fn const_record_expression_for_expression(
        &self,
        expression: &Expression,
        location: &SourceLocation,
        visited_records: &mut FxHashSet<InternedPath>,
    ) -> Result<Option<Expression>, CompilerError> {
        if !expression.is_const_record_value() {
            return Ok(None);
        }

        match &expression.kind {
            ExpressionKind::StructInstance(_) => Ok(Some(expression.to_owned())),

            ExpressionKind::Reference(name) => {
                if !visited_records.insert(name.to_owned()) {
                    return_hir_transformation_error!(
                        format!(
                            "Cyclic const-record reference detected while lowering '{}'",
                            self.symbol_name_for_diagnostics(name)
                        ),
                        self.hir_error_location(location)
                    );
                }

                let Some(declaration) = self
                    .local_const_records_by_name
                    .get(name)
                    .or_else(|| self.module_constants_by_name.get(name))
                else {
                    return_hir_transformation_error!(
                        format!(
                            "Const record '{}' reached HIR without compile-time record data",
                            self.symbol_name_for_diagnostics(name)
                        ),
                        self.hir_error_location(location)
                    );
                };

                self.const_record_expression_for_expression(
                    &declaration.value,
                    location,
                    visited_records,
                )
            }

            _ => {
                return_hir_transformation_error!(
                    "Const record reached HIR field lowering as a non-record expression",
                    self.hir_error_location(location)
                );
            }
        }
    }

    // WHAT: resolves a function path through the HIR declaration table.
    // WHY: expression lowering should fail with a structured HIR error instead of assuming AST
    //      declaration registration stayed in sync.
    pub(crate) fn resolve_function_id_or_error(
        &self,
        name: &InternedPath,
        location: &SourceLocation,
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

    // WHAT: resolves a field path within one nominal struct declaration.
    // WHY: field access lowering must use declaration-time IDs so later passes can reason about
    //      fields without path scans.
    pub(crate) fn resolve_field_id_or_error(
        &self,
        struct_id: StructId,
        field_name: &InternedPath,
        location: &SourceLocation,
    ) -> Result<FieldId, CompilerError> {
        let Some(field_id) = self
            .fields_by_struct_and_name
            .get(&(struct_id, field_name.to_owned()))
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

    pub(super) fn resolve_struct_id_from_nominal_path(
        &self,
        nominal_path: &InternedPath,
        location: &SourceLocation,
    ) -> Result<StructId, CompilerError> {
        let Some(struct_id) = self.structs_by_name.get(nominal_path).copied() else {
            return_hir_transformation_error!(
                format!(
                    "Unresolved struct '{}' during HIR lowering",
                    self.symbol_name_for_diagnostics(nominal_path)
                ),
                self.hir_error_location(location)
            );
        };

        Ok(struct_id)
    }

    fn resolve_field_id_for_base_place_or_error(
        &mut self,
        base_place: &HirPlace,
        field_name: StringId,
        location: &SourceLocation,
    ) -> Result<FieldId, CompilerError> {
        let struct_id = self.resolve_struct_id_for_place_or_error(base_place, location)?;
        let Some(struct_path) = self.side_table.struct_name_path(struct_id) else {
            return_hir_transformation_error!(
                format!(
                    "Struct {:?} is missing a side-table path binding",
                    struct_id
                ),
                self.hir_error_location(location)
            );
        };

        let field_path = struct_path.append(field_name);

        self.resolve_field_id_or_error(struct_id, &field_path, location)
    }

    fn resolve_struct_id_for_place_or_error(
        &mut self,
        place: &HirPlace,
        location: &SourceLocation,
    ) -> Result<StructId, CompilerError> {
        let ty = self.resolve_place_type_id_or_error(place, location)?;
        let path = match self.type_environment.get(ty).cloned() {
            Some(TypeDefinition::Struct(def)) => Some(def.path),
            Some(TypeDefinition::GenericInstance(instance))
                if self
                    .type_environment
                    .struct_definition(instance.base)
                    .is_some() =>
            {
                let Some(nominal_path) = self.type_environment.nominal_path_by_id(instance.base)
                else {
                    return_hir_transformation_error!(
                        "Generic struct instance is missing nominal path metadata",
                        self.hir_error_location(location)
                    );
                };
                let nominal_path = nominal_path.to_owned();
                let Some(TypeIdentityKey::GenericInstance(key)) =
                    self.type_environment.type_id_to_type_identity_key(ty)
                else {
                    return_hir_transformation_error!(
                        "Generic struct instance is missing a canonical key during field access lowering",
                        self.hir_error_location(location)
                    );
                };

                return self.resolve_or_register_generic_struct(&key, &nominal_path, ty, location);
            }
            _ => {
                return_hir_transformation_error!(
                    "Field access base does not resolve to a struct value in this HIR phase",
                    self.hir_error_location(location)
                )
            }
        };
        let Some(path) = path else {
            return_hir_transformation_error!(
                "Field access base is missing nominal struct path metadata",
                self.hir_error_location(location)
            );
        };

        match self.structs_by_name.get(&path).copied() {
            Some(struct_id) => Ok(struct_id),
            None => {
                return_hir_transformation_error!(
                    format!(
                        "Struct '{}' is not registered in HIR builder",
                        path.to_string(self.string_table)
                    ),
                    self.hir_error_location(location)
                )
            }
        }
    }

    fn resolve_place_type_id_or_error(
        &self,
        place: &HirPlace,
        location: &SourceLocation,
    ) -> Result<TypeId, CompilerError> {
        match place {
            HirPlace::Local(local_id) => self.resolve_local_type_id_or_error(*local_id, location),
            HirPlace::Field { field, .. } => self.resolve_field_type_id_or_error(*field, location),
            HirPlace::Index { base, .. } => {
                let base_type = self.resolve_place_type_id_or_error(base, location)?;
                match self.type_environment.collection_element_type(base_type) {
                    Some(element) => Ok(element),
                    None => {
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
        location: &SourceLocation,
    ) -> Result<TypeId, CompilerError> {
        self.local_type_id_or_error(local_id, location)
    }

    fn resolve_field_type_id_or_error(
        &self,
        field_id: FieldId,
        location: &SourceLocation,
    ) -> Result<TypeId, CompilerError> {
        self.field_type_id_or_error(field_id, location)
    }

    fn place_from_expression(
        &self,
        value: &HirExpression,
        location: &SourceLocation,
    ) -> Result<HirPlace, CompilerError> {
        let HirExpressionKind::Load(place) = &value.kind else {
            return_hir_transformation_error!(
                "Expected a place-producing expression while lowering place",
                self.hir_error_location(location)
            );
        };

        Ok(place.to_owned())
    }
}
