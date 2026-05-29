//! Data shapes for structured template control flow.
//!
//! These types are AST-stage handoff objects. Parser code fills them, render
//! planning annotates them, const folding reads them, and HIR lowering consumes
//! runtime-capable instances without flattening lazy branches too early.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{LoopBindings, RangeLoopSpec};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::match_patterns::MatchPattern;
use crate::compiler_frontend::ast::templates::template::TemplateContent;
use crate::compiler_frontend::ast::templates::template_render_plan::{
    RenderPiece, TemplateRenderPlan,
};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

/// Structural control-flow carried by a template AST node.
#[derive(Clone, Debug)]
pub(crate) enum TemplateControlFlow {
    BranchChain(Box<TemplateBranchChain>),
    Loop(Box<TemplateLoopControlFlow>),
    LoopControl(TemplateLoopControlSignal),
}

/// Ordered template branch chain after the body has been split into arms.
///
/// Current source syntax can only produce one conditional branch and one
/// optional fallback. The chain shape is still useful now because every
/// `[else if ...]` appends another conditional arm that shares preparation,
/// validation, folding, and lazy runtime lowering.
#[derive(Clone, Debug)]
pub(crate) struct TemplateBranchChain {
    pub(crate) branches: Vec<TemplateConditionalBranch>,
    pub(crate) fallback: Option<TemplateFallbackBranch>,
    pub(crate) location: SourceLocation,
}

/// One selectable branch in a template branch chain.
#[derive(Clone, Debug)]
pub(crate) struct TemplateConditionalBranch {
    pub(crate) selector: TemplateBranchSelector,
    pub(crate) content: TemplateContent,
    pub(crate) render_plan: Option<TemplateRenderPlan>,
    pub(crate) location: SourceLocation,
}

/// Supported template branch selectors.
#[derive(Clone, Debug)]
pub(crate) enum TemplateBranchSelector {
    Bool(Expression),
    OptionPresentCapture {
        scrutinee: Expression,
        pattern: Box<MatchPattern>,
    },
}

/// Optional fallback branch used by `[else]`.
#[derive(Clone, Debug)]
pub(crate) struct TemplateFallbackBranch {
    pub(crate) content: TemplateContent,
    pub(crate) render_plan: Option<TemplateRenderPlan>,
    pub(crate) location: SourceLocation,
}

/// Template `loop` structure after the body has been parsed.
#[derive(Clone, Debug)]
pub(crate) struct TemplateLoopControlFlow {
    pub(crate) header: TemplateLoopHeader,
    pub(crate) body_content: TemplateContent,
    pub(crate) body_render_plan: Option<TemplateRenderPlan>,
    pub(crate) aggregate_render_plan: Option<TemplateLoopAggregateRenderPlan>,
    pub(crate) location: SourceLocation,
}

/// Supported template loop headers.
#[derive(Clone, Debug)]
pub(crate) enum TemplateLoopHeader {
    Conditional {
        condition: Box<Expression>,
    },
    Range {
        bindings: Box<LoopBindings>,
        range: Box<RangeLoopSpec>,
    },
    Collection {
        bindings: Box<LoopBindings>,
        iterable: Box<Expression>,
    },
}

/// Structural loop-control marker authored as a standalone template child.
///
/// These signals are AST template control flow, not renderable template output.
/// Folding and HIR lowering consume them at the nearest active template loop.
#[derive(Clone, Debug)]
pub struct TemplateLoopControlSignal {
    pub(crate) kind: TemplateLoopControlKind,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemplateLoopControlKind {
    Break,
    Continue,
}

/// Output/control result from a selected template body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TemplateBodyEmission {
    NoOutput,
    Output,
    Break,
    Continue,
}

/// Prepared render plan for applying the owning loop template head to the
/// runtime aggregate output exactly once.
#[derive(Clone, Debug)]
pub(crate) struct TemplateLoopAggregateRenderPlan {
    pub(crate) pieces: Vec<TemplateLoopAggregatePiece>,
}

/// A loop aggregate plan is mostly an ordinary render plan, with one explicit
/// placeholder for the runtime aggregate local.
#[derive(Clone, Debug)]
pub(crate) enum TemplateLoopAggregatePiece {
    Render(Box<RenderPiece>),
    Aggregate,
}

/// Body parser mode selected by the template head.
///
/// Template heads build this handoff, then body parsing consumes the non-normal
/// modes to split branch/body content and construct `TemplateControlFlow`.
#[derive(Clone)]
pub(crate) enum TemplateBodyParseMode {
    Normal,
    If(Box<TemplateIfBodyParseInput>),
    Loop(Box<TemplateLoopBodyParseInput>),
}

/// Selects the post-parse validation rules for structured template control flow.
///
/// Runtime-capable templates must not leave control-flow-local slot artifacts for
/// HIR. Const-required templates use stricter foldability validation instead,
/// which lets compile-time helper templates keep slot structure until a parent
/// template composes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TemplateControlFlowValidationMode {
    RuntimeCapable,
    ConstRequired,
}

/// Parsed `if` suffix state needed by the body parser.
#[derive(Clone)]
pub(crate) struct TemplateIfBodyParseInput {
    pub(crate) selector: TemplateBranchSelector,
    pub(crate) then_context: ScopeContext,
    pub(crate) else_context: ScopeContext,
    pub(crate) location: SourceLocation,
}

/// Parsed `loop` suffix state needed by the body parser.
#[derive(Clone)]
pub(crate) struct TemplateLoopBodyParseInput {
    pub(crate) header: TemplateLoopHeader,
    pub(crate) body_context: ScopeContext,
    pub(crate) location: SourceLocation,
}
