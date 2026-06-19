//! Temporary converter from AST `Template` to TIR.
//!
//! WHAT: translates current `Template` / `TemplateContent` values into
//! `TemplateIrStore` entries, preserving source locations, style metadata,
//! template kind, text origin, slot placeholders, control-flow structure,
//! runtime slot application metadata, and reactive subscriptions.
//!
//! WHY: the converter exists to prove parity between the old `Template`-based
//! representation and the new TIR tree. Once TIR is the authoritative path,
//! this converter and the old `Template` internals it replaces will be deleted
//! at a documented checkpoint.
//!
//! ## Ownership contract
//!
//! The converter is AST-local. It reads `Template` values and writes into a
//! `TemplateIrStore`. It does not own HIR, backend, or public API data.
//!
//! ## Semantic parity constraint
//!
//! The converter must produce the same structural shape as the current
//! `Template` → `TemplateContent` path. Behaviour changes are out of scope
//! unless they are bug fixes with regression tests.
//!
//! ## Summary computation
//!
//! `TemplateIrSummary` is computed during the conversion walk without a
//! second traversal. Each helper updates the running summary as it creates
//! nodes.

use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::template::{
    SlotPlaceholder, TemplateAtom, TemplateContent, TemplateSegmentOrigin,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchChain, TemplateControlFlow, TemplateLoopControlFlow, TemplateLoopControlKind,
};
use crate::compiler_frontend::ast::templates::template_slots::RuntimeSlotSiteId;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::tir::ids::{TemplateIrId, TemplateIrNodeId};
use crate::compiler_frontend::ast::templates::tir::node::{
    TemplateIr, TemplateIrBranch, TemplateIrNode, TemplateIrNodeKind,
};
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::ast::templates::tir::summary::TemplateIrSummary;
use crate::compiler_frontend::instrumentation::{
    AstCounter, add_ast_counter, record_ast_counter_max,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

// -------------------------
//  Converter Context
// -------------------------

/// Accumulates summary metadata and node depth during conversion.
///
/// WHAT: holds the running summary counters and the current tree depth so each
/// helper can update them without a second traversal.
/// WHY: computing the summary inline with conversion avoids a separate tree walk
/// and keeps the summary consistent with the actual TIR structure.
struct ConvertSummary {
    summary: TemplateIrSummary,
    current_depth: u16,
}

impl ConvertSummary {
    fn new() -> Self {
        Self {
            summary: TemplateIrSummary::empty(),
            current_depth: 0,
        }
    }

    /// Records a text node and updates the running summary.
    fn record_text_node(&mut self, byte_len: usize) {
        self.summary.text_node_count += 1;
        self.summary.text_byte_count += byte_len;
        self.summary.estimated_output_bytes += byte_len;
        self.update_depth();
    }

    /// Records a dynamic expression node.
    fn record_dynamic_expression(&mut self, has_reactive_subscription: bool) {
        self.summary.dynamic_expression_count += 1;
        if has_reactive_subscription {
            self.summary.has_reactivity = true;
        }
        self.summary.is_const_evaluable_shape = false;
        self.update_depth();
    }

    /// Records a child template reference.
    fn record_child_template(&mut self) {
        self.summary.child_template_count += 1;
        self.update_depth();
    }

    /// Records a slot placeholder.
    fn record_slot(&mut self) {
        self.summary.slot_count += 1;
        self.summary.has_slots = true;
        self.update_depth();
    }

    /// Records a control-flow node (branch, loop, or loop control).
    fn record_control_flow(&mut self) {
        self.summary.has_control_flow = true;
        self.summary.is_const_evaluable_shape = false;
        self.update_depth();
    }

    /// Records a runtime slot site.
    fn record_runtime_slot_site(&mut self) {
        self.summary.has_slots = true;
        self.summary.is_const_evaluable_shape = false;
        self.update_depth();
    }

    /// Bumps depth tracking for the current node.
    fn update_depth(&mut self) {
        if self.current_depth > self.summary.max_depth {
            self.summary.max_depth = self.current_depth;
        }
    }

    /// Enters a nested level.
    fn enter_depth(&mut self) {
        self.current_depth += 1;
    }

    /// Exits a nested level.
    fn exit_depth(&mut self) {
        self.current_depth = self.current_depth.saturating_sub(1);
    }
}

// -------------------------
//  Public Converter Entry Point
// -------------------------

/// Converts an AST `Template` into a TIR template entry.
///
/// WHAT: walks the template's content atoms, control-flow structure, and
/// runtime slot application metadata, creating TIR nodes in the store.
/// Returns the `TemplateIrId` for the newly created template entry.
///
/// WHY: this converter is the parity bridge between the old `Template`-based
/// representation and the new TIR tree. It is temporary — once TIR is the
/// authoritative path, this function and the old `Template` internals will
/// be deleted.
///
/// ## Parameters
///
/// - `template`: the source AST template to convert.
/// - `store`: the TIR store to write nodes and the template entry into.
/// - `string_table`: used to compute byte lengths for text nodes.
///
/// ## Returns
///
/// The `TemplateIrId` identifying the newly created template entry in the store.
pub(crate) fn convert_template_to_tir(
    template: &Template,
    store: &mut TemplateIrStore,
    string_table: &StringTable,
) -> TemplateIrId {
    let mut summary = ConvertSummary::new();

    // Record store sizes before conversion so counters reflect only this template.
    let templates_before = store.template_count();
    let nodes_before = store.node_count();

    // Track formatter presence from the style.
    if template.style.formatter.is_some() {
        summary.summary.has_formatter = true;
    }

    // Convert the root content into a TIR node tree.
    let root = convert_content_to_node(
        &template.content,
        &template.control_flow,
        template.runtime_slot_application.as_ref().map(|plan| {
            plan.slot_sites
                .iter()
                .map(|site| site.id)
                .collect::<Vec<_>>()
        }),
        store,
        string_table,
        &mut summary,
        &template.location,
    );

    // Record wrapper set if there are conditional child wrappers.
    if !template.conditional_child_wrappers.is_empty() {
        summary.summary.wrapper_count += template.conditional_child_wrappers.len() as u32;
        store.push_wrapper_set(
            crate::compiler_frontend::ast::templates::tir::store::TemplateWrapperSet {
                _reserved: (),
            },
        );
        add_ast_counter(AstCounter::TirWrapperSetsCreated, 1);
    }

    // Build the template entry.
    let mut tir_template = TemplateIr::new(
        root,
        template.style.clone(),
        template.kind.clone(),
        summary.summary.clone(),
        template.location.clone(),
    );

    // Preserve the parent wrappers so the TIR fold path can apply them later.
    tir_template.conditional_child_wrappers = template.conditional_child_wrappers.clone();

    let template_id = store.push_template(tir_template);

    // Update global TIR counters with deltas for this conversion only.
    let templates_created = store.template_count() - templates_before;
    let nodes_created = store.node_count() - nodes_before;

    add_ast_counter(AstCounter::TirTemplatesCreated, templates_created);
    add_ast_counter(
        AstCounter::TirConverterTemplatesConverted,
        templates_created,
    );
    add_ast_counter(AstCounter::TirNodesCreated, nodes_created);
    add_ast_counter(AstCounter::TirConverterNodesConverted, nodes_created);
    add_ast_counter(
        AstCounter::TirTextNodesCreated,
        summary.summary.text_node_count as usize,
    );
    add_ast_counter(
        AstCounter::TirTextBytesRecorded,
        summary.summary.text_byte_count,
    );
    record_ast_counter_max(AstCounter::TirMaxDepth, summary.summary.max_depth as usize);

    template_id
}

// -------------------------
//  Content Conversion
// -------------------------

/// Converts a `TemplateContent` (and optional control flow) into a TIR node.
///
/// WHAT: for a single atom, returns the atom's node directly. For multiple atoms
/// or content with control flow, wraps them in a `Sequence` node.
/// WHY: most template bodies are sequences of atoms; wrapping single atoms in
/// a sequence would add unnecessary depth.
fn convert_content_to_node(
    content: &TemplateContent,
    control_flow: &Option<TemplateControlFlow>,
    runtime_slot_sites: Option<Vec<RuntimeSlotSiteId>>,
    store: &mut TemplateIrStore,
    string_table: &StringTable,
    summary: &mut ConvertSummary,
    location: &SourceLocation,
) -> TemplateIrNodeId {
    // If there's control flow, convert it first — it becomes the primary node.
    if let Some(cf) = control_flow {
        return convert_control_flow(cf, store, string_table, summary, location);
    }

    // If there are runtime slot sites, create a RuntimeSlotSite node for the first one.
    // (In practice, runtime slot application plans have one primary site.)
    if let Some(sites) = runtime_slot_sites
        && let Some(&site_id) = sites.first()
    {
        summary.record_runtime_slot_site();
        return store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::RuntimeSlotSite { site: site_id },
            location.clone(),
        ));
    }

    // For a single atom, convert directly without wrapping in a sequence.
    if content.atoms.len() == 1 {
        return convert_atom(&content.atoms[0], store, string_table, summary, location);
    }

    // For multiple atoms, create a sequence node.
    summary.enter_depth();
    let children: Vec<TemplateIrNodeId> = content
        .atoms
        .iter()
        .map(|atom| convert_atom(atom, store, string_table, summary, location))
        .collect();
    summary.exit_depth();

    store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence { children },
        location.clone(),
    ))
}

