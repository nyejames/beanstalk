//! Data shapes for structured template control flow.
//!
//! These types are AST-stage handoff objects. Parser code fills them, render
//! planning annotates them, const folding reads them, and HIR lowering consumes
//! runtime-capable instances without flattening lazy branches too early.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{LoopBindings, RangeLoopSpec};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::match_patterns::MatchPattern;
use crate::compiler_frontend::ast::templates::tir::{
    TemplateIrNodeId, TemplateIrStore, TemplateIrStoreOwner, TemplateOverlaySetId, TemplateStoreId,
    TemplateTirBodyReference, TemplateTirPhase,
};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use std::sync::Arc;

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
    /// TIR-authoritative root for this branch body, set by the parser and
    /// refreshed by render-unit preparation.
    ///
    /// WHAT: pairs the body root node with the owning `TemplateIrStore` token.
    /// WHY: lets current-state materialization and validation consume the
    /// finalized TIR body directly instead of re-resolving through the owner
    /// template reference.
    pub(crate) body_tir_reference: Option<TemplateControlFlowTirReference>,
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
    /// TIR-authoritative root for this fallback body, set by the parser and
    /// refreshed by render-unit preparation.
    pub(crate) body_tir_reference: Option<TemplateControlFlowTirReference>,
    pub(crate) location: SourceLocation,
}

/// Template `loop` structure after the body has been parsed.
#[derive(Clone, Debug)]
pub(crate) struct TemplateLoopControlFlow {
    pub(crate) header: TemplateLoopHeader,
    /// TIR-authoritative root for the loop body, set by the parser and
    /// refreshed by render-unit preparation.
    pub(crate) body_tir_reference: Option<TemplateControlFlowTirReference>,
    /// TIR-authoritative root for the loop aggregate wrapper.
    ///
    /// WHAT: points at the composed aggregate-wrapper subtree installed during
    /// render-unit preparation.
    /// WHY: current-state materialization can copy the wrapper directly without
    /// searching the owning template root for its `Loop` node on every loop.
    pub(crate) aggregate_wrapper_tir_reference: Option<TemplateControlFlowTirReference>,
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

/// Parser-to-render-unit scratch for control-flow body roots.
///
/// WHAT: carries parsed branch/fallback/loop TIR body roots only across the
/// immediate body-parse to render-unit-preparation boundary.
/// WHY: the parser now emits control-flow bodies directly into the module-scoped
///      `TemplateIrStore`, so the scratch only needs to pair each parsed body
///      with its owning control-flow arm. Final control-flow structs retain
///      selectors/headers, locations, and TIR body-root references.
#[derive(Clone, Debug)]
pub(crate) enum TemplateControlFlowBodyScratch {
    None,
    BranchChain(TemplateBranchChainBodyScratch),
    Loop(TemplateLoopBodyScratch),
}

#[derive(Clone, Debug)]
pub(crate) struct TemplateBranchChainBodyScratch {
    pub(crate) branches: Vec<TemplateIrNodeId>,
    pub(crate) fallback: Option<TemplateIrNodeId>,
}

#[derive(Clone, Debug)]
pub(crate) struct TemplateLoopBodyScratch {
    pub(crate) body: TemplateIrNodeId,
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

/// Store-qualified TIR root for one control-flow payload.
///
/// WHAT: wraps a `TemplateTirBodyReference` so branch/fallback/loop and
/// aggregate-wrapper body roots carry the full view-system identity: store,
/// phase, overlay set, source location, and same-store owner proof.
/// WHY: TIR node IDs are store-local indexes and a bare owner-plus-root tuple
/// loses phase and overlay context. Keeping the full identity on the reference
/// lets finalization and later view consumers treat body roots as structured
/// inputs instead of ad hoc `TemplateIrNodeId` consumers.
#[derive(Clone, Debug)]
pub(crate) struct TemplateControlFlowTirReference {
    body: TemplateTirBodyReference,
}

impl TemplateControlFlowTirReference {
    /// Creates a control-flow body reference from a store-local root and the
    /// store's current identity.
    ///
    /// WHAT: uses the store's assigned `TemplateStoreId`, the empty overlay set,
    ///       and a default source location. This is the narrow constructor for
    ///       internal sites that do not yet have richer context.
    /// WHY: keeps the common case readable while still producing a
    ///      store-qualified reference rather than a raw node ID.
    #[allow(
        dead_code,
        reason = "used by test fixtures; production paths prefer with_full_identity or with_phase"
    )]
    pub(crate) fn new(store: &TemplateIrStore, root: TemplateIrNodeId) -> Self {
        Self {
            body: TemplateTirBodyReference::with_store_local_identity(
                store,
                root,
                TemplateTirPhase::Composed,
            ),
        }
    }

