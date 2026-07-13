//! Tests for the final TIR reference contract.
//!
//! WHAT: exercises store-qualified `TemplateTirReference` ownership and cloning
//!       of the live `Template` payload.
//! WHY: these handle invariants remain useful for AST-owned TIR payloads.

use super::super::ids::TemplateIrId;
use super::super::overlays::TemplateOverlaySetId;
use super::super::store::TemplateIrStore;
use super::super::{TemplateRef, TemplateTirReference};
use crate::compiler_frontend::ast::templates::template_types::Template;

use std::sync::Arc;

fn make_reference(template_id: TemplateIrId, store: &TemplateIrStore) -> TemplateTirReference {
    TemplateTirReference {
        root: TemplateRef::new(store.store_id(), template_id),
        store_owner: store.owner(),
        is_composed: false,
        phase: crate::compiler_frontend::ast::templates::tir::TemplateTirPhase::Parsed,
        overlay_set_id: TemplateOverlaySetId::empty_for_test(),
    }
}

#[test]
fn tir_reference_carries_store_qualified_root_and_store_owner() {
    let store = TemplateIrStore::new();
    let reference = make_reference(TemplateIrId::new(7), &store);

    assert_eq!(reference.root.template_id, TemplateIrId::new(7));
    assert_eq!(reference.root.store_id, store.store_id());
    assert!(Arc::ptr_eq(&reference.store_owner, &store.owner()));
}

#[test]
fn tir_references_from_same_store_have_ptr_eq_owners() {
    let store = TemplateIrStore::new();
    let first = make_reference(TemplateIrId::new(0), &store);
    let second = make_reference(TemplateIrId::new(1), &store);

    assert!(Arc::ptr_eq(&first.store_owner, &second.store_owner));
}

#[test]
fn tir_reference_matches_correct_store_owner_and_rejects_wrong_owner() {
    let correct_store = TemplateIrStore::new();
    let wrong_store = TemplateIrStore::new();
    let reference = make_reference(TemplateIrId::new(0), &correct_store);

    assert!(Arc::ptr_eq(&reference.store_owner, &correct_store.owner()));
    assert!(!Arc::ptr_eq(&reference.store_owner, &wrong_store.owner()));
}

#[test]
fn template_clone_preserves_tir_reference() {
    let store = TemplateIrStore::new();
    let mut original = Template::empty();
    original.tir_reference = Some(make_reference(TemplateIrId::new(5), &store));

    let cloned = original.clone();

    let cloned_reference = cloned
        .tir_reference
        .expect("clone must preserve finalized TIR reference");
    assert_eq!(cloned_reference.root.template_id, TemplateIrId::new(5));
    assert!(Arc::ptr_eq(&cloned_reference.store_owner, &store.owner()));
}
