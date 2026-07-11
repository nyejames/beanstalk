//! Validation entry points for structured template control flow.
//!
//! Runtime-capable templates are validated for escaped helper artifacts that
//! should have been composed or routed into AST-owned slot application plans.
//! Const-required templates are validated for full foldability while still
//! allowing slot/helper structure that a parent const template may compose
//! before folding.

use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::TemplateType;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::tir::{
    TemplateIrBranch, TemplateIrId, TemplateIrNodeId, TemplateIrNodeKind, TemplateIrRegistry,
    TemplateIrStore, TemplateLoopHeaderExpressionSites, TemplateNodeRef, TemplateOverlaySetId,
    TemplateParserIrBuilderState, TemplateRef, TemplateStoreId, TemplateTirPhase, TirView,
    current_same_store_tir_roots_for_template, effective_branch_selector_for_view,
    effective_loop_header_for_view, tir_subtree_contains_slot_insertions,
    tir_subtree_has_unresolved_slots, tir_view_expression_is_const_evaluable_value_with_bindings,
    tir_view_option_capture_presence_is_const_decidable, tir_view_subtree_is_const_evaluable_value,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidTemplateStructureReason,
};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use std::collections::HashSet;
use std::sync::Arc;

use super::const_eval::{collect_option_capture_binding_path, loop_body_const_evaluation_bindings};
use super::types::{TemplateBranchSelector, TemplateLoopHeader};

/// Validates structured template control flow in a const-required context.
///
/// Runtime-capable templates keep structured control flow for HIR lowering.
/// Const-required callers use this narrower entry point so supported forms can
/// fold now while unsupported const shapes produce source diagnostics instead of
/// leaking as infrastructure errors during finalization.
///
/// WHAT: validates through the template's registry-backed Composed-or-later
///       `TirView` so expression overlays and qualified children are authoritative.
/// WHY: every production const-required template has completed construction before
///      this entry runs. Missing view authority is therefore a compiler invariant,
///      not permission to fall back to a raw store walk.
pub(crate) fn validate_const_required_template_control_flow(
    template: &Template,
    registry: &TemplateIrRegistry,
    string_table: &StringTable,
) -> Result<(), CompilerDiagnostic> {
    validate_const_required_template_control_flow_with_bindings(
        template,
        &[],
        registry,
        string_table,
    )
}

/// Rejects slot composition artifacts that would otherwise reach runtime
/// control-flow lowering.
///
/// Compile-time-required callers use the const validator above because slots can
/// still be resolved or folded before runtime. This runtime-only check runs
/// after composition/formatting, when any remaining slot or insertion inside a
/// control-flow body would otherwise become a HIR invariant failure.
pub(crate) fn validate_runtime_template_control_flow_slot_artifacts(
    template: &Template,
    registry: &TemplateIrRegistry,
    store: &TemplateIrStore,
    builder: Option<&TemplateParserIrBuilderState>,
) -> Result<(), TemplateError> {
    // TIR-authoritative view path: when the template owns a durable registry-backed
    // reference, validate through `TirView` so effective slot-resolution overlays
    // are respected. Runtime validation runs during construction, so any post-parse
    // phase is sufficient; we do not require `Finalized` here.
    if let Some(view) = try_runtime_tir_view_for_template(template, registry) {
        return validate_runtime_tir_view_control_flow_slot_artifacts(&view, registry);
    }

    // Raw same-store TIR fallback: for roots that lack a stable registry-backed
    // view identity (parser-local builder roots, cross-store references, or
    // templates without a `tir_reference`), keep the existing store-local behavior.
    if let Some(roots) = current_same_store_tir_roots_for_template(template, store, builder) {
        return validate_runtime_tir_subtree_control_flow_slot_artifacts(store, &roots.roots);
    }

    // No same-store TIR authority is available. The TIR path is the only source
    // for runtime control-flow artifact validation, so the top-level recursion
    // returns OK.
    Ok(())
}