// -------------------------
//  Atom Conversion
// -------------------------

/// Converts a single `TemplateAtom` into a TIR node.
///
/// WHAT: dispatches on the atom kind to produce the appropriate `TemplateIrNodeKind`.
/// WHY: each atom kind has a distinct TIR representation that downstream passes
/// (fold, format, HIR) can dispatch on cleanly.
fn convert_atom(
    atom: &TemplateAtom,
    store: &mut TemplateIrStore,
    string_table: &StringTable,
    summary: &mut ConvertSummary,
    default_location: &SourceLocation,
) -> TemplateIrNodeId {
    match atom {
        TemplateAtom::Content(segment) => convert_segment(
            &segment.expression,
            segment.origin,
            segment.reactive_subscription.is_some(),
            store,
            string_table,
            summary,
            &segment.expression.location,
        ),

        TemplateAtom::Slot(placeholder) => {
            convert_slot_placeholder(placeholder, store, summary, default_location)
        }
    }
}

/// Converts a template segment expression into the appropriate TIR node.
///
/// WHAT: examines the expression kind to determine if it's text, a child template,
/// or a dynamic expression, and creates the matching TIR node.
/// WHY: TIR distinguishes text from expressions so folding can handle them differently;
/// child templates get their own opaque reference node.
fn convert_segment(
    expression: &Expression,
    origin: TemplateSegmentOrigin,
    has_reactive_subscription: bool,
    store: &mut TemplateIrStore,
    string_table: &StringTable,
    summary: &mut ConvertSummary,
    location: &SourceLocation,
) -> TemplateIrNodeId {
    match &expression.kind {
        // Interned string literals become text nodes with byte-length metadata.
        ExpressionKind::StringSlice(string_id) => {
            let text = *string_id;
            let byte_len = string_table.resolve(text).len();
            summary.record_text_node(byte_len);

            store.push_node(TemplateIrNode::new(
                TemplateIrNodeKind::Text {
                    text,
                    byte_len: byte_len as u32,
                    origin,
                },
                location.clone(),
            ))
        }

        // Nested template expressions become child template references unless
        // they still require legacy formatter handling. Phase B3 will make
        // formatted children TIR-native; until then, preserve formatter
        // semantics by keeping those children as expression nodes that call
        // back into the legacy formatter-aware fold path.
        ExpressionKind::Template(child_template) => {
            if child_template.content_needs_formatting {
                summary.record_dynamic_expression(has_reactive_subscription);

                return store.push_node(TemplateIrNode::new(
                    TemplateIrNodeKind::DynamicExpression {
                        expression: Box::new(expression.clone()),
                        origin,
                    },
                    location.clone(),
                ));
            }

            let child_id = convert_template_to_tir(child_template, store, string_table);
            summary.record_child_template();

            store.push_node(TemplateIrNode::new(
                TemplateIrNodeKind::ChildTemplate { template: child_id },
                location.clone(),
            ))
        }

        // All other expressions become dynamic expression nodes.
        _ => {
            summary.record_dynamic_expression(has_reactive_subscription);

            store.push_node(TemplateIrNode::new(
                TemplateIrNodeKind::DynamicExpression {
                    expression: Box::new(expression.clone()),
                    origin,
                },
                location.clone(),
            ))
        }
    }
}

