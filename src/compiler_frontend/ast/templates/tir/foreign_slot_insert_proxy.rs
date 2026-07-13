//! Foreign SlotInsert proxy construction.
//!
//! WHAT: builds a local proxy `TemplateIr` for a cross-store `SlotInsert` head
//!       so the current store can route nested inserts through
//!       `InsertContribution` nodes — which carry a bare local `TemplateIrId`
//!       and cannot represent a foreign reference directly.
//!
//! WHY: `InsertContribution` nodes carry a bare local `TemplateIrId`, so a
//!      cross-store `SlotInsert` head cannot be referenced directly. The proxy
//!      preserves the target slot key for TIR-native slot routing while
//!      separating nested inserts from body content so
//!      `collect_insert_contribution_content` discovers and routes nested
//!      inserts to their own target slots — exactly like the same-store
//!      contract. Non-insert content stays store-qualified through
//!      `ChildTemplate` references to narrow derived foreign templates. This
//!      preserves store-qualified identity without eager foreign-tree copying.
//!
//! ## Ownership contract
//!
//! This module owns proxy construction and derived-template creation only.
//! Foreign-store mutation always goes through `TemplateIrRegistry::store_mut`
//! so the registry's lifecycle enforcement (Building vs Frozen) is preserved.
//! The proxy is built in the current store; derived content templates are
//! pushed into the foreign store through the registry. Render-unit conversion
//! in `render_unit.rs` calls this module's entry point as orchestration.

use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::{Style, TemplateType};
use crate::compiler_frontend::ast::templates::tir::ids::{TemplateIrId, TemplateIrNodeId};
use crate::compiler_frontend::ast::templates::tir::node::{
    TemplateIr, TemplateIrNode, TemplateIrNodeKind,
};
use crate::compiler_frontend::ast::templates::tir::refs::{
    TemplateRef, TemplateStoreId, TemplateTirChildReference,
};
use crate::compiler_frontend::ast::templates::tir::registry::TemplateIrRegistry;
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::ast::templates::tir::summary::summarize_existing_nodes;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use std::collections::HashSet;

// -------------------------
//  Proxy entry point
// -------------------------

/// Builds a local proxy template for a cross-store `SlotInsert` head.
///
/// WHAT: creates a `TemplateIr` entry in the current store whose `kind`
///       carries the `SlotInsert` target key and whose root is a `Sequence`
///       mirroring the foreign `SlotInsert` template's body structure. Nested
///       `$insert` helpers become local `InsertContribution` nodes pointing to
///       recursively-built proxies, and non-insert content becomes local
///       `ChildTemplate` references to narrow derived templates in the foreign
///       store that wrap only that content segment.
///
/// WHY: see the module-level documentation for the rationale.
pub(in crate::compiler_frontend::ast::templates) fn build_foreign_slot_insert_proxy(
    store: &mut TemplateIrStore,
    registry: &TemplateIrRegistry,
    foreign_reference: &TemplateTirChildReference,
    kind: &TemplateType,
    location: &SourceLocation,
) -> Result<TemplateIrId, TemplateError> {
    // Walk the full nested foreign insert graph before allocating any proxy
    // nodes, templates, occurrence IDs or derived foreign templates. This
    // read-only preflight rejects cycles, missing templates, missing nodes,
    // and non-SlotInsert nested helpers with an internal TemplateError.
    // Because no mutation has started, a rejected graph leaves both stores
    // unchanged — the current store has no new allocations and the foreign
    // store has no derived templates.
    preflight_foreign_insert_graph(registry, foreign_reference)?;

    build_foreign_slot_insert_proxy_inner(store, registry, foreign_reference, kind, location)
}

