//! TIR node types.
//!
//! WHAT: defines `TemplateIr`, `TemplateIrNode`, and `TemplateIrNodeKind` — the
//! core data shapes of the Template IR. A template owns a root node ID; nodes
//! form a tree via child references back into the store.
//!
//! WHY: the current AST `Template` type mixes content atoms, render plans,
//! control-flow metadata, and formatting state. TIR separates those concerns into
//! a clean tree of typed nodes so folding, formatting, and HIR preparation can
//! work from a single stable representation without ping-pong between
//! `TemplateContent`, `TemplateRenderPlan`, and rebuilt content.
//!
//! ## Ownership contract
//!
//! TIR nodes are owned by `TemplateIrStore`. They do not own HIR or backend
//! data. Node children are `Copy` IDs that index back into the store, keeping
//! the tree cheap to traverse and clone.
//!
//! ## Semantic parity constraint
//!
//! TIR must preserve the same user-visible template semantics as the current
//! `Template` → `TemplateContent` path. Behaviour changes are out of scope
//! unless they are bug fixes with regression tests.
//!
//! ## Temporary converter deletion plan
//!
//! The `convert_from_template.rs` converter (Phase B1) translates current
//! `Template` values into TIR. That converter is temporary — once TIR is the
//! authoritative path, the converter and the old `Template`-based folding/formatting
//! internals it replaces will be deleted at a documented checkpoint.
//!
//! ## No feature flag
//!
//! TIR is implemented directly on `main`. There is no feature flag gating
//! TIR types or the eventual production route.

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::template::Style;
use crate::compiler_frontend::ast::templates::template::{
    SlotPlaceholder, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateAggregateRenderPlan, TemplateBranchSelector, TemplateLoopControlKind,
    TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_slots::RuntimeSlotSiteId;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::tir::ids::{TemplateIrId, TemplateIrNodeId};
use crate::compiler_frontend::ast::templates::tir::summary::TemplateIrSummary;
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

// -------------------------
//  Top-Level Template IR
// -------------------------

/// A top-level template entry in the TIR store.
///
/// WHAT: connects a root node, style, kind, summary metadata, and source location.
/// WHY: TIR templates are the authoritative internal representation after parsing.
/// Each template is a thin handle; the body tree lives in `TemplateIrNode` entries
/// owned by the store.
#[derive(Clone, Debug)]
pub(crate) struct TemplateIr {
    /// Root node of this template's body tree.
    pub(crate) root: TemplateIrNodeId,

    /// Style directive configuration (e.g., `$markdown`, `$raw`).
    pub(crate) style: Style,

    /// High-level template classification (string, string-function, slot, etc.).
    pub(crate) kind: TemplateType,

    /// Summary metadata for capacity planning and quick shape queries.
    pub(crate) summary: TemplateIrSummary,

    /// Source location for diagnostics.
    pub(crate) location: SourceLocation,

    /// Parent `$children(..)` wrappers that must be applied around this
    /// template's output during folding.
    ///
    /// WHAT: preserves the wrappers from the AST `Template` so the TIR fold
    /// path can apply them without re-walking the old `Template` tree.
    /// WHY: conditional child wrappers are consumed during fold-time
    /// composition; storing them here keeps the TIR fold path self-contained.
    pub(crate) conditional_child_wrappers: Vec<Template>,
}

impl TemplateIr {
    pub(crate) fn new(
        root: TemplateIrNodeId,
        style: Style,
        kind: TemplateType,
        summary: TemplateIrSummary,
        location: SourceLocation,
    ) -> Self {
        Self {
            root,
            style,
            kind,
            summary,
            location,
            conditional_child_wrappers: Vec::new(),
        }
    }
}

// -------------------------
//  Template IR Node
// -------------------------

/// A single node in a template body tree.
///
/// WHAT: pairs a `TemplateIrNodeKind` with a source location.
/// WHY: every node carries its source position for diagnostics, and the kind
/// enum determines what the node represents (text, expression, slot, etc.).
#[derive(Clone, Debug)]
pub(crate) struct TemplateIrNode {
    /// Discriminant and payload for this node.
    pub(crate) kind: TemplateIrNodeKind,

    /// Source location for diagnostics and source-map output.
    pub(crate) location: SourceLocation,
}

impl TemplateIrNode {
    pub(crate) fn new(kind: TemplateIrNodeKind, location: SourceLocation) -> Self {
        Self { kind, location }
    }
}

// -------------------------
//  Template IR Node Kind
// -------------------------

/// Structural variant for a TIR node.
///
/// WHAT: enumerates every kind of structural element a template body can contain.
/// WHY: the old `TemplateAtom` enum mixes content segments and slot placeholders
/// without distinguishing text from expressions or child templates. TIR node kinds
/// give each structural role its own variant so folding, formatting, and HIR
/// lowering can dispatch cleanly.
///
/// ## Design notes
///
/// - `Vec` fields in `Sequence` and `BranchChain` may be replaced with typed
///   ranges into store-owned side vectors in later phases if clone pressure
///   becomes measurable.
/// - `DynamicExpression` boxes the AST `Expression` to keep enum size reasonable.
///   TIR does not own expression resolution; it holds the AST-produced value.
#[derive(Clone, Debug)]
#[allow(
    clippy::large_enum_variant,
    reason = "DynamicExpression holds an AST Expression; boxing is sufficient"
)]
pub(crate) enum TemplateIrNodeKind {
    /// Ordered sequence of child nodes (the common case for template bodies).
    Sequence { children: Vec<TemplateIrNodeId> },

