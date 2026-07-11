//! Compatibility-content materialization and control-flow root installation.
//!
//! WHAT: materializes remaining detached `TemplateContent` payloads into TIR and
//! installs TIR-derived control-flow body roots.
//! WHY: both compatibility boundaries belong inside TIR while Phase G removes
//! the remaining old-authority consumers.

#[cfg(test)]
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
#[cfg(test)]
use crate::compiler_frontend::ast::templates::template::{
    TemplateAtom, TemplateConstValueKind, TemplateContent, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::TemplateControlFlowTirReference;
#[cfg(test)]
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchChain, TemplateControlFlow, TemplateLoopControlFlow,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
#[cfg(test)]
use crate::compiler_frontend::ast::templates::tir::{
    CurrentStateMaterializationSummary, TemplateIrBuilder, TemplateIrId, TemplateIrNode,
    TemplateIrNodeKind, TemplateIrStoreOwner, TemplateIrSummary, TemplateTirChildReference,
    classify_materialized_current_tir_template,
};
use crate::compiler_frontend::ast::templates::tir::{
    TemplateIrNodeId, TemplateIrStore, TemplateOverlaySetId, TemplateParserIrBuilderState,
    TemplateTirPhase,
};
#[cfg(test)]
use crate::compiler_frontend::instrumentation::ast_counters::{AstCounter, increment_ast_counter};
#[cfg(test)]
use crate::compiler_frontend::symbols::string_interning::StringTable;
#[cfg(test)]
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
#[cfg(test)]
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

// -------------------------
//  Control-flow body target
// -------------------------

/// Identifies which body inside a control-flow TIR node should receive a
/// finalized simple TIR root.
///
/// WHAT: branch chains have multiple branch bodies plus an optional fallback;
///       loops have one body. This enum lets the store helper locate the right
///       body node ID without exposing vector indexes directly.
/// WHY: keeps `TemplateIrStore` replacement logic independent of AST branch
///      types while still describing the replacement target precisely.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ControlFlowBodyKind {
    /// A branch in a `BranchChain`, identified by its position in the branch vector.
    Branch { index: usize },

    /// The optional fallback body of a `BranchChain`.
    Fallback,

    /// The per-iteration body of a `Loop`.
    LoopBody,
}

// -------------------------
//  Content materialization failure
// -------------------------

/// Reason why compatibility content could not be materialized into TIR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(test)]
pub(crate) enum TemplateTirSyncMissReason {
    NonContentAtom,
    ChildTemplateCycle,
    /// The TIR reference already points to a TIR-native composed tree.
    ///
    /// WHAT: `compose_tir_head_chain` produced this reference directly, so the
    ///       tree already reflects composed content and does not need to be
    ///       rebuilt from the template's stored content.
    /// WHY: content materialization would discard the composed structure.
    ComposedTirReference,
}

// -------------------------
//  Child construction context
// -------------------------

/// Tracks child-template TIR construction state within one materialization pass.
///
/// WHAT: holds a memo map from child template addresses to their built
///       `TemplateIrId` in the current store, plus a set of templates currently
///       being built for cycle detection.
/// WHY: recursive child construction can encounter the same child multiple
///      times (e.g., repeated template references) and must not recurse into
///      itself (e.g., a child that references its parent through a cycle).
#[cfg(test)]
pub(crate) struct ChildMaterializationContext {
    /// Maps a child template's address to its built `TemplateIrId`.
    ///
    /// WHAT: uses the stable address of the `Template` value inside its owning
    ///       `ExpressionKind::Template(Box<Template>)` as an identity key.
    /// WHY: each `Box<Template>` has a unique address during one construction
    ///      pass, and the address is never dereferenced after the pass. This
    ///      avoids requiring `Template` to implement `Hash`/`Eq` and keeps the
    ///      memo local to the current store.
    pub(crate) memo: HashMap<usize, TemplateIrId>,

    /// Addresses of templates currently being built (cycle detection).
    ///
    /// WHAT: records every child whose construction is in flight.
    /// WHY: with owned template data a true cycle cannot be constructed through
    ///      ordinary safe code, but defensive detection guards against future
    ///      composition paths that might share child references or against
    ///      accidental re-entrant calls.
    pub(crate) in_progress: HashSet<usize>,
}

