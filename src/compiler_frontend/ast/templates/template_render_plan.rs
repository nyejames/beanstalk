//! Intermediate Representation for Template Rendering
//!
//! Converts composed `TemplateContent` into a structured `TemplateRenderPlan`
//! that formatters can process safely without string-level guard characters.

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::templates::template::{
    SlotPlaceholder, TemplateAtom, TemplateContent, TemplateSegmentOrigin,
};
use crate::compiler_frontend::string_interning::StringId;
use crate::compiler_frontend::tokenizer::tokens::TextLocation;

/// A template's content after composition, represented as an ordered
/// sequence of typed pieces ready for formatter runs and final folding.
#[derive(Debug, Clone)]
pub struct TemplateRenderPlan {
    pub pieces: Vec<RenderPiece>,
}

/// Individual piece in a render plan. Child templates are kept as
/// opaque anchors so parent formatters can never inspect their bytes.
#[derive(Debug, Clone)]
pub enum RenderPiece {
    /// Body-origin text eligible for the current template's formatter.
    Text(RenderTextPiece),
    /// Head-origin content that must bypass body formatters.
    HeadContent(RenderTextPiece),
    /// Opaque child template output — position preserved, content sealed.
    ChildTemplate(RenderChildPiece),
    /// Runtime expression that cannot be folded at compile time.
    DynamicExpression(RenderExpressionPiece),
    /// Unresolved slot placeholder that will be filled later.
    Slot(SlotPlaceholder),
    #[allow(dead_code)]
    /// Comment or suppressed content.
    Omitted,
}

#[derive(Debug, Clone)]
pub struct RenderTextPiece {
    pub text: StringId,
    pub location: TextLocation,
}

#[derive(Debug, Clone)]
pub struct RenderChildPiece {
    pub expression: Expression,
}

#[derive(Debug, Clone)]
pub struct RenderExpressionPiece {
    pub expression: Expression,
    pub origin: TemplateSegmentOrigin,
}

/// Stable opaque anchor into compiler-owned non-text pieces.
/// A formatter may preserve or reorder these anchors, but it must not inspect
/// or interpret the content they represent (child templates, dynamic expressions, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FormatterAnchorId(pub usize);

/// The only data a formatter should see:
/// - body text it may rewrite
/// - opaque anchors that preserve ordering around non-text content
#[derive(Debug, Clone)]
pub struct FormatterInput {
    pub pieces: Vec<FormatterInputPiece>,
}

/// A single piece of formatter input — either rewritable text or an opaque anchor.
#[derive(Debug, Clone)]
pub enum FormatterInputPiece {
    Text(FormatterTextPiece),
    Opaque(FormatterAnchorId),
}

/// Body text visible to a formatter, with source location for diagnostics.
#[derive(Debug, Clone)]
pub struct FormatterTextPiece {
    pub text: StringId,
    #[allow(dead_code)] // Preserved for future formatter diagnostics and source mapping
    pub location: TextLocation,
}

impl FormatterInput {
    /// Convenience adapter for string-based formatters such as `$markdown`.
    ///
    /// Concatenates all text pieces into a single buffer, inserts guard-char
    /// placeholders for opaque anchors, runs the formatting closure, then
    /// reconstructs the output by locating the preserved placeholders.
    /// Guard characters are an internal implementation detail of this adapter.
    pub fn invoke_legacy_formatter<F>(
        &self,
        string_table: &crate::compiler_frontend::string_interning::StringTable,
        format_fn: F,
    ) -> FormatterOutput
    where
        F: FnOnce(&mut String),
    {
        use crate::compiler_frontend::ast::templates::styles::TEMPLATE_FORMAT_GUARD_CHAR;

        let mut buffer = String::new();
        let mut anchors_in_order: Vec<FormatterAnchorId> = Vec::new();

        for piece in &self.pieces {
            match piece {
                FormatterInputPiece::Text(text_piece) => {
                    buffer.push_str(string_table.resolve(text_piece.text));
                }
                FormatterInputPiece::Opaque(anchor_id) => {
                    let ordinal = anchors_in_order.len();
                    anchors_in_order.push(*anchor_id);

                    buffer.push(TEMPLATE_FORMAT_GUARD_CHAR);
                    buffer.push_str(&ordinal.to_string());
                    buffer.push(TEMPLATE_FORMAT_GUARD_CHAR);
                }
            }
        }

        format_fn(&mut buffer);

        let mut output_pieces = Vec::new();
        let mut remaining = buffer.as_str();

        for (ordinal, anchor_id) in anchors_in_order.into_iter().enumerate() {
            let placeholder = format!(
                "{guard}{ordinal}{guard}",
                guard = TEMPLATE_FORMAT_GUARD_CHAR
            );

            let Some(split_index) = remaining.find(&placeholder) else {
                continue;
            };

            let before = &remaining[..split_index];
            if !before.is_empty() {
                output_pieces.push(FormatterOutputPiece::Text(before.to_owned()));
            }

            output_pieces.push(FormatterOutputPiece::Opaque(anchor_id));
            remaining = &remaining[split_index + placeholder.len()..];
        }

        if !remaining.is_empty() {
            output_pieces.push(FormatterOutputPiece::Text(remaining.to_owned()));
        }

        FormatterOutput {
            pieces: output_pieces,
        }
    }
}

