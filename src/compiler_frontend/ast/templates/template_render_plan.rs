//! Intermediate Representation for Template Rendering
//!
//! Converts composed `TemplateContent` into a structured `TemplateRenderPlan`
//! that formatters can process safely without string-level guard characters.

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::expressions::expression::ExpressionValueShape;
use crate::compiler_frontend::ast::expressions::expression_types::ConstRecordState;
use crate::compiler_frontend::ast::templates::template::{
    ReactiveSubscription, SlotPlaceholder, TemplateAtom, TemplateContent, TemplateSegment,
    TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateControlFlow, TemplateLoopControlSignal,
};
use crate::compiler_frontend::ast::templates::template_slots::RuntimeSlotSiteId;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringIdRemap};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;

// -------------------------
//  Render Plan IR
// -------------------------

/// A template's content after composition, represented as an ordered
/// sequence of typed pieces ready for formatter runs and final folding.
#[derive(Debug, Clone)]
pub struct TemplateRenderPlan {
    pub pieces: Vec<RenderPiece>,
}

/// Individual piece in a render plan. Body-bearing child templates are kept as
/// opaque child anchors so parent formatters can never inspect their bytes.
#[derive(Debug, Clone)]
pub enum RenderPiece {
    /// Body-origin text eligible for the current template's formatter.
    Text(RenderTextPiece),
    /// Head-origin content that must bypass body formatters.
    HeadContent(RenderTextPiece),
    /// Opaque child template output — position preserved, content sealed.
    ChildTemplate(RenderChildPiece),
    /// Opaque expression insertion that parent formatters must preserve without inspection.
    DynamicExpression(RenderExpressionPiece),
    /// Structural template-loop control marker consumed by the nearest active loop.
    LoopControl(TemplateLoopControlSignal),
    /// Unresolved slot placeholder that will be filled later.
    Slot(SlotPlaceholder),
    /// Runtime slot placeholder occurrence resolved by AST planning.
    ///
    /// Each site points to placeholder-local wrapper behavior while contribution
    /// sources stay evaluated once in the runtime slot application plan.
    RuntimeSlotSite(RuntimeSlotSiteId),
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
    pub reactive_subscription: Option<ReactiveSubscription>,
}

impl TemplateRenderPlan {
    /// Remap all pieces in this render plan recursively.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        for piece in &mut self.pieces {
            piece.remap_string_ids(remap);
        }
    }
}

impl RenderPiece {
    /// Remap text strings, expressions, and slot placeholders in this piece.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            RenderPiece::Text(text) | RenderPiece::HeadContent(text) => {
                text.remap_string_ids(remap);
            }

            RenderPiece::ChildTemplate(child) => {
                child.remap_string_ids(remap);
            }

            RenderPiece::DynamicExpression(dynamic) => {
                dynamic.remap_string_ids(remap);
            }

            RenderPiece::LoopControl(signal) => {
                signal.location.remap_string_ids(remap);
            }

            RenderPiece::Slot(placeholder) => {
                placeholder.remap_string_ids(remap);
            }

            RenderPiece::RuntimeSlotSite(_) => {}
        }
    }
}

impl RenderTextPiece {
    /// Remap text string ID and location.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.text = remap.get(self.text);
        self.location.remap_string_ids(remap);
    }
}

impl RenderChildPiece {
    /// Remap the child expression.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.expression.remap_string_ids(remap);
    }
}

impl RenderExpressionPiece {
    /// Remap the dynamic expression and any attached subscription metadata.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.expression.remap_string_ids(remap);
        if let Some(subscription) = &mut self.reactive_subscription {
            subscription.remap_string_ids(remap);
        }
    }
}

// -------------------------
//  Formatter Anchors
// -------------------------

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

// -------------------------
//  Formatter Input/Output
// -------------------------

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

// -------------------------
//  Render Plan Implementation
// -------------------------

