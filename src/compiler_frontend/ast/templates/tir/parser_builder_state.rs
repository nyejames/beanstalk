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
use super::refs::{TemplateTirChildReference, TemplateTirReference};
use super::store::TemplateIrStore;
use super::summary::TemplateIrSummary;
use super::view::TemplateTirPhase;

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
    pub(crate) fn new() -> Self {
        Self {
            template_id: None,
            children: Vec::new(),
            summary: TemplateIrSummary::default(),
            head_node_count: 0,
        }
    }

    /// Returns the durable module-local reference for a finished root.
    pub(crate) fn finalized_reference(
        &self,
        template_id: TemplateIrId,
        overlay_set_id: TemplateOverlaySetId,
        phase: TemplateTirPhase,
    ) -> TemplateTirReference {
        TemplateTirReference {
            root: template_id,
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
        self.summary.record_text_node(byte_len);
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
        self.summary
            .record_dynamic_expression(reactive_subscription.is_some());

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
        self.summary.record_child_template();

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
        self.summary.record_slot();

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
        self.summary.record_insert_contribution();
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

    /// Returns this builder state's template-owned control-flow node ID, if any.
    ///
    /// WHAT: searches the in-progress root children, walking only nested sequence
    ///       nodes and never following child-template references.
    /// WHY: render-unit preparation runs before
    ///      `TemplateConstructionContext::finish`, so the finalized
    ///      `tir_reference` does not exist yet. Runtime slot fills can wrap the
    ///      control-flow node before validation, so direct-child lookup is not
    ///      enough.
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

        // Use a single control-flow child directly as the root. Render-unit
        // preparation has already moved the shared head prefix into branch
        // bodies or the loop aggregate wrapper, and the structural root shape
        // now carries that fact without a second lifecycle marker.
        //
        // Linear templates and multi-child owner roots stay Sequence-shaped.
        let (root, direct_control_flow_root) = match root_children.as_slice() {
            [child_id] => {
                let use_direct = store.get_node(*child_id).is_some_and(|node| {
                    matches!(
                        node.kind,
                        TemplateIrNodeKind::BranchChain { .. } | TemplateIrNodeKind::Loop { .. }
                    )
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