/// Formatter output — newly generated text and preserved opaque anchors.
/// No slots, no expressions, no child-template contents, no head content.
#[derive(Debug, Clone)]
pub struct FormatterOutput {
    pub pieces: Vec<FormatterOutputPiece>,
}

/// A single piece of formatter output — either transformed text or a preserved anchor.
#[derive(Debug, Clone)]
pub enum FormatterOutputPiece {
    Text(String),
    Opaque(FormatterAnchorId),
}

impl TemplateRenderPlan {
    pub fn from_content(content: &TemplateContent) -> Self {
        let mut pieces = Vec::with_capacity(content.atoms.len());

        for atom in &content.atoms {
            match atom {
                TemplateAtom::Slot(slot) => {
                    pieces.push(RenderPiece::Slot(slot.clone()));
                }
                TemplateAtom::Content(segment) => {
                    if segment.is_child_template_output
                        && let Some(_source) = &segment.source_child_template
                    {
                        pieces.push(RenderPiece::ChildTemplate(RenderChildPiece {
                            expression: segment.expression.clone(),
                        }));
                        continue;
                    }

                    if let ExpressionKind::StringSlice(intern_id) = segment.expression.kind {
                        let text_piece = RenderTextPiece {
                            text: intern_id,
                            location: segment.expression.location.clone(),
                        };

                        if segment.origin == TemplateSegmentOrigin::Head {
                            pieces.push(RenderPiece::HeadContent(text_piece));
                        } else {
                            pieces.push(RenderPiece::Text(text_piece));
                        }
                    } else {
                        pieces.push(RenderPiece::DynamicExpression(RenderExpressionPiece {
                            expression: segment.expression.clone(),
                            origin: segment.origin,
                        }));
                    }
                }
            }
        }

        Self { pieces }
    }

    /// Reconstructs a `TemplateContent` from this render plan.
    /// Kept for future round-trip and debug tooling — not yet wired into the main pipeline.
    #[allow(dead_code)]
    pub fn rebuild_content(&self) -> TemplateContent {
        use crate::compiler_frontend::ast::expressions::expression::Expression;
        use crate::compiler_frontend::ast::templates::template::{
            TemplateAtom, TemplateContent, TemplateSegment, TemplateSegmentOrigin,
        };
        use crate::compiler_frontend::datatypes::DataType;
        use crate::compiler_frontend::datatypes::Ownership;

        let mut atoms = Vec::new();

        for piece in &self.pieces {
            match piece {
                RenderPiece::HeadContent(p) => {
                    atoms.push(TemplateAtom::Content(TemplateSegment::new(
                        Expression::string_slice(
                            p.text,
                            p.location.clone(),
                            Ownership::ImmutableOwned,
                        ),
                        TemplateSegmentOrigin::Head,
                    )));
                }
                RenderPiece::Text(p) => {
                    atoms.push(TemplateAtom::Content(TemplateSegment::new(
                        Expression::string_slice(
                            p.text,
                            p.location.clone(),
                            Ownership::ImmutableOwned,
                        ),
                        TemplateSegmentOrigin::Body,
                    )));
                }
                RenderPiece::ChildTemplate(c) => {
                    atoms.push(TemplateAtom::Content(TemplateSegment {
                        expression: Expression {
                            kind: c.expression.kind.clone(),
                            data_type: DataType::Template,
                            ownership: Ownership::ImmutableOwned,
                            location: c.expression.location.clone(),
                        },
                        origin: TemplateSegmentOrigin::Body,
                        is_child_template_output: true,
                        source_child_template: None,
                    }));
                }
                RenderPiece::DynamicExpression(p) => {
                    atoms.push(TemplateAtom::Content(TemplateSegment::new(
                        p.expression.clone(),
                        p.origin,
                    )));
                }
                RenderPiece::Slot(placeholder) => {
                    atoms.push(TemplateAtom::Slot(placeholder.clone()));
                }
                RenderPiece::Omitted => {
                    // Omitted content produces no AST representation
                }
            }
        }

        TemplateContent { atoms }
    }

    /// Extracts all evaluatable expressions from the plan, discarding slots and omissions.
    pub fn flatten_expressions(&self) -> Vec<Expression> {
        use crate::compiler_frontend::ast::expressions::expression::Expression;
        use crate::compiler_frontend::datatypes::Ownership;

        self.pieces
            .iter()
            .filter_map(|piece| match piece {
                RenderPiece::Text(p) | RenderPiece::HeadContent(p) => Some(
                    Expression::string_slice(p.text, p.location.clone(), Ownership::ImmutableOwned),
                ),
                RenderPiece::ChildTemplate(p) => Some(p.expression.clone()),
                RenderPiece::DynamicExpression(p) => Some(p.expression.clone()),
                RenderPiece::Slot(_) | RenderPiece::Omitted => None,
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "tests/render_plan_tests.rs"]
mod render_plan_tests;
