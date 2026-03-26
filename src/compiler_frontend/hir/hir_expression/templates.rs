use crate::compiler_frontend::ast::templates::create_template_node::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_datatypes::{HirTypeKind, TypeId};
use crate::compiler_frontend::hir::hir_nodes::{
    FunctionId, HirBlock, HirExpression, HirExpressionKind, HirFunction, HirLocal, HirPlace,
    HirTerminator, LocalId, RegionId, ValueKind,
};
use crate::compiler_frontend::host_functions::CallTarget;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::tokenizer::tokens::TextLocation;
use crate::return_hir_transformation_error;

use super::LoweredExpression;

impl<'a> HirBuilder<'a> {
    // WHAT: Lowers template expressions to either folded constants or runtime helper calls.
    // WHY: HIR must preserve template semantics without embedding AST template machinery.
    pub(crate) fn lower_runtime_template_expression(
        &mut self,
        template: &Template,
        location: &TextLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        if !self.currently_lowering_constants.is_empty() {
            // Module constant evaluation must stay statement-free. When constant structs carry
            // wrapper templates, fold them into string literals with unresolved slots rendered
            // as empty segments so constant references can lower without runtime template calls.
            let mut fold_context = self.new_template_fold_context(&template.location.scope);
            let folded = template.fold_into_stringid(&mut fold_context)?;
            let region = self.current_region_or_error(location)?;
            let string_ty = self.intern_type_kind(HirTypeKind::String);

            return Ok(LoweredExpression {
                prelude: vec![],
                value: self.make_expression(
                    location,
                    HirExpressionKind::StringLiteral(self.string_table.resolve(folded).to_owned()),
                    string_ty,
                    ValueKind::Const,
                    region,
                ),
            });
        }

        // Unfilled slots are rendered as empty strings when templates are used directly.
        // Keep lowering focused on the authored content atoms that carry expressions.
        // Fall back to building a plan from raw content for templates that bypassed
        // the full parser pipeline (e.g. test helpers or slot wrappers without formatting).
        let fallback_plan;
        let plan = match &template.render_plan {
            Some(plan) => plan,
            None => {
                fallback_plan = crate::compiler_frontend::ast::templates::template_render_plan::TemplateRenderPlan::from_content(&template.content);
                &fallback_plan
            }
        };
        let chunks = plan.flatten_expressions();
        let chunk_types: Vec<DataType> =
            chunks.iter().map(|chunk| chunk.data_type.clone()).collect();
        let template_function = self.create_runtime_template_function(&chunk_types, location)?;

        self.lower_call_expression(
            CallTarget::UserFunction(template_function),
            &chunks,
            &[DataType::StringSlice],
            location,
        )
    }

    // WHAT: Synthesizes a helper function that concatenates runtime template chunks.
    // WHY: Template lowering reuses normal call/return plumbing instead of special HIR nodes.
    fn create_runtime_template_function(
        &mut self,
        chunk_types: &[DataType],
        location: &TextLocation,
    ) -> Result<FunctionId, CompilerError> {
        let current_function_id = self.current_function_id_or_error(location)?;

        let Some(current_function_name) = self
            .side_table
            .function_name_path(current_function_id)
            .cloned()
        else {
            return_hir_transformation_error!(
                format!(
                    "Missing function symbol for {:?} while lowering runtime template",
                    current_function_id
                ),
                self.hir_error_location(location)
            );
        };

        let template_function_name = self.allocate_template_function_name(&current_function_name);
        let template_function_id = self.allocate_function_id();
        let string_ty = self.intern_type_kind(HirTypeKind::String);

        let entry_region = self.allocate_region_id();
        self.push_region(
            crate::compiler_frontend::hir::hir_nodes::HirRegion::lexical(entry_region, None),
        );

        let entry_block_id = self.allocate_block_id();
        let entry_block = HirBlock {
            id: entry_block_id,
            region: entry_region,
            locals: vec![],
            statements: vec![],
            terminator: HirTerminator::Panic { message: None },
        };
        self.side_table.map_block(location, &entry_block);
        self.push_block(entry_block);

        let template_function = HirFunction {
            id: template_function_id,
            entry: entry_block_id,
            params: vec![],
            return_type: string_ty,
            return_aliases: vec![None],
        };

        self.functions_by_name
            .insert(template_function_name.clone(), template_function_id);
        self.side_table
            .bind_function_name(template_function_id, template_function_name.clone());
        self.side_table.map_function(location, &template_function);
        self.push_function(template_function);

        let mut params = Vec::with_capacity(chunk_types.len());
        for (index, chunk_type) in chunk_types.iter().enumerate() {
            let param_ty = self.lower_template_chunk_type(chunk_type, location)?;
            let local_id = self.allocate_local_id();

            let local_name =
                template_function_name.join_str(&format!("chunk_{index}"), self.string_table);
            let local = HirLocal {
                id: local_id,
                ty: param_ty,
                mutable: false,
                region: entry_region,
                source_info: Some(location.clone()),
            };

            {
                let block = self.block_mut_by_id_or_error(entry_block_id, location)?;
                block.locals.push(local.clone());
            }

            {
                let function = self.function_mut_by_id_or_error(template_function_id, location)?;
                function.params.push(local_id);
            }

            self.side_table.bind_local_name(local_id, local_name);
            self.side_table.map_local_source(&local);

            params.push((local_id, param_ty));
        }

        let return_value =
            self.build_runtime_template_return_expression(&params, location, entry_region);
        self.set_block_terminator(
            entry_block_id,
            HirTerminator::Return(return_value),
            location,
        )?;

        Ok(template_function_id)
    }

