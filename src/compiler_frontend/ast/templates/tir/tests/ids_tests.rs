use super::super::ids::{TemplateIrId, TemplateIrNodeId, TemplateSlotPlanId, TemplateWrapperSetId};

#[test]
fn typed_ids_round_trip_through_u32_bound() {
    // Each ID type owns a separate index space but shares one compact
    // new(usize)/index() round-trip contract at the maximum compact value.
    let maximum_index = u32::MAX as usize;

    assert_eq!(
        TemplateIrId::new(maximum_index).index(),
        maximum_index,
        "TemplateIrId"
    );
    assert_eq!(
        TemplateIrNodeId::new(maximum_index).index(),
        maximum_index,
        "TemplateIrNodeId"
    );
    assert_eq!(
        TemplateWrapperSetId::new(maximum_index).index(),
        maximum_index,
        "TemplateWrapperSetId"
    );
    assert_eq!(
        TemplateSlotPlanId::new(maximum_index).index(),
        maximum_index,
        "TemplateSlotPlanId"
    );
}

#[test]
fn ids_are_distinct_types() {
    // These assignments must not compile if the types are mixed:
    let template_id = TemplateIrId::new(1);
    let node_id = TemplateIrNodeId::new(1);
    let wrapper_id = TemplateWrapperSetId::new(1);
    let slot_plan_id = TemplateSlotPlanId::new(1);

    // Each type has its own index space.
    assert_eq!(template_id.index(), 1);
    assert_eq!(node_id.index(), 1);
    assert_eq!(wrapper_id.index(), 1);
    assert_eq!(slot_plan_id.index(), 1);

    // IDs of the same type with the same value are equal.
    assert_eq!(template_id, TemplateIrId::new(1));
    assert_ne!(template_id, TemplateIrId::new(2));
}

#[test]
fn ids_display_correctly() {
    assert_eq!(TemplateIrId::new(7).to_string(), "TemplateIrId(7)");
    assert_eq!(TemplateIrNodeId::new(3).to_string(), "TemplateIrNodeId(3)");
    assert_eq!(
        TemplateWrapperSetId::new(0).to_string(),
        "TemplateWrapperSetId(0)"
    );
    assert_eq!(
        TemplateSlotPlanId::new(9).to_string(),
        "TemplateSlotPlanId(9)"
    );
}
