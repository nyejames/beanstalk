//! Declaration and symbol-registration helpers for HIR statement lowering.
//!
//! WHAT: owns top-level declaration registration, module-constant lowering, and local creation.
//! WHY: these paths build the HIR symbol tables and compile-time data pool that later control-flow
//! lowering depends on.

use crate::compiler_frontend::ast::ast::Ast;
use crate::compiler_frontend::ast::ast_nodes::{Declaration, NodeKind, SourceLocation};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_nodes::{
    HirConstField, HirConstValue, HirField, HirFunction, HirLocal, HirModuleConst, HirPlace,
    HirRegion, HirStruct, HirTerminator, LocalId,
};
use crate::compiler_frontend::hir::hir_side_table::HirLocation;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;
use crate::return_hir_transformation_error;

impl<'a> HirBuilder<'a> {
    // WHAT: pre-registers all structs and functions before any HIR body lowering starts.
    // WHY: later statement and expression lowering relies on complete symbol tables for stable ID lookups.
    pub(crate) fn prepare_hir_declarations(&mut self, ast: &Ast) -> Result<(), CompilerError> {
        for node in &ast.nodes {
            if let NodeKind::StructDefinition(name, fields) = &node.kind {
                self.register_struct_declaration(name, fields, &node.location)?;
            }
        }

        for node in &ast.nodes {
            if let NodeKind::Function(name, signature, _) = &node.kind {
                self.register_function_declaration(name, signature, &node.location)?;
            }
        }

        self.resolve_start_function(ast)
    }

    // WHAT: lowers the AST module-constant pool into HIR's dedicated constant metadata arena.
    // WHY: module constants should remain compile-time data instead of turning into runtime statements.
    pub(crate) fn lower_module_constants(&mut self, ast: &Ast) -> Result<(), CompilerError> {
        self.module.module_constants.clear();
        self.module_constants_by_name.clear();

        for declaration in &ast.module_constants {
            self.module_constants_by_name
                .insert(declaration.id.to_owned(), declaration.to_owned());

            let location = declaration.value.location.to_owned();
            let Some(const_value) =
                self.lower_const_value_for_module_pool(&declaration.value, &location)?
            else {
                continue;
            };

            let const_id = self.allocate_const_id();
            let const_type = self.lower_data_type(&declaration.value.data_type, &location)?;

            self.module.module_constants.push(HirModuleConst {
                id: const_id,
                name: declaration.id.to_string(self.string_table),
                ty: const_type,
                value: const_value,
            });
        }

        Ok(())
    }

    fn lower_const_value_for_module_pool(
        &mut self,
        expression: &Expression,
        location: &SourceLocation,
    ) -> Result<Option<HirConstValue>, CompilerError> {
        self.lower_const_value(expression, location)
    }

    fn lower_const_value(
        &mut self,
        expression: &Expression,
        location: &SourceLocation,
    ) -> Result<Option<HirConstValue>, CompilerError> {
        match &expression.kind {
            ExpressionKind::Int(value) => Ok(Some(HirConstValue::Int(*value))),
            ExpressionKind::Float(value) => Ok(Some(HirConstValue::Float(*value))),
            ExpressionKind::Bool(value) => Ok(Some(HirConstValue::Bool(*value))),
            ExpressionKind::Char(value) => Ok(Some(HirConstValue::Char(*value))),
            ExpressionKind::StringSlice(value) => Ok(Some(HirConstValue::String(
                self.string_table.resolve(*value).to_string(),
            ))),
            ExpressionKind::Collection(items) => {
                let mut lowered_items = Vec::with_capacity(items.len());
                for item in items {
                    let Some(lowered_item) = self.lower_const_value(item, location)? else {
                        return Ok(None);
                    };
                    lowered_items.push(lowered_item);
                }
                Ok(Some(HirConstValue::Collection(lowered_items)))
            }
            ExpressionKind::StructInstance(fields) => {
                let mut lowered_fields = Vec::with_capacity(fields.len());
                for field in fields {
                    let Some(lowered_value) = self.lower_const_value(&field.value, location)?
                    else {
                        return Ok(None);
                    };
                    lowered_fields.push(HirConstField {
                        name: field.id.to_string(self.string_table),
                        value: lowered_value,
                    });
                }
                // Const-eligible struct constructors in top-level '#' constants are coerced
                // in AST to data-only struct instances, and land here as HIR const records.
                Ok(Some(HirConstValue::Record(lowered_fields)))
            }
            ExpressionKind::Range(start, end) => {
                let Some(lowered_start) = self.lower_const_value(start, location)? else {
                    return Ok(None);
                };
                let Some(lowered_end) = self.lower_const_value(end, location)? else {
                    return Ok(None);
                };

                Ok(Some(HirConstValue::Range(
                    Box::new(lowered_start),
                    Box::new(lowered_end),
                )))
            }
            ExpressionKind::Template(template) => {
                // WHAT: omit unresolved wrapper/slot helpers from the HIR constant pool.
                // WHY: these are AST-time composition values, not concrete runtime metadata.
                match template.const_value_kind() {
                    crate::compiler_frontend::ast::templates::template::TemplateConstValueKind::RenderableString => {
                        let mut fold_context =
                            self.new_template_fold_context(&template.location.scope);
                        let folded = template.fold_into_stringid(&mut fold_context)?;
                        Ok(Some(HirConstValue::String(
                            self.string_table.resolve(folded).to_string(),
                        )))
                    }
                    crate::compiler_frontend::ast::templates::template::TemplateConstValueKind::WrapperTemplate
                    | crate::compiler_frontend::ast::templates::template::TemplateConstValueKind::SlotInsertHelper => {
                        Ok(None)
                    }
                    crate::compiler_frontend::ast::templates::template::TemplateConstValueKind::NonConst => {
                        return_hir_transformation_error!(
                            "Template constant reached HIR without compile-time const semantics.",
                            self.hir_error_location(location)
                        )
                    }
                }
            }
            _ => return_hir_transformation_error!(
                format!(
                    "Unsupported constant expression during HIR lowering: {:?}",
                    expression.kind
                ),
                self.hir_error_location(location)
            ),
        }
    }