/// Inner proxy construction without the preflight.
///
/// WHAT: the entry point runs the preflight once for the full graph, so the
///       recursive calls bypass it to avoid redundant re-walks of subtrees
///       the top-level preflight already validated.
/// WHY: keeps the construction path clean and avoids quadratic traversal.
fn build_foreign_slot_insert_proxy_inner(
    store: &mut TemplateIrStore,
    registry: &TemplateIrRegistry,
    foreign_reference: &TemplateTirChildReference,
    kind: &TemplateType,
    location: &SourceLocation,
) -> Result<TemplateIrId, TemplateError> {
    let foreign_store_id = foreign_reference.root.store_id;
    let foreign_template_id = foreign_reference.root.template_id;

    // Clone the shared store handle so reads are independent of the registry's
    // borrow lifetime. All reads are temporary — the `Ref` is dropped before
    // any `registry.store_mut` call in `create_derived_foreign_content_template`.
    let foreign_handle = registry.store_handle(foreign_store_id).ok_or_else(|| {
        TemplateError::from(CompilerError::compiler_error(
            "TIR foreign slot-insert proxy: foreign store was not present in the registry.",
        ))
    })?;

    // Read the foreign SlotInsert template's body children. The body is a
    // Sequence of content nodes: text, dynamic expressions, nested
    // InsertContribution markers, and other child content.
    let body_children: Vec<TemplateIrNodeId> = {
        let foreign_store = foreign_handle.borrow();
        let template = foreign_store
            .get_template(foreign_template_id)
            .ok_or_else(|| {
                TemplateError::from(CompilerError::compiler_error(format!(
                    "TIR foreign slot-insert proxy: foreign template {} was not present in store {}.",
                    foreign_template_id, foreign_store_id
                )))
            })?;
        let root = foreign_store.get_node(template.root).ok_or_else(|| {
            TemplateError::from(CompilerError::compiler_error(
                "TIR foreign slot-insert proxy: foreign template root node was not present in the store.",
            ))
        })?;

        match &root.kind {
            TemplateIrNodeKind::Sequence { children } => children.clone(),
            _ => {
                return Err(TemplateError::from(CompilerError::compiler_error(
                    "TIR foreign slot-insert proxy: foreign template root was not a Sequence.",
                )));
            }
        }
    };

    // Walk the foreign body children in source order, building local proxy
    // nodes. Consecutive non-insert content nodes are grouped into one
    // derived foreign template to minimize derived entries, while nested
    // InsertContribution markers become local InsertContribution nodes pointing
    // to recursively-built proxies.
    let mut proxy_children = Vec::new();
    let mut pending_non_insert: Vec<TemplateIrNodeId> = Vec::new();

    for foreign_child_id in body_children {
        let child_kind = foreign_handle
            .borrow()
            .get_node(foreign_child_id)
            .map(|node| node.kind.clone());

        match child_kind {
            Some(TemplateIrNodeKind::InsertContribution {
                template: nested_id,
            }) => {
                // Flush accumulated non-insert content as one derived foreign
                // template before processing the nested insert.
                if !pending_non_insert.is_empty() {
                    let derived_ref = create_derived_foreign_content_template(
                        registry,
                        foreign_store_id,
                        &pending_non_insert,
                        foreign_reference,
                    )?;
                    let occurrence_id = store.next_child_template_occurrence_id();
                    proxy_children.push(store.push_node(TemplateIrNode::new(
                        TemplateIrNodeKind::ChildTemplate {
                            reference: derived_ref,
                            occurrence_id,
                        },
                        location.to_owned(),
                    )));
                    pending_non_insert.clear();
                }

                // Read the nested template's kind to preserve its target key.
                let nested_kind = foreign_handle
                    .borrow()
                    .get_template(nested_id)
                    .map(|template| template.kind.to_owned())
                    .ok_or_else(|| {
                        TemplateError::from(CompilerError::compiler_error(format!(
                            "TIR foreign slot-insert proxy: nested insert template {} was not present in the foreign store.",
                            nested_id
                        )))
                    })?;

                // Recursively build a proxy for the nested insert so its own
                // nested helpers are discovered and routed.
                let nested_reference = TemplateTirChildReference::new(
                    TemplateRef::new(foreign_store_id, nested_id),
                    foreign_reference.phase,
                    foreign_reference.overlay_set_id,
                );
                let nested_proxy_id = build_foreign_slot_insert_proxy_inner(
                    store,
                    registry,
                    &nested_reference,
                    &nested_kind,
                    location,
                )?;

                proxy_children.push(store.push_node(TemplateIrNode::new(
                    TemplateIrNodeKind::InsertContribution {
                        template: nested_proxy_id,
                    },
                    location.to_owned(),
                )));
            }

            Some(_) => {
                // Accumulate non-insert content for grouped derived template
                // creation. Source order is preserved because the group is
                // flushed when a nested insert is encountered or at the end of
                // the body.
                pending_non_insert.push(foreign_child_id);
            }

            None => {
                return Err(TemplateError::from(CompilerError::compiler_error(
                    "TIR foreign slot-insert proxy: foreign body child node was not present in the store.",
                )));
            }
        }
    }

    // Flush any remaining non-insert content.
    if !pending_non_insert.is_empty() {
        let derived_ref = create_derived_foreign_content_template(
            registry,
            foreign_store_id,
            &pending_non_insert,
            foreign_reference,
        )?;
        let occurrence_id = store.next_child_template_occurrence_id();
        proxy_children.push(store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::ChildTemplate {
                reference: derived_ref,
                occurrence_id,
            },
            location.to_owned(),
        )));
    }

    let proxy_root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence {
            children: proxy_children.clone(),
        },
        location.to_owned(),
    ));

    // Compute an accurate summary from the proxy children so downstream
    // consumers (capacity planning, folding, classification) see real shape
    // facts instead of an all-zero default.
    let summary = summarize_existing_nodes(store, &proxy_children);

    Ok(store.push_template(TemplateIr::new(
        proxy_root,
        Style::default(),
        kind.to_owned(),
        summary,
        location.to_owned(),
    )))
}

