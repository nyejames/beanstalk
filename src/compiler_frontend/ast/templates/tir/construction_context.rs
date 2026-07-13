//! Parser-local construction context for template TIR emission.
//!
//! WHAT: `TemplateConstructionContext` owns the in-progress
//! `TemplateParserIrBuilderState` while a template is being parsed and shaped.
//! It is the parser-facing owner for recording literal parser output (text,
//! dynamic expressions, child templates, slots, control flow) into the
//! module-scoped `TemplateIrStore`.
//!
//! WHY: `Template` is the durable AST template value. The mutable parser
//! accumulator is shorter-lived: it exists only while syntax is being parsed,
//! render units are shaped, and parser-emitted TIR is finalized into a
//! `TemplateTirReference`. Keeping the store handle, store identity, source
//! location, and registry identity on the construction context means parser
//! callers record through the context instead of repeatedly borrowing
//! `context.template_ir_store`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::{
    ReactiveSubscription, SlotPlaceholder, Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateLoopControlKind, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::tir::overlays::TemplateOverlaySet;
use crate::compiler_frontend::ast::templates::tir::parser_builder_state::{
    TemplateParserIrBuilderState, TemplateTirReference,
};
use crate::compiler_frontend::ast::templates::tir::store::{TemplateIrStore, TemplateIrStoreOwner};
use crate::compiler_frontend::ast::templates::tir::{
    TemplateIrRegistry, TemplateStoreId, TemplateTirPhase,
    ids::{TemplateIrId, TemplateIrNodeId},
    node::TemplateIrBranch,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

/// Parser-local owner for in-progress TIR builder state.
///
/// WHAT: wraps the active `TemplateParserIrBuilderState` and the registry/store
///       identity it writes into, and provides the recording API that head/body
///       parsing and render-unit preparation use to emit TIR nodes.
/// WHY: keeps parse-time accumulator state and store-borrowing details off
///      `Template` so the long-lived template only carries the finalized
///      `TemplateTirReference`.
pub(crate) struct TemplateConstructionContext {
    builder: TemplateParserIrBuilderState,

    /// Registry-owned store that receives the parser-emitted nodes.
    store: Rc<RefCell<TemplateIrStore>>,

    /// Registry-level identity of `store`.
    ///
    /// WHAT: lets callers prove store ownership without pointer comparisons.
    store_id: TemplateStoreId,

    /// Module-local TIR registry that owns `store`.
    ///
    /// WHAT: keeps the construction context anchored to the registry identity
    ///       available from `ScopeContext`, not just the store-owner token.
    registry: Rc<RefCell<TemplateIrRegistry>>,

    /// Source location where this template started parsing.
    ///
    /// WHAT: preserves the construction site location for diagnostics and
    ///       finalization without asking callers to thread it separately.
    location: SourceLocation,
}

impl TemplateConstructionContext {
    /// Creates a fresh construction context bound to the given registry-owned store.
    ///
    /// WHAT: allocates a new builder state that will record parser output into
    ///       `store`, and remembers `store_id`/`registry` so later code can
    ///       recover store-qualified identity without reaching back into
    ///       `ScopeContext`.
    pub(crate) fn new(
        store: Rc<RefCell<TemplateIrStore>>,
        store_id: TemplateStoreId,
        registry: Rc<RefCell<TemplateIrRegistry>>,
        location: SourceLocation,
    ) -> Self {
        let store_owner = store.borrow().owner();

        Self {
            builder: TemplateParserIrBuilderState::new(store_owner),
            store,
            store_id,
            registry,
            location,
        }
    }

    /// Returns the store-owner token for the store this context writes into.
    pub(crate) fn store_owner(&self) -> Arc<TemplateIrStoreOwner> {
        self.debug_assert_registered_store();
        self.store.borrow().owner()
    }

    /// Returns the source location captured when this context was created.
    pub(crate) fn location(&self) -> &SourceLocation {
        &self.location
    }

    /// Returns a shared reference to the in-progress builder state.
    ///
    /// WHAT: lets validation and render-unit sync read root children,
    ///       control-flow node IDs, and the store-owner token while parsing is
    ///       still in progress.
    pub(crate) fn builder(&self) -> &TemplateParserIrBuilderState {
        &self.builder
    }

    /// Returns a shared borrow of the underlying TIR store.
    ///
    /// WHAT: lets callers inspect finalized or in-progress nodes while the
    ///       construction context still owns the builder state.
    /// WHY: body-boundary validation and node lookup need store access without
    ///      taking ownership of the store handle from the context.
    pub(crate) fn store(&self) -> std::cell::Ref<'_, TemplateIrStore> {
        self.store.borrow()
    }

    // -------------------------
    //  Recording — text
    // -------------------------

    /// Records a literal body text segment.
    pub(crate) fn record_text(
        &mut self,
        text: StringId,
        byte_len: usize,
        location: SourceLocation,
    ) {
        let mut store = self.store.borrow_mut();
        self.builder.record_text(
            &mut store,
            text,
            byte_len,
            TemplateSegmentOrigin::Body,
            None,
            location,
        );
    }

    /// Records a literal head text segment.
    ///
    /// WHAT: mirrors `record_text` but marks the segment as `Head` origin so it
    ///       appears before any body nodes in source order.
    pub(crate) fn record_head_text(
        &mut self,
        text: StringId,
        byte_len: usize,
        location: SourceLocation,
    ) {
        let mut store = self.store.borrow_mut();
        self.builder.record_text(
            &mut store,
            text,
            byte_len,
            TemplateSegmentOrigin::Head,
            None,
            location,
        );
    }

    /// Records a literal head text segment that may be reactive.
    pub(crate) fn record_reactive_head_text(
        &mut self,
        text: StringId,
        byte_len: usize,
        reactive_subscription: Option<ReactiveSubscription>,
        location: SourceLocation,
    ) {
        let mut store = self.store.borrow_mut();
        self.builder.record_text(
            &mut store,
            text,
            byte_len,
            TemplateSegmentOrigin::Head,
            reactive_subscription,
            location,
        );
    }

    // -------------------------
    //  Recording — expressions
    // -------------------------

    /// Records a non-literal head expression.
    ///
    /// WHAT: creates a `DynamicExpression` node for scalar/reference/reactive
    ///       head values. Reactive subscriptions update `summary.has_reactivity`.
    pub(crate) fn record_head_dynamic_expression(
        &mut self,
        expression: Expression,
        reactive_subscription: Option<ReactiveSubscription>,
        location: SourceLocation,
    ) {
        let mut store = self.store.borrow_mut();
        self.builder.record_dynamic_expression(
            &mut store,
            expression,
            TemplateSegmentOrigin::Head,
            reactive_subscription,
            location,
        );
    }

    // -------------------------
    //  Recording — structure
    // -------------------------

    /// Records a child-template reference.
    pub(crate) fn record_child_template(
        &mut self,
        child_reference: &TemplateTirReference,
        origin: TemplateSegmentOrigin,
        location: SourceLocation,
    ) {
        let mut store = self.store.borrow_mut();
        self.builder
            .record_child_template(&mut store, child_reference, origin, location);
    }

    /// Records a structural slot-placeholder node.
    pub(crate) fn record_slot(
        &mut self,
        slot: SlotPlaceholder,
        location: SourceLocation,
    ) -> Result<(), TemplateError> {
        let mut store = self.store.borrow_mut();
        self.builder.record_slot(&mut store, slot, location)
    }

    /// Records an `$insert("name")` contribution node.
    pub(crate) fn record_insert_contribution(
        &mut self,
        contribution_template_id: TemplateIrId,
        location: SourceLocation,
    ) {
        let mut store = self.store.borrow_mut();
        self.builder
            .record_insert_contribution(&mut store, contribution_template_id, location);
    }

    // -------------------------
    //  Recording — control flow
    // -------------------------

    /// Records a conditional branch-chain node.
    pub(crate) fn record_branch_chain(
        &mut self,
        branches: Vec<TemplateIrBranch>,
        fallback: Option<TemplateIrNodeId>,
        location: SourceLocation,
    ) {
        let mut store = self.store.borrow_mut();
        self.builder
            .record_branch_chain(&mut store, branches, fallback, location);
    }

    /// Records a template loop node.
    pub(crate) fn record_loop(
        &mut self,
        header: TemplateLoopHeader,
        body: TemplateIrNodeId,
        location: SourceLocation,
    ) {
        let mut store = self.store.borrow_mut();
        self.builder.record_loop(&mut store, header, body, location);
    }

    /// Records a loop-control marker node.
    pub(crate) fn record_loop_control(
        &mut self,
        kind: TemplateLoopControlKind,
        location: SourceLocation,
    ) {
        let mut store = self.store.borrow_mut();
        self.builder.record_loop_control(&mut store, kind, location);
    }

    // -------------------------
    //  Whitespace trimming
    // -------------------------

    /// Trims leading whitespace-only text nodes for control-flow body
    /// boundary cleanup.
    pub(crate) fn trim_leading_whitespace(&mut self, string_table: &StringTable) {
        let store = self.store.borrow();
        self.builder
            .trim_leading_whitespace_text(&store, string_table);
    }

    /// Trims trailing whitespace-only text nodes for loop-control sentinel
    /// cleanup.
    pub(crate) fn trim_trailing_whitespace(&mut self, string_table: &StringTable) {
        let store = self.store.borrow();
        self.builder
            .trim_trailing_whitespace_text(&store, string_table);
    }

    // -------------------------
    //  Finalization
    // -------------------------

    /// Finalizes the builder state and returns the long-lived TIR reference.
    ///
    /// WHAT: seals accumulated children under a root sequence node, stores the
    ///       finished `TemplateIr` entry, and returns the required
    ///       `TemplateTirReference` (store-qualified root + store-owner token).
    ///       The `phase` parameter records how far the root has progressed:
    ///       `Parsed` for ordinary body/wrapper construction, `Formatted` for
    ///       prepared control-flow owner roots.
    /// WHY: after this call, the builder state is consumed and the caller
    ///      constructs the durable `Template` with the returned reference.
    pub(crate) fn finish(
        &mut self,
        style: Style,
        kind: TemplateType,
        phase: TemplateTirPhase,
        location: SourceLocation,
    ) -> TemplateTirReference {
        self.debug_assert_registered_store();

        let template_id = {
            let mut store = self.store.borrow_mut();
            self.builder
                .finish(&mut store, style, kind, phase, location)
        };

        // Allocate the canonical empty overlay set through the registry so the
        // reference carries a real registry-backed ID. In this carrier-only
        // phase every parser-emitted reference defaults to "no overlays"; later
        // phases will thread non-empty overlay sets through the same path.
        let overlay_set_id = self
            .registry
            .borrow_mut()
            .allocate_overlay_set(TemplateOverlaySet::empty());

        self.builder
            .finalized_reference(template_id, self.store_id, overlay_set_id, phase)
    }

    /// Debug-check that the direct store handle still matches its registry ID.
    ///
    /// WHAT: validates the context's registry identity without forcing every
    ///       parser recording call through registry lookups.
    /// WHY: the parser still writes through the shared store handle for
    ///      borrow simplicity, but final TIR phases depend on store-qualified
    ///      identity remaining coherent.
    fn debug_assert_registered_store(&self) {
        let registered_store = self.registry.borrow().store_handle(self.store_id);
        debug_assert!(
            registered_store.is_some_and(|store| Rc::ptr_eq(&store, &self.store)),
            "TemplateConstructionContext store handle no longer matches its registry ID"
        );
    }
}
