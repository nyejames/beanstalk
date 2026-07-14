use super::super::builder::TemplateIrBuilder;
use super::super::ids::{TemplateIrId, TemplateWrapperSetId};
use super::super::node::{TemplateIrNode, TemplateIrNodeKind};
use super::super::overlays::TemplateOverlaySetId;
use super::super::refs::{
    TemplateNodeRef, TemplateRef, TemplateStoreId, TemplateStringDomainId,
    TemplateWrapperReference, TemplateWrapperSetRef,
};
use super::super::registry::{RegisteredTemplateIrStore, TemplateIrRegistry};
use super::super::store::{TemplateIrStore, TemplateStoreState, TemplateWrapperSet};
use super::super::summary::TemplateIrSummary;
use super::super::view::TemplateTirPhase;
use crate::compiler_frontend::ast::templates::template::{Style, TemplateType};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use std::cell::RefCell;
use std::rc::Rc;

fn empty_location() -> SourceLocation {
    SourceLocation::default()
}

fn build_empty_template_in_store(store: &mut TemplateIrStore) -> TemplateIrId {
    let mut builder = TemplateIrBuilder::new(store);
    let root = builder.push_sequence_node(vec![], empty_location());
    builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    )
}

#[test]
fn registry_starts_empty() {
    let registry = TemplateIrRegistry::new();
    assert_eq!(registry.store_count(), 0);
}

#[test]
fn allocate_store_returns_sequential_ids() {
    let mut registry = TemplateIrRegistry::new();

    let a = registry.allocate_store();
    let b = registry.allocate_store();

    assert_eq!(a.index(), 0);
    assert_eq!(b.index(), 1);
    assert_eq!(registry.store_count(), 2);
}

#[test]
fn store_lookup_returns_building_store() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    let store = registry.store(store_id).expect("store should exist");
    assert_eq!(store.template_count(), 0);

    assert_eq!(
        registry.store_state(store_id),
        Some(TemplateStoreState::Building)
    );
}

#[test]
fn store_mut_lookup_allows_mutation() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    let template_id = {
        let mut store = registry.store_mut(store_id).expect("store should exist");
        build_empty_template_in_store(&mut store)
    };

    let template = registry
        .template(TemplateRef::new(store_id, template_id))
        .expect("template should exist");
    assert_eq!(template.kind, TemplateType::String);
}

#[test]
fn store_mut_rejects_frozen_store() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    registry.freeze_store(store_id).unwrap();

    assert!(registry.store_mut(store_id).is_err());
}

#[test]
fn freeze_store_transitions_to_frozen_module_local() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    let domain = registry
        .freeze_store(store_id)
        .expect("freeze should succeed");

    assert_eq!(
        registry.store_state(store_id),
        Some(TemplateStoreState::FrozenModuleLocal {
            string_domain: domain
        })
    );
}

#[test]
fn freeze_store_fails_for_missing_store() {
    let mut registry = TemplateIrRegistry::new();
    let result = registry.freeze_store(TemplateStoreId::new(99));
    assert!(result.is_err());
}

#[test]
fn freeze_store_fails_when_already_frozen() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    registry.freeze_store(store_id).unwrap();
    let second_freeze = registry.freeze_store(store_id);

    assert!(second_freeze.is_err());
}

#[test]
fn freeze_store_with_domain_groups_stores() {
    let mut registry = TemplateIrRegistry::new();
    let a = registry.allocate_store();
    let b = registry.allocate_store();
    let domain = TemplateStringDomainId::new(0);

    registry.freeze_store_with_domain(a, domain).unwrap();
    registry.freeze_store_with_domain(b, domain).unwrap();

    assert!(registry.validate_same_domain(a, b).is_ok());
}

#[test]
fn validate_same_domain_succeeds_for_same_store() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    assert!(registry.validate_same_domain(store_id, store_id).is_ok());
}

#[test]
fn validate_same_domain_succeeds_for_same_domain() {
    let mut registry = TemplateIrRegistry::new();
    let a = registry.allocate_store();
    let b = registry.allocate_store();
    let domain = TemplateStringDomainId::new(0);

    registry.freeze_store_with_domain(a, domain).unwrap();
    registry.freeze_store_with_domain(b, domain).unwrap();

    assert!(registry.validate_same_domain(a, b).is_ok());
}

