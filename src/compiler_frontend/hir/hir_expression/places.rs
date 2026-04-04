//! Reference and place-lowering helpers for HIR expressions.
//!
//! WHAT: lowers AST nodes that identify storage locations, field paths, and module constants.
//! WHY: HIR must distinguish assignable places from value expressions before later alias and
//! mutation analysis can reason about them.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::builtins::BuiltinMethodKind;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_datatypes::{HirTypeKind, TypeId};
use crate::compiler_frontend::hir::hir_nodes::{
    FieldId, FunctionId, HirExpression, HirExpressionKind, HirPlace, HirStatement,
    HirStatementKind, LocalId, StructId, ValueKind,
};
use crate::compiler_frontend::host_functions::CallTarget;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringId;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_hir_transformation_error;

use super::LoweredExpression;

impl<'a> HirBuilder<'a> {
    // WHAT: converts an AST node that semantically yields a value into HIR expression form.
    // WHY: some runtime AST containers store expressions as general nodes, and HIR still needs a
    //      single value-producing lowering path for them.
    pub(crate) fn lower_ast_node_as_expression(
        &mut self,
        node: &AstNode,
    ) -> Result<LoweredExpression, CompilerError> {
        match &node.kind {
            NodeKind::Rvalue(expr) => self.lower_expression(expr),

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
                CallTarget::HostFunction(host_function_id.to_owned()),
                args,
                result_types,
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
                        self.lower_reference_expression(name, &expr.data_type, &node.location)?;
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
                    let lowered = self.lower_expression(expr)?;
                    let place = self.place_from_expression(&lowered.value, &node.location)?;
                    Ok((lowered.prelude, place))
                }
            },

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
                let place = self.place_from_expression(&lowered.value, &node.location)?;
                Ok((lowered.prelude, place))
            }

            NodeKind::ResultHandledFunctionCall {
                name,
                args,
                result_types,
                handling,
                location,
            } => {
                let function_id = self.resolve_function_id_or_error(name, location)?;
                let lowered = self.lower_result_handled_call_expression(
                    CallTarget::UserFunction(function_id),
                    args,
                    result_types,
                    handling,
                    true,
                    location,
                )?;
                let place = self.place_from_expression(&lowered.value, &node.location)?;
                Ok((lowered.prelude, place))
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
                let place = self.place_from_expression(&lowered.value, &node.location)?;
                Ok((lowered.prelude, place))
            }

            NodeKind::MethodCall {
                receiver,
                method_path,
                builtin,
                args,
                result_types,
                location,
                ..
            } => {
                if matches!(builtin, Some(BuiltinMethodKind::CollectionGet)) {
                    return self.lower_collection_get_place(receiver, args, location);
                }

                let lowered = self.lower_receiver_method_call_expression(
                    method_path,
                    *builtin,
                    receiver,
                    args,
                    result_types,
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
        args: &[crate::compiler_frontend::ast::expressions::expression::Expression],
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
        let lowered_index = self.lower_expression(&args[0])?;

        let mut prelude = receiver_prelude;
        prelude.extend(lowered_index.prelude);

        Ok((
            prelude,
            HirPlace::Index {
                base: Box::new(receiver_place),
                index: Box::new(lowered_index.value),
            },
        ))
    }

    pub(super) fn lower_reference_expression(
        &mut self,
        name: &InternedPath,
        data_type: &DataType,
        location: &SourceLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        let region = self.current_region_or_error(location)?;
        let ty = self.lower_data_type(data_type, location)?;

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

        if let ExpressionKind::Template(template) = &constant_declaration.value.kind {
            match template.const_value_kind() {
                crate::compiler_frontend::ast::templates::template::TemplateConstValueKind::RenderableString => {
                    let mut fold_context = self.new_template_fold_context(&template.location.scope);
                    let folded = template.fold_into_stringid(&mut fold_context)?;
                    let string_ty = self.intern_type_kind(HirTypeKind::String);
                    let region = self.current_region_or_error(location)?;

                    return Ok(Some(self.make_expression(
                        location,
                        HirExpressionKind::StringLiteral(self.string_table.resolve(folded).to_owned()),
                        string_ty,
                        ValueKind::Const,
                        region,
                    )));
                }
                crate::compiler_frontend::ast::templates::template::TemplateConstValueKind::WrapperTemplate => {
                    // WHAT: allow direct runtime uses of wrapper constants to lower as strings.
                    // WHY: unresolved wrapper slots render as empty segments when no fill-site
                    // consumes them, matching runtime template rendering semantics.
                    let mut fold_context = self.new_template_fold_context(&template.location.scope);
                    let folded = template.fold_into_stringid(&mut fold_context)?;
                    let string_ty = self.intern_type_kind(HirTypeKind::String);
                    let region = self.current_region_or_error(location)?;

                    return Ok(Some(self.make_expression(
                        location,
                        HirExpressionKind::StringLiteral(self.string_table.resolve(folded).to_owned()),
                        string_ty,
                        ValueKind::Const,
                        region,
                    )));
                }
                crate::compiler_frontend::ast::templates::template::TemplateConstValueKind::SlotInsertHelper => {
                    return_hir_transformation_error!(
                        format!(
                            "Template helper constant '{}' reached HIR expression lowering before AST wrapper-slot resolution.",
                            self.symbol_name_for_diagnostics(name)
                        ),
                        self.hir_error_location(location)
                    );
                }
                crate::compiler_frontend::ast::templates::template::TemplateConstValueKind::NonConst => {}
            }
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

    pub(super) fn resolve_struct_id_from_nominal_fields(
        &self,
        fields: &[Declaration],
        location: &SourceLocation,
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
                .contains_key(&(struct_id, field.id.to_owned()))
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
        &self,
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
        &self,
        place: &HirPlace,
        location: &SourceLocation,
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
        location: &SourceLocation,
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
