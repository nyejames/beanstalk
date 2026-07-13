//! Parser-emitted TIR builder state.
//!
//! WHAT: `TemplateParserIrBuilderState` is the in-progress builder state that
//! records literal parser output (text, dynamic expressions, child templates,
//! slots, control flow) into the shared AST-local `TemplateIrStore` while a
//! template is being parsed.
//!
//! WHY: the parser needs a mutable accumulator that owns child-node IDs and
//! summary state during parsing, before the tree is finalized into a
//! `TemplateIrId` and handed off to the long-lived `TemplateTirReference`.
//! Keeping this builder state under `tir/` makes it explicit that parser
//! construction records TIR directly instead of using `Template` as an
//! accumulator.

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::{
    ReactiveSubscription, SlotPlaceholder, Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateLoopControlKind, TemplateLoopHeader,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

use super::builder::TemplateIrBuilder;
use super::ids::{TemplateIrId, TemplateIrNodeId};
use super::node::{TemplateIrBranch, TemplateIrNodeKind};
use super::overlays::TemplateOverlaySetId;
use super::refs::{TemplateRef, TemplateStoreId, TemplateTirChildReference};
use super::store::{TemplateIrStore, TemplateIrStoreOwner};
use super::summary::TemplateIrSummary;
use super::view::TemplateTirPhase;
use std::sync::Arc;

// -------------------------
//  Finalized Parser-TIR Reference
// -------------------------

/// Long-lived reference to a finalized parser-emitted TIR template.
///
/// WHAT: holds the store-qualified `TemplateRef` root allocated when a parser
///       builder state is finished, plus the store-owner token needed to prove
///       same-store instance identity.
/// WHY: after parsing, the in-progress `TemplateParserIrBuilderState` is
///      discarded; this narrow reference keeps the registry-resolvable root and
///      ownership token without carrying the builder state's in-progress
///      child/summary state. The store-owner `Arc` remains the authoritative
///      same-store proof because detached snapshots and direct stores can share
///      numeric `TemplateStoreId` values.
#[derive(Clone, Debug)]
pub(crate) struct TemplateTirReference {
    pub(crate) root: TemplateRef,
    pub(crate) store_owner: Arc<TemplateIrStoreOwner>,

    /// True when this reference points to a TIR tree produced by TIR-native
    /// head-chain composition rather than direct parser emission.
    ///
    /// WHAT: records that the referenced root already reflects composed output.
    /// WHY: downstream classification and handoff must preserve the composed
    ///      structural root rather than treating it as raw parser output.
    pub(crate) is_composed: bool,

    /// Pipeline phase represented by this root reference.
    ///
    /// WHAT: records whether the referenced root is raw parser output,
    ///       TIR-composed, TIR-formatted, or later finalized output.
    /// WHY: render-unit formatting and later passes need to distinguish raw
    ///      parser output from formatter-derived and finalized roots.
    pub(crate) phase: TemplateTirPhase,

    /// Registry-owned overlay-set ID carried by this reference.
    ///
    /// WHAT: identifies the `TemplateOverlaySet` in the module-local
    ///       `TemplateIrRegistry` that holds contextual overlays (expression
    ///       overrides, slot resolution, wrapper context) for this template.
    ///       Production parser-emitted and composed references carry the
    ///       canonical empty overlay set until non-empty overlay wiring lands.
    /// WHY: threading the overlay-set ID on the reference lets later phases
    ///      resolve contextual changes through one stable handle instead of ad
    ///      hoc maps. Keeping it AST-template-local avoids exposing
    ///      registry/view/overlay internals to HIR or backends.
    pub(crate) overlay_set_id: TemplateOverlaySetId,
}

impl TemplateTirReference {
    /// Returns true when this linear-template reference is a current structural root.
    ///
    /// WHAT: admits any root at phase Composed or higher. Composed roots are
    /// the authority for slot-routed head-chain output; Formatted and Finalized
    /// roots are authoritative once render-unit preparation has run.
    /// WHY: parsed roots may still carry pre-format or pre-composition structure,
    /// while later phases are safe for current-state consumers to reuse.
    #[cfg(test)]
    pub(crate) fn can_reuse_as_linear_current_state(&self) -> bool {
        self.phase.is_at_least(TemplateTirPhase::Composed)
    }
}

// -------------------------
//  Parser TIR Builder State
// -------------------------

/// In-progress builder state for parser-emitted TIR.
///
/// WHAT: keeps builder-local summary state and node IDs for nodes allocated
/// in the shared module-scoped `TemplateIrStore` while a template is being
/// parsed.
/// WHY: the parser needs a mutable accumulator for child-node IDs and summary
/// state until the tree is finalized into a `TemplateIrId`. The builder state
/// is discarded once the finalized reference is produced, preventing a
/// per-template store from becoming a second permanent ownership model.
#[derive(Debug)]
pub(crate) struct TemplateParserIrBuilderState {
    pub(crate) template_id: Option<TemplateIrId>,
    children: Vec<TemplateIrNodeId>,
    summary: TemplateIrSummary,

    /// Identity token proving this builder state belongs to a specific `TemplateIrStore`.
    ///
    /// WHAT: cloned from the store when the builder state starts; `Arc::ptr_eq`
    ///       against the current store's owner proves the builder state's
    ///       `template_id` is safe to use in that store.
    /// WHY: cross-context template references may carry IDs from a different store;
    ///      the owner token lets head-reference handling reject those IDs without
    ///      comparing store handles or inspecting private vectors.
    pub(crate) store_owner: Arc<TemplateIrStoreOwner>,

    /// When true, `finish` must not set `summary.has_formatter` even if the
    /// style carries a formatter.
    ///
    /// WHAT: render-unit preparation may refresh every control-flow body so the
    ///       parser-TIR summary no longer represents a formatter-pending surface.
    /// WHY: `finish` cannot re-derive whether bodies were refreshed from the node
    ///      tree alone; this flag is the narrow state bridge from render-unit
    ///      preparation back to builder-state finalization.
    suppress_formatter_summary_on_finish: bool,

    /// Number of children recorded while the parser was still in the head section.
    ///
    /// WHAT: counts every node emitted by a head-record call (head text, head
    ///       dynamic expression, or head-origin child template).
    /// WHY: `$children(..)` wrapper application and other composition passes need
    ///      to distinguish head-origin structural nodes from body direct children
    ///      without relying on a body-origin text marker that may not exist.
    head_node_count: u32,
}

impl TemplateParserIrBuilderState {
    pub(crate) fn new(store_owner: Arc<TemplateIrStoreOwner>) -> Self {
        Self {
            template_id: None,
            children: Vec::new(),
            summary: TemplateIrSummary::default(),
            store_owner,
            suppress_formatter_summary_on_finish: false,
            head_node_count: 0,
        }
    }

    /// Returns a narrow finalized reference that preserves only the
    /// store-qualified root and store-owner token, dropping all in-progress
    /// child/summary state.
    ///
    /// WHAT: lets ordinary `Template::clone()` carry just enough information to
    ///       prove same-store ownership of a finalized parser-emitted ID without
    ///       preserving the full builder-state children or summary.
    /// WHY: the builder state is parse-time only; the long-lived reference
    ///      should be a small, explicitly named type rather than a trimmed builder state.
    pub(crate) fn finalized_reference(
        &self,
        template_id: TemplateIrId,
        store_id: TemplateStoreId,
        overlay_set_id: TemplateOverlaySetId,
    ) -> TemplateTirReference {
        let phase = if self.suppress_formatter_summary_on_finish {
            // Render-unit preparation sets this flag only after every
            // control-flow body has passed through `format_tir_body_root`.
            // The finalized owner root therefore represents formatted body
            // authority even though it is sealed by the parser builder.
            TemplateTirPhase::Formatted
        } else {
            TemplateTirPhase::Parsed
        };

        TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner: Arc::clone(&self.store_owner),
            is_composed: false,
            phase,
            overlay_set_id,
        }
    }

    /// Returns the in-progress root child nodes recorded by the parser.
    ///
    /// WHAT: exposes the builder state's accumulated children so callers that
    ///       already proved same-store ownership can walk the incomplete TIR tree
    ///       before `finish` seals it under a root sequence node.
    /// WHY: runtime control-flow validation needs to traverse parser-emitted TIR
    ///      without forcing the builder to finalize first.
    pub(crate) fn root_children(&self) -> &[TemplateIrNodeId] {
        &self.children
    }

    /// Records a literal text node into this builder state.
    ///
    /// WHAT: creates a `Text` node and updates summary counters for output-size
    ///       estimation and head/body origin tracking.
    /// WHY: parser TIR preserves literal head and body segments in source order
    ///      for composition, formatting, folding and runtime handoff.
    pub(crate) fn record_text(
        &mut self,
        store: &mut TemplateIrStore,
        text: StringId,
        byte_len: usize,
        origin: TemplateSegmentOrigin,
        reactive_subscription: Option<ReactiveSubscription>,
        location: SourceLocation,
    ) {
        let byte_len_u32 = u32::try_from(byte_len).unwrap_or(u32::MAX);
        let node_id = {
            let mut builder = TemplateIrBuilder::new(store);
            builder.push_text_node_with_subscription(
                text,
                byte_len_u32,
                origin,
                reactive_subscription.clone(),
                location,
            )
        };

        self.children.push(node_id);
        self.summary.estimated_output_bytes += byte_len;
        self.summary.text_node_count += 1;
        self.summary.text_byte_count += byte_len;
        if reactive_subscription.is_some() {
            self.summary.has_reactivity = true;
        }

        if origin == TemplateSegmentOrigin::Head {
            self.head_node_count += 1;
        }
    }

    /// Records a dynamic expression splice into this builder state.
    ///
    /// WHAT: creates a `DynamicExpression` node for non-literal head/body segments.
    /// WHY: parser TIR represents ordinary scalar, reference and reactive head
    ///      values in source order for later template passes.
    pub(crate) fn record_dynamic_expression(
        &mut self,
        store: &mut TemplateIrStore,
        expression: Expression,
        origin: TemplateSegmentOrigin,
        reactive_subscription: Option<ReactiveSubscription>,
        location: SourceLocation,
    ) {
        let node_id = {
            let mut builder = TemplateIrBuilder::new(store);
            builder.push_dynamic_expression_node(
                expression,
                origin,
                reactive_subscription.clone(),
                location,
            )
        };

        self.children.push(node_id);
        self.summary.dynamic_expression_count += 1;
        self.summary.is_const_evaluable_shape = false;
        if reactive_subscription.is_some() {
            self.summary.has_reactivity = true;
        }

        if origin == TemplateSegmentOrigin::Head {
            self.head_node_count += 1;
        }
    }

    /// Records a child-template reference node into this builder state.
    ///
    /// WHAT: pushes a `ChildTemplate` node for a same-store finalized template
    ///       reference, preserving the child phase and overlay context.
    /// WHY: head and body template-valued segments must remain structural TIR
    ///      references so later composition can resolve slots and wrappers.
    ///      Rebuilding the child reference as Parsed/empty would silently drop
    ///      wrapper-context overlays attached during child construction.
    pub(crate) fn record_child_template(
        &mut self,
        store: &mut TemplateIrStore,
        reference: &TemplateTirReference,
        origin: TemplateSegmentOrigin,
        location: SourceLocation,
    ) {
        let node_id = {
            let mut builder = TemplateIrBuilder::new(store);
            let child_reference = TemplateTirChildReference::new(
                reference.root,
                reference.phase,
                reference.overlay_set_id,
            );
            builder.push_child_template_node_with_reference(child_reference, location)
        };

        self.children.push(node_id);
        self.summary.child_template_count += 1;

        if origin == TemplateSegmentOrigin::Head {
            self.head_node_count += 1;
        }
    }

    /// Records a structural slot-placeholder node into this builder state.
    pub(crate) fn record_slot(
        &mut self,
        store: &mut TemplateIrStore,
        slot: SlotPlaceholder,
        location: SourceLocation,
    ) -> Result<(), TemplateError> {
        let placeholder = store.tir_slot_placeholder_from_ast(&slot, location.clone())?;
        let node_id = {
            let mut builder = TemplateIrBuilder::new(store);
            builder.push_tir_slot_placeholder_node(placeholder)
        };

        self.children.push(node_id);
        self.summary.slot_count += 1;
        self.summary.has_slots = true;

        Ok(())
    }

    /// Records an `$insert("name")` contribution node into this builder state.
    pub(crate) fn record_insert_contribution(
        &mut self,
        store: &mut TemplateIrStore,
        template: TemplateIrId,
        location: SourceLocation,
    ) {
        let node_id = {
            let mut builder = TemplateIrBuilder::new(store);
            builder.push_insert_contribution_node(template, location)
        };

        self.children.push(node_id);
        self.summary.insert_contribution_count += 1;
        self.summary.has_insert_contributions = true;
        // Insert contributions are slot-insertion helpers that must be routed
        // by composition before the template can fold to a const string.
        self.summary.is_const_evaluable_shape = false;
    }

    /// Records a conditional branch-chain node into this builder state.
    pub(crate) fn record_branch_chain(
        &mut self,
        store: &mut TemplateIrStore,
        branches: Vec<TemplateIrBranch>,
        fallback: Option<TemplateIrNodeId>,
        location: SourceLocation,
    ) {
        let node_id = {
            let mut builder = TemplateIrBuilder::new(store);
            builder.push_branch_chain_node(branches, fallback, location)
        };

        self.children.push(node_id);
        self.summary.has_control_flow = true;
    }

    /// Records a template loop node into this builder state.
    pub(crate) fn record_loop(
        &mut self,
        store: &mut TemplateIrStore,
        header: TemplateLoopHeader,
        body: TemplateIrNodeId,
        location: SourceLocation,
    ) {
        let node_id = {
            let mut builder = TemplateIrBuilder::new(store);
            builder.push_loop_node(header, body, None, location)
        };

        self.children.push(node_id);
        self.summary.has_control_flow = true;
    }

    /// Records a loop-control marker node into this builder state.
    pub(crate) fn record_loop_control(
        &mut self,
        store: &mut TemplateIrStore,
        kind: TemplateLoopControlKind,
        location: SourceLocation,
    ) {
        let node_id = {
            let mut builder = TemplateIrBuilder::new(store);
            builder.push_loop_control_node(kind, location)
        };

        self.children.push(node_id);
        self.summary.has_control_flow = true;
    }

    /// Clears the formatter-pending flag on this builder state's summary.
    ///
    /// WHAT: after render-unit preparation has refreshed every control-flow body,
    ///       the owner summary no longer represents a formatter-pending surface.
    /// WHY: the builder-state summary is copied into the finalized `TemplateIr` by
    ///      `finish`, so clearing it here ensures the finalized owner reflects the
    ///      post-format state. The suppression flag prevents `finish` from
    ///      re-setting the flag based on the style formatter alone.
    pub(crate) fn clear_has_formatter_summary(&mut self) {
        self.summary.has_formatter = false;
        self.suppress_formatter_summary_on_finish = true;
    }

    /// Returns this builder state's template-owned control-flow node ID, if any.
    ///
    /// WHAT: searches the in-progress root children, walking only nested sequence
    ///       nodes and never following child-template references.
    /// WHY: body sync runs before `TemplateConstructionContext::finish`, so the
    ///      finalized `tir_reference` does not exist yet. Runtime slot fills can
    ///      wrap the control-flow node before validation, so direct-child lookup
    ///      is not enough.
    pub(crate) fn control_flow_node_id(&self, store: &TemplateIrStore) -> Option<TemplateIrNodeId> {
        self.children
            .iter()
            .copied()
            .find_map(|child_id| store.control_flow_node_id_in_subtree(child_id))
    }

    /// Removes leading whitespace-only text nodes from the builder state.
    ///
    /// WHAT: removes whitespace-only text emitted after a direct control-flow
    ///       sentinel splits a body.
    /// WHY: branch and fallback bodies begin at the first meaningful boundary.
    pub(crate) fn trim_leading_whitespace_text(
        &mut self,
        store: &TemplateIrStore,
        string_table: &StringTable,
    ) {
        let first_meaningful_index = self
            .children
            .iter()
            .position(|child_id| !node_is_whitespace_only_text(*child_id, store, string_table))
            .unwrap_or(self.children.len());

        if first_meaningful_index == 0 {
            return;
        }

        let mut removed_text_node_count = 0;
        let mut removed_text_byte_count = 0;

        for child_id in &self.children[..first_meaningful_index] {
            if let Some(byte_len) = whitespace_text_byte_len(*child_id, store, string_table) {
                removed_text_node_count += 1;
                removed_text_byte_count += byte_len;
            }
        }

        self.remove_text_summary(removed_text_node_count, removed_text_byte_count);
        self.children.drain(0..first_meaningful_index);
    }

    /// Removes trailing whitespace-only text nodes from the builder state.
    ///
    /// WHAT: removes whitespace-only text before a `[break]` or `[continue]`
    ///       marker is inserted.
    /// WHY: loop-control nodes sit at the structural boundary after any
    ///      meaningful output and don't retain sentinel indentation.
    pub(crate) fn trim_trailing_whitespace_text(
        &mut self,
        store: &TemplateIrStore,
        string_table: &StringTable,
    ) {
        while self
            .children
            .last()
            .is_some_and(|child_id| node_is_whitespace_only_text(*child_id, store, string_table))
        {
            let Some(child_id) = self.children.pop() else {
                break;
            };

            if let Some(byte_len) = whitespace_text_byte_len(child_id, store, string_table) {
                self.remove_text_summary(1, byte_len);
            }
        }
    }

    fn remove_text_summary(&mut self, node_count: u32, byte_len: usize) {
        self.summary.estimated_output_bytes =
            self.summary.estimated_output_bytes.saturating_sub(byte_len);
        self.summary.text_byte_count = self.summary.text_byte_count.saturating_sub(byte_len);
        self.summary.text_node_count = self.summary.text_node_count.saturating_sub(node_count);
    }

    /// Seals this builder state into a finalized `TemplateIr` entry.
    ///
    /// WHAT: wraps accumulated children in a root sequence node, copies summary
    ///       metadata (including head-node count), and stores the finished template.
    /// WHY: parsing completes with a stable `TemplateIrId` that later sync,
    ///      composition, and folding paths can reference without the builder state.
    pub(crate) fn finish(
        &mut self,
        store: &mut TemplateIrStore,
        style: Style,
        kind: TemplateType,
        location: SourceLocation,
    ) -> TemplateIrId {
        if let Some(template_id) = self.template_id {
            return template_id;
        }

        if style.formatter.is_some() && !self.suppress_formatter_summary_on_finish {
            self.summary.has_formatter = true;
        }

        self.summary.head_node_count = self.head_node_count;

        // Render-unit preparation moves a control-flow template's shared head
        // prefix into branch bodies or the loop aggregate wrapper. The owner
        // root must not retain those prefix nodes as ordinary siblings, or
        // skipped branches and zero-iteration loops still render the wrapper
        // shell.
        let root_children: Vec<TemplateIrNodeId> = if self.summary.has_control_flow {
            let first_control_flow_index = self.children.iter().position(|&child_id| {
                store.get_node(child_id).is_some_and(|node| {
                    matches!(
                        node.kind,
                        TemplateIrNodeKind::BranchChain { .. } | TemplateIrNodeKind::Loop { .. }
                    )
                })
            });

            match first_control_flow_index {
                Some(index) if index > 0 => {
                    self.summary.head_node_count = 0;
                    self.children[index..].to_vec()
                }
                _ => self.children.to_owned(),
            }
        } else {
            self.children.to_owned()
        };

        // Use the control-flow node directly as the root instead of wrapping it
        // in a Sequence when one of two conditions holds:
        //
        // 1. A shared head prefix was dropped (the control-flow node was not the
        //    first child). The remaining single BranchChain must not be wrapped in
        //    a Sequence, or skipped branches would still render the prefix shell.
        //    This is the existing head-prefix rule for template `if`.
        //
        // 2. All control-flow bodies were refreshed during render-unit
        //    preparation (suppress_formatter_summary_on_finish). The refreshed
        //    bodies carry their own head-prefix content and formatted output, so
        //    a single BranchChain or Loop is the authoritative root. Wrapping it
        //    in a Sequence would hide that root and force unnecessary subtree
        //    reconstruction.
        //
        // Linear templates, unrefreshed control-flow templates, and multi-child
        // owner roots stay Sequence-shaped.
        let (root, direct_control_flow_root) = match root_children.as_slice() {
            [child_id] => {
                let prefix_was_dropped = *child_id != self.children[0];
                let use_direct = store
                    .get_node(*child_id)
                    .is_some_and(|node| match &node.kind {
                        TemplateIrNodeKind::BranchChain { .. } => {
                            prefix_was_dropped || self.suppress_formatter_summary_on_finish
                        }
                        TemplateIrNodeKind::Loop { .. } => {
                            self.suppress_formatter_summary_on_finish
                        }
                        _ => false,
                    });
                if use_direct {
                    (*child_id, true)
                } else {
                    let mut builder = TemplateIrBuilder::new(store);
                    let root = builder.push_sequence_node(root_children, location.clone());
                    (root, false)
                }
            }
            _ => {
                let mut builder = TemplateIrBuilder::new(store);
                let root = builder.push_sequence_node(root_children, location.clone());
                (root, false)
            }
        };

        self.summary.max_depth = if direct_control_flow_root {
            0
        } else {
            u16::from(!self.children.is_empty())
        };

        let template_id = {
            let mut builder = TemplateIrBuilder::new(store);
            builder.finish_template(root, style, kind, self.summary.to_owned(), location)
        };

        self.template_id = Some(template_id);
        template_id
    }
}

/// Returns true when a builder-state node is whitespace-only literal text.
fn node_is_whitespace_only_text(
    node_id: TemplateIrNodeId,
    store: &TemplateIrStore,
    string_table: &StringTable,
) -> bool {
    whitespace_text_byte_len(node_id, store, string_table).is_some()
}

/// Returns the byte length when `node_id` is whitespace-only literal text.
fn whitespace_text_byte_len(
    node_id: TemplateIrNodeId,
    store: &TemplateIrStore,
    string_table: &StringTable,
) -> Option<usize> {
    let node = store.get_node(node_id)?;
    let TemplateIrNodeKind::Text { text, byte_len, .. } = &node.kind else {
        return None;
    };

    string_table
        .resolve(*text)
        .trim()
        .is_empty()
        .then_some(*byte_len as usize)
}
