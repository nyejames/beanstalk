use super::super::summary::TemplateIrSummary;

#[test]
fn empty_summary_has_zero_counts_and_false_flags() {
    let summary = TemplateIrSummary::empty();

    assert_eq!(summary.estimated_output_bytes, 0);
    assert_eq!(summary.text_node_count, 0);
    assert_eq!(summary.text_byte_count, 0);
    assert_eq!(summary.dynamic_expression_count, 0);
    assert_eq!(summary.child_template_count, 0);
    assert_eq!(summary.head_node_count, 0);
    assert_eq!(summary.slot_count, 0);
    assert_eq!(summary.insert_contribution_count, 0);
    assert_eq!(summary.wrapper_count, 0);
    assert_eq!(summary.max_depth, 0);
    assert!(!summary.has_slots);
    assert!(!summary.has_insert_contributions);
    assert!(!summary.has_control_flow);
    assert!(!summary.has_reactivity);
    assert!(summary.is_const_evaluable_shape);
}

#[test]
fn record_helpers_preserve_summary_shape_contracts() {
    let mut summary = TemplateIrSummary::empty();
    summary.record_text_node(10);
    summary.record_text_node(5);
    summary.record_dynamic_expression(false);
    summary.record_dynamic_expression(true);
    summary.record_child_template();
    summary.record_child_template();
    summary.record_control_flow();
    summary.record_runtime_slot_site();
    summary.record_insert_contribution();

    assert_eq!(summary.text_node_count, 2);
    assert_eq!(summary.text_byte_count, 15);
    assert_eq!(summary.estimated_output_bytes, 15);
    assert_eq!(summary.dynamic_expression_count, 2);
    assert_eq!(summary.child_template_count, 2);
    assert!(summary.has_slots);
    assert!(summary.has_control_flow);
    assert_eq!(summary.insert_contribution_count, 1);
    assert!(summary.has_insert_contributions);
    assert!(summary.has_reactivity);
    assert!(!summary.is_const_evaluable_shape);

    let mut unresolved_slot_summary = TemplateIrSummary::empty();
    unresolved_slot_summary.record_slot();
    assert_eq!(unresolved_slot_summary.slot_count, 1);
    assert!(unresolved_slot_summary.has_slots);
    assert!(
        unresolved_slot_summary.is_const_evaluable_shape,
        "unresolved slot construction preserves the later conversion decision"
    );
}

#[test]
fn merge_converted_wrapper_tree_adds_counts_and_clears_const_evaluable() {
    let mut summary = TemplateIrSummary::empty();
    summary.record_text_node(8);
    summary.record_child_template();

    let mut other = TemplateIrSummary::empty();
    other.record_dynamic_expression(false);
    other.record_insert_contribution();
    other.record_slot();
    other.record_control_flow();
    other.wrapper_count = 3;
    other.head_node_count = 2;
    other.max_depth = 4;
    other.has_reactivity = true;

    summary.merge_converted_wrapper_tree(&other);

    assert_eq!(summary.text_node_count, 1);
    assert_eq!(summary.text_byte_count, 8);
    assert_eq!(summary.estimated_output_bytes, 8);
    assert_eq!(summary.child_template_count, 1);
    assert_eq!(summary.dynamic_expression_count, 1);
    assert_eq!(summary.insert_contribution_count, 1);
    assert_eq!(summary.wrapper_count, 3);
    assert_eq!(summary.head_node_count, 2);
    assert_eq!(summary.max_depth, 4);
    assert_eq!(summary.slot_count, 0);
    assert!(summary.has_slots);
    assert!(summary.has_insert_contributions);
    assert!(summary.has_control_flow);
    assert!(summary.has_reactivity);
    assert!(
        !summary.is_const_evaluable_shape,
        "merged wrapper tree must not be const-evaluable"
    );
}