/// Returns a same-store `TirView` for runtime artifact validation.
///
/// WHAT: accepts any post-parse phase because runtime validation is invoked
///       during template construction, before finalization is guaranteed.
/// WHY: keeps the view path open for composed/formatted roots while letting
///      unsupported roots fall back to the raw same-store walker.
fn try_runtime_tir_view_for_template<'a>(
    template: &Template,
    registry: &'a TemplateIrRegistry,
) -> Option<TirView<'a>> {
    let reference = template.tir_reference.as_ref()?;

    {
        let store = registry.store(reference.root.store_id)?;
        if !Arc::ptr_eq(&reference.store_owner, &store.owner()) {
            return None;
        }
        if reference.root.store_id != store.store_id() {
            return None;
        }
    }

    TirView::new(
        registry,
        reference.root,
        reference.phase,
        reference.overlay_set_id,
    )
    .ok()
}

fn validate_const_required_template_control_flow_with_bindings(
    template: &Template,
    loop_binding_paths: &[InternedPath],
    registry: &TemplateIrRegistry,
    string_table: &StringTable,
) -> Result<(), CompilerDiagnostic> {
    let view = const_required_tir_view_for_template(template, registry)
        .map_err(TemplateError::into_diagnostic)?;
    let store = view
        .store()
        .map_err(|error| TemplateError::from(error).into_diagnostic())?;

    validate_const_required_tir_view_control_flow(&view, &store, loop_binding_paths, string_table)
}

/// Validates every reachable runtime control-flow body through a registry-backed
/// `TirView`.
///
/// WHAT: walks the view's structural tree, checking `BranchChain` and `Loop`
///       bodies for unresolved slots and escaped `$insert(...)` contributions.
///       Slot occurrences are checked against the view's effective slot-resolution
///       overlay so resolved slots are not falsely reported as artifacts.
/// WHY: this is the registry-backed replacement for the raw same-store walker;
///      it keeps overlay resolution centralized while child-template recursion
///      deliberately falls back to the raw-store walker to keep the slice narrow.
fn validate_runtime_tir_view_control_flow_slot_artifacts(
    view: &TirView<'_>,
    registry: &TemplateIrRegistry,
) -> Result<(), TemplateError> {
    let store_id = view.root_ref().store_id;
    let root_node_id = view.root_template()?.root;
    let mut visited = HashSet::new();

    validate_runtime_tir_view_node(
        view,
        registry,
        store_id,
        TemplateNodeRef::new(store_id, root_node_id),
        &mut visited,
    )
}

/// Validates every reachable runtime control-flow body in a same-store TIR subtree.
fn validate_runtime_tir_view_node(
    view: &TirView<'_>,
    registry: &TemplateIrRegistry,
    store_id: TemplateStoreId,
    node_ref: TemplateNodeRef,
    visited: &mut HashSet<TemplateIrNodeId>,
) -> Result<(), TemplateError> {
    if !visited.insert(node_ref.node_id) {
        return Ok(());
    }

    let node = match view.effective_node(node_ref) {
        Ok(node) => node,
        Err(_) => return Ok(()),
    };

    match &node.kind {
        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            let branches = branches.clone();
            let fallback = *fallback;
            let node_location = node.location.clone();
            drop(node);

            for branch in branches {
                validate_runtime_tir_view_control_flow_body(
                    view,
                    registry,
                    branch.body,
                    &branch.location,
                )?;
                validate_runtime_tir_view_node(
                    view,
                    registry,
                    store_id,
                    TemplateNodeRef::new(store_id, branch.body),
                    visited,
                )?;
            }

            if let Some(fallback_id) = fallback {
                validate_runtime_tir_view_control_flow_body(
                    view,
                    registry,
                    fallback_id,
                    &node_location,
                )?;
                validate_runtime_tir_view_node(
                    view,
                    registry,
                    store_id,
                    TemplateNodeRef::new(store_id, fallback_id),
                    visited,
                )?;
            }
        }

        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            let body = *body;
            let aggregate_wrapper = *aggregate_wrapper;
            let node_location = node.location.clone();
            drop(node);

            validate_runtime_tir_view_control_flow_body(view, registry, body, &node_location)?;
            validate_runtime_tir_view_node(
                view,
                registry,
                store_id,
                TemplateNodeRef::new(store_id, body),
                visited,
            )?;

            if let Some(wrapper_id) = aggregate_wrapper {
                validate_runtime_tir_view_node(
                    view,
                    registry,
                    store_id,
                    TemplateNodeRef::new(store_id, wrapper_id),
                    visited,
                )?;
            }
        }

        TemplateIrNodeKind::Sequence { children } => {
            let children = children.clone();
            drop(node);
            for child in children {
                validate_runtime_tir_view_node(
                    view,
                    registry,
                    store_id,
                    TemplateNodeRef::new(store_id, child),
                    visited,
                )?;
            }
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            let reference = *reference;
            drop(node);

            // Child-template bodies carry their own phase and overlay context;
            // the runtime view path stays narrow by falling back to the raw-store
            // walker for nested children.
            if let Some(store) = registry.store(store_id)
                && let Some(template_id) = reference.template_id_in_store(store_id)
                && let Some(template_ir) = store.get_template(template_id)
            {
                validate_runtime_tir_subtree_node(&store, template_ir.root, &mut HashSet::new())?;
            }
        }

        TemplateIrNodeKind::InsertContribution { template } => {
            let template_id = *template;
            drop(node);

            if let Some(store) = registry.store(store_id)
                && let Some(template_ir) = store.get_template(template_id)
            {
                validate_runtime_tir_subtree_node(&store, template_ir.root, &mut HashSet::new())?;
            }
        }

        TemplateIrNodeKind::Text { .. }
        | TemplateIrNodeKind::DynamicExpression { .. }
        | TemplateIrNodeKind::Slot { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::LoopControl { .. }
        | TemplateIrNodeKind::RuntimeSlotSite { .. } => {}
    }

    Ok(())
}

