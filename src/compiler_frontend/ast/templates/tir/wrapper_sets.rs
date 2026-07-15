//! TIR wrapper-set and wrapper-context overlay helpers.
//!
//! WHAT: owns the conservative equivalence predicate used to deduplicate
//! `$children(..)` wrapper sets in the `TemplateIrStore` side table, and the
//! wrapper-context overlay construction that records inherited wrapper sets
//! and `$fresh` suppression for child-template occurrences on a template's
//! authoritative structural root.
//!
//! WHY: wrapper sets and wrapper-context overlays both describe how
//! `$children(..)` wrappers apply to child-template boundaries. Keeping the
//! equivalence predicate, wrapper-reference normalization, and overlay
//! construction in one module makes the wrapper application boundary explicit
//! and easy to audit without leaking store or registry internals into the
//! template construction orchestrator.

use crate::compiler_frontend::ast::templates::template::Template;
use crate::compiler_frontend::ast::templates::tir::ids::{
    ChildTemplateOccurrenceId, TemplateIrNodeId,
};
use crate::compiler_frontend::ast::templates::tir::node::TemplateIrNodeKind;
use crate::compiler_frontend::ast::templates::tir::overlays::{
    TemplateOverlaySet, TirWrapperApplicationMode, TirWrapperContext, TirWrapperContextOverlay,
};
use crate::compiler_frontend::ast::templates::tir::parser_builder_state::TemplateTirReference;
use crate::compiler_frontend::ast::templates::tir::refs::{
    TemplateTirChildReference, TemplateWrapperReference, TemplateWrapperSetRef,
};
use crate::compiler_frontend::ast::templates::tir::registry::{
    RegisteredTemplateIrStore, TemplateIrRegistry,
};
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::ast::templates::tir::view::TemplateTirPhase;
use crate::compiler_frontend::compiler_errors::CompilerError;
use std::sync::Arc;

/// Returns true when two wrapper template ref vectors are equivalent.
///
/// WHAT: compares the two vectors element-wise using `TemplateWrapperReference`
/// equality. Because wrapper sets store effective refs (root + phase +
/// overlay_set_id), two sets are equivalent exactly when all three fields match
/// for every wrapper in the same order.
///
/// Empty wrapper vectors are always equivalent, so control-flow children that
/// receive no inherited wrappers share one side-table entry.
///
/// WHY: wrapper-set reuse must never merge wrappers that differ in dynamic
/// behavior, formatter output, slot routing, or conditional semantics. Identity
/// comparison on all three fields is the safe, precise reuse authority; no
/// intermediate content representation is inspected.
pub(crate) fn wrapper_sets_are_equivalent(
    left: &[TemplateWrapperReference],
    right: &[TemplateWrapperReference],
) -> bool {
    left.len() == right.len() && left.iter().zip(right.iter()).all(|(l, r)| l == r)
}

/// Converts a wrapper `Template` into an effective wrapper reference without
/// requiring same-store ownership.
///
/// WHAT: extracts the template's TIR reference (root, phase, overlay-set ID)
///       and validates its overlay, store owner, and template identity. The
///       current store is checked directly so callers holding its mutable
///       `RefCell` borrow never re-enter it through the registry.
/// WHY: the final cross-store wrapper strategy uses store-qualified wrapper
///      references resolved through the registry. This helper is the
///      normalization step that converts AST `Template` values into the
///      wrapper-ref shape before they enter a wrapper set.
///
/// Returns `None` when the wrapper has no valid registry-backed TIR identity,
/// including a wrong store, missing template, owner mismatch or missing overlay.
/// Callers must not recover through an intermediate content representation.
pub(crate) fn wrapper_reference_for_template(
    template: &Template,
    current_store: &TemplateIrStore,
    registry: &TemplateIrRegistry,
) -> Option<TemplateWrapperReference> {
    let reference = &template.tir_reference;
    registry.overlay_set(reference.overlay_set_id)?;

    if reference.root.store_id == current_store.store_id() {
        if !Arc::ptr_eq(&reference.store_owner, &current_store.owner()) {
            return None;
        }
        current_store.get_template(reference.root.template_id)?;
    } else {
        let foreign_store_handle = registry.store_handle(reference.root.store_id)?;
        let foreign_store = foreign_store_handle.borrow();
        if !Arc::ptr_eq(&reference.store_owner, &foreign_store.owner()) {
            return None;
        }
        foreign_store.get_template(reference.root.template_id)?;
    }

    Some(TemplateWrapperReference::new(
        reference.root,
        reference.phase,
        reference.overlay_set_id,
    ))
}

// -------------------------
//  Wrapper-context overlay construction
// -------------------------

