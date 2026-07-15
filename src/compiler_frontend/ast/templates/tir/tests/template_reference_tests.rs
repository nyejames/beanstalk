//! Tests for the final TIR reference contract.
//!
//! WHAT: exercises store-qualified `TemplateTirReference` ownership, the
//!       numeric-store-ID collision invariant, and cloning of the live
//!       `Template` payload.
//! WHY: `TemplateStoreId` is a registry-local index that can collide across
//!      registries. The owner token prevents a local `TemplateIrId` from being
//!      used against a different logical store at the same numeric index.

use super::super::ids::TemplateIrId;
use super::super::overlays::TemplateOverlaySetId;
use super::super::registry::TemplateIrRegistry;
use super::super::store::TemplateIrStore;
use super::super::{TemplateRef, TemplateTirReference};
use crate::compiler_frontend::ast::templates::template::Template;
use crate::compiler_frontend::ast::templates::template::{SlotKey, TemplateType};

use std::sync::Arc;

fn make_reference(template_id: TemplateIrId, store: &TemplateIrStore) -> TemplateTirReference {
    TemplateTirReference {
        root: TemplateRef::new(store.store_id(), template_id),
        store_owner: store.owner(),
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
fn registry_local_store_ids_need_owner_identity() {
    let mut correct_registry = TemplateIrRegistry::new();
    let mut wrong_registry = TemplateIrRegistry::new();
    let correct_store_id = correct_registry.allocate_store();
    let wrong_store_id = wrong_registry.allocate_store();

    assert_eq!(correct_store_id, wrong_store_id);

    let correct_handle = correct_registry
        .store_handle(correct_store_id)
        .expect("correct registry store should exist");
    let wrong_handle = wrong_registry
        .store_handle(wrong_store_id)
        .expect("wrong registry store should exist");
    let correct_store = correct_handle.borrow();
    let wrong_store = wrong_handle.borrow();
    let reference = make_reference(TemplateIrId::new(0), &correct_store);

    assert!(Arc::ptr_eq(&reference.store_owner, &correct_store.owner()));
    assert!(!Arc::ptr_eq(&reference.store_owner, &wrong_store.owner()));
}

#[test]
fn template_clone_preserves_tir_reference() {
    let store = TemplateIrStore::new();
    let original = Template {
        kind: crate::compiler_frontend::ast::templates::template::TemplateType::StringFunction,
        tir_reference: make_reference(TemplateIrId::new(5), &store),
        location: crate::compiler_frontend::tokenizer::tokens::SourceLocation::default(),
    };

    let cloned = original.clone();

    let cloned_reference = &cloned.tir_reference;
    assert_eq!(cloned_reference.root.template_id, TemplateIrId::new(5));
    assert!(Arc::ptr_eq(&cloned_reference.store_owner, &store.owner()));
}

#[test]
fn template_kind_lookup_rejects_same_numeric_store_id_from_another_registry() {
    let mut correct_registry = TemplateIrRegistry::new();
    let mut wrong_registry = TemplateIrRegistry::new();
    let correct_store_id = correct_registry.allocate_store();
    let wrong_store_id = wrong_registry.allocate_store();

    assert_eq!(correct_store_id, wrong_store_id);

    let correct_handle = correct_registry
        .store_handle(correct_store_id)
        .expect("correct registry store should exist");
    let correct_template_id = push_template_with_kind(
        &mut correct_handle.borrow_mut(),
        TemplateType::SlotDefinition(SlotKey::Default),
    );

    let wrong_handle = wrong_registry
        .store_handle(wrong_store_id)
        .expect("wrong registry store should exist");
    push_template_with_kind(&mut wrong_handle.borrow_mut(), TemplateType::String);

    let template = Template {
        kind: TemplateType::StringFunction,
        tir_reference: make_reference(correct_template_id, &correct_handle.borrow()),
        location: crate::compiler_frontend::tokenizer::tokens::SourceLocation::default(),
    };

    assert!(matches!(
        template.tir_kind_via_registry(&correct_registry),
        Some(TemplateType::SlotDefinition(_))
    ));
    assert_eq!(template.tir_kind_via_registry(&wrong_registry), None);
}

fn push_template_with_kind(store: &mut TemplateIrStore, kind: TemplateType) -> TemplateIrId {
    use super::super::node::{TemplateIr, TemplateIrNode, TemplateIrNodeKind};
    use super::super::summary::TemplateIrSummary;

    let location = crate::compiler_frontend::tokenizer::tokens::SourceLocation::default();
    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence { children: vec![] },
        location.clone(),
    ));

    store.push_template(TemplateIr::new(
        root,
        crate::compiler_frontend::ast::templates::template::Style::default(),
        kind,
        TemplateIrSummary::default(),
        location,
    ))
}
