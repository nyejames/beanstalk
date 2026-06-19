//! Converter parity tests.
//!
//! WHAT: exercises `convert_template_to_tir` with various template shapes and
//! asserts that the resulting TIR structure and summary match expectations.
//! WHY: the converter is the parity bridge between old `Template`-based
//! representation and the new TIR tree; these tests prove structural fidelity.

use super::super::convert_from_template::convert_template_to_tir;
use super::super::node::TemplateIrNodeKind;
use super::super::store::TemplateIrStore;
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, ExpressionRpn, ExpressionValueShape,
};
use crate::compiler_frontend::ast::expressions::expression_types::ConstRecordState;
use crate::compiler_frontend::ast::templates::template::{
    SlotKey, SlotPlaceholder, Style, TemplateAtom, TemplateContent, TemplateSegment,
    TemplateSegmentOrigin,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchChain, TemplateBranchSelector, TemplateConditionalBranch, TemplateControlFlow,
    TemplateFallbackBranch, TemplateLoopControlFlow, TemplateLoopControlKind,
    TemplateLoopControlSignal, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;

// -------------------------
//  Helpers
// -------------------------

fn empty_location() -> SourceLocation {
    SourceLocation::default()
}

fn string_expression(string_table: &mut StringTable, text: &str) -> Expression {
    let id = string_table.intern(text);
    Expression {
        kind: ExpressionKind::StringSlice(id),
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
    }
}

fn runtime_expression() -> Expression {
    Expression {
        kind: ExpressionKind::Runtime(ExpressionRpn { items: vec![] }),
        type_id: builtin_type_ids::STRING,
        diagnostic_type: DataType::StringSlice,
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

fn make_template(content: TemplateContent) -> Template {
    let mut template = Template::empty();
    template.content = content;
    template.location = empty_location();
    template
}

// -------------------------
//  Text Conversion Tests
// -------------------------

#[test]
fn convert_simple_text_template() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let content = TemplateContent {
        atoms: vec![TemplateAtom::Content(TemplateSegment::new(
            string_expression(&mut string_table, "hello"),
            TemplateSegmentOrigin::Body,
        ))],
    };

    let template = make_template(content);
    let template_id = convert_template_to_tir(&template, &mut store, &string_table);

    // Verify template was created.
    assert_eq!(store.template_count(), 1);
    let tir = store
        .get_template(template_id)
        .expect("template should exist");

    // Verify root node is a Text node.
    let root = store.get_node(tir.root).expect("root node should exist");
    match &root.kind {
        TemplateIrNodeKind::Text {
            text,
            byte_len,
            origin,
        } => {
            assert_eq!(string_table.resolve(*text), "hello");
            assert_eq!(*byte_len, 5);
            assert_eq!(*origin, TemplateSegmentOrigin::Body);
        }
        _ => panic!("expected Text node, got {:?}", root.kind),
    }

    // Verify summary.
    assert_eq!(tir.summary.text_node_count, 1);
    assert_eq!(tir.summary.text_byte_count, 5);
    assert_eq!(tir.summary.estimated_output_bytes, 5);
    assert_eq!(tir.summary.dynamic_expression_count, 0);
    assert!(tir.summary.is_const_evaluable_shape);
}

#[test]
fn convert_multi_text_template_creates_sequence() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let content = TemplateContent {
        atoms: vec![
            TemplateAtom::Content(TemplateSegment::new(
                string_expression(&mut string_table, "hello"),
                TemplateSegmentOrigin::Body,
            )),
            TemplateAtom::Content(TemplateSegment::new(
                string_expression(&mut string_table, " world"),
                TemplateSegmentOrigin::Body,
            )),
        ],
    };

    let template = make_template(content);
    let template_id = convert_template_to_tir(&template, &mut store, &string_table);

    let tir = store
        .get_template(template_id)
        .expect("template should exist");
    let root = store.get_node(tir.root).expect("root node should exist");

    // Multiple atoms should produce a Sequence node.
    match &root.kind {
        TemplateIrNodeKind::Sequence { children } => {
            assert_eq!(children.len(), 2);
        }
        _ => panic!("expected Sequence node, got {:?}", root.kind),
    }

    assert_eq!(tir.summary.text_node_count, 2);
    assert_eq!(tir.summary.text_byte_count, 11);
}

// -------------------------
//  Dynamic Expression Tests
// -------------------------

#[test]
fn convert_dynamic_expression_template() {
    let string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let content = TemplateContent {
        atoms: vec![TemplateAtom::Content(TemplateSegment::new(
            runtime_expression(),
            TemplateSegmentOrigin::Body,
        ))],
    };

    let template = make_template(content);
    let template_id = convert_template_to_tir(&template, &mut store, &string_table);

    let tir = store
        .get_template(template_id)
        .expect("template should exist");
    let root = store.get_node(tir.root).expect("root node should exist");

    match &root.kind {
        TemplateIrNodeKind::DynamicExpression { origin, .. } => {
            assert_eq!(*origin, TemplateSegmentOrigin::Body);
        }
        _ => panic!("expected DynamicExpression node, got {:?}", root.kind),
    }

    assert_eq!(tir.summary.dynamic_expression_count, 1);
    assert!(!tir.summary.is_const_evaluable_shape);
}

// -------------------------
//  Child Template Tests
// -------------------------

#[test]
fn convert_child_template_creates_child_reference() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    // Create a child template expression.
    let child_template = make_template(TemplateContent {
        atoms: vec![TemplateAtom::Content(TemplateSegment::new(
            string_expression(&mut string_table, "child"),
            TemplateSegmentOrigin::Body,
        ))],
    });

    let child_expr = Expression {
        kind: ExpressionKind::Template(Box::new(child_template)),
        type_id: builtin_type_ids::STRING,
        diagnostic_type: DataType::StringSlice,
        function_receiver: None,
        value_mode: ValueMode::ImmutableOwned,
        location: empty_location(),
        reactive_source: None,
        reactive_template: None,
        const_record_state: ConstRecordState::RuntimeValue,
        contains_regular_division: false,
        value_shape: ExpressionValueShape::Ordinary,
    };

    let content = TemplateContent {
        atoms: vec![TemplateAtom::Content(TemplateSegment::new(
            child_expr,
            TemplateSegmentOrigin::Body,
        ))],
    };

    let template = make_template(content);
    let template_id = convert_template_to_tir(&template, &mut store, &string_table);

    let tir = store
        .get_template(template_id)
        .expect("template should exist");
    let root = store.get_node(tir.root).expect("root node should exist");

    match &root.kind {
        TemplateIrNodeKind::ChildTemplate { template: child_id } => {
            // Verify the child template exists in the store.
            let child = store
                .get_template(*child_id)
                .expect("child template should exist");
            assert_eq!(child.summary.text_node_count, 1);
        }
        _ => panic!("expected ChildTemplate node, got {:?}", root.kind),
    }

    // Parent should have 1 child template reference and 2 total templates (parent + child).
    assert_eq!(tir.summary.child_template_count, 1);
    assert_eq!(store.template_count(), 2);
}