    /// Creates a control-flow body reference with full view identity.
    pub(crate) fn with_full_identity(
        store_owner: Arc<TemplateIrStoreOwner>,
        store_id: TemplateStoreId,
        root: TemplateIrNodeId,
        phase: TemplateTirPhase,
        overlay_set_id: TemplateOverlaySetId,
        location: SourceLocation,
    ) -> Self {
        Self {
            body: TemplateTirBodyReference::new(
                store_owner,
                store_id,
                root,
                phase,
                overlay_set_id,
                location,
            ),
        }
    }

    /// Creates a reference from an already-built body/root view reference.
    pub(crate) fn from_body_reference(body: TemplateTirBodyReference) -> Self {
        Self { body }
    }

    /// Returns the underlying body/root view reference.
    #[allow(
        dead_code,
        reason = "used by finalization and tests that inspect the view identity"
    )]
    pub(crate) fn body_reference(&self) -> &TemplateTirBodyReference {
        &self.body
    }

    /// Returns a mutable borrow of the underlying body/root view reference.
    ///
    /// WHAT: lets finalization update the body root's overlay set and phase after
    ///       expression-overlay normalization.
    /// WHY: the body reference is the durable identity for the subtree; its
    ///      overlay set must carry the normalized expression layer so consumers
    ///      read the correct effective view.
    pub(crate) fn body_reference_mut(&mut self) -> &mut TemplateTirBodyReference {
        &mut self.body
    }

    pub(crate) fn with_phase(
        store: &TemplateIrStore,
        root: TemplateIrNodeId,
        phase: TemplateTirPhase,
    ) -> Self {
        Self {
            body: TemplateTirBodyReference::with_store_local_identity(store, root, phase),
        }
    }

    pub(crate) fn same_store_root(&self, store: &TemplateIrStore) -> Option<TemplateIrNodeId> {
        self.body.same_store_root(store)
    }

    /// Pipeline phase represented by this body root.
    #[allow(
        dead_code,
        reason = "used by focused tests today; will drive phase-gated view consumption in a later slice"
    )]
    pub(crate) fn phase(&self) -> TemplateTirPhase {
        self.body.phase
    }

    /// Overlay set that applies when this body root is consumed as a view.
    #[allow(
        dead_code,
        reason = "used by focused tests today; will drive overlay-aware view consumption in a later slice"
    )]
    pub(crate) fn overlay_set_id(&self) -> TemplateOverlaySetId {
        self.body.overlay_set_id
    }

    /// Source location for diagnostics pointing at this body root.
    #[allow(
        dead_code,
        reason = "used by focused tests today; will drive diagnostics once view consumption lands"
    )]
    pub(crate) fn location(&self) -> &SourceLocation {
        &self.body.location
    }

    /// Replaces the pipeline phase on this body reference.
    #[allow(
        dead_code,
        reason = "used by test fixtures that advance a hand-built reference to the required pipeline phase"
    )]
    pub(crate) fn set_phase(&mut self, phase: TemplateTirPhase) {
        self.body.set_phase(phase);
    }
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