// -------------------------
//  Slot Placeholder Conversion
// -------------------------

/// Converts a `SlotPlaceholder` into a TIR slot node.
///
/// WHAT: preserves the slot key, child wrappers, and skip flag.
/// WHY: slot placeholders are structural markers that composition and HIR
/// lowering must consume; TIR preserves them as-is.
fn convert_slot_placeholder(
    placeholder: &SlotPlaceholder,
    store: &mut TemplateIrStore,
    summary: &mut ConvertSummary,
    location: &SourceLocation,
) -> TemplateIrNodeId {
    summary.record_slot();

    store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Slot {
            slot: placeholder.clone(),
        },
        location.clone(),
    ))
}

// -------------------------
//  Control Flow Conversion
// -------------------------

/// Converts a `TemplateControlFlow` into a TIR node.
///
/// WHAT: dispatches on the control-flow kind (branch chain, loop, loop control)
/// and creates the matching TIR node.
/// WHY: control-flow structure must be preserved in TIR so folding and HIR
/// lowering can handle branches and loops correctly.
fn convert_control_flow(
    control_flow: &TemplateControlFlow,
    store: &mut TemplateIrStore,
    string_table: &StringTable,
    summary: &mut ConvertSummary,
    location: &SourceLocation,
) -> TemplateIrNodeId {
    match control_flow {
        TemplateControlFlow::BranchChain(chain) => {
            convert_branch_chain(chain, store, string_table, summary)
        }

        TemplateControlFlow::Loop(loop_cf) => {
            convert_loop(loop_cf, store, string_table, summary, location)
        }

        TemplateControlFlow::LoopControl(signal) => {
            convert_loop_control(signal.kind, store, summary, &signal.location)
        }
    }
}