fn validate_runtime_tir_view_control_flow_body(
    view: &TirView<'_>,
    registry: &TemplateIrRegistry,
    body_root: TemplateIrNodeId,
    location: &SourceLocation,
) -> Result<(), TemplateError> {
    let store_id = view.root_ref().store_id;
    let body_ref = TemplateNodeRef::new(store_id, body_root);

    if tir_view_subtree_contains_runtime_artifact(
        view,
        registry,
        body_ref,
        RuntimeControlFlowArtifact::EscapedInsert,
    )? {
        return Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::RuntimeControlFlowUnresolvedInsert,
            location.clone(),
        )
        .into());
    }

    if tir_view_subtree_contains_runtime_artifact(
        view,
        registry,
        body_ref,
        RuntimeControlFlowArtifact::UnresolvedSlot,
    )? {
        return Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::RuntimeControlFlowUnresolvedSlot,
            location.clone(),
        )
        .into());
    }

    Ok(())
}

#[derive(Clone, Copy)]
enum RuntimeControlFlowArtifact {
    UnresolvedSlot,
    EscapedInsert,
}

fn tir_view_subtree_contains_runtime_artifact(
    view: &TirView<'_>,
    registry: &TemplateIrRegistry,
    node_ref: TemplateNodeRef,
    artifact: RuntimeControlFlowArtifact,
) -> Result<bool, TemplateError> {
    let node = match view.effective_node(node_ref) {
        Ok(node) => node,
        Err(_) => return Ok(false),
    };

    let store_id = node_ref.store_id;

    match &node.kind {
        TemplateIrNodeKind::Slot { placeholder } => {
            let occurrence_id = placeholder.occurrence_id;
            drop(node);

            if !matches!(artifact, RuntimeControlFlowArtifact::UnresolvedSlot) {
                return Ok(false);
            }

            let resolution = view.effective_slot_resolution(occurrence_id)?;
            let is_resolved = resolution.is_some_and(|r| !r.is_unresolved());

            Ok(!is_resolved)
        }

        TemplateIrNodeKind::Sequence { children } => {
            let children = children.clone();
            drop(node);
            for child in children {
                if tir_view_subtree_contains_runtime_artifact(
                    view,
                    registry,
                    TemplateNodeRef::new(store_id, child),
                    artifact,
                )? {
                    return Ok(true);
                }
            }
            Ok(false)
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            let bodies: Vec<_> = branches.iter().map(|branch| branch.body).collect();
            let fallback = *fallback;
            drop(node);

            for body in bodies {
                if tir_view_subtree_contains_runtime_artifact(
                    view,
                    registry,
                    TemplateNodeRef::new(store_id, body),
                    artifact,
                )? {
                    return Ok(true);
                }
            }

            if let Some(fallback) = fallback
                && tir_view_subtree_contains_runtime_artifact(
                    view,
                    registry,
                    TemplateNodeRef::new(store_id, fallback),
                    artifact,
                )?
            {
                return Ok(true);
            }

            Ok(false)
        }

        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            let body = *body;
            let aggregate_wrapper = *aggregate_wrapper;
            drop(node);

            if tir_view_subtree_contains_runtime_artifact(
                view,
                registry,
                TemplateNodeRef::new(store_id, body),
                artifact,
            )? {
                return Ok(true);
            }

            if let Some(wrapper_id) = aggregate_wrapper
                && tir_view_subtree_contains_runtime_artifact(
                    view,
                    registry,
                    TemplateNodeRef::new(store_id, wrapper_id),
                    artifact,
                )?
            {
                return Ok(true);
            }

            Ok(false)
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            let reference = *reference;
            drop(node);

            if let Some(store) = registry.store(store_id)
                && let Some(template_id) = reference.template_id_in_store(store_id)
            {
                return Ok(tir_template_contains_runtime_artifact(
                    &store,
                    template_id,
                    artifact,
                ));
            }

            Ok(false)
        }

        TemplateIrNodeKind::InsertContribution { template } => {
            let template_id = *template;
            drop(node);

            if let Some(store) = registry.store(store_id) {
                return Ok(tir_template_contains_runtime_artifact(
                    &store,
                    template_id,
                    artifact,
                ));
            }

            Ok(false)
        }

        TemplateIrNodeKind::Text { .. }
        | TemplateIrNodeKind::DynamicExpression { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::LoopControl { .. }
        | TemplateIrNodeKind::RuntimeSlotSite { .. } => Ok(false),
    }
}