// -------------------------
//  Slot Placeholder Tests
// -------------------------

#[test]
fn convert_slot_placeholder() {
    let string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let content = TemplateContent {
        atoms: vec![TemplateAtom::Slot(SlotPlaceholder::new(SlotKey::Default))],
    };

    let template = make_template(content);
    let template_id = convert_template_to_tir(&template, &mut store, &string_table);

    let tir = store
        .get_template(template_id)
        .expect("template should exist");
    let root = store.get_node(tir.root).expect("root node should exist");

    match &root.kind {
        TemplateIrNodeKind::Slot { slot } => {
            assert_eq!(slot.key, SlotKey::Default);
        }
        _ => panic!("expected Slot node, got {:?}", root.kind),
    }

    assert_eq!(tir.summary.slot_count, 1);
    assert!(tir.summary.has_slots);
}

// -------------------------
//  Branch Chain Tests
// -------------------------

#[test]
fn convert_branch_chain() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let branch_content = TemplateContent {
        atoms: vec![TemplateAtom::Content(TemplateSegment::new(
            string_expression(&mut string_table, "branch"),
            TemplateSegmentOrigin::Body,
        ))],
    };

    let fallback_content = TemplateContent {
        atoms: vec![TemplateAtom::Content(TemplateSegment::new(
            string_expression(&mut string_table, "fallback"),
            TemplateSegmentOrigin::Body,
        ))],
    };

    let branch_chain = TemplateBranchChain {
        branches: vec![TemplateConditionalBranch {
            selector: TemplateBranchSelector::Bool(runtime_expression()),
            content: branch_content,
            render_plan: None,
            location: empty_location(),
        }],
        fallback: Some(TemplateFallbackBranch {
            content: fallback_content,
            render_plan: None,
            location: empty_location(),
        }),
        location: empty_location(),
    };

    let mut template = make_template(TemplateContent::default());
    template.control_flow = Some(TemplateControlFlow::BranchChain(Box::new(branch_chain)));

    let template_id = convert_template_to_tir(&template, &mut store, &string_table);

    let tir = store
        .get_template(template_id)
        .expect("template should exist");
    let root = store.get_node(tir.root).expect("root node should exist");

    match &root.kind {
        TemplateIrNodeKind::BranchChain {
            branches, fallback, ..
        } => {
            assert_eq!(branches.len(), 1);
            assert!(fallback.is_some());
        }
        _ => panic!("expected BranchChain node, got {:?}", root.kind),
    }

    assert!(tir.summary.has_control_flow);
    assert!(!tir.summary.is_const_evaluable_shape);
}

// -------------------------
//  Loop Tests
// -------------------------

