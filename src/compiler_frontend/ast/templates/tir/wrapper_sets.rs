//! TIR wrapper-set helpers.
//!
//! WHAT: owns the conservative equivalence predicate used to deduplicate
//! `$children(..)` wrapper sets in the `TemplateIrStore` side table.
//!
//! WHY: wrapper sets live in the store rather than on each `TemplateIr`, so
//! keeping the reuse policy close to the wrapper-set concept makes the
//! deduplication boundary explicit and easy to tighten without leaking store
//! internals into the converter or fold paths.

use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::tir::refs::TemplateWrapperReference;
use crate::compiler_frontend::ast::templates::tir::registry::TemplateIrRegistry;
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
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