#[cfg(test)]
impl ChildMaterializationContext {
    /// Creates an empty child-construction memo and cycle-detection set.
    pub(crate) fn new() -> Self {
        Self {
            memo: HashMap::new(),
            in_progress: HashSet::new(),
        }
    }
}

/// Builds a finalized simple TIR sequence root from an arbitrary linear
/// `TemplateContent` slice.
///
/// WHAT: converts one linear compatibility payload. The caller supplies the
///       content and root location so detached body roots retain source identity.
/// WHY: avoids duplicating the atom-to-node conversion path and gives each call
///      its own isolated child-construction memo.
#[cfg(test)]
pub(crate) fn build_finalized_tir_root_from_content(
    template: &Template,
    store: &mut TemplateIrStore,
    string_table: &StringTable,
    content: &TemplateContent,
    root_location: SourceLocation,
) -> Result<(TemplateIrNodeId, TemplateIrSummary), TemplateTirSyncMissReason> {
    let mut child_context = ChildMaterializationContext::new();
    build_finalized_tir_root_with_child_context(
        template,
        store,
        string_table,
        content,
        root_location,
        &mut child_context,
    )
}

/// Inner content-to-TIR conversion that carries an explicit child-construction
/// context.
///
/// WHAT: identical to `build_finalized_tir_root_from_content` except that
///       `ExpressionKind::Template` children are resolved through
///       `materialize_child_template`, allowing recursive child construction
///       and same-store reuse within one pass.
/// WHY: keeping the context explicit lets recursive materialization reuse one
///      memo across the entire root.
#[cfg(test)]
fn build_finalized_tir_root_with_child_context(
    _template: &Template,
    store: &mut TemplateIrStore,
    string_table: &StringTable,
    content: &TemplateContent,
    root_location: SourceLocation,
    child_context: &mut ChildMaterializationContext,
) -> Result<(TemplateIrNodeId, TemplateIrSummary), TemplateTirSyncMissReason> {
    let mut summary = TemplateIrSummary::default();
    let mut children = Vec::with_capacity(content.atoms.len());
    let store_owner = store.owner();

    for atom in &content.atoms {
        match atom {
            TemplateAtom::Content(segment) => match &segment.expression.kind {
                ExpressionKind::StringSlice(text) => {
                    let byte_len = string_table.resolve(*text).len();
                    summary.estimated_output_bytes += byte_len;
                    summary.text_node_count += 1;
                    summary.text_byte_count += byte_len;

                    let byte_len_u32 = u32::try_from(byte_len).unwrap_or(u32::MAX);
                    let mut builder = TemplateIrBuilder::new(store);
                    children.push(builder.push_text_node_with_subscription(
                        *text,
                        byte_len_u32,
                        segment.origin,
                        segment.reactive_subscription.clone(),
                        segment.expression.location.to_owned(),
                    ));
                }

                ExpressionKind::Template(child) => {
                    // Prefer an existing same-store finalized TIR reference.
                    // If the child only has a cross-store or missing reference,
                    // recursively build its TIR in the current store so the
                    // parent can keep a safe local reference.
                    let child_reference = resolve_child_template_reference(
                        child,
                        store,
                        &store_owner,
                        string_table,
                        child_context,
                    )?;
                    let child_template_id = child_reference
                        .template_id_in_store(store.store_id())
                        .expect(
                            "resolve_child_template_reference must return a same-store reference",
                        );

                    if matches!(child.kind, TemplateType::SlotInsert(_)) {
                        // Slot insertion helpers are converted to InsertContribution
                        // nodes so slot composition can consume them. The fold path
                        // rejects these if they escape composition.
                        summary.is_const_evaluable_shape = false;
                        summary.insert_contribution_count += 1;
                        summary.has_insert_contributions = true;

                        let mut builder = TemplateIrBuilder::new(store);
                        children.push(builder.push_insert_contribution_node(
                            child_template_id,
                            segment.expression.location.to_owned(),
                        ));
                    } else {
                        summary.child_template_count += 1;
                        // Derive child const-evaluable shape from the same-store
                        // TIR summary. The child was just resolved into this
                        // store (either reused or recursively built), so its TIR
                        // entry carries a fresh `is_const_evaluable_shape`
                        // computed from the authoritative template fields.
                        let child_is_const_evaluable = store
                            .get_template(child_template_id)
                            .is_some_and(|child_tir| child_tir.summary.is_const_evaluable_shape);
                        summary.is_const_evaluable_shape &= child_is_const_evaluable;

                        let mut builder = TemplateIrBuilder::new(store);
                        children.push(builder.push_child_template_node_with_reference(
                            child_reference,
                            segment.expression.location.to_owned(),
                        ));
                    }
                }

                _ => {
                    summary.dynamic_expression_count += 1;
                    summary.has_reactivity |= segment.reactive_subscription.is_some();
                    summary.is_const_evaluable_shape &= materialized_expression_is_const_evaluable(
                        &segment.expression,
                        store,
                        &store_owner,
                        string_table,
                        child_context,
                    );

                    let mut builder = TemplateIrBuilder::new(store);
                    children.push(builder.push_dynamic_expression_node(
                        segment.expression.to_owned(),
                        segment.origin,
                        segment.reactive_subscription.clone(),
                        segment.expression.location.to_owned(),
                    ));
                }
            },
        }
    }

    summary.max_depth = u16::from(!children.is_empty());

    // Always wrap built atoms in a Sequence node. This matches the parser
    // builder's method, which always wraps root children in a Sequence, and
    // ensures all TIR consumers (control-flow body roots, recursive child
    // construction, folding, classification) see a uniform Sequence-rooted
    // shape.
    let mut builder = TemplateIrBuilder::new(store);
    let root = builder.push_sequence_node(children, root_location);

    Ok((root, summary))
}

