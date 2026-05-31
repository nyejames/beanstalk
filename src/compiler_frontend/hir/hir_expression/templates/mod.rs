//! Runtime-template lowering entry points for HIR expression construction.
//!
//! WHAT: routes finalized AST runtime templates through the inline accumulator path.
//! WHY: AST owns template composition, foldability, and render-plan preparation. HIR only lowers
//! the runtime surface that remains after those decisions are complete.
//!
//! Submodule map:
//! - `append_context`: shared append target and runtime slot source/site context.
//! - `linear`: ordinary render-plan appending for runtime templates without control flow.
//! - `control_flow`: structured `if` / `loop` dispatch that mutates the enclosing CFG lazily.
//! - `render_append`: render-plan chunk appending and string coercion shared by runtime paths.
//! - `option_capture`: option-present template `if` capture lowering.
//! - `aggregate`: shared aggregate wrapping after a loop or child emitted output.
//! - `slot_application`: AST-planned runtime slots lowered through slot accumulators.

use crate::compiler_frontend::ast::templates::template::{TemplateConstValueKind, TemplateType};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_hir_transformation_error;

use super::LoweredExpression;

mod aggregate;
mod append_context;
mod control_flow;
mod linear;
mod option_capture;
mod render_append;
mod slot_application;

impl<'a> HirBuilder<'a> {
    // WHAT: Lowers runtime template expressions into inline CFG appends.
    // WHY: AST must already have folded any compile-time template value before HIR sees it.
    pub(crate) fn lower_runtime_template_expression(
        &mut self,
        template: &Template,
        location: &SourceLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        self.validate_runtime_template_lowering_input(template, location)?;

        if let Some(plan) = &template.runtime_slot_application {
            self.lower_runtime_slot_application_template_expression(plan, location)
        } else if template.control_flow.is_some() {
            self.lower_runtime_control_flow_template_expression(template, location)
        } else {
            self.lower_runtime_linear_template_expression(template, location)
        }
    }

    fn validate_runtime_template_lowering_input(
        &mut self,
        template: &Template,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        if !self.currently_lowering_constants.is_empty() {
            return_hir_transformation_error!(
                "Template reached HIR constant lowering before AST materialized the compile-time value.",
                self.hir_error_location(location)
            );
        }

        if matches!(template.kind, TemplateType::SlotInsert(_)) {
            return_hir_transformation_error!(
                "Template helper reached HIR runtime-template lowering before AST wrapper-slot resolution.",
                self.hir_error_location(location)
            );
        }

        match template.const_value_kind() {
            TemplateConstValueKind::RenderableString => {
                return_hir_transformation_error!(
                    "Compile-time template reached HIR runtime-template lowering before AST folding.",
                    self.hir_error_location(location)
                );
            }
            TemplateConstValueKind::SlotInsertHelper => {
                return_hir_transformation_error!(
                    "Template helper reached HIR runtime-template lowering before AST wrapper-slot resolution.",
                    self.hir_error_location(location)
                );
            }
            TemplateConstValueKind::WrapperTemplate | TemplateConstValueKind::NonConst => {}
        }

        Ok(())
    }
}