#[test]
fn validate_same_domain_fails_for_building_store() {
    let mut registry = TemplateIrRegistry::new();
    let a = registry.allocate_store();
    let b = registry.allocate_store();
    let domain = TemplateStringDomainId::new(0);

    registry.freeze_store_with_domain(a, domain).unwrap();

    assert!(registry.validate_same_domain(a, b).is_err());
    assert!(registry.validate_same_domain(b, a).is_err());
}

#[test]
fn validate_same_domain_fails_for_different_domains() {
    let mut registry = TemplateIrRegistry::new();
    let a = registry.allocate_store();
    let b = registry.allocate_store();

    registry.freeze_store(a).unwrap();
    registry.freeze_store(b).unwrap();

    assert!(registry.validate_same_domain(a, b).is_err());
}

#[test]
fn validate_same_domain_fails_for_missing_store() {
    let registry = TemplateIrRegistry::new();
    let a = TemplateStoreId::new(0);
    let b = TemplateStoreId::new(1);

    assert!(registry.validate_same_domain(a, b).is_err());
}

#[test]
fn validate_store_is_building_succeeds_for_building_store() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    assert!(registry.validate_store_is_building(store_id).is_ok());
}

#[test]
fn validate_store_is_building_fails_for_frozen_store() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    registry.freeze_store(store_id).unwrap();

    assert!(registry.validate_store_is_building(store_id).is_err());
}

#[test]
fn validate_store_is_frozen_succeeds_for_frozen_store() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    registry.freeze_store(store_id).unwrap();

    assert!(registry.validate_store_is_frozen(store_id).is_ok());
}

#[test]
fn validate_store_is_frozen_fails_for_building_store() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    assert!(registry.validate_store_is_frozen(store_id).is_err());
}

#[test]
fn node_lookup_returns_store_qualified_node() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    let node_id = {
        let mut store = registry.store_mut(store_id).expect("store should exist");
        store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Sequence { children: vec![] },
            empty_location(),
        ))
    };

    let node = registry
        .node(TemplateNodeRef::new(store_id, node_id))
        .expect("node should exist");
    assert!(matches!(node.kind, TemplateIrNodeKind::Sequence { .. }));
}

#[test]
fn wrapper_set_lookup_returns_store_qualified_wrapper_set() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    let wrapper_set_id = {
        let mut store = registry.store_mut(store_id).expect("store should exist");
        store.push_wrapper_set(TemplateWrapperSet { wrappers: vec![] })
    };

    let wrapper_set = registry
        .wrapper_set(TemplateWrapperSetRef::new(store_id, wrapper_set_id))
        .expect("wrapper set should exist");
    assert!(wrapper_set.wrappers.is_empty());
}

#[test]
fn store_qualified_lookup_returns_none_for_missing_store() {
    let registry = TemplateIrRegistry::new();
    let reference = TemplateRef::new(TemplateStoreId::new(99), TemplateIrId::new(0));

    assert!(registry.template(reference).is_none());
}

#[test]
fn store_qualified_lookup_returns_none_for_missing_entry() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    let reference = TemplateRef::new(store_id, TemplateIrId::new(99));
    assert!(registry.template(reference).is_none());
}

// -------------------------
//  Shared-handle and capacity tests
// -------------------------

#[test]
fn adopt_store_registers_existing_store_handle() {
    let store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut registry = TemplateIrRegistry::new();

    let store_id = registry.adopt_store(Rc::clone(&store));

    assert_eq!(store_id.index(), 0);
    assert_eq!(registry.store_count(), 1);
    assert_eq!(
        registry.store_state(store_id),
        Some(TemplateStoreState::Building)
    );
}

#[test]
fn adopt_store_restamps_existing_wrapper_refs() {
    let store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let wrapper_set_id = {
        let mut store = store.borrow_mut();
        let template_id = build_empty_template_in_store(&mut store);

        let wrapper_ref = TemplateWrapperReference::new(
            store.qualify_template_ref(template_id),
            TemplateTirPhase::Finalized,
            TemplateOverlaySetId::empty(),
        );
        store.push_or_reuse_wrapper_set(vec![wrapper_ref])
    };

    let mut registry = TemplateIrRegistry::new();
    let _first_store_id = registry.allocate_store();
    let adopted_store_id = registry.adopt_store(Rc::clone(&store));

    assert_eq!(adopted_store_id, TemplateStoreId::new(1));

    let wrapper_set = registry
        .wrapper_set(TemplateWrapperSetRef::new(adopted_store_id, wrapper_set_id))
        .expect("adopted wrapper set should be visible through registry");

    assert_eq!(wrapper_set.wrappers.len(), 1);
    assert_eq!(wrapper_set.wrappers[0].root.store_id, adopted_store_id);
}

