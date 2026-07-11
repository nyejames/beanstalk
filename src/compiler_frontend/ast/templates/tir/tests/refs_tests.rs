use super::super::ids::{TemplateIrId, TemplateIrNodeId, TemplateWrapperSetId};
use super::super::refs::{
    TemplateNodeRef, TemplateRef, TemplateStoreId, TemplateStringDomainId, TemplateWrapperSetRef,
};

#[test]
fn template_store_id_round_trips_through_u32_bound() {
    let id = TemplateStoreId::new(5);
    assert_eq!(id.index(), 5);
}

#[test]
fn template_string_domain_id_round_trips_through_u32_bound() {
    let id = TemplateStringDomainId::new(7);
    assert_eq!(id.index(), 7);
}

#[test]
fn template_ref_carries_store_and_template_id() {
    let store_id = TemplateStoreId::new(1);
    let template_id = TemplateIrId::new(2);
    let reference = TemplateRef::new(store_id, template_id);

    assert_eq!(reference.store_id, store_id);
    assert_eq!(reference.template_id, template_id);
}

#[test]
fn template_node_ref_carries_store_and_node_id() {
    let store_id = TemplateStoreId::new(3);
    let node_id = TemplateIrNodeId::new(4);
    let reference = TemplateNodeRef::new(store_id, node_id);

    assert_eq!(reference.store_id, store_id);
    assert_eq!(reference.node_id, node_id);
}

#[test]
fn template_wrapper_set_ref_carries_store_and_wrapper_set_id() {
    let store_id = TemplateStoreId::new(5);
    let wrapper_set_id = TemplateWrapperSetId::new(6);
    let reference = TemplateWrapperSetRef::new(store_id, wrapper_set_id);

    assert_eq!(reference.store_id, store_id);
    assert_eq!(reference.wrapper_set_id, wrapper_set_id);
}

#[test]
fn refs_display_correctly() {
    assert_eq!(TemplateStoreId::new(1).to_string(), "TemplateStoreId(1)");
    assert_eq!(
        TemplateStringDomainId::new(2).to_string(),
        "TemplateStringDomainId(2)"
    );
    assert_eq!(
        TemplateRef::new(TemplateStoreId::new(3), TemplateIrId::new(4)).to_string(),
        "TemplateRef(TemplateStoreId(3), TemplateIrId(4))"
    );
    assert_eq!(
        TemplateNodeRef::new(TemplateStoreId::new(5), TemplateIrNodeId::new(6)).to_string(),
        "TemplateNodeRef(TemplateStoreId(5), TemplateIrNodeId(6))"
    );
    assert_eq!(
        TemplateWrapperSetRef::new(TemplateStoreId::new(7), TemplateWrapperSetId::new(8))
            .to_string(),
        "TemplateWrapperSetRef(TemplateStoreId(7), TemplateWrapperSetId(8))"
    );
}

#[test]
fn refs_are_distinct_types() {
    let store_id = TemplateStoreId::new(0);
    let template_id = TemplateIrId::new(0);
    let node_id = TemplateIrNodeId::new(0);
    let wrapper_set_id = TemplateWrapperSetId::new(0);

    let template_ref = TemplateRef::new(store_id, template_id);
    let node_ref = TemplateNodeRef::new(store_id, node_id);
    let wrapper_set_ref = TemplateWrapperSetRef::new(store_id, wrapper_set_id);

    assert_eq!(template_ref.store_id, node_ref.store_id);
    assert_eq!(template_ref.store_id, wrapper_set_ref.store_id);
}