/// Converts a branch chain into a TIR `BranchChain` node.
///
/// WHAT: converts each conditional branch's body content and selector expression,
/// plus an optional fallback body.
/// WHY: the branch chain structure matches the AST shape exactly, preserving
/// condition ordering and fallback behavior.
fn convert_branch_chain(
    chain: &TemplateBranchChain,
    store: &mut TemplateIrStore,
    string_table: &StringTable,
    summary: &mut ConvertSummary,
) -> TemplateIrNodeId {
    summary.record_control_flow();
    summary.enter_depth();

    let branches: Vec<TemplateIrBranch> = chain
        .branches
        .iter()
        .map(|branch| {
            let body_node = convert_content_to_node(
                &branch.content,
                &None,
                None,
                store,
                string_table,
                summary,
                &branch.location,
            );

            TemplateIrBranch::new(branch.selector.clone(), body_node, branch.location.clone())
        })
        .collect();

    let fallback = chain.fallback.as_ref().map(|fb| {
        convert_content_to_node(
            &fb.content,
            &None,
            None,
            store,
            string_table,
            summary,
            &fb.location,
        )
    });

    summary.exit_depth();

    store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::BranchChain { branches, fallback },
        chain.location.clone(),
    ))
}

/// Converts a loop control flow into a TIR `Loop` node.
///
/// WHAT: converts the loop header, body content, and optional aggregate wrapper.
/// WHY: loops are a core template control-flow construct; TIR preserves their
/// structure for folding and HIR lowering.
fn convert_loop(
    loop_cf: &TemplateLoopControlFlow,
    store: &mut TemplateIrStore,
    string_table: &StringTable,
    summary: &mut ConvertSummary,
    _location: &SourceLocation,
) -> TemplateIrNodeId {
    summary.record_control_flow();
    summary.enter_depth();

    let body = convert_content_to_node(
        &loop_cf.body_content,
        &None,
        None,
        store,
        string_table,
        summary,
        &loop_cf.location,
    );

    // Convert the aggregate wrapper if present.
    //
    // The aggregate wrapper node is currently an empty placeholder because
    // full render-unit migration belongs to Phase B4. To preserve folding
    // semantics in Phase B2, we also carry the original AST aggregate render
    // plan in a temporary field that the TIR fold path consumes.
    let aggregate_render_plan = loop_cf.aggregate_render_plan.clone();
    let aggregate_wrapper = aggregate_render_plan.as_ref().map(|_aggregate| {
        store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Sequence { children: vec![] },
            loop_cf.location.clone(),
        ))
    });

    summary.exit_depth();

    store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Loop {
            header: loop_cf.header.clone(),
            body,
            aggregate_wrapper,
            aggregate_render_plan,
        },
        loop_cf.location.clone(),
    ))
}

/// Converts a loop control signal into a TIR `LoopControl` node.
///
/// WHAT: preserves the break/continue kind.
/// WHY: loop control signals are structural markers consumed by the nearest
/// active loop during folding and HIR lowering.
fn convert_loop_control(
    kind: TemplateLoopControlKind,
    store: &mut TemplateIrStore,
    summary: &mut ConvertSummary,
    location: &SourceLocation,
) -> TemplateIrNodeId {
    summary.record_control_flow();

    store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::LoopControl { kind },
        location.clone(),
    ))
}