/// Classifies an expression while compatibility materialization owns its child
/// template construction.
///
/// WHAT: keeps ordinary expression recursion in `Expression`, resolves every
///       embedded template into the same store, then classifies the resulting
///       TIR entry.
/// WHY: this boundary is already converting detached compatibility content.
///      Nested templates must join that one conversion pass instead of calling
///      a general API that can reconstruct content from inside TIR recursion.
#[cfg(test)]
fn materialized_expression_is_const_evaluable(
    expression: &Expression,
    store: &mut TemplateIrStore,
    store_owner: &Arc<TemplateIrStoreOwner>,
    string_table: &StringTable,
    child_context: &mut ChildMaterializationContext,
) -> bool {
    let mut materialization_failed = false;
    let kind = expression
        .const_value_kind_with_template_classifier(&mut |child| {
            let Ok(reference) = resolve_child_template_reference(
                child,
                store,
                store_owner,
                string_table,
                child_context,
            ) else {
                materialization_failed = true;
                return Ok(TemplateConstValueKind::NonConst);
            };
            let Some(template_id) = reference.template_id_in_store(store.store_id()) else {
                materialization_failed = true;
                return Ok(TemplateConstValueKind::NonConst);
            };

            match classify_materialized_current_tir_template(
                &child.kind,
                store,
                template_id,
                string_table,
            ) {
                Ok(classification) => Ok(classification.const_value_kind),
                Err(_) => {
                    materialization_failed = true;
                    Ok(TemplateConstValueKind::NonConst)
                }
            }
        })
        .ok();

    if materialization_failed {
        return false;
    }

    kind.is_some_and(|kind| kind.is_compile_time_value())
}

/// Resolves a child template to a same-store `TemplateIrId`.
///
/// WHAT: reuses an existing finalized TIR reference when it belongs to
///       the current store; otherwise triggers recursive child construction
///       and records the outcome counters.
/// WHY: centralizes the reuse/construction decision so the content walker
///      stays focused on node emission.
#[cfg(test)]
fn resolve_child_template_reference(
    child: &Template,
    store: &mut TemplateIrStore,
    store_owner: &Arc<TemplateIrStoreOwner>,
    string_table: &StringTable,
    child_context: &mut ChildMaterializationContext,
) -> Result<TemplateTirChildReference, TemplateTirSyncMissReason> {
    if let Some(reference) = child.tir_reference.as_ref()
        && Arc::ptr_eq(&reference.store_owner, store_owner)
        && reference.root.store_id == store.store_id()
        && (reference.can_reuse_as_linear_current_state() || child.control_flow.is_none())
    {
        increment_ast_counter(AstCounter::TemplateTirChildSameStoreReuse);
        return Ok(TemplateTirChildReference::new(
            reference.root,
            reference.phase,
            reference.overlay_set_id,
        ));
    }

    let template_id = materialize_child_template(child, store, string_table, child_context)?;
    Ok(TemplateTirChildReference::same_store(
        template_id,
        store.store_id(),
        TemplateTirPhase::Finalized,
        TemplateOverlaySetId::empty(),
    ))
}