impl TemplateRenderPlan {
    /// Builds a render plan from composed template content.
    ///
    /// WHAT:
    /// - Maps body-origin text to `Text` pieces eligible for formatting.
    /// - Maps head-origin text to `HeadContent` pieces that bypass formatters.
    /// - Maps body-bearing child template outputs to `ChildTemplate` opaque anchors.
    /// - Maps expression insertions to `DynamicExpression` opaque anchors.
    ///
    /// WHY:
    /// - Formatters should only see text they are allowed to rewrite and opaque
    ///   anchors that preserve ordering. They must never inspect nested child
    ///   template bytes directly.
    pub fn from_content(content: &TemplateContent) -> Self {
        let mut pieces = Vec::with_capacity(content.atoms.len());

        for atom in &content.atoms {
            match atom {
                TemplateAtom::Slot(slot) => {
                    pieces.push(RenderPiece::Slot(slot.clone()));
                }

                TemplateAtom::Content(segment) => {
                    if let ExpressionKind::Template(template) = &segment.expression.kind
                        && let Some(TemplateControlFlow::LoopControl(signal)) =
                            &template.control_flow
                    {
                        pieces.push(RenderPiece::LoopControl(signal.clone()));
                        continue;
                    }

                    if segment.is_child_template_output
                        && let Some(source_child_template) =
                            segment.source_child_template.as_deref()
                    {
                        if child_template_is_head_expression_insert(source_child_template) {
                            pieces.push(RenderPiece::DynamicExpression(RenderExpressionPiece {
                                expression: segment.expression.clone(),
                                origin: segment.origin,
                                reactive_subscription: segment.reactive_subscription.clone(),
                            }));
                        } else {
                            pieces.push(RenderPiece::ChildTemplate(RenderChildPiece {
                                expression: segment.expression.clone(),
                            }));
                        }

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
                            reactive_subscription: segment.reactive_subscription.clone(),
                        }));
                    }
                }
            }
        }

        Self { pieces }
    }

    /// Reconstructs a `TemplateContent` from this render plan.
    ///
    /// ## Intentionally lost metadata
    ///
    /// `source_child_template` is **not** restored on rebuild. It exists only
    /// during the pre-format → plan phase so `from_content` can distinguish
    /// folded child-template outputs from generic dynamic expressions. After
    /// formatting, child templates are opaque anchors; their internal source
    /// linkage is no longer needed because composition has already run and
    /// `is_child_template_output` remains `true` for downstream consumers.
    pub fn rebuild_content(&self) -> TemplateContent {
        let mut atoms = Vec::new();

        for piece in &self.pieces {
            match piece {
                RenderPiece::HeadContent(text_piece) => {
                    atoms.push(TemplateAtom::Content(TemplateSegment::new(
                        Expression::string_slice(
                            text_piece.text,
                            text_piece.location.clone(),
                            ValueMode::ImmutableOwned,
                        ),
                        TemplateSegmentOrigin::Head,
                    )));
                }

                RenderPiece::Text(text_piece) => {
                    atoms.push(TemplateAtom::Content(TemplateSegment::new(
                        Expression::string_slice(
                            text_piece.text,
                            text_piece.location.clone(),
                            ValueMode::ImmutableOwned,
                        ),
                        TemplateSegmentOrigin::Body,
                    )));
                }

                RenderPiece::ChildTemplate(child_piece) => {
                    atoms.push(TemplateAtom::Content(TemplateSegment {
                        expression: Expression {
                            kind: child_piece.expression.kind.clone(),
                            type_id: builtin_type_ids::STRING,
                            diagnostic_type: DataType::Template,
                            function_receiver: None,
                            value_mode: ValueMode::ImmutableOwned,
                            reactive_source: None,
                            reactive_template: child_piece.expression.reactive_template.clone(),
                            const_record_state: ConstRecordState::RuntimeValue,
                            location: child_piece.expression.location.clone(),
                            contains_regular_division: child_piece
                                .expression
                                .contains_regular_division,
                            value_shape: ExpressionValueShape::TemplateString,
                        },
                        origin: TemplateSegmentOrigin::Body,
                        is_child_template_output: true,
                        reactive_subscription: None,
                        source_child_template: None,
                    }));
                }

                RenderPiece::DynamicExpression(expression_piece) => {
                    let mut segment = TemplateSegment::new(
                        expression_piece.expression.clone(),
                        expression_piece.origin,
                    );
                    segment.reactive_subscription = expression_piece.reactive_subscription.clone();
                    atoms.push(TemplateAtom::Content(segment));
                }

                RenderPiece::LoopControl(signal) => {
                    let mut template =
                        crate::compiler_frontend::ast::templates::template_types::Template::empty();
                    template.control_flow = Some(TemplateControlFlow::LoopControl(signal.clone()));
                    template.location = signal.location.clone();
                    atoms.push(TemplateAtom::Content(TemplateSegment::new(
                        Expression::template(template, ValueMode::ImmutableOwned),
                        TemplateSegmentOrigin::Body,
                    )));
                }

                RenderPiece::Slot(placeholder) => {
                    atoms.push(TemplateAtom::Slot(placeholder.clone()));
                }

                RenderPiece::RuntimeSlotSite(_) => {}
            }
        }

        TemplateContent { atoms }
    }

    /// Extracts all evaluatable expressions from the plan, discarding slots and omissions.
    pub fn flatten_expressions(&self) -> Vec<Expression> {
        self.pieces
            .iter()
            .filter_map(|piece| match piece {
                RenderPiece::Text(p) | RenderPiece::HeadContent(p) => Some(
                    Expression::string_slice(p.text, p.location.clone(), ValueMode::ImmutableOwned),
                ),
                RenderPiece::ChildTemplate(p) => Some(p.expression.clone()),
                RenderPiece::DynamicExpression(p) => Some(p.expression.clone()),
                RenderPiece::LoopControl(_) => None,
                RenderPiece::Slot(_) => None,
                RenderPiece::RuntimeSlotSite(_) => None,
            })
            .collect()
    }
}

/// Returns true for nested `[value]` insertions that have no body of their own.
///
/// WHAT:
/// - Head-only scalar/path insertions stay opaque to parent formatters, but they
///   are expression anchors rather than child-template boundaries.
/// - Template-valued head expressions remain child boundaries because they can
///   carry wrapper/slot semantics that belong to the child template.
///
/// WHY:
/// - `$markdown` can pair parent-authored inline-code delimiters across an
///   inserted string literal without inspecting the inserted bytes, while
///   body-bearing child templates stay sealed from the parent formatter.
fn child_template_is_head_expression_insert(template: &Template) -> bool {
    if template.control_flow.is_some() || template.runtime_slot_application.is_some() {
        return false;
    }

    if matches!(
        template.kind,
        TemplateType::SlotDefinition(_) | TemplateType::SlotInsert(_) | TemplateType::Comment(_)
    ) {
        return false;
    }

    if template.content.atoms.is_empty() {
        return false;
    }

    template.content.atoms.iter().all(|atom| {
        let TemplateAtom::Content(segment) = atom else {
            return false;
        };

        segment.origin == TemplateSegmentOrigin::Head
            && !segment.is_child_template_output
            && !matches!(segment.expression.kind, ExpressionKind::Template(_))
    })
}

#[cfg(test)]
#[path = "tests/render_plan_tests.rs"]
mod render_plan_tests;