    fn register_struct_declaration(
        &mut self,
        name: &InternedPath,
        fields: &[Declaration],
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        if self.structs_by_name.contains_key(name) {
            return_hir_transformation_error!(
                format!(
                    "Duplicate struct declaration '{}' during HIR lowering",
                    self.symbol_name_for_diagnostics(name)
                ),
                self.hir_error_location(location)
            );
        }

        let struct_id = self.allocate_struct_id();
        let mut hir_fields = Vec::with_capacity(fields.len());

        for field in fields {
            // AST guarantees module-wide unique InternedPath symbols. For struct fields this
            // means each field path must be prefixed by its parent struct path.
            let Some(parent) = field.id.parent() else {
                return_hir_transformation_error!(
                    format!(
                        "Field '{}' has no parent struct path during HIR lowering",
                        self.symbol_name_for_diagnostics(&field.id)
                    ),
                    self.hir_error_location(location)
                );
            };

            if parent != *name {
                return_hir_transformation_error!(
                    format!(
                        "Field '{}' is not prefixed by struct '{}'",
                        self.symbol_name_for_diagnostics(&field.id),
                        self.symbol_name_for_diagnostics(name)
                    ),
                    self.hir_error_location(location)
                );
            }

            if self
                .fields_by_struct_and_name
                .contains_key(&(struct_id, field.id.to_owned()))
            {
                return_hir_transformation_error!(
                    format!(
                        "Duplicate field '{}' in struct '{}'",
                        self.symbol_name_for_diagnostics(&field.id),
                        self.symbol_name_for_diagnostics(name)
                    ),
                    self.hir_error_location(location)
                );
            }

            let field_location = if field.value.location == SourceLocation::default() {
                location.clone()
            } else {
                field.value.location.clone()
            };

            let field_type = self.lower_data_type(&field.value.data_type, &field_location)?;
            let field_id = self.allocate_field_id();

            self.fields_by_struct_and_name
                .insert((struct_id, field.id.to_owned()), field_id);
            self.side_table
                .bind_field_name(field_id, field.id.to_owned());
            self.side_table
                .map_ast_to_hir(&field_location, HirLocation::Field(field_id));
            self.side_table
                .map_hir_source_location(HirLocation::Field(field_id), &field_location);

            hir_fields.push(HirField {
                id: field_id,
                ty: field_type,
            });
        }

        let hir_struct = HirStruct {
            id: struct_id,
            fields: hir_fields,
        };

        self.structs_by_name.insert(name.to_owned(), struct_id);
        self.side_table.bind_struct_name(struct_id, name.to_owned());
        self.side_table
            .map_ast_to_hir(location, HirLocation::Struct(struct_id));
        self.side_table
            .map_hir_source_location(HirLocation::Struct(struct_id), location);
        self.push_struct(hir_struct);

        Ok(())
    }