    /// Literal interned text.
    Text {
        text: StringId,
        byte_len: u32,
        origin: TemplateSegmentOrigin,
    },

    /// Runtime or compile-time expression splice.
    DynamicExpression {
        expression: Box<Expression>,
        origin: TemplateSegmentOrigin,
    },

    /// Opaque child template reference.
    ChildTemplate { template: TemplateIrId },

    /// Structural slot placeholder awaiting composition.
    Slot { slot: SlotPlaceholder },

    /// Content contributed by an `$insert("name")` directive.
    InsertContribution { template: TemplateIrId },

    /// Conditional branch chain (`if` / `else if` / `else`).
    BranchChain {
        branches: Vec<TemplateIrBranch>,
        fallback: Option<TemplateIrNodeId>,
    },

    /// Loop with a body node and optional aggregate wrapper.
    Loop {
        header: TemplateLoopHeader,
        body: TemplateIrNodeId,
        aggregate_wrapper: Option<TemplateIrNodeId>,

        /// Temporary Phase B2 support for loop aggregate wrapper folding.
        ///
        /// WHAT: carries the AST `TemplateAggregateRenderPlan` that wraps the
        /// loop aggregate output until Phase B4 replaces it with a TIR-native
        /// render-unit representation.
        /// WHY: the current TIR `aggregate_wrapper` node is an empty placeholder;
        /// folding needs the actual render-plan pieces to preserve wrapper
        /// semantics. This field is a narrow, local bridge and must be deleted
        /// once TIR owns aggregate wrappers natively.
        aggregate_render_plan: Option<TemplateAggregateRenderPlan>,
    },

    /// Loop control signal (`break` / `continue`).
    LoopControl { kind: TemplateLoopControlKind },

    /// Runtime slot site placeholder resolved by AST planning.
    RuntimeSlotSite { site: RuntimeSlotSiteId },
}

// -------------------------
//  TIR Branch
// -------------------------

/// One conditional branch within a `BranchChain` node.
///
/// WHAT: pairs a branch selector with the body node that executes when
/// the selector condition is satisfied.
/// WHY: keeping branches as (selector, body) pairs matches the AST shape and
/// avoids encoding branch logic into the node kind itself. Storing the full
/// `TemplateBranchSelector` (rather than just the extracted expression) lets
/// the TIR fold path handle bool conditions and option-capture scrutinees
/// uniformly.
#[derive(Clone, Debug)]
pub(crate) struct TemplateIrBranch {
    /// Selector that determines when this branch is active.
    ///
    /// Stores the full `TemplateBranchSelector` from the AST so the fold path
    /// can distinguish bool conditions from option-capture scrutinees.
    pub(crate) selector: TemplateBranchSelector,

    /// Body node to execute when the selector condition is satisfied.
    pub(crate) body: TemplateIrNodeId,

    /// Source location for diagnostics.
    pub(crate) location: SourceLocation,
}

impl TemplateIrBranch {
    pub(crate) fn new(
        selector: TemplateBranchSelector,
        body: TemplateIrNodeId,
        location: SourceLocation,
    ) -> Self {
        Self {
            selector,
            body,
            location,
        }
    }

    /// Returns the condition expression for this branch.
    ///
    /// WHAT: for `Bool` selectors, returns the bool expression directly.
    /// For `OptionPresentCapture` selectors, returns the scrutinee expression.
    /// WHY: validation and fold passes that only need the condition expression
    /// can call this helper without repeating the selector match at every site.
    pub(crate) fn condition_expression(&self) -> &Expression {
        match &self.selector {
            TemplateBranchSelector::Bool(expression) => expression,
            TemplateBranchSelector::OptionPresentCapture { scrutinee, .. } => scrutinee,
        }
    }
}