fn tir_template_contains_runtime_artifact(
    store: &TemplateIrStore,
    template_id: TemplateIrId,
    artifact: RuntimeControlFlowArtifact,
) -> bool {
    let Some(template_ir) = store.get_template(template_id) else {
        return false;
    };

    match artifact {
        RuntimeControlFlowArtifact::UnresolvedSlot => {
            tir_subtree_has_unresolved_slots(store, template_ir.root)
        }
        RuntimeControlFlowArtifact::EscapedInsert => {
            matches!(template_ir.kind, TemplateType::SlotInsert(_))
                || tir_subtree_contains_slot_insertions(store, template_ir.root)
        }
    }
}

/// Validates every reachable runtime control-flow body in a same-store TIR subtree.
///
/// WHAT: walks from `roots`, recursing through sequences, child-template references,
///       insert-contribution references, and control-flow bodies/aggregate wrappers.
///       Every `BranchChain` and `Loop` body is checked for unresolved slots and
///       escaped `$insert(...)` contributions using the TIR node or branch location
///       for diagnostics.
/// WHY: this is the production authority for runtime control-flow artifact
///      validation; no `TemplateContent` fallback exists.
fn validate_runtime_tir_subtree_control_flow_slot_artifacts(
    store: &TemplateIrStore,
    roots: &[TemplateIrNodeId],
) -> Result<(), TemplateError> {
    let mut visited = HashSet::new();
    for root in roots {
        validate_runtime_tir_subtree_node(store, *root, &mut visited)?;
    }
    Ok(())
}

fn validate_runtime_tir_subtree_node(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
    visited: &mut HashSet<TemplateIrNodeId>,
) -> Result<(), TemplateError> {
    if !visited.insert(node_id) {
        return Ok(());
    }

    let Some(node) = store.get_node(node_id) else {
        return Ok(());
    };

    match &node.kind {
        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            for branch in branches {
                validate_runtime_tir_control_flow_body(store, branch.body, &branch.location)?;
                validate_runtime_tir_subtree_node(store, branch.body, visited)?;
            }

            if let Some(fallback_id) = fallback {
                validate_runtime_tir_control_flow_body(store, *fallback_id, &node.location)?;
                validate_runtime_tir_subtree_node(store, *fallback_id, visited)?;
            }
        }

        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            validate_runtime_tir_control_flow_body(store, *body, &node.location)?;
            validate_runtime_tir_subtree_node(store, *body, visited)?;

            if let Some(wrapper_id) = aggregate_wrapper {
                validate_runtime_tir_subtree_node(store, *wrapper_id, visited)?;
            }
        }

        TemplateIrNodeKind::Sequence { children } => {
            for child in children {
                validate_runtime_tir_subtree_node(store, *child, visited)?;
            }
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            if let Some(template_id) = reference.template_id_in_store(store.store_id())
                && let Some(template_ir) = store.get_template(template_id)
            {
                validate_runtime_tir_subtree_node(store, template_ir.root, visited)?;
            }
        }

        TemplateIrNodeKind::InsertContribution { template } => {
            if let Some(template_ir) = store.get_template(*template) {
                validate_runtime_tir_subtree_node(store, template_ir.root, visited)?;
            }
        }

        TemplateIrNodeKind::Text { .. }
        | TemplateIrNodeKind::DynamicExpression { .. }
        | TemplateIrNodeKind::Slot { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::LoopControl { .. }
        | TemplateIrNodeKind::RuntimeSlotSite { .. } => {}
    }

    Ok(())
}

