//! Shared template body formatting pipeline.
//!
//! WHAT:
//! - Collects contiguous body-run pieces (text, child templates, dynamic expressions)
//!   and applies whitespace passes and optional style formatter logic.
//! - Maps non-text pieces to opaque `FormatterAnchorId` anchors so that parent
//!   formatters can never inspect child template or expression content.
//!
//! WHY:
//! - Keeps `create_template_node.rs` focused on AST construction/composition.
//! - Parent formatters such as `$markdown` should ignore child template output
//!   entirely rather than reparsing or escaping it.

use crate::compiler_frontend::ast::templates::styles::whitespace::{
    TemplateBodyRunPosition, TemplateWhitespacePassProfile, apply_whitespace_passes_to_input,
};

use crate::compiler_frontend::ast::templates::template::{
    BodyWhitespacePolicy, Style, TemplateContent,
};
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

use crate::compiler_frontend::ast::templates::template_render_plan::{
    FormatterAnchorId, FormatterInput, FormatterInputPiece, FormatterOpaqueKind,
    FormatterOpaquePiece, FormatterOutputPiece, FormatterTextPiece, RenderExpressionPiece,
    RenderPiece, RenderTextPiece, TemplateRenderPlan,
};

pub(crate) struct BodyFormattingResult {
    pub plan: TemplateRenderPlan,
    pub warnings: Vec<CompilerWarning>,
}

/// Applies the body formatter and whitespace passes to a template's content.
///
/// Walks the render plan, groups contiguous body-run pieces into formatter runs,
/// maps non-text pieces to opaque anchors, then runs whitespace passes and the
/// style formatter before mapping results back to render pieces.
pub(crate) fn apply_body_formatter(
    content: &TemplateContent,
    style: &Style,
    string_table: &mut StringTable,
) -> Result<BodyFormattingResult, CompilerMessages> {
    let mut plan = TemplateRenderPlan::from_content(content);

    let formatter = style.formatter.as_ref();
    let implicit_default_whitespace_pass = (style.body_whitespace_policy
        == BodyWhitespacePolicy::DefaultTemplateBehavior
        && formatter.is_none())
    .then_some(TemplateWhitespacePassProfile::default_template_body());

    if implicit_default_whitespace_pass.is_none() && formatter.is_none() {
        return Ok(BodyFormattingResult {
            plan,
            warnings: Vec::new(),
        });
    }

    let mut new_plan_pieces = Vec::with_capacity(plan.pieces.len());
    let mut all_warnings: Vec<CompilerWarning> = Vec::new();

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

    // Processes a contiguous body run through whitespace passes and the style formatter.
    // Non-text pieces (child templates, dynamic expressions) are mapped to opaque anchors
    // so formatters never see their content. After formatting, anchors are mapped back.
    let process_run = |run: Vec<RenderPiece>,
                       run_position: TemplateBodyRunPosition,
                       string_table: &mut StringTable|
     -> Result<(Vec<RenderPiece>, Vec<CompilerWarning>), CompilerMessages> {
        if run.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }

        // Formatter-emitted text is derived from this run, not authored directly.
        // We intentionally preserve only a coarse representative location span.
        let representative_location = representative_text_location_for_run(&run);

        // Build the opaque-anchor side-table: each non-text piece gets a FormatterAnchorId
        // that the formatter sees but cannot inspect.
        let mut input_pieces = Vec::with_capacity(run.len());
        let mut anchor_side_table: Vec<RenderPiece> = Vec::new();

        for piece in &run {
            match piece {
                RenderPiece::Text(t) => {
                    input_pieces.push(FormatterInputPiece::Text(FormatterTextPiece {
                        text: t.text,
                        location: t.location.clone(),
                    }));
                }
                // Child templates and dynamic expressions both become opaque anchors.
                other => {
                    let anchor_id = FormatterAnchorId(anchor_side_table.len());
                    anchor_side_table.push(other.clone());
                    input_pieces.push(FormatterInputPiece::Opaque(FormatterOpaquePiece {
                        id: anchor_id,
                        kind: opaque_kind_for_render_piece(other).map_err(|error| {
                            CompilerMessages::from_error_ref(error, string_table)
                        })?,
                    }));
                }
            }
        }

        let input = FormatterInput {
            pieces: input_pieces,
        };

        // 1. Pre-format whitespace passes (operates directly on structured input).
        let mut output =
            apply_whitespace_passes_to_input(input, pre_format_passes, run_position, string_table);

        // 2. Style formatter
        let mut formatter_warnings = Vec::new();
        if let Some(fmt) = formatter {
            let next_input = output_to_input(output, &representative_location, string_table);
            let formatter_result = fmt.formatter.format(next_input, string_table)?;
            formatter_warnings.extend(formatter_result.warnings);
            output = formatter_result.output;
        }

        // 3. Post-format whitespace passes
        if !post_format_passes.is_empty() {
            let post_input = output_to_input(output, &representative_location, string_table);
            output = apply_whitespace_passes_to_input(
                post_input,
                post_format_passes,
                run_position,
                string_table,
            );
        }

        // Map formatter output back to render pieces using the anchor side-table.
        let mut replacement_pieces = Vec::with_capacity(output.pieces.len());
        for out_piece in output.pieces {
            match out_piece {
                FormatterOutputPiece::Text(text) => {
                    let id = string_table.intern(&text);
                    replacement_pieces.push(RenderPiece::Text(RenderTextPiece {
                        text: id,
                        location: representative_location.clone(),
                    }));
                }
                FormatterOutputPiece::Opaque(anchor) => {
                    replacement_pieces.push(anchor_side_table[anchor.id.0].clone());
                }
            }
        }

        Ok((replacement_pieces, formatter_warnings))
    };

    let mut is_first_run = true;
    for piece in std::mem::take(&mut plan.pieces) {
        match &piece {
            // Body text and non-head non-slot content forms contiguous formatter runs.
            RenderPiece::Text(_)
            | RenderPiece::ChildTemplate(_)
            | RenderPiece::DynamicExpression(RenderExpressionPiece {
                origin:
                    crate::compiler_frontend::ast::templates::template::TemplateSegmentOrigin::Body,
                ..
            }) => {
                current_run.push(piece);
            }
            _other => {
                if !current_run.is_empty() {
                    let run_position = if is_first_run {
                        TemplateBodyRunPosition::First
                    } else {
                        TemplateBodyRunPosition::Middle
                    };
                    is_first_run = false;
                    let (replacement, warnings) =
                        process_run(std::mem::take(&mut current_run), run_position, string_table)?;
                    all_warnings.extend(warnings);
                    new_plan_pieces.extend(replacement);
                }
                new_plan_pieces.push(piece);
            }
        }
    }

    if !current_run.is_empty() {
        let run_position = if is_first_run {
            TemplateBodyRunPosition::Only
        } else {
            TemplateBodyRunPosition::Last
        };
        let (replacement, warnings) = process_run(current_run, run_position, string_table)?;
        all_warnings.extend(warnings);
        new_plan_pieces.extend(replacement);
    }

    plan.pieces = new_plan_pieces;
    Ok(BodyFormattingResult {
        plan,
        warnings: all_warnings,
    })
}