    // WHAT: Allocates a collision-free synthesized function name under the current parent symbol.
    // WHY: Runtime template helpers are compiler-generated and must not shadow user functions.
    fn allocate_template_function_name(&mut self, parent_function: &InternedPath) -> InternedPath {
        loop {
            let candidate = parent_function.join_str(
                &format!("__template_fn_{}", self.template_function_counter),
                self.string_table,
            );
            self.template_function_counter += 1;

            if !self.functions_by_name.contains_key(&candidate) {
                return candidate;
            }
        }
    }

    fn lower_template_chunk_type(
        &mut self,
        chunk_type: &DataType,
        location: &TextLocation,
    ) -> Result<TypeId, CompilerError> {
        let normalized = match chunk_type {
            // Runtime template chunks must have a concrete lowered type.
            DataType::Inferred => DataType::CoerceToString,
            other => other.clone(),
        };

        self.lower_data_type(&normalized, location)
    }

    fn build_runtime_template_return_expression(
        &mut self,
        params: &[(LocalId, TypeId)],
        location: &TextLocation,
        region: RegionId,
    ) -> HirExpression {
        let string_ty = self.intern_type_kind(HirTypeKind::String);
        let mut accumulated = self.make_expression(
            location,
            HirExpressionKind::StringLiteral(String::new()),
            string_ty,
            ValueKind::Const,
            region,
        );

        for (local_id, local_ty) in params {
            let chunk = self.make_expression(
                location,
                HirExpressionKind::Load(HirPlace::Local(*local_id)),
                *local_ty,
                ValueKind::Place,
                region,
            );
            let chunk_as_string =
                self.coerce_expression_to_string(chunk, location, string_ty, region);

            accumulated = self.make_expression(
                location,
                HirExpressionKind::BinOp {
                    left: Box::new(accumulated),
                    op: crate::compiler_frontend::hir::hir_nodes::HirBinOp::Add,
                    right: Box::new(chunk_as_string),
                },
                string_ty,
                ValueKind::RValue,
                region,
            );
        }

        accumulated
    }

    pub(crate) fn coerce_expression_to_string(
        &mut self,
        expression: HirExpression,
        location: &TextLocation,
        string_ty: TypeId,
        region: RegionId,
    ) -> HirExpression {
        if matches!(
            self.type_context.get(expression.ty).kind,
            HirTypeKind::String
        ) {
            return expression;
        }

        if matches!(self.type_context.get(expression.ty).kind, HirTypeKind::Unit) {
            return self.make_expression(
                location,
                HirExpressionKind::StringLiteral(String::new()),
                string_ty,
                ValueKind::Const,
                region,
            );
        }

        let empty = self.make_expression(
            location,
            HirExpressionKind::StringLiteral(String::new()),
            string_ty,
            ValueKind::Const,
            region,
        );

        self.make_expression(
            location,
            HirExpressionKind::BinOp {
                left: Box::new(empty),
                op: crate::compiler_frontend::hir::hir_nodes::HirBinOp::Add,
                right: Box::new(expression),
            },
            string_ty,
            ValueKind::RValue,
            region,
        )
    }
}