/// Recursively builds a child template's TIR in the current store.
///
/// WHAT: checks the memo map and cycle guard, confirms the child has no
///       structural blockers, builds a finalized TIR tree for the child's
///       content, and finishes a new top-level `TemplateIr` entry.
/// WHY: a parent template may reference a child that was composed or cloned
///      from another store and therefore lacks a same-store TIR proof.
///      Building the child locally keeps the parent's TIR self-contained.
#[cfg(test)]
pub(crate) fn materialize_child_template(
    child: &Template,
    store: &mut TemplateIrStore,
    string_table: &StringTable,
    child_context: &mut ChildMaterializationContext,
) -> Result<TemplateIrId, TemplateTirSyncMissReason> {
    let child_address = child as *const Template as usize;

    if let Some(&template_id) = child_context.memo.get(&child_address) {
        return Ok(template_id);
    }

    if !child_context.in_progress.insert(child_address) {
        return Err(TemplateTirSyncMissReason::ChildTemplateCycle);
    }

    increment_ast_counter(AstCounter::TemplateTirChildRecursiveMaterializationAttempts);

    // Control flow and conditional child wrappers are handled by the
    // control-flow-aware builder. Composed references remain rejection reasons.
    let build_result = if let Some(control_flow) = &child.control_flow {
        build_finalized_tir_root_with_control_flow(child, control_flow, store, string_table)
    } else {
        // Composed roots must be consumed through their store-qualified
        // reference. Rebuilding them from compatibility content would discard
        // slot routing and overlay context.
        if child
            .tir_reference
            .as_ref()
            .is_some_and(|reference| reference.is_composed)
        {
            increment_ast_counter(AstCounter::TemplateTirChildRecursiveMaterializationFailures);
            child_context.in_progress.remove(&child_address);
            return Err(TemplateTirSyncMissReason::ComposedTirReference);
        }
        // Conditional child wrappers without control flow: build from
        // content (the wrappers are handled after template construction).

        build_finalized_tir_root_with_child_context(
            child,
            store,
            string_table,
            &child.content,
            child.location.to_owned(),
            child_context,
        )
    };

    match build_result {
        Ok((root, summary)) => {
            increment_ast_counter(AstCounter::TemplateTirChildRecursiveMaterializationSuccesses);

            let mut builder = TemplateIrBuilder::new(store);
            let template_id = builder.finish_template(
                root,
                child.style.to_owned(),
                child.kind.to_owned(),
                summary,
                child.location.to_owned(),
            );

            child_context.memo.insert(child_address, template_id);
            child_context.in_progress.remove(&child_address);
            Ok(template_id)
        }
        Err(reason) => {
            increment_ast_counter(AstCounter::TemplateTirChildRecursiveMaterializationFailures);
            child_context.in_progress.remove(&child_address);
            Err(reason)
        }
    }
}

/// Replaces one control-flow body node with an already-built TIR root.
///
/// WHAT: locates the owning control-flow node through the template's same-store
///       finalized reference or the in-progress parser builder, then points the
///       requested body slot at `new_body_root`.
/// WHY: TIR-derived branch/fallback and loop body paths need the same
///      control-flow-node lookup and replacement
///      step. Sharing it keeps the store-owner proof and replacement logic in
///      one place so callers only differ in how they build the root.
pub(crate) fn replace_control_flow_body_tir_root(
    template: &Template,
    store: &mut TemplateIrStore,
    body_kind: ControlFlowBodyKind,
    new_body_root: TemplateIrNodeId,
    phase: TemplateTirPhase,
    builder: Option<&TemplateParserIrBuilderState>,
) -> Option<TemplateControlFlowTirReference> {
    let control_flow_node_id = template
        .tir_reference
        .as_ref()
        .filter(|reference| Arc::ptr_eq(&reference.store_owner, &store.owner()))
        .and_then(|reference| store.control_flow_node_id_for_template(reference.root.template_id))
        .or_else(|| builder.and_then(|builder| builder.control_flow_node_id(store)));

    let control_flow_node_id = control_flow_node_id?;

    if store.replace_control_flow_body_node_by_id(control_flow_node_id, body_kind, new_body_root) {
        Some(TemplateControlFlowTirReference::with_phase(
            store,
            new_body_root,
            phase,
        ))
    } else {
        None
    }
}

