//! Validation tests.
//!
//! WHAT: exercises `validate_tir_store` with well-formed and malformed TIR stores
//! to confirm that structural invariants are enforced.
//! WHY: validation is the safety net that catches converter bugs before downstream
//! passes trust the TIR data.

use super::super::ids::TemplateIrNodeId;
use super::super::node::{TemplateIr, TemplateIrNode, TemplateIrNodeKind};
use super::super::store::TemplateIrStore;
use super::super::summary::TemplateIrSummary;
use super::super::validation::validate_tir_store;
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, ExpressionValueShape,
};
use crate::compiler_frontend::ast::expressions::expression_types::ConstRecordState;
use crate::compiler_frontend::ast::templates::template::{
    Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::TemplateBranchSelector;
use crate::compiler_frontend::ast::templates::tir::node::TemplateIrBranch;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;

fn empty_location() -> SourceLocation {
    SourceLocation::default()
}

fn bool_expression() -> Expression {
    Expression {
        kind: ExpressionKind::Bool(true),
        type_id: builtin_type_ids::BOOL,
        diagnostic_type: DataType::Bool,
        function_receiver: None,
        value_mode: ValueMode::ImmutableOwned,
        location: empty_location(),
        reactive_source: None,
        reactive_template: None,
        const_record_state: ConstRecordState::RuntimeValue,
        contains_regular_division: false,
        value_shape: ExpressionValueShape::Ordinary,
    }
}

// -------------------------
//  Well-Formed Store Tests
// -------------------------

#[test]
fn empty_store_is_valid() {
    let store = TemplateIrStore::new();
    assert!(validate_tir_store(&store).is_none());
}

#[test]
fn store_with_valid_template_is_valid() {
    let mut store = TemplateIrStore::new();
    let mut string_table = StringTable::new();

    let node_id = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Text {
            text: string_table.intern("test"),
            byte_len: 4,
            origin: TemplateSegmentOrigin::Body,
        },
        empty_location(),
    ));

    store.push_template(TemplateIr::new(
        node_id,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    ));

    assert!(validate_tir_store(&store).is_none());
}

#[test]
fn store_with_sequence_is_valid() {
    let mut store = TemplateIrStore::new();
    let mut string_table = StringTable::new();

    let child_a = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Text {
            text: string_table.intern("a"),
            byte_len: 1,
            origin: TemplateSegmentOrigin::Body,
        },
        empty_location(),
    ));

    let child_b = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Text {
            text: string_table.intern("b"),
            byte_len: 1,
            origin: TemplateSegmentOrigin::Body,
        },
        empty_location(),
    ));

    let sequence = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence {
            children: vec![child_a, child_b],
        },
        empty_location(),
    ));

    store.push_template(TemplateIr::new(
        sequence,
        Style::default(),
        TemplateType::StringFunction,
        TemplateIrSummary::default(),
        empty_location(),
    ));

    assert!(validate_tir_store(&store).is_none());
}

// -------------------------
//  Invalid Root Tests
// -------------------------

#[test]
fn template_with_out_of_bounds_root_is_invalid() {
    let mut store = TemplateIrStore::new();

    // Push a template with a root that points beyond the nodes vector.
    store.templates.push(TemplateIr::new(
        TemplateIrNodeId::new(99),
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    ));

    let diagnostic = validate_tir_store(&store);
    assert!(diagnostic.is_some());
    let msg = format!("{:?}", diagnostic.unwrap());
    assert!(msg.contains("out of bounds"));
}

// -------------------------
//  Invalid Node Reference Tests
// -------------------------

#[test]
fn sequence_with_out_of_bounds_child_is_invalid() {
    let mut store = TemplateIrStore::new();

    // Create a sequence that references a non-existent child.
    let sequence_id = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence {
            children: vec![TemplateIrNodeId::new(99)],
        },
        empty_location(),
    ));

    store.push_template(TemplateIr::new(
        sequence_id,
        Style::default(),
        TemplateType::StringFunction,
        TemplateIrSummary::default(),
        empty_location(),
    ));

    let diagnostic = validate_tir_store(&store);
    assert!(diagnostic.is_some());
    let msg = format!("{:?}", diagnostic.unwrap());
    assert!(msg.contains("out of bounds"));
}

#[test]
fn branch_chain_with_out_of_bounds_body_is_invalid() {
    let mut store = TemplateIrStore::new();

    let chain_id = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::BranchChain {
            branches: vec![TemplateIrBranch::new(
                TemplateBranchSelector::Bool(bool_expression()),
                TemplateIrNodeId::new(99),
                empty_location(),
            )],
            fallback: None,
        },
        empty_location(),
    ));

    store.push_template(TemplateIr::new(
        chain_id,
        Style::default(),
        TemplateType::StringFunction,
        TemplateIrSummary::default(),
        empty_location(),
    ));

    let diagnostic = validate_tir_store(&store);
    assert!(diagnostic.is_some());
}

#[test]
fn loop_with_out_of_bounds_body_is_invalid() {
    use crate::compiler_frontend::ast::templates::template_control_flow::TemplateLoopHeader;

    let mut store = TemplateIrStore::new();

    let loop_id = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Loop {
            header: TemplateLoopHeader::Conditional {
                condition: Box::new(bool_expression()),
            },
            body: TemplateIrNodeId::new(99),
            aggregate_wrapper: None,
            aggregate_render_plan: None,
        },
        empty_location(),
    ));

    store.push_template(TemplateIr::new(
        loop_id,
        Style::default(),
        TemplateType::StringFunction,
        TemplateIrSummary::default(),
        empty_location(),
    ));

    let diagnostic = validate_tir_store(&store);
    assert!(diagnostic.is_some());
}

// -------------------------
//  Converter + Validation Integration
// -------------------------

#[test]
fn converted_template_passes_validation() {
    use super::super::convert_from_template::convert_template_to_tir;
    use crate::compiler_frontend::ast::templates::template::{
        SlotKey, SlotPlaceholder, TemplateAtom, TemplateContent, TemplateSegment,
    };
    use crate::compiler_frontend::ast::templates::template_types::Template;

    let mut string_table = StringTable::new();

    // Build a template with mixed content.
    let content = TemplateContent {
        atoms: vec![
            TemplateAtom::Content(TemplateSegment::new(
                Expression {
                    kind: ExpressionKind::StringSlice(string_table.intern("hello")),
                    type_id: builtin_type_ids::STRING,
                    diagnostic_type: DataType::StringSlice,
                    function_receiver: None,
                    value_mode: ValueMode::ImmutableOwned,
                    location: empty_location(),
                    reactive_source: None,
                    reactive_template: None,
                    const_record_state: ConstRecordState::RuntimeValue,
                    contains_regular_division: false,
                    value_shape: ExpressionValueShape::PlainStringSlice,
                },
                TemplateSegmentOrigin::Body,
            )),
            TemplateAtom::Slot(SlotPlaceholder::new(SlotKey::Default)),
        ],
    };

    let mut template = Template::empty();
    template.content = content;
    template.location = empty_location();

    let mut store = TemplateIrStore::new();
    let _template_id = convert_template_to_tir(&template, &mut store, &string_table);

    // Converted store should pass validation.
    assert!(validate_tir_store(&store).is_none());
}