#[test]
fn store_handle_returns_same_store() {
    let store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut registry = TemplateIrRegistry::new();

    let store_id = registry.adopt_store(Rc::clone(&store));
    let handle = registry
        .store_handle(store_id)
        .expect("handle should exist");

    // The returned handle shares the same RefCell as the original.
    assert!(Rc::ptr_eq(&handle, &store));
}

#[test]
fn store_handle_returns_none_for_missing_store() {
    let registry = TemplateIrRegistry::new();
    assert!(registry.store_handle(TemplateStoreId::new(99)).is_none());
}

#[test]
fn registered_store_rejects_same_id_handle_from_another_registry() {
    let mut registry = TemplateIrRegistry::new();
    let registered_store_id = registry.allocate_store();
    let registry = Rc::new(RefCell::new(registry));

    let mut foreign_registry = TemplateIrRegistry::new();
    let foreign_store_id = foreign_registry.allocate_store();
    let foreign_store = foreign_registry
        .store_handle(foreign_store_id)
        .expect("foreign store should exist");

    assert_eq!(registered_store_id, foreign_store_id);
    assert!(RegisteredTemplateIrStore::from_registry_and_store(registry, foreign_store).is_err());
}

#[test]
fn allocate_primary_store_with_capacity_creates_building_store() {
    use crate::compiler_frontend::arena::FrontendArenaCapacityEstimate;

    let mut registry = TemplateIrRegistry::new();
    let store_id =
        registry.allocate_primary_store_with_capacity(FrontendArenaCapacityEstimate::default());

    assert_eq!(store_id.index(), 0);
    assert_eq!(registry.store_count(), 1);
    assert_eq!(
        registry.store_state(store_id),
        Some(TemplateStoreState::Building)
    );
}

#[test]
fn store_borrow_via_handle_allows_mutation() {
    let store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut registry = TemplateIrRegistry::new();

    let store_id = registry.adopt_store(Rc::clone(&store));

    // Mutate through the registry's store_mut.
    {
        let mut borrowed = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        build_empty_template_in_store(&mut borrowed);
    }

    // The mutation is visible through the original handle.
    assert_eq!(store.borrow().template_count(), 1);
}

// -------------------------
//  Store-Qualified Wrapper Ref Tests
// -------------------------

#[test]
fn registry_stamps_store_id_so_wrapper_sets_carry_qualified_refs() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    // The store should be stamped with its registry-assigned ID.
    {
        let store = registry.store(store_id).expect("store should exist");
        assert_eq!(store.store_id(), store_id);
    }

    // Create a template and a wrapper set referencing it.
    let template_id = {
        let mut store = registry.store_mut(store_id).expect("store should exist");
        let template_id = build_empty_template_in_store(&mut store);
        let wrapper_ref = TemplateWrapperReference::new(
            store.qualify_template_ref(template_id),
            TemplateTirPhase::Finalized,
            TemplateOverlaySetId::empty(),
        );

        // push_or_reuse_wrapper_set should qualify the store-local ID.
        let set_id = store.push_or_reuse_wrapper_set(vec![wrapper_ref]);

        let wrapper_set = store
            .get_wrapper_set(set_id)
            .expect("wrapper set should exist");
        assert_eq!(wrapper_set.wrappers.len(), 1);
        assert_eq!(wrapper_set.wrappers[0].root.store_id, store_id);
        assert_eq!(wrapper_set.wrappers[0].root.template_id, template_id);

        template_id
    };

    // The registry-level wrapper_set lookup should also return the qualified refs.
    let wrapper_set = registry
        .wrapper_set(TemplateWrapperSetRef::new(
            store_id,
            TemplateWrapperSetId::new(0),
        ))
        .expect("wrapper set should be visible through registry");
    assert_eq!(wrapper_set.wrappers[0].root.store_id, store_id);
    assert_eq!(wrapper_set.wrappers[0].root.template_id, template_id);
}
