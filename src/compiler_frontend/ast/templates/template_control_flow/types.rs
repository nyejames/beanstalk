//! Data shapes for structured template control flow.
//!
//! These types are AST-stage handoff objects. Parser code fills them, render
//! planning annotates them, const folding reads them, and HIR lowering consumes
//! runtime-capable instances without flattening lazy branches too early.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{LoopBindings, RangeLoopSpec};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::match_patterns::MatchPattern;
use crate::compiler_frontend::ast::templates::tir::TemplateTirBodyReference;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

/// Structural control-flow carried by a template AST node.
#[derive(Clone, Debug)]
pub(crate) enum TemplateControlFlow {
    BranchChain(Box<TemplateBranchChain>),
    Loop(Box<TemplateLoopControlFlow>),
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
    /// TIR-authoritative root for this branch body, set by the parser.
    /// Render-unit preparation installs the prepared root.
    ///
    /// WHAT: pairs the body root node with the owning `TemplateIrStore` token.
    /// WHY: lets current-state materialization and validation consume the
    /// finalized TIR body directly instead of re-resolving through the owner
    /// template reference.
    pub(crate) body_tir_reference: TemplateTirBodyReference,
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
    /// TIR-authoritative root for this fallback body, set by the parser.
    /// Render-unit preparation installs the prepared root.
    pub(crate) body_tir_reference: TemplateTirBodyReference,
    pub(crate) location: SourceLocation,
}

/// Template `loop` structure after the body has been parsed.
#[derive(Clone, Debug)]
pub(crate) struct TemplateLoopControlFlow {
    pub(crate) header: TemplateLoopHeader,
    /// TIR-authoritative root for the loop body, set by the parser.
    /// Render-unit preparation installs the prepared root.
    pub(crate) body_tir_reference: TemplateTirBodyReference,
    /// TIR-authoritative root for the loop aggregate wrapper.
    ///
    /// WHAT: points at the composed aggregate-wrapper subtree installed during
    /// render-unit preparation.
    /// WHY: current-state materialization can copy the wrapper directly without
    /// searching the owning template root for its `Loop` node on every loop.
    pub(crate) aggregate_wrapper_tir_reference: Option<TemplateTirBodyReference>,
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
/// Runtime-capable templates may carry AST-prepared runtime slot application
/// plans, but unresolved helper artifacts that escape routing/composition are
/// invalid before HIR. Const-required templates use stricter foldability
/// validation instead, which lets compile-time helper templates keep slot
/// structure until a parent template composes it.
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