/// Replaces the aggregate-wrapper subtree on the owning template's same-store
/// `Loop` control-flow node.
///
/// WHAT: finds the loop node through the same owner/template-or-builder lookup
///       used by `replace_control_flow_body_tir_root`, then installs
///       `new_aggregate_wrapper_root` as the `aggregate_wrapper` field.
/// WHY: loop aggregate wrappers are now composed as normal TIR subtrees during
///      render-unit preparation. Sharing the lookup keeps the installation path
///      consistent with body-root replacement.
pub(crate) fn replace_loop_aggregate_wrapper_tir_root(
    template: &Template,
    store: &mut TemplateIrStore,
    new_aggregate_wrapper_root: TemplateIrNodeId,
    builder: Option<&TemplateParserIrBuilderState>,
) -> bool {
    let control_flow_node_id = template
        .tir_reference
        .as_ref()
        .filter(|reference| Arc::ptr_eq(&reference.store_owner, &store.owner()))
        .and_then(|reference| store.control_flow_node_id_for_template(reference.root.template_id))
        .or_else(|| builder.and_then(|builder| builder.control_flow_node_id(store)));

    let Some(control_flow_node_id) = control_flow_node_id else {
        return false;
    };

    store
        .replace_loop_aggregate_wrapper_node_by_id(control_flow_node_id, new_aggregate_wrapper_root)
}

/// Resolves a finalized control-flow body root from the module TIR store.
///
/// WHAT: finds the owning control-flow node through the template's same-store
/// finalized reference, then returns the requested body root as a
/// store-proven reference.
/// WHY: production readers use this TIR-owned root after render-unit
/// preparation installs finalized bodies into the store. A missing same-store
/// root means the template is not available through the TIR authority.
pub(crate) fn finalized_control_flow_body_tir_reference(
    template: &Template,
    store: &TemplateIrStore,
    body_kind: ControlFlowBodyKind,
) -> Option<TemplateControlFlowTirReference> {
    let store_owner = store.owner();
    let control_flow_node_id = template
        .tir_reference
        .as_ref()
        .filter(|reference| Arc::ptr_eq(&reference.store_owner, &store_owner))
        .and_then(|reference| {
            store.control_flow_node_id_for_template(reference.root.template_id)
        })?;

    let root = store.control_flow_body_node_id_by_id(control_flow_node_id, body_kind)?;

    Some(TemplateControlFlowTirReference::with_full_identity(
        store_owner.clone(),
        store.store_id(),
        root,
        template
            .tir_reference
            .as_ref()
            .map(|reference| reference.phase)
            .unwrap_or(TemplateTirPhase::Composed),
        template
            .tir_reference
            .as_ref()
            .map(|reference| reference.overlay_set_id)
            .unwrap_or(TemplateOverlaySetId::empty()),
        template.location.to_owned(),
    ))
}

// -------------------------
//  Control-flow-aware TIR root construction
// -------------------------

/// Builds a finalized TIR root from a template's control-flow body references.
///
/// WHAT: when a template carries control flow but no same-store `tir_reference`,
///       this helper constructs the TIR root from the control-flow body
///       references that render-unit preparation installed as same-store TIR
///       roots. The control-flow node (BranchChain/Loop) is the
///       root directly — head-prefix atoms are already included in each branch
///       body during render-unit preparation.
/// WHY: `finalized_template_tir_id` uses this path for control-flow templates
///      that lack a same-store reference, typically from test fixtures or edge
///      cases where the parser did not emit one directly.
#[cfg(test)]
pub(crate) fn build_finalized_tir_root_with_control_flow(
    template: &Template,
    control_flow: &TemplateControlFlow,
    store: &mut TemplateIrStore,
    _string_table: &StringTable,
) -> Result<(TemplateIrNodeId, TemplateIrSummary), TemplateTirSyncMissReason> {
    let mut summary = CurrentStateMaterializationSummary::new();
    let location = template.location.to_owned();

    // Head-prefix atoms are already included in each branch/fallback body
    // during render-unit preparation, so the control-flow node is the root
    // directly — no root-level Sequence wrapping that would emit the prefix
    // unconditionally (e.g. when no branch is selected).
    let control_flow_root = match control_flow {
        TemplateControlFlow::BranchChain(chain) => {
            build_branch_chain_root(template, chain, store, &mut summary)?
        }
        TemplateControlFlow::Loop(loop_cf) => {
            build_loop_root(template, loop_cf, store, &mut summary, &location)?
        }
    };

    Ok((control_flow_root, summary.summary))
}

