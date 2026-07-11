use super::super::summary::TemplateIrSummary;

#[test]
fn empty_summary_has_zero_counts_and_false_flags() {
    let summary = TemplateIrSummary::empty();

    assert_eq!(summary.estimated_output_bytes, 0);
    assert_eq!(summary.text_node_count, 0);
    assert_eq!(summary.text_byte_count, 0);
    assert_eq!(summary.dynamic_expression_count, 0);
    assert_eq!(summary.child_template_count, 0);
    assert_eq!(summary.slot_count, 0);
    assert_eq!(summary.insert_contribution_count, 0);
    assert_eq!(summary.wrapper_count, 0);
    assert_eq!(summary.max_depth, 0);
    assert!(!summary.has_slots);
    assert!(!summary.has_insert_contributions);
    assert!(!summary.has_formatter);
    assert!(!summary.has_control_flow);
    assert!(!summary.has_reactivity);
    assert!(summary.is_const_evaluable_shape);
}

#[test]
fn default_summary_is_empty() {
    let summary = TemplateIrSummary::default();
    assert_eq!(summary.text_node_count, 0);
    assert!(!summary.has_slots);
    assert!(!summary.has_insert_contributions);
    assert!(summary.is_const_evaluable_shape);
}

#[test]
fn summary_fields_are_mutable() {
    let mut summary = TemplateIrSummary::empty();
    summary.estimated_output_bytes = 256;
    summary.text_node_count = 3;
    summary.text_byte_count = 128;
    summary.max_depth = 2;
    summary.has_slots = true;
    summary.insert_contribution_count = 1;
    summary.has_insert_contributions = true;
    summary.is_const_evaluable_shape = false;

    assert_eq!(summary.estimated_output_bytes, 256);
    assert_eq!(summary.text_node_count, 3);
    assert_eq!(summary.text_byte_count, 128);
    assert_eq!(summary.max_depth, 2);
    assert!(summary.has_slots);
    assert_eq!(summary.insert_contribution_count, 1);
    assert!(summary.has_insert_contributions);
    assert!(!summary.is_const_evaluable_shape);
}
