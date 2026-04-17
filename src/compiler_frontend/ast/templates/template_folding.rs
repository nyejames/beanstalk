//! Compile-time template folding.
//!
//! WHAT: Converts fully-resolved template content into interned string IDs
//! by recursively folding atoms (text, nested templates, head/body segments).
//!
//! WHY: Separates folding logic from parsing and composition so it can later
//! be rebuilt on top of the render-plan IR without entangling parser code.

use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::templates::template::TemplateType;
use crate::compiler_frontend::ast::templates::template_formatting::apply_body_formatter;
use crate::compiler_frontend::ast::templates::template_render_plan::{
    RenderPiece, TemplateRenderPlan,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::type_coercion::string::{
    FoldedStringPiece, fold_expression_kind_to_string,
};
use crate::{ast_log, return_compiler_error};

/// Required context for compile-time template folding.
///
/// WHAT: carries all project-aware state that folding can require.
/// WHY: folding must not rely on ad-hoc inherited-style placeholders or
///       resolver-less fallback branches.
pub struct TemplateFoldContext<'a> {
    pub string_table: &'a mut StringTable,
    pub(crate) project_path_resolver: &'a ProjectPathResolver,
    pub path_format_config: &'a PathStringFormatConfig,
    pub source_file_scope: &'a InternedPath,
}

impl Template {
    /// Folds a fully-resolved template into an interned string ID.
    /// Applies deferred formatting if needed, then recursively folds all pieces.
    pub fn fold_into_stringid(
        &self,
        fold_context: &mut TemplateFoldContext<'_>,
    ) -> Result<StringId, CompilerError> {
        // Keep resolver/path/scope in the fold contract even when a specific template
        // only needs string interning today. Callers must propagate full project context.
        let _required_project_context = (
            fold_context.project_path_resolver,
            fold_context.path_format_config,
            fold_context.source_file_scope,
        );

        let plan = if self.content_needs_formatting {
            apply_body_formatter(
                &self.unformatted_content,
                &self.style,
                fold_context.string_table,
            )
            .map(|result| result.plan)
            .map_err(|messages| {
                messages.errors.into_iter().next().unwrap_or_else(|| {
                    CompilerError::compiler_error(
                        "Template formatter failed without returning a compiler error.",
                    )
                })
            })?
        } else {
            self.render_plan
                .clone()
                .unwrap_or_else(|| TemplateRenderPlan::from_content(&self.content))
        };

        fold_plan(&plan, fold_context)
    }
}

fn fold_plan(
    plan: &TemplateRenderPlan,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<StringId, CompilerError> {
    let mut final_string = String::new();

    for piece in &plan.pieces {
        // Map each render piece to an optional expression to fold. Head and body text
        // are treated identically during folding — the distinction only matters for
        // formatter boundary detection, which already ran before this stage.
        let expression = match piece {
            RenderPiece::Text(p) => Some(ExpressionKind::StringSlice(p.text)),
            RenderPiece::HeadContent(p) => Some(ExpressionKind::StringSlice(p.text)),
            RenderPiece::ChildTemplate(p) => Some(p.expression.kind.clone()),
            RenderPiece::DynamicExpression(p) => Some(p.expression.kind.clone()),
            RenderPiece::Slot(_) => {
                // Unfilled slots intentionally fold to empty; the surrounding authored
                // content still renders.
                None
            }
        };

        let Some(expression_kind) = expression else {
            continue;
        };

        // Delegate the "what can become string content" policy to the coercion module.
        // Template mechanics (slot resolution, formatting) live in the template subsystem;
        // the decision about which expression kinds are renderable lives in type_coercion::string.
        match fold_expression_kind_to_string(&expression_kind, fold_context.string_table) {
            Some(FoldedStringPiece::Text(text)) => {
                final_string.push_str(&text);
            }

            Some(FoldedStringPiece::Char(ch)) => {
                final_string.push(ch);
            }

            Some(FoldedStringPiece::Skip) => {
                continue;
            }

            Some(FoldedStringPiece::NestedTemplate) => {
                // The expression kind was a Template — retrieve the template from the
                // original piece to recursively fold it with full project context.
                let ExpressionKind::Template(template) = expression_kind else {
                    return_compiler_error!(
                        "String coercion returned NestedTemplate for a non-Template expression kind."
                    );
                };

                if matches!(template.kind, TemplateType::SlotInsert(_))
                    || template.content.contains_slot_insertions()
                {
                    return_compiler_error!(
                        "Invalid template content reached string folding: unresolved slot insertions cannot be rendered directly."
                    );
                }

                // Nested templates that became fully resolved only after wrapper
                // composition are folded here to preserve authored nesting order.
                let folded_nested = template.fold_into_stringid(fold_context)?;
                final_string.push_str(fold_context.string_table.resolve(folded_nested));
            }

            // Anything else can't be folded and should not get to this stage.
            None => {
                return_compiler_error!(
                    "Invalid Expression Used Inside template when trying to fold into a string.\
                         The compiler_frontend should not be trying to fold this template."
                );
            }
        }
    }

    ast_log!("Folded template into: ", final_string);

    Ok(fold_context.string_table.intern(&final_string))
}