/// Attaches a wrapper-context overlay to a template's TIR reference.
///
/// WHAT: walks the owning template's structural root, finds every
/// `ChildTemplate` occurrence, and records `$fresh` suppression or inherited
/// wrapper-set context. The resulting overlay is composed with the reference's
/// current overlay set so downstream `TirView` resolution applies wrappers at
/// child-template boundaries without mutating shared structural roots.
///
/// WHY: wrapper-context overlay construction is TIR-owned because wrapper-set
/// canonicalization, overlay storage, and wrapper-reference validation already
/// live here. Moving the traversal out of the template construction orchestrator
/// keeps the orchestrator focused on ordering and lets the wrapper owner enforce
/// required authority and propagate failures.
///
/// Semantics preserved from the prior local implementation:
/// - `$fresh` suppresses only the immediate parent's wrappers.
/// - Ordinary children use `Always`; structurally control-flow children use
///   `IfChildEmits`.
/// - Wrapper order is unchanged.
/// - No contexts means no wrapper overlay is attached.
///
/// Missing owning template, root node, traversed node, child store, child
/// template, or overlay composition failures return `CompilerError` instead of
/// silently skipping.
pub(crate) fn attach_wrapper_context_overlay(
    tir_reference: &mut TemplateTirReference,
    inherited_wrapper_refs: &[TemplateWrapperReference],
    registered_store: &RegisteredTemplateIrStore,
) -> Result<(), CompilerError> {
    if tir_reference.root.store_id != registered_store.store_id() {
        return Err(CompilerError::compiler_error(
            "wrapper-context overlay: template reference points at a different registered store.",
        ));
    }

    {
        let registry = registered_store.registry().borrow();
        registry
            .overlay_set(tir_reference.overlay_set_id)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "wrapper-context overlay: current overlay set {} does not exist.",
                    tir_reference.overlay_set_id
                ))
            })?;
    }

    // Validate ownership and read the root before mutating anything. Required
    // authority is proven before durable wrapper or overlay state is allocated.
    let root = {
        let store = registered_store.store().borrow();
        if !Arc::ptr_eq(&tir_reference.store_owner, &store.owner()) {
            return Err(CompilerError::compiler_error(
                "wrapper-context overlay: template reference does not belong to the current store.",
            ));
        }
        store
            .get_template(tir_reference.root.template_id)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "wrapper-context overlay: owning template {} not found in store.",
                    tir_reference.root.template_id
                ))
            })?
            .root
    };

    let mut pending_contexts = Vec::new();
    {
        let store = registered_store.store().borrow();
        let registry = registered_store.registry().borrow();
        collect_wrapper_contexts(
            &store,
            &registry,
            root,
            inherited_wrapper_refs,
            &mut pending_contexts,
        )?;
    }

    if pending_contexts.is_empty() {
        return Ok(());
    }

    // Every inherited context uses the same ordered wrapper references. Allocate
    // or reuse that set once, after the full structural walk has validated all
    // nodes and child references.
    let inherited_wrapper_set = if pending_contexts
        .iter()
        .any(|context| !context.skip_parent_child_wrappers)
    {
        let mut store = registered_store.store().borrow_mut();
        let wrapper_set_id = store.push_or_reuse_wrapper_set(inherited_wrapper_refs.to_vec());
        Some(TemplateWrapperSetRef::new(store.store_id(), wrapper_set_id))
    } else {
        None
    };

    let contexts = pending_contexts
        .into_iter()
        .map(|context| {
            let inherited_wrapper_set = if context.skip_parent_child_wrappers {
                None
            } else {
                inherited_wrapper_set
            };

            (
                context.occurrence_id,
                TirWrapperContext {
                    inherited_wrapper_set,
                    skip_parent_child_wrappers: context.skip_parent_child_wrappers,
                    application_mode: context.application_mode,
                },
            )
        })
        .collect();

    let mut registry = registered_store.registry().borrow_mut();
    let wrapper_overlay_id =
        registry.allocate_wrapper_context_overlay(TirWrapperContextOverlay { contexts });
    let wrapper_only_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: None,
        wrapper_context: Some(wrapper_overlay_id),
    });
    let merged_overlay_set_id = registry
        .compose_overlay_sets(&[tir_reference.overlay_set_id, wrapper_only_overlay_set_id])?;

    tir_reference.overlay_set_id = merged_overlay_set_id;
    if !tir_reference.phase.is_at_least(TemplateTirPhase::Composed) {
        tir_reference.phase = TemplateTirPhase::Composed;
    }
    Ok(())
}

/// Validated occurrence context collected before wrapper-set allocation.
struct PendingWrapperContext {
    occurrence_id: ChildTemplateOccurrenceId,
    skip_parent_child_wrappers: bool,
    application_mode: TirWrapperApplicationMode,
}

