//! Shared template body formatting pipeline.
//!
//! WHAT:
//! - Collects contiguous template body string runs and applies whitespace passes
//!   and optional style formatter logic.
//! - Preserves child template positions using guarded numeric placeholders while
//!   parent formatters run over the surrounding body text.
//!
//! WHY:
//! - Keeps `create_template_node.rs` focused on AST construction/composition.
//! - Parent formatters such as `$markdown` should ignore child template output
//!   entirely rather than reparsing or escaping it.
//!
//! NOTE:
//! - This file uses the requested minimal placeholder form:
//!   `TEMPLATE_FORMAT_GUARD_CHAR + "12" + TEMPLATE_FORMAT_GUARD_CHAR`.
//! - Because built-in formatters strip the guard chars and copy only the payload
//!   through, reinsertion later searches for the numeric payload text in order.
//!   That is not collision-proof if the formatted output naturally contains the
//!   same digit sequence in the wrong place.

use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::templates::styles::whitespace::{
    TemplateBodyRunPosition, TemplateWhitespacePassProfile, apply_whitespace_passes,
};
use crate::compiler_frontend::ast::templates::template::{
    BodyWhitespacePolicy, Style, TemplateContent,
};
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::TextLocation;

use crate::compiler_frontend::ast::templates::template_render_plan::{
    FormatterInput, FormatterInputPiece, FormatterOutputPiece, RenderPiece, RenderTextPiece,
    TemplateRenderPlan,
};

pub(crate) fn apply_body_formatter(
    content: &TemplateContent,
    style: &Style,
    string_table: &mut StringTable,
) -> TemplateRenderPlan {
    let mut plan = TemplateRenderPlan::from_content(content);

    let formatter = style.formatter.as_ref();
    let implicit_default_whitespace_pass = (style.body_whitespace_policy
        == BodyWhitespacePolicy::DefaultTemplateBehavior
        && formatter.is_none())
    .then_some(TemplateWhitespacePassProfile::default_template_body());

    if implicit_default_whitespace_pass.is_none() && formatter.is_none() {
        return plan;
    }

    let mut new_plan_pieces = Vec::with_capacity(plan.pieces.len());

    let pre_format_passes = formatter
        .map(|f| f.pre_format_whitespace_passes.as_slice())
        .unwrap_or_else(|| {
            if let Some(pass) = &implicit_default_whitespace_pass {
                std::slice::from_ref(pass)
            } else {
                &[]
            }
        });

    let post_format_passes = formatter
        .map(|f| f.post_format_whitespace_passes.as_slice())
        .unwrap_or(&[]);

    let mut current_run = Vec::new();

    let process_run = |run: Vec<RenderPiece>,
                       run_position: TemplateBodyRunPosition,
                       string_table: &mut StringTable|
     -> Vec<RenderPiece> {
        if run.is_empty() {
            return Vec::new();
        }

        let mut input_pieces = Vec::with_capacity(run.len());
        let mut child_templates = Vec::new();

        for piece in &run {
            match piece {
                RenderPiece::Text(t) => input_pieces.push(FormatterInputPiece::Text(t.text)),
                RenderPiece::ChildTemplate(c) => {
                    if let ExpressionKind::StringSlice(id) = c.expression.kind {
                        input_pieces.push(FormatterInputPiece::ChildTemplate(id));
                        child_templates.push(c.clone());
                    } else {
                        unreachable!("Child template expression must be StringSlice");
                    }
                }
                _ => unreachable!("Only text and child templates are formatted"),
            }
        }

        let input = FormatterInput {
            pieces: input_pieces,
        };
        let child_map: Vec<_> = child_templates
            .iter()
            .map(|c| {
                if let ExpressionKind::StringSlice(id) = c.expression.kind {
                    id
                } else {
                    unreachable!()
                }
            })
            .collect();

        // 1. Whitespace passes (Legacy `&mut String` boundary)
        let mut output = input.invoke_legacy_formatter(string_table, |text_buffer| {
            apply_whitespace_passes(text_buffer, pre_format_passes, run_position);
        });

        // 2. Style formatter
        if let Some(fmt) = formatter {
            let next_input = output.into_input(string_table, &child_map);
            output = fmt.formatter.format(next_input, string_table);
        }

        // 3. Post-format whitespace passes
        let final_output = output
            .into_input(string_table, &child_map)
            .invoke_legacy_formatter(string_table, |text_buffer| {
                apply_whitespace_passes(text_buffer, post_format_passes, run_position);
            });

        // Map `FormatterOutputPiece` back to `RenderPiece`
        let mut replacement_pieces = Vec::with_capacity(final_output.pieces.len());
        for out_piece in final_output.pieces {
            match out_piece {
                FormatterOutputPiece::Text(text) => {
                    let id = string_table.intern(&text);
                    replacement_pieces.push(RenderPiece::Text(RenderTextPiece {
                        text: id,
                        location: TextLocation::default(),
                    }));
                }
                FormatterOutputPiece::ChildTemplate(index) => {
                    replacement_pieces
                        .push(RenderPiece::ChildTemplate(child_templates[index].clone()));
                }
            }
        }

        replacement_pieces
    };

    let mut is_first_run = true;
    for piece in std::mem::take(&mut plan.pieces) {
        match piece {
            RenderPiece::Text(_) | RenderPiece::ChildTemplate(_) => {
                current_run.push(piece);
            }
            other => {
                if !current_run.is_empty() {
                    let run_position = if is_first_run {
                        TemplateBodyRunPosition::First
                    } else {
                        TemplateBodyRunPosition::Middle
                    };
                    is_first_run = false;
                    new_plan_pieces.extend(process_run(
                        std::mem::take(&mut current_run),
                        run_position,
                        string_table,
                    ));
                }
                new_plan_pieces.push(other);
            }
        }
    }

    if !current_run.is_empty() {
        let run_position = if is_first_run {
            TemplateBodyRunPosition::Only
        } else {
            TemplateBodyRunPosition::Last
        };
        new_plan_pieces.extend(process_run(current_run, run_position, string_table));
    }

    plan.pieces = new_plan_pieces;
    plan
}
