//! TIR builder — parser-facing facade for direct TIR emission.
//!
//! WHAT: `TemplateIrBuilder` is a narrow mutable facade over `TemplateIrStore`.
//! It provides the small set of operations the template parser needs to build
//! TIR trees directly: pushing text and sequence nodes, finishing a template,
//! and leaving storage ownership with the caller.
//!
//! WHY: the parser emits TIR directly instead of building
//! `TemplateContent` and then converting it. The builder is the narrow API the
//! parser uses; it delegates all storage to `TemplateIrStore` so the store
//! remains the single owner of TIR data and no second store or parallel index
//! logic is introduced.
//!
//! ## Ownership contract
//!
//! The builder borrows the store mutably for the duration of parsing. It does
//! not own HIR, backend, or public API data. It is a temporary construction
//! helper, not a long-lived query surface.

use crate::compiler_frontend::ast::expressions::expression::Expression;
#[cfg(test)]
use crate::compiler_frontend::ast::templates::template::SlotKey;
use crate::compiler_frontend::ast::templates::template::{
    ReactiveSubscription, Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateLoopControlKind, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::tir::ids::{TemplateIrId, TemplateIrNodeId};
use crate::compiler_frontend::ast::templates::tir::node::{
    TemplateIr, TemplateIrBranch, TemplateIrNode, TemplateIrNodeKind, TirSlotPlaceholder,
};
#[cfg(test)]
use crate::compiler_frontend::ast::templates::tir::overlays::TemplateOverlaySetId;
use crate::compiler_frontend::ast::templates::tir::refs::TemplateTirChildReference;
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::ast::templates::tir::summary::TemplateIrSummary;
#[cfg(test)]
use crate::compiler_frontend::ast::templates::tir::view::TemplateTirPhase;
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

// -------------------------
//  Template IR Builder
// -------------------------

/// Narrow mutable facade for building TIR nodes and templates inside a store.
///
/// WHAT: provides push/finish methods that delegate to `TemplateIrStore`.
/// WHY: keeps parser construction code focused on tree shape while the store
/// owns indexing, capacity, and side-table details.
pub(crate) struct TemplateIrBuilder<'store> {
    /// Underlying store that owns all allocated TIR data. The builder only
    /// borrows it; the caller retains ownership after the builder is dropped.
    store: &'store mut TemplateIrStore,
}

impl<'store> TemplateIrBuilder<'store> {
    /// Creates a builder that writes into the given store.
    pub(crate) fn new(store: &'store mut TemplateIrStore) -> Self {
        Self { store }
    }

    /// Pushes a literal text node and returns its ID.
    ///
    /// WHAT: test-only convenience for fixtures that do not need reactive
    ///       subscription metadata.
    #[cfg(test)]
    pub(crate) fn push_text_node(
        &mut self,
        text: StringId,
        byte_len: u32,
        origin: TemplateSegmentOrigin,
        location: SourceLocation,
    ) -> TemplateIrNodeId {
        self.push_text_node_with_subscription(text, byte_len, origin, None, location)
    }