    fn register_function_declaration(
        &mut self,
        name: &InternedPath,
        signature: &FunctionSignature,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        if self.functions_by_name.contains_key(name) {
            return_hir_transformation_error!(
                format!(
                    "Duplicate function declaration '{}' during HIR lowering",
                    self.symbol_name_for_diagnostics(name)
                ),
                self.hir_error_location(location)
            );
        }

        let function_id = self.allocate_function_id();
        let return_type =
            self.lower_data_type(&DataType::Returns(signature.return_data_types()), location)?;

        let region_id = self.allocate_region_id();
        self.push_region(HirRegion::lexical(region_id, None));

        let entry_block_id = self.allocate_block_id();
        let entry_block = crate::compiler_frontend::hir::hir_nodes::HirBlock {
            id: entry_block_id,
            region: region_id,
            locals: vec![],
            statements: vec![],
            terminator: HirTerminator::Panic { message: None },
        };

        self.side_table.map_block(location, &entry_block);
        self.push_block(entry_block);

        let function = HirFunction {
            id: function_id,
            entry: entry_block_id,
            params: vec![],
            return_type,
            return_aliases: signature
                .returns
                .iter()
                .map(|return_value| {
                    return_value
                        .alias_candidates()
                        .map(|indices| indices.to_vec())
                })
                .collect(),
        };

        self.functions_by_name.insert(name.to_owned(), function_id);
        self.side_table
            .bind_function_name(function_id, name.to_owned());
        self.side_table.map_function(location, &function);
        self.push_function(function);

        Ok(())
    }

    fn resolve_start_function(&mut self, ast: &Ast) -> Result<(), CompilerError> {
        let start_name = ast
            .entry_path
            .join_str(IMPLICIT_START_FUNC_NAME, self.string_table);

        let Some(start_function) = self.functions_by_name.get(&start_name).copied() else {
            let error_location = ast
                .nodes
                .first()
                .map(|node| node.location.clone())
                .unwrap_or_default();

            return_hir_transformation_error!(
                format!(
                    "Failed to resolve module start function '{}' during HIR lowering",
                    self.symbol_name_for_diagnostics(&start_name)
                ),
                self.hir_error_location(&error_location)
            );
        };

        self.module.start_function = start_function;
        Ok(())
    }

    pub(super) fn lower_parameter_locals(
        &mut self,
        function_id: crate::compiler_frontend::hir::hir_nodes::FunctionId,
        signature: &FunctionSignature,
        fallback_location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        for param in &signature.parameters {
            let param_location = if param.value.location == SourceLocation::default() {
                fallback_location.clone()
            } else {
                param.value.location.clone()
            };

            let param_type = self.lower_data_type(&param.value.data_type, &param_location)?;
            let local_id = self.allocate_named_local(
                param.id.to_owned(),
                param_type,
                param.value.ownership.is_mutable(),
                Some(param_location.clone()),
            )?;

            let function = self.function_mut_by_id_or_error(function_id, &param_location)?;
            function.params.push(local_id);
        }

        Ok(())
    }

    pub(super) fn lower_variable_declaration_statement(
        &mut self,
        variable: &Declaration,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let source_location = if variable.value.location == SourceLocation::default() {
            location.clone()
        } else {
            variable.value.location.clone()
        };

        let local_type = self.lower_data_type(&variable.value.data_type, &source_location)?;
        let local_id = self.allocate_named_local(
            variable.id.to_owned(),
            local_type,
            variable.value.ownership.is_mutable(),
            Some(source_location),
        )?;

        let lowered = self.lower_expression(&variable.value)?;

        for prelude in lowered.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }

        self.emit_statement_kind(
            crate::compiler_frontend::hir::hir_nodes::HirStatementKind::Assign {
                target: HirPlace::Local(local_id),
                value: lowered.value,
            },
            location,
        )
    }

    pub(super) fn allocate_named_local(
        &mut self,
        name: InternedPath,
        ty: crate::compiler_frontend::hir::hir_datatypes::TypeId,
        mutable: bool,
        source_info: Option<SourceLocation>,
    ) -> Result<LocalId, CompilerError> {
        let local_location = source_info.to_owned().unwrap_or_default();

        // AST forbids shadowing and provides module-wide unique symbol paths, so a duplicate
        // path here indicates invalid redeclaration in the current function lowering context.
        if self.locals_by_name.contains_key(&name) {
            return_hir_transformation_error!(
                format!(
                    "Local '{}' is already declared in this function scope",
                    self.symbol_name_for_diagnostics(&name)
                ),
                self.hir_error_location(&local_location)
            );
        }

        let region = self.current_region_or_error(&local_location)?;
        let block_id = self.current_block_id_or_error(&local_location)?;
        let local_id = self.allocate_local_id();

        let local = HirLocal {
            id: local_id,
            ty,
            mutable,
            region,
            source_info,
        };

        self.side_table.map_local_source(&local);
        self.register_local_in_block(block_id, local, &local_location)?;

        self.locals_by_name.insert(name.to_owned(), local_id);
        self.side_table.bind_local_name(local_id, name);
        self.side_table
            .map_ast_to_hir(&local_location, HirLocation::Local(local_id));

        Ok(local_id)
    }
}