/// Recursively collects wrapper contexts for child-template occurrences in the
/// structural tree rooted at `node_id`.
///
/// WHAT: traverses `Sequence`, `BranchChain`, and `Loop` structural nodes to
///       find `ChildTemplate` occurrences. For each occurrence, resolves the
///       child template's metadata (same-store directly, foreign through the
///       registry) and records `$fresh` suppression or inherited wrapper-set
///       context.
///
/// WHY: wrapper context belongs to the occurrence in the owning structural
///      tree. This traversal does not recurse into a child's own root — it only
///      walks the structural containers that surround child-template
///      occurrences. The store is borrowed immutably for the entire traversal
///      and no mutation occurs until `collect_wrapper_contexts` returns, so the
///      node kind can be matched directly without cloning child vectors into a
///      transient fact enum.
fn collect_wrapper_contexts(
    store: &TemplateIrStore,
    registry: &TemplateIrRegistry,
    node_id: TemplateIrNodeId,
    inherited_wrapper_refs: &[TemplateWrapperReference],
    contexts: &mut Vec<PendingWrapperContext>,
) -> Result<(), CompilerError> {
    let node = store.get_node(node_id).ok_or_else(|| {
        CompilerError::compiler_error(format!(
            "wrapper-context overlay: traversed TIR node {} not found in store.",
            node_id
        ))
    })?;

    match &node.kind {
        TemplateIrNodeKind::ChildTemplate {
            reference,
            occurrence_id,
        } => {
            let metadata = resolve_child_wrapper_metadata(store, registry, reference)?;
            if metadata.skip_parent_child_wrappers {
                contexts.push(PendingWrapperContext {
                    occurrence_id: *occurrence_id,
                    skip_parent_child_wrappers: true,
                    application_mode: TirWrapperApplicationMode::Always,
                });
            } else if !inherited_wrapper_refs.is_empty() {
                let application_mode = if metadata.has_control_flow {
                    TirWrapperApplicationMode::IfChildEmits
                } else {
                    TirWrapperApplicationMode::Always
                };
                contexts.push(PendingWrapperContext {
                    occurrence_id: *occurrence_id,
                    skip_parent_child_wrappers: false,
                    application_mode,
                });
            }
        }
        TemplateIrNodeKind::Sequence { children } => {
            for child_id in children {
                collect_wrapper_contexts(
                    store,
                    registry,
                    *child_id,
                    inherited_wrapper_refs,
                    contexts,
                )?;
            }
        }
        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            for branch in branches {
                collect_wrapper_contexts(
                    store,
                    registry,
                    branch.body,
                    inherited_wrapper_refs,
                    contexts,
                )?;
            }
            if let Some(fallback_id) = fallback {
                collect_wrapper_contexts(
                    store,
                    registry,
                    *fallback_id,
                    inherited_wrapper_refs,
                    contexts,
                )?;
            }
        }
        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            collect_wrapper_contexts(store, registry, *body, inherited_wrapper_refs, contexts)?;
            if let Some(wrapper_id) = aggregate_wrapper {
                collect_wrapper_contexts(
                    store,
                    registry,
                    *wrapper_id,
                    inherited_wrapper_refs,
                    contexts,
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Wrapper-relevant metadata for a child-template occurrence, resolved from the
/// child's TIR entry.
struct ChildWrapperMetadata {
    has_control_flow: bool,
    skip_parent_child_wrappers: bool,
}

/// Resolves child-template metadata for wrapper-context decisions.
///
/// WHAT: reads `has_control_flow` and `skip_parent_child_wrappers` from the
///       child's TIR entry. Same-store references use the already-held store
///       directly to avoid `RefCell` re-entry. Foreign references resolve
///       through the registry.
///
/// WHY: wrapper context belongs to the occurrence in the owning structural
///      tree, not to the child's own root. Only the child's metadata is needed
///      to decide `$fresh` suppression and application mode.
fn resolve_child_wrapper_metadata(
    current_store: &TemplateIrStore,
    registry: &TemplateIrRegistry,
    reference: &TemplateTirChildReference,
) -> Result<ChildWrapperMetadata, CompilerError> {
    registry
        .overlay_set(reference.overlay_set_id)
        .ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "wrapper-context overlay: child reference uses missing overlay set {}.",
                reference.overlay_set_id
            ))
        })?;

    if reference.root.store_id == current_store.store_id() {
        let child = current_store
            .get_template(reference.root.template_id)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "wrapper-context overlay: child template {} not found in current store.",
                    reference.root.template_id
                ))
            })?;
        Ok(ChildWrapperMetadata {
            has_control_flow: child.summary.has_control_flow,
            skip_parent_child_wrappers: child.style.skip_parent_child_wrappers,
        })
    } else {
        let foreign_store = registry.store(reference.root.store_id).ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "wrapper-context overlay: foreign store {} not found in registry.",
                reference.root.store_id
            ))
        })?;
        let child = foreign_store
            .get_template(reference.root.template_id)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "wrapper-context overlay: child template {} not found in foreign store {}.",
                    reference.root.template_id, reference.root.store_id
                ))
            })?;
        Ok(ChildWrapperMetadata {
            has_control_flow: child.summary.has_control_flow,
            skip_parent_child_wrappers: child.style.skip_parent_child_wrappers,
        })
    }
}

#[cfg(test)]
#[path = "tests/wrapper_context_construction_tests.rs"]
mod wrapper_context_construction_tests;