fn validate_runtime_tir_control_flow_body(
    store: &TemplateIrStore,
    body_root: TemplateIrNodeId,
    location: &SourceLocation,
) -> Result<(), TemplateError> {
    if tir_subtree_contains_slot_insertions(store, body_root) {
        return Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::RuntimeControlFlowUnresolvedInsert,
            location.clone(),
        )
        .into());
    }

    if tir_subtree_has_unresolved_slots(store, body_root) {
        return Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::RuntimeControlFlowUnresolvedSlot,
            location.clone(),
        )
        .into());
    }

    Ok(())
}

// -------------------------
//  Const-required TIR subtree validation
// -------------------------

// -------------------------
//  Const-required TirView validation
// -------------------------

/// Constructs the required `TirView` for a const-required template.
///
/// WHAT: validates the durable reference, owning registry store and minimum
///       composition phase before constructing the effective view.
/// WHY: production callers run after template construction and finalization
///      preserves the same reference. Missing authority indicates compiler drift.
fn const_required_tir_view_for_template<'a>(
    template: &Template,
    registry: &'a TemplateIrRegistry,
) -> Result<TirView<'a>, TemplateError> {
    let reference = template.tir_reference.as_ref().ok_or_else(|| {
        CompilerError::compiler_error(
            "Const-required template validation requires a registry-backed TIR reference.",
        )
    })?;
    if !reference.phase.is_at_least(TemplateTirPhase::Composed) {
        return Err(CompilerError::compiler_error(format!(
            "Const-required template root {} is at phase {}, but validation requires Composed or later.",
            reference.root, reference.phase
        ))
        .into());
    }

    let store = registry.store(reference.root.store_id).ok_or_else(|| {
        CompilerError::compiler_error(format!(
            "Const-required template root {} refers to an unregistered TIR store.",
            reference.root
        ))
    })?;
    if !Arc::ptr_eq(&reference.store_owner, &store.owner()) {
        return Err(CompilerError::compiler_error(format!(
            "Const-required template root {} does not match its registry store owner.",
            reference.root
        ))
        .into());
    }

    TirView::with_minimum_phase(
        registry,
        reference.root,
        reference.phase,
        TemplateTirPhase::Composed,
        reference.overlay_set_id,
    )
    .map_err(TemplateError::from)
}

/// Validates every reachable const-required control-flow node through a
/// registry-backed `TirView`.
///
/// WHAT: walks the view's structural tree, using effective expression overlays
///       for branch selectors and loop headers and checking branch/loop bodies
///       for const-evaluability. Qualified child views preserve each nested
///       template's own store, phase and overlay identity.
/// WHY: one view-native path reads expression overrides without duplicating the
///      const-evaluability walker in `tir/classification.rs`.
fn validate_const_required_tir_view_control_flow(
    view: &TirView<'_>,
    store: &TemplateIrStore,
    loop_binding_paths: &[InternedPath],
    string_table: &StringTable,
) -> Result<(), CompilerDiagnostic> {
    let root_node_id = view
        .root_template()
        .map(|template| template.root)
        .map_err(|error| TemplateError::from(error).into_diagnostic())?;
    let store_id = view.root_ref().store_id;
    let root_node_ref = TemplateNodeRef::new(store_id, root_node_id);
    let mut visiting_templates = HashSet::from([view.root_ref()]);

    validate_const_required_tir_view_node(
        view,
        store,
        root_node_ref,
        loop_binding_paths,
        string_table,
        &mut visiting_templates,
    )
}