// -------------------------
//  Read-only graph preflight
// -------------------------

/// Walks the full nested foreign insert graph without mutating either store.
///
/// WHAT: traverses every `InsertContribution` -> `SlotInsert` template -> body
///       chain reachable from `foreign_reference`, checking that:
///       - no store-qualified `TemplateRef` re-enters the active path (cycle),
///       - every referenced template exists,
///       - every body child node exists,
///       - every nested helper template has `SlotInsert` kind.
/// WHY: the proxy construction allocates nodes, templates and derived foreign
///      templates incrementally. Without a preflight, a cyclic or malformed
///      graph would leave partial mutations in both stores — the current store
///      with half-built proxy nodes and the foreign store with stray derived
///      templates. Rejecting the entire graph up front keeps both stores
///      unchanged on failure, preserving the atomicity contract.
///
/// ## Recursion safety
///
/// The visited set is keyed by store-qualified `TemplateRef`, so the same
/// template ID in different stores is not confused. Borrows are scoped: each
/// template level collects its nested refs inside a `Ref` guard, drops the
/// guard, then recurses — so the `RefCell` is never borrowed twice.
fn preflight_foreign_insert_graph(
    registry: &TemplateIrRegistry,
    foreign_reference: &TemplateTirChildReference,
) -> Result<(), TemplateError> {
    let mut visiting: HashSet<TemplateRef> = HashSet::new();
    preflight_visit_template(registry, foreign_reference.root, &mut visiting)
}

/// Recursively visits one foreign template in the insert graph.
fn preflight_visit_template(
    registry: &TemplateIrRegistry,
    template_ref: TemplateRef,
    visiting: &mut HashSet<TemplateRef>,
) -> Result<(), TemplateError> {
    // Reject cycles only when a store-qualified template ref re-enters the
    // active recursion path. Removing it after this branch completes permits
    // valid DAGs that reuse the same nested insert helper more than once.
    if !visiting.insert(template_ref) {
        return Err(TemplateError::from(CompilerError::compiler_error(format!(
            "TIR foreign slot-insert proxy: cyclic insert graph detected at {}.",
            template_ref
        ))));
    }

    let store_id = template_ref.store_id;
    let template_id = template_ref.template_id;

    let foreign_handle = registry.store_handle(store_id).ok_or_else(|| {
        TemplateError::from(CompilerError::compiler_error(format!(
            "TIR foreign slot-insert proxy preflight: store {} was not present in the registry.",
            store_id
        )))
    })?;

    // Collect nested InsertContribution refs inside the borrow scope, then
    // drop the Ref before recursing so the RefCell is free for the next level.
    let nested_refs: Vec<TemplateRef> = {
        let store = foreign_handle.borrow();

        let template = store.get_template(template_id).ok_or_else(|| {
            TemplateError::from(CompilerError::compiler_error(format!(
                "TIR foreign slot-insert proxy preflight: template {} was not present in store {}.",
                template_id, store_id
            )))
        })?;

        if !matches!(template.kind, TemplateType::SlotInsert(_)) {
            return Err(TemplateError::from(CompilerError::compiler_error(format!(
                "TIR foreign slot-insert proxy preflight: template {} has kind {:?}, expected SlotInsert.",
                template_id, template.kind
            ))));
        }

        let root = store.get_node(template.root).ok_or_else(|| {
            TemplateError::from(CompilerError::compiler_error(
                "TIR foreign slot-insert proxy preflight: template root node was not present in the store.",
            ))
        })?;

        let body_children = match &root.kind {
            TemplateIrNodeKind::Sequence { children } => children.clone(),
            _ => {
                return Err(TemplateError::from(CompilerError::compiler_error(
                    "TIR foreign slot-insert proxy preflight: foreign template root was not a Sequence.",
                )));
            }
        };

        let mut collected: Vec<TemplateRef> = Vec::new();

        for child_id in body_children {
            let child = store.get_node(child_id).ok_or_else(|| {
                TemplateError::from(CompilerError::compiler_error(
                    "TIR foreign slot-insert proxy preflight: foreign body child node was not present in the store.",
                ))
            })?;

            if let TemplateIrNodeKind::InsertContribution {
                template: nested_id,
            } = &child.kind
            {
                let nested_template = store.get_template(*nested_id).ok_or_else(|| {
                    TemplateError::from(CompilerError::compiler_error(format!(
                        "TIR foreign slot-insert proxy preflight: nested insert template {} was not present in the foreign store.",
                        nested_id
                    )))
                })?;

                // Nested helpers must be SlotInsert kinds — only SlotInsert
                // templates carry a target key the proxy can route.
                if !matches!(nested_template.kind, TemplateType::SlotInsert(_)) {
                    return Err(TemplateError::from(CompilerError::compiler_error(format!(
                        "TIR foreign slot-insert proxy preflight: nested insert template {} has kind {:?}, expected SlotInsert.",
                        nested_id, nested_template.kind
                    ))));
                }

                collected.push(TemplateRef::new(store_id, *nested_id));
            }
        }

        collected
    };

    // RefCell borrow dropped — safe to recurse into nested templates.
    for nested_ref in nested_refs {
        preflight_visit_template(registry, nested_ref, visiting)?;
    }

    visiting.remove(&template_ref);
    Ok(())
}