#[test]
fn convert_loop() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let body_content = TemplateContent {
        atoms: vec![TemplateAtom::Content(TemplateSegment::new(
            string_expression(&mut string_table, "item"),
            TemplateSegmentOrigin::Body,
        ))],
    };

    let loop_cf = TemplateLoopControlFlow {
        header: TemplateLoopHeader::Conditional {
            condition: Box::new(runtime_expression()),
        },
        body_content,
        body_render_plan: None,
        aggregate_render_plan: None,
        location: empty_location(),
    };

    let mut template = make_template(TemplateContent::default());
    template.control_flow = Some(TemplateControlFlow::Loop(Box::new(loop_cf)));

    let template_id = convert_template_to_tir(&template, &mut store, &string_table);

    let tir = store
        .get_template(template_id)
        .expect("template should exist");
    let root = store.get_node(tir.root).expect("root node should exist");

    match &root.kind {
        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            // Body should exist.
            let _body_node = store.get_node(*body).expect("loop body node should exist");
            // No aggregate wrapper in this test.
            assert!(aggregate_wrapper.is_none());
        }
        _ => panic!("expected Loop node, got {:?}", root.kind),
    }

    assert!(tir.summary.has_control_flow);
}

// -------------------------
//  Loop Control Tests
// -------------------------

#[test]
fn convert_loop_control_break() {
    let string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let mut template = make_template(TemplateContent::default());
    template.control_flow = Some(TemplateControlFlow::LoopControl(
        TemplateLoopControlSignal {
            kind: TemplateLoopControlKind::Break,
            location: empty_location(),
        },
    ));

    let template_id = convert_template_to_tir(&template, &mut store, &string_table);

    let tir = store
        .get_template(template_id)
        .expect("template should exist");
    let root = store.get_node(tir.root).expect("root node should exist");

    match &root.kind {
        TemplateIrNodeKind::LoopControl { kind } => {
            assert_eq!(*kind, TemplateLoopControlKind::Break);
        }
        _ => panic!("expected LoopControl node, got {:?}", root.kind),
    }

    assert!(tir.summary.has_control_flow);
}

#[test]
fn convert_loop_control_continue() {
    let string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let mut template = make_template(TemplateContent::default());
    template.control_flow = Some(TemplateControlFlow::LoopControl(
        TemplateLoopControlSignal {
            kind: TemplateLoopControlKind::Continue,
            location: empty_location(),
        },
    ));

    let template_id = convert_template_to_tir(&template, &mut store, &string_table);

    let tir = store
        .get_template(template_id)
        .expect("template should exist");
    let root = store.get_node(tir.root).expect("root node should exist");

    match &root.kind {
        TemplateIrNodeKind::LoopControl { kind } => {
            assert_eq!(*kind, TemplateLoopControlKind::Continue);
        }
        _ => panic!("expected LoopControl node, got {:?}", root.kind),
    }

    assert!(tir.summary.has_control_flow);
}

// -------------------------
//  Summary Invariants Tests
// -------------------------

#[test]
fn empty_template_produces_empty_summary() {
    let string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let template = make_template(TemplateContent::default());
    let template_id = convert_template_to_tir(&template, &mut store, &string_table);

    let tir = store
        .get_template(template_id)
        .expect("template should exist");

    assert_eq!(tir.summary.text_node_count, 0);
    assert_eq!(tir.summary.text_byte_count, 0);
    assert_eq!(tir.summary.dynamic_expression_count, 0);
    assert_eq!(tir.summary.child_template_count, 0);
    assert_eq!(tir.summary.slot_count, 0);
    assert_eq!(tir.summary.max_depth, 0);
    assert!(!tir.summary.has_slots);
    assert!(!tir.summary.has_control_flow);
    assert!(!tir.summary.has_reactivity);
    assert!(tir.summary.is_const_evaluable_shape);
}

#[test]
fn formatter_presence_is_detected() {
    let string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let mut template = make_template(TemplateContent::default());
    // Style has formatter: None by default, so has_formatter should be false.
    template.style = Style {
        id: "markdown",
        ..Style::default()
    };
    let template_id = convert_template_to_tir(&template, &mut store, &string_table);

    let tir = store
        .get_template(template_id)
        .expect("template should exist");
    // Style has formatter: None, so has_formatter should be false.
    assert!(!tir.summary.has_formatter);
}

// -------------------------
//  Multiple Templates Conversion
// -------------------------

#[test]
fn convert_multiple_templates_returns_sequential_ids() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let content_a = TemplateContent {
        atoms: vec![TemplateAtom::Content(TemplateSegment::new(
            string_expression(&mut string_table, "first"),
            TemplateSegmentOrigin::Body,
        ))],
    };
    let content_b = TemplateContent {
        atoms: vec![TemplateAtom::Content(TemplateSegment::new(
            string_expression(&mut string_table, "second"),
            TemplateSegmentOrigin::Body,
        ))],
    };

    let id_a = convert_template_to_tir(&make_template(content_a), &mut store, &string_table);
    let id_b = convert_template_to_tir(&make_template(content_b), &mut store, &string_table);

    assert_eq!(id_a.index(), 0);
    assert_eq!(id_b.index(), 1);
    assert_eq!(store.template_count(), 2);
}