fn validate_const_required_tir_view_node(
    view: &TirView<'_>,
    store: &TemplateIrStore,
    node_ref: TemplateNodeRef,
    loop_binding_paths: &[InternedPath],
    string_table: &StringTable,
    visiting_templates: &mut HashSet<TemplateRef>,
) -> Result<(), CompilerDiagnostic> {
    let store_id = view.root_ref().store_id;
    let node = match view.effective_node(node_ref) {
        Ok(node) => node,
        Err(_) => return Ok(()),
    };

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            let children = children.clone();
            drop(node);
            for child in children {
                validate_const_required_tir_view_node(
                    view,
                    store,
                    TemplateNodeRef::new(store_id, child),
                    loop_binding_paths,
                    string_table,
                    visiting_templates,
                )?;
            }
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            validate_const_required_tir_view_branch_chain(
                view,
                store,
                ConstRequiredTirViewBranchInputs {
                    branches,
                    fallback: *fallback,
                    node_location: &node.location,
                },
                loop_binding_paths,
                string_table,
                visiting_templates,
            )?;
        }

        TemplateIrNodeKind::Loop {
            header,
            header_sites,
            body,
            aggregate_wrapper,
            ..
        } => {
            validate_const_required_tir_view_loop(
                view,
                store,
                ConstRequiredTirViewLoopInputs {
                    header,
                    header_sites: *header_sites,
                    body: *body,
                    aggregate_wrapper: *aggregate_wrapper,
                    node_location: &node.location,
                },
                loop_binding_paths,
                string_table,
                visiting_templates,
            )?;
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            let reference = *reference;
            drop(node);
            validate_const_required_qualified_child_view(
                view,
                reference.root,
                reference.phase,
                reference.overlay_set_id,
                loop_binding_paths,
                string_table,
                visiting_templates,
            )?;
        }

        TemplateIrNodeKind::InsertContribution { template } => {
            let template_id = *template;
            drop(node);
            validate_const_required_qualified_child_view(
                view,
                TemplateRef::new(store_id, template_id),
                view.phase(),
                view.overlay_set_id(),
                loop_binding_paths,
                string_table,
                visiting_templates,
            )?;
        }

        TemplateIrNodeKind::Text { .. }
        | TemplateIrNodeKind::DynamicExpression { .. }
        | TemplateIrNodeKind::Slot { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::LoopControl { .. }
        | TemplateIrNodeKind::RuntimeSlotSite { .. } => {}
    }

    Ok(())
}

fn validate_const_required_qualified_child_view(
    parent_view: &TirView<'_>,
    child_root: TemplateRef,
    child_phase: TemplateTirPhase,
    child_overlay_set_id: TemplateOverlaySetId,
    loop_binding_paths: &[InternedPath],
    string_table: &StringTable,
    visiting_templates: &mut HashSet<TemplateRef>,
) -> Result<(), CompilerDiagnostic> {
    if !visiting_templates.insert(child_root) {
        return Ok(());
    }

    let child_view = parent_view
        .child_view(child_root, child_phase, child_overlay_set_id)
        .map_err(|error| TemplateError::from(error).into_diagnostic())?;
    let child_store = child_view
        .store()
        .map_err(|error| TemplateError::from(error).into_diagnostic())?;
    let child_root_node = child_view
        .root_template()
        .map(|template_ir| template_ir.root)
        .map_err(|error| TemplateError::from(error).into_diagnostic())?;
    let result = validate_const_required_tir_view_node(
        &child_view,
        &child_store,
        TemplateNodeRef::new(child_root.store_id, child_root_node),
        loop_binding_paths,
        string_table,
        visiting_templates,
    );

    visiting_templates.remove(&child_root);
    result
}

