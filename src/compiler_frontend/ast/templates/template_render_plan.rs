//! Intermediate Representation for Template Rendering
//!
//! Converts composed `TemplateContent` into a structured `TemplateRenderPlan`
//! that formatters can process safely without string-level guard characters.

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::templates::template::{
    SlotPlaceholder, TemplateAtom, TemplateContent, TemplateSegmentOrigin,
};
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

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
}

#[derive(Debug, Clone)]
pub struct RenderTextPiece {
    pub text: StringId,
    pub location: SourceLocation,
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

/// Structural classification for opaque formatter anchors.
///
/// WHAT:
/// - Distinguishes folded child-template outputs from generic dynamic expressions.
///
/// WHY:
/// - Some formatters such as `$markdown` need narrow structural behavior changes
///   for direct child templates without inspecting their sealed content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FormatterOpaqueKind {
    ChildTemplate,
    DynamicExpression,
}

/// Opaque formatter piece metadata carried through whitespace/formatter pipelines.
///
/// WHAT:
/// - Preserves both the stable side-table id and the anchor classification.
///
/// WHY:
/// - Formatter chaining must retain whether an anchor is a child template or a
///   generic runtime expression without exposing the underlying content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FormatterOpaquePiece {
    pub id: FormatterAnchorId,
    pub kind: FormatterOpaqueKind,
}

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
    Opaque(FormatterOpaquePiece),
}

/// Body text visible to a formatter, with source location for diagnostics.
#[derive(Debug, Clone)]
pub struct FormatterTextPiece {
    pub text: StringId,
    pub location: SourceLocation,
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
    Opaque(FormatterOpaquePiece),
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
    pub fn rebuild_content(&self) -> TemplateContent {
        use crate::compiler_frontend::ast::templates::template::{
            TemplateAtom, TemplateContent, TemplateSegment, TemplateSegmentOrigin,
        };
        use crate::compiler_frontend::datatypes::DataType;
        use crate::compiler_frontend::value_mode::ValueMode;

        let mut atoms = Vec::new();

        for piece in &self.pieces {
            match piece {
                RenderPiece::HeadContent(p) => {
                    atoms.push(TemplateAtom::Content(TemplateSegment::new(
                        Expression::string_slice(
                            p.text,
                            p.location.clone(),
                            ValueMode::ImmutableOwned,
                        ),
                        TemplateSegmentOrigin::Head,
                    )));
                }
                RenderPiece::Text(p) => {
                    atoms.push(TemplateAtom::Content(TemplateSegment::new(
                        Expression::string_slice(
                            p.text,
                            p.location.clone(),
                            ValueMode::ImmutableOwned,
                        ),
                        TemplateSegmentOrigin::Body,
                    )));
                }
                RenderPiece::ChildTemplate(c) => {
                    atoms.push(TemplateAtom::Content(TemplateSegment {
                        expression: Expression {
                            kind: c.expression.kind.clone(),
                            data_type: DataType::Template,
                            value_mode: ValueMode::ImmutableOwned,
                            location: c.expression.location.clone(),
                            contains_regular_division: c.expression.contains_regular_division,
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
            }
        }

        TemplateContent { atoms }
    }

    /// Extracts all evaluatable expressions from the plan, discarding slots and omissions.
    pub fn flatten_expressions(&self) -> Vec<Expression> {
        use crate::compiler_frontend::ast::expressions::expression::Expression;
        use crate::compiler_frontend::value_mode::ValueMode;

        self.pieces
            .iter()
            .filter_map(|piece| match piece {
                RenderPiece::Text(p) | RenderPiece::HeadContent(p) => Some(
                    Expression::string_slice(p.text, p.location.clone(), ValueMode::ImmutableOwned),
                ),
                RenderPiece::ChildTemplate(p) => Some(p.expression.clone()),
                RenderPiece::DynamicExpression(p) => Some(p.expression.clone()),
                RenderPiece::Slot(_) => None,
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "tests/render_plan_tests.rs"]
mod render_plan_tests;