/// Builds a `BranchChain` TIR node from the template's branch body references.
#[cfg(test)]
fn build_branch_chain_root(
    template: &Template,
    chain: &TemplateBranchChain,
    store: &mut TemplateIrStore,
    summary: &mut CurrentStateMaterializationSummary,
) -> Result<TemplateIrNodeId, TemplateTirSyncMissReason> {
    use crate::compiler_frontend::ast::templates::tir::node::TemplateIrBranch;

    summary.record_control_flow();

    let mut branches = Vec::with_capacity(chain.branches.len());
    for branch in &chain.branches {
        let body = required_same_store_body_root(
            template,
            store,
            ControlFlowBodyKind::Branch {
                index: branches.len(),
            },
            branch.body_tir_reference.as_ref(),
            "branch",
        )?;

        branches.push(TemplateIrBranch::new(
            branch.selector.clone(),
            body,
            branch.location.clone(),
        ));
    }

    let fallback = chain
        .fallback
        .as_ref()
        .map(|fallback| {
            required_same_store_body_root(
                template,
                store,
                ControlFlowBodyKind::Fallback,
                fallback.body_tir_reference.as_ref(),
                "fallback",
            )
        })
        .transpose()?;

    Ok(store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::BranchChain { branches, fallback },
        chain.location.clone(),
    )))
}

/// Builds a `Loop` TIR node from the template's loop body and aggregate-wrapper
/// references.
#[cfg(test)]
fn build_loop_root(
    template: &Template,
    loop_cf: &TemplateLoopControlFlow,
    store: &mut TemplateIrStore,
    summary: &mut CurrentStateMaterializationSummary,
    location: &SourceLocation,
) -> Result<TemplateIrNodeId, TemplateTirSyncMissReason> {
    use crate::compiler_frontend::ast::templates::tir::subtree_copy::copy_tir_subtree_with_active_slot_plan;

    summary.record_control_flow();
    summary.enter_depth();

    let body = required_same_store_body_root(
        template,
        store,
        ControlFlowBodyKind::LoopBody,
        loop_cf.body_tir_reference.as_ref(),
        "loop body",
    )?;

    let body = copy_tir_subtree_with_active_slot_plan(body, None, store, summary)
        .map_err(|_| TemplateTirSyncMissReason::NonContentAtom)?;

    let aggregate_wrapper = if let Some(reference) =
        loop_cf.aggregate_wrapper_tir_reference.as_ref()
        && let Some(wrapper_root) = reference.same_store_root(store)
    {
        Some(
            copy_tir_subtree_with_active_slot_plan(wrapper_root, None, store, summary)
                .map_err(|_| TemplateTirSyncMissReason::NonContentAtom)?,
        )
    } else {
        None
    };

    summary.exit_depth();

    let header_sites = store.allocate_loop_header_expression_sites(&loop_cf.header);
    Ok(store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Loop {
            header: loop_cf.header.clone(),
            header_sites,
            body,
            aggregate_wrapper,
        },
        location.clone(),
    )))
}

/// Resolves a required same-store control-flow body root from a body TIR
/// reference or the finalized control-flow body reference.
#[cfg(test)]
fn required_same_store_body_root(
    template: &Template,
    store: &TemplateIrStore,
    body_kind: ControlFlowBodyKind,
    body_tir_reference: Option<&TemplateControlFlowTirReference>,
    _body_label: &str,
) -> Result<TemplateIrNodeId, TemplateTirSyncMissReason> {
    if let Some(reference) = body_tir_reference
        && let Some(root) = reference.same_store_root(store)
    {
        return Ok(root);
    }

    if let Some(reference) = finalized_control_flow_body_tir_reference(template, store, body_kind)
        && let Some(root) = reference.same_store_root(store)
    {
        return Ok(root);
    }

    Err(TemplateTirSyncMissReason::NonContentAtom)
}