fn validate_const_required_tir_view_branch_chain(
    view: &TirView<'_>,
    store: &TemplateIrStore,
    inputs: ConstRequiredTirViewBranchInputs<'_>,
    loop_binding_paths: &[InternedPath],
    string_table: &StringTable,
    visiting_templates: &mut HashSet<TemplateRef>,
) -> Result<(), CompilerDiagnostic> {
    let store_id = view.root_ref().store_id;

    for branch in inputs.branches {
        let effective_selector =
            effective_branch_selector_for_view(view, &branch.selector, branch.selector_site_id)
                .map_err(TemplateError::into_diagnostic)?;

        let branch_binding_paths = validate_const_required_tir_view_branch_selector(
            view,
            &effective_selector,
            inputs.node_location,
            loop_binding_paths,
            store,
            string_table,
        )?;

        if !tir_view_subtree_is_const_evaluable_value(
            view,
            store,
            string_table,
            branch.body,
            &branch_binding_paths,
        )
        .map_err(TemplateError::into_diagnostic)?
        {
            return Err(CompilerDiagnostic::invalid_template_structure(
                InvalidTemplateStructureReason::TemplateIfBranchNotConst,
                inputs.node_location.clone(),
            ));
        }

        validate_const_required_tir_view_node(
            view,
            store,
            TemplateNodeRef::new(store_id, branch.body),
            &branch_binding_paths,
            string_table,
            visiting_templates,
        )?;
    }

    if let Some(fallback_id) = inputs.fallback {
        if !tir_view_subtree_is_const_evaluable_value(
            view,
            store,
            string_table,
            fallback_id,
            loop_binding_paths,
        )
        .map_err(TemplateError::into_diagnostic)?
        {
            return Err(CompilerDiagnostic::invalid_template_structure(
                InvalidTemplateStructureReason::TemplateIfBranchNotConst,
                inputs.node_location.clone(),
            ));
        }

        validate_const_required_tir_view_node(
            view,
            store,
            TemplateNodeRef::new(store_id, fallback_id),
            loop_binding_paths,
            string_table,
            visiting_templates,
        )?;
    }

    Ok(())
}

struct ConstRequiredTirViewBranchInputs<'a> {
    branches: &'a [TemplateIrBranch],
    fallback: Option<TemplateIrNodeId>,
    node_location: &'a SourceLocation,
}

fn validate_const_required_tir_view_loop(
    view: &TirView<'_>,
    store: &TemplateIrStore,
    inputs: ConstRequiredTirViewLoopInputs<'_>,
    loop_binding_paths: &[InternedPath],
    string_table: &StringTable,
    visiting_templates: &mut HashSet<TemplateRef>,
) -> Result<(), CompilerDiagnostic> {
    let effective_header = effective_loop_header_for_view(view, inputs.header, inputs.header_sites)
        .map_err(TemplateError::into_diagnostic)?;

    validate_const_required_tir_view_loop_header(
        view,
        &effective_header,
        inputs.node_location,
        store,
        string_table,
    )?;

    let body_binding_paths =
        loop_body_const_evaluation_bindings(&effective_header, loop_binding_paths);

    if !tir_view_subtree_is_const_evaluable_value(
        view,
        store,
        string_table,
        inputs.body,
        &body_binding_paths,
    )
    .map_err(TemplateError::into_diagnostic)?
    {
        return Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::TemplateLoopBodyNotConst,
            inputs.node_location.clone(),
        ));
    }

    let store_id = view.root_ref().store_id;
    validate_const_required_tir_view_node(
        view,
        store,
        TemplateNodeRef::new(store_id, inputs.body),
        &body_binding_paths,
        string_table,
        visiting_templates,
    )?;

    if let Some(wrapper_id) = inputs.aggregate_wrapper {
        validate_const_required_tir_view_node(
            view,
            store,
            TemplateNodeRef::new(store_id, wrapper_id),
            loop_binding_paths,
            string_table,
            visiting_templates,
        )?;
    }

    Ok(())
}

/// Input bundle for `validate_const_required_tir_view_loop`.
///
/// WHAT: groups the loop node fields and diagnostic location so the view-based
///       validator keeps explicit named inputs without a broad argument list.
struct ConstRequiredTirViewLoopInputs<'a> {
    header: &'a TemplateLoopHeader,
    header_sites: TemplateLoopHeaderExpressionSites,
    body: TemplateIrNodeId,
    aggregate_wrapper: Option<TemplateIrNodeId>,
    node_location: &'a SourceLocation,
}