    /// Pushes a literal text node that may carry a reactive subscription.
    ///
    /// WHAT: reactive literal bodies (e.g. formatter output) produce text that
    ///       must keep its `ReactiveSubscription` so the runtime can later
    ///       invalidate stale content. The subscription is stored in a side-table
    ///       keyed by the node ID rather than widening every `Text` payload.
    pub(crate) fn push_text_node_with_subscription(
        &mut self,
        text: StringId,
        byte_len: u32,
        origin: TemplateSegmentOrigin,
        reactive_subscription: Option<ReactiveSubscription>,
        location: SourceLocation,
    ) -> TemplateIrNodeId {
        let node_id = self.store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Text {
                text,
                byte_len,
                origin,
            },
            location,
        ));

        if let Some(subscription) = reactive_subscription {
            self.store
                .set_node_reactive_subscription(node_id, subscription);
        }

        node_id
    }

    /// Pushes a sequence node containing the given child node IDs.
    pub(crate) fn push_sequence_node(
        &mut self,
        children: Vec<TemplateIrNodeId>,
        location: SourceLocation,
    ) -> TemplateIrNodeId {
        self.store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Sequence { children },
            location,
        ))
    }

    /// Pushes a same-store child-template reference node and returns its ID.
    ///
    /// WHAT: convenience for tests and early construction paths that do not yet
    ///       know the child's phase or overlay context. The emitted node carries
    ///       [`TemplateTirPhase::Parsed`] and the canonical empty overlay set.
    /// WHY: keeps fixture code readable while the production paths that *do* know
    ///      the phase/overlay use [`Self::push_child_template_node_with_reference`].
    #[cfg(test)]
    pub(crate) fn push_child_template_node(
        &mut self,
        template: TemplateIrId,
        location: SourceLocation,
    ) -> TemplateIrNodeId {
        let reference = TemplateTirChildReference::same_store(
            template,
            self.store.store_id(),
            TemplateTirPhase::Parsed,
            TemplateOverlaySetId::empty(),
        );
        self.push_child_template_node_with_reference(reference, location)
    }

    /// Pushes a child-template reference node with an explicit view identity.
    ///
    /// WHAT: production paths use this so the node carries the precise root,
    ///       phase, and overlay set needed to build a [`TirView`](super::view::TirView)
    ///       when the child is folded.
    /// WHY: the convenience [`Self::push_child_template_node`] defaults to
    ///      `Parsed`/empty, which is correct for parser-emitted fixtures but not
    ///      for finalized/composed children.
    pub(crate) fn push_child_template_node_with_reference(
        &mut self,
        reference: TemplateTirChildReference,
        location: SourceLocation,
    ) -> TemplateIrNodeId {
        let occurrence_id = self.store.next_child_template_occurrence_id();
        self.store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::ChildTemplate {
                reference,
                occurrence_id,
            },
            location,
        ))
    }

    /// Pushes a runtime or compile-time expression splice node and returns its ID.
    pub(crate) fn push_dynamic_expression_node(
        &mut self,
        expression: Expression,
        origin: TemplateSegmentOrigin,
        reactive_subscription: Option<ReactiveSubscription>,
        location: SourceLocation,
    ) -> TemplateIrNodeId {
        let site_id = self.store.next_expression_site_id();
        self.store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::DynamicExpression {
                expression: Box::new(expression),
                origin,
                reactive_subscription,
                site_id,
            },
            location,
        ))
    }

    /// Pushes a structural slot-placeholder node and returns its ID.
    #[cfg(test)]
    pub(crate) fn push_slot_node(
        &mut self,
        key: SlotKey,
        location: SourceLocation,
    ) -> TemplateIrNodeId {
        let occurrence_id = self.store.next_slot_occurrence_id();
        let placeholder = TirSlotPlaceholder::new(key, occurrence_id, location.clone());

        self.push_tir_slot_placeholder_node(placeholder)
    }

    /// Pushes a prebuilt TIR slot-placeholder payload and returns its node ID.
    ///
    /// WHAT: stores a `TirSlotPlaceholder` that may already carry TIR-owned
    /// wrapper-set IDs.
    /// WHY: parser/current-state boundaries convert legacy AST slot metadata
    /// before entering the TIR node store, so this builder never stores
    /// recursive `Template` wrapper payloads.
    pub(crate) fn push_tir_slot_placeholder_node(
        &mut self,
        placeholder: TirSlotPlaceholder,
    ) -> TemplateIrNodeId {
        let location = placeholder.location.clone();
        self.store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Slot { placeholder },
            location,
        ))
    }

    /// Pushes an `$insert("name")` contribution node and returns its ID.
    pub(crate) fn push_insert_contribution_node(
        &mut self,
        template: TemplateIrId,
        location: SourceLocation,
    ) -> TemplateIrNodeId {
        self.store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::InsertContribution { template },
            location,
        ))
    }

    /// Pushes a conditional branch-chain node and returns its ID.
    ///
    /// WHAT: allocates a fresh `ExpressionSiteId` for each branch selector in
    /// document order before storing the node, overwriting the placeholder
    /// set by `TemplateIrBranch::new`.
    /// WHY: the parser constructs branches before the store is borrowed, so
    /// site-ID allocation happens here: the single point where the store is
    /// available and branches are ready. This keeps one document-order counter
    /// shared with `DynamicExpression` and loop-header sites.
    pub(crate) fn push_branch_chain_node(
        &mut self,
        mut branches: Vec<TemplateIrBranch>,
        fallback: Option<TemplateIrNodeId>,
        location: SourceLocation,
    ) -> TemplateIrNodeId {
        for branch in &mut branches {
            branch.selector_site_id = self.store.next_expression_site_id();
        }
        self.store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::BranchChain { branches, fallback },
            location,
        ))
    }

    /// Pushes a template loop node and returns its ID.
    ///
    /// WHAT: allocates `TemplateLoopHeaderExpressionSites` from the store's
    /// document-order expression-site counter before storing the node.
    /// WHY: every construction path that builds a `Loop` TIR node allocates
    /// header sites through the store helper so the counter stays consistent
    /// and the allocation logic is not duplicated.
    pub(crate) fn push_loop_node(
        &mut self,
        header: TemplateLoopHeader,
        body: TemplateIrNodeId,
        aggregate_wrapper: Option<TemplateIrNodeId>,
        location: SourceLocation,
    ) -> TemplateIrNodeId {
        let header_sites = self.store.allocate_loop_header_expression_sites(&header);
        self.store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Loop {
                header,
                header_sites,
                body,
                aggregate_wrapper,
            },
            location,
        ))
    }

    /// Pushes a loop-control marker node and returns its ID.
    pub(crate) fn push_loop_control_node(
        &mut self,
        kind: TemplateLoopControlKind,
        location: SourceLocation,
    ) -> TemplateIrNodeId {
        self.store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::LoopControl { kind },
            location,
        ))
    }

    /// Finishes a template from its root node and metadata, returning its ID.
    pub(crate) fn finish_template(
        &mut self,
        root: TemplateIrNodeId,
        style: Style,
        kind: TemplateType,
        summary: TemplateIrSummary,
        location: SourceLocation,
    ) -> TemplateIrId {
        self.store
            .push_template(TemplateIr::new(root, style, kind, summary, location))
    }
}
