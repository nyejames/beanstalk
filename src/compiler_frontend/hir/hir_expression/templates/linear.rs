//! Linear runtime-template lowering through the inline accumulator path.
//!
//! WHAT: appends ordinary runtime template render plans directly into the enclosing CFG.
//! WHY: linear and control-flow templates must share one runtime concatenation path so future
//! template features do not have to preserve separate call-based semantics.

use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::hir::expressions::{HirExpressionKind, ValueKind};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_expression::LoweredExpression;
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_hir_transformation_error;

impl<'a> HirBuilder<'a> {
    // WHAT: Lowers ordinary runtime templates by appending their prepared render plan inline.
    // WHY: this keeps string coercion and chunk ordering owned by `render_append` for every
    //      runtime template shape instead of splitting linear templates into helper functions.
    pub(super) fn lower_runtime_linear_template_expression(
        &mut self,
        template: &Template,
        location: &SourceLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        let Some(plan) = &template.render_plan else {
            return_hir_transformation_error!(
                "Runtime template reached HIR without a render plan. AST must finalize template planning before HIR lowering.",
                self.hir_error_location(location)
            );
        };

        if template
            .reactive_template_metadata()
            .is_some_and(|metadata| metadata.has_runtime_dependency())
        {
            return self.lower_runtime_reactive_linear_template_expression(plan, location);
        }

        let accumulator = self.initialize_runtime_template_accumulator(location)?;
        self.append_template_render_plan_to_accumulator(plan, accumulator, location)?;

        let region = self.current_region_or_error(location)?;
        let value = self.make_expression(
            location,
            HirExpressionKind::Copy(HirPlace::Local(accumulator)),
            builtin_type_ids::STRING,
            ValueKind::RValue,
            region,
        );

        Ok(LoweredExpression {
            prelude: vec![],
            value,
        })
    }
}