fn validate_const_required_tir_view_branch_selector(
    view: &TirView<'_>,
    selector: &TemplateBranchSelector,
    fallback_location: &SourceLocation,
    loop_binding_paths: &[InternedPath],
    store: &TemplateIrStore,
    string_table: &StringTable,
) -> Result<Vec<InternedPath>, CompilerDiagnostic> {
    let mut branch_binding_paths = loop_binding_paths.to_vec();

    match selector {
        TemplateBranchSelector::Bool(condition) => {
            if !tir_view_expression_is_const_evaluable_value_with_bindings(
                view,
                store,
                condition,
                loop_binding_paths,
                string_table,
            )
            .map_err(TemplateError::into_diagnostic)?
            {
                return Err(CompilerDiagnostic::invalid_template_structure(
                    InvalidTemplateStructureReason::TemplateIfConditionNotConst,
                    condition.location.clone(),
                ));
            }
        }

        TemplateBranchSelector::OptionPresentCapture { scrutinee, pattern } => {
            let location = if scrutinee.location == SourceLocation::default() {
                fallback_location.clone()
            } else {
                scrutinee.location.clone()
            };

            if !tir_view_option_capture_presence_is_const_decidable(
                view,
                store,
                scrutinee,
                loop_binding_paths,
                string_table,
            )
            .map_err(TemplateError::into_diagnostic)?
            {
                return Err(CompilerDiagnostic::invalid_template_structure(
                    InvalidTemplateStructureReason::TemplateOptionCaptureConstDeferred,
                    location,
                ));
            }

            collect_option_capture_binding_path(pattern, &mut branch_binding_paths);
        }
    }

    Ok(branch_binding_paths)
}

fn validate_const_required_tir_view_loop_header(
    view: &TirView<'_>,
    header: &TemplateLoopHeader,
    loop_location: &SourceLocation,
    store: &TemplateIrStore,
    string_table: &StringTable,
) -> Result<(), CompilerDiagnostic> {
    match header {
        TemplateLoopHeader::Conditional { condition } => {
            validate_const_required_conditional_loop_condition(condition, loop_location)?;
        }

        TemplateLoopHeader::Range { range, .. } => {
            let start_is_const = tir_view_expression_is_const_evaluable_value_with_bindings(
                view,
                store,
                &range.start,
                &[],
                string_table,
            )
            .map_err(TemplateError::into_diagnostic)?;
            let end_is_const = tir_view_expression_is_const_evaluable_value_with_bindings(
                view,
                store,
                &range.end,
                &[],
                string_table,
            )
            .map_err(TemplateError::into_diagnostic)?;
            let step_is_const = range
                .step
                .as_ref()
                .map(|step| {
                    tir_view_expression_is_const_evaluable_value_with_bindings(
                        view,
                        store,
                        step,
                        &[],
                        string_table,
                    )
                })
                .transpose()
                .map_err(TemplateError::into_diagnostic)?
                .is_none_or(|is_const| is_const);

            if !start_is_const || !end_is_const || !step_is_const {
                return Err(CompilerDiagnostic::invalid_template_structure(
                    InvalidTemplateStructureReason::TemplateLoopRangeBoundsNotConst,
                    loop_location.clone(),
                ));
            }
        }

        TemplateLoopHeader::Collection { iterable, .. } => {
            if !tir_view_expression_is_const_evaluable_value_with_bindings(
                view,
                store,
                iterable,
                &[],
                string_table,
            )
            .map_err(TemplateError::into_diagnostic)?
            {
                return Err(CompilerDiagnostic::invalid_template_structure(
                    InvalidTemplateStructureReason::TemplateLoopSourceNotConst,
                    iterable.location.clone(),
                ));
            }
        }
    }

    Ok(())
}

fn validate_const_required_conditional_loop_condition(
    condition: &Expression,
    loop_location: &SourceLocation,
) -> Result<(), CompilerDiagnostic> {
    match &condition.kind {
        ExpressionKind::Bool(false) => Ok(()),

        ExpressionKind::Bool(true) => Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::TemplateConditionalLoopConstTrue,
            condition_location_or_loop_location(condition, loop_location),
        )),

        ExpressionKind::Coerced { value, .. } => {
            validate_const_required_conditional_loop_condition(value, loop_location)
        }

        _ => Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::TemplateLoopConditionNotConst,
            condition_location_or_loop_location(condition, loop_location),
        )),
    }
}

fn condition_location_or_loop_location(
    condition: &Expression,
    loop_location: &SourceLocation,
) -> SourceLocation {
    if condition.location == SourceLocation::default() {
        loop_location.clone()
    } else {
        condition.location.clone()
    }
}
