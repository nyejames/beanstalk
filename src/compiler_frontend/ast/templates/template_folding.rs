//! Compile-time template folding.
//!
//! WHAT: Converts fully-resolved template content into interned string IDs
//! by recursively folding atoms (text, nested templates, head/body segments).
//!
//! WHY: Separates folding logic from parsing and composition so it can later
//! be rebuilt on top of the render-plan IR without entangling parser code.

use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::templates::template::{Style, TemplateType};
use crate::compiler_frontend::ast::templates::template_composition::effective_inherited_style_for_nested_templates;
use crate::compiler_frontend::ast::templates::template_formatting::apply_body_formatter;
use crate::compiler_frontend::ast::templates::template_render_plan::{
    RenderPiece, TemplateRenderPlan,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::{ast_log, return_compiler_error};

impl Template {
    /// Folds a fully-resolved template into an interned string ID.
    /// Applies deferred formatting if needed, then recursively folds all pieces.
    pub fn fold_into_stringid(
        &self,
        // Passed through for future use: formatter inheritance between nested templates.
        inherited_style: &Option<Style>,
        string_table: &mut StringTable,
    ) -> Result<StringId, CompilerError> {
        let plan = if self.content_needs_formatting {
            apply_body_formatter(&self.unformatted_content, &self.style, string_table)
        } else {
            self.render_plan
                .clone()
                .unwrap_or_else(|| TemplateRenderPlan::from_content(&self.content))
        };

        fold_plan(&plan, inherited_style, &self.style, string_table)
    }
}

fn fold_plan(
    plan: &TemplateRenderPlan,
    _inherited_style: &Option<Style>,
    style: &Style,
    string_table: &mut StringTable,
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
            RenderPiece::Slot(_) | RenderPiece::Omitted => {
                // Unfilled slots and omitted content intentionally fold to empty — the
                // surrounding authored content still renders.
                None
            }
        };

        let Some(expression_kind) = expression else {
            continue;
        };

        match expression_kind {
            ExpressionKind::StringSlice(string) => {
                final_string.push_str(string_table.resolve(string));
            }

            ExpressionKind::Float(float) => {
                final_string.push_str(&float.to_string());
            }

            ExpressionKind::Int(int) => {
                final_string.push_str(&int.to_string());
            }

            ExpressionKind::Bool(value) => {
                final_string.push_str(&value.to_string());
            }

            ExpressionKind::Char(value) => {
                final_string.push(value);
            }

            ExpressionKind::Template(template) => {
                if matches!(template.kind, TemplateType::Comment(_)) {
                    continue;
                }

                if matches!(template.kind, TemplateType::SlotInsert(_))
                    || template.content.contains_slot_insertions()
                {
                    return_compiler_error!(
                        "Invalid template content reached string folding: unresolved slot insertions cannot be rendered directly."
                    );
                }

                // Nested templates that became fully resolved only after wrapper
                // composition are folded here to preserve authored nesting order.
                let nested_inherited_style = effective_inherited_style_for_nested_templates(style);
                let folded_nested =
                    template.fold_into_stringid(&nested_inherited_style, string_table)?;
                final_string.push_str(string_table.resolve(folded_nested));
            }

            // Anything else can't be folded and should not get to this stage.
            _ => {
                return_compiler_error!(
                    "Invalid Expression Used Inside template when trying to fold into a string.\
                         The compiler_frontend should not be trying to fold this template."
                )
            }
        }
    }

    ast_log!("Folded template into: ", final_string);

    Ok(string_table.intern(&final_string))
}