/// Converts formatter output back into formatter input for chaining pipeline stages.
/// Text pieces are interned, opaque anchors are preserved as-is.
fn output_to_input(
    output: crate::compiler_frontend::ast::templates::template_render_plan::FormatterOutput,
    representative_location: &SourceLocation,
    string_table: &mut StringTable,
) -> FormatterInput {
    let pieces = output
        .pieces
        .into_iter()
        .map(|piece| match piece {
            FormatterOutputPiece::Text(t) => FormatterInputPiece::Text(FormatterTextPiece {
                text: string_table.intern(&t),
                location: representative_location.clone(),
            }),
            FormatterOutputPiece::Opaque(anchor) => FormatterInputPiece::Opaque(anchor),
        })
        .collect();
    FormatterInput { pieces }
}

/// Narrows render-plan pieces into the formatter-visible opaque anchor kinds.
fn opaque_kind_for_render_piece(piece: &RenderPiece) -> Result<FormatterOpaqueKind, CompilerError> {
    match piece {
        RenderPiece::ChildTemplate(_) => Ok(FormatterOpaqueKind::ChildTemplate),
        RenderPiece::DynamicExpression(_) => Ok(FormatterOpaqueKind::DynamicExpression),
        other => Err(CompilerError::compiler_error(format!(
            "Template formatter attempted to anchor unsupported render piece kind: {other:?}"
        ))),
    }
}

/// Derives a coarse source location for text emitted by formatter/whitespace output.
///
/// WHAT:
/// - Prefers an aggregated span across literal text pieces in the original run.
/// - Falls back to the first available dynamic/child piece location when a run has no text.
///
/// WHY:
/// - Formatter output can rewrite arbitrary text, so exact per-character mapping is
///   not feasible here. A representative run span preserves useful provenance for
///   diagnostics without pretending to be precise.
fn representative_text_location_for_run(run: &[RenderPiece]) -> SourceLocation {
    if let Some(aggregated) = aggregate_text_piece_location(run) {
        return aggregated;
    }

    for piece in run {
        match piece {
            RenderPiece::Text(text_piece) => return text_piece.location.clone(),
            RenderPiece::ChildTemplate(child_piece) => {
                return child_piece.expression.location.clone();
            }
            RenderPiece::DynamicExpression(dynamic_piece) => {
                return dynamic_piece.expression.location.clone();
            }
            RenderPiece::HeadContent(_) | RenderPiece::Slot(_) => {}
        }
    }

    SourceLocation::default()
}

fn aggregate_text_piece_location(run: &[RenderPiece]) -> Option<SourceLocation> {
    let mut first: Option<SourceLocation> = None;
    let mut last: Option<SourceLocation> = None;

    for piece in run {
        if let RenderPiece::Text(text_piece) = piece {
            if first.is_none() {
                first = Some(text_piece.location.clone());
            }
            last = Some(text_piece.location.clone());
        }
    }

    let start = first?;
    let end = last?;

    if start.scope != end.scope {
        return Some(start);
    }

    Some(SourceLocation {
        scope: start.scope,
        start_pos: start.start_pos,
        end_pos: end.end_pos,
    })
}