// -------------------------
//  Derived foreign content template
// -------------------------

/// Creates a narrow derived template in the foreign store wrapping a group of
/// non-insert body content nodes.
///
/// WHAT: pushes a new `TemplateIr` entry into the foreign store whose root is a
///       `Sequence` containing the given foreign node IDs. Returns a
///       store-qualified `TemplateTirChildReference` so the local proxy can
///       reference this content through a `ChildTemplate` node without
///       deep-copying the foreign tree.
///
/// WHY: the proxy must separate nested `InsertContribution` markers from
///      non-insert body content so routing discovers nested inserts. Non-insert
///      content cannot be referenced by a bare foreign node ID in the current
///      store, so a thin derived template wraps the content segment in its
///      owning store. The derived template is narrow — it references only the
///      content nodes, not the entire `SlotInsert` template, so nested inserts
///      that were routed separately do not reappear during folding.
///
/// ## Lifecycle enforcement
///
/// All mutation goes through `TemplateIrRegistry::store_mut`, which rejects
/// writes to a frozen (non-`Building`) store with a precise internal error.
/// This preserves the registry's lifecycle invariant — no raw
/// `Rc<RefCell<TemplateIrStore>>::borrow_mut()` bypasses the guard.
fn create_derived_foreign_content_template(
    registry: &TemplateIrRegistry,
    foreign_store_id: TemplateStoreId,
    content_node_ids: &[TemplateIrNodeId],
    foreign_reference: &TemplateTirChildReference,
) -> Result<TemplateTirChildReference, TemplateError> {
    let mut foreign_store = registry
        .store_mut(foreign_store_id)
        .map_err(TemplateError::from)?;

    let location = foreign_store
        .get_node(content_node_ids[0])
        .map(|node| node.location.to_owned())
        .unwrap_or_default();

    // Summarize the wrapped content nodes so the derived template carries an
    // honest shape — text bytes, dynamic-expression counts, reactivity, control
    // flow, slots, and depth — instead of a false all-zero default.
    let summary = summarize_existing_nodes(&foreign_store, content_node_ids);

    let derived_root = foreign_store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence {
            children: content_node_ids.to_vec(),
        },
        location.to_owned(),
    ));

    let derived_template_id = foreign_store.push_template(TemplateIr::new(
        derived_root,
        Style::default(),
        TemplateType::String,
        summary,
        location,
    ));

    // Drop the mutable borrow before constructing the reference so the RefCell
    // is free for subsequent reads in the proxy loop.
    drop(foreign_store);

    Ok(TemplateTirChildReference::new(
        TemplateRef::new(foreign_store_id, derived_template_id),
        foreign_reference.phase,
        foreign_reference.overlay_set_id,
    ))
}
