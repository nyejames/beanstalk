use super::super::node::{TemplateIr, TemplateIrNode, TemplateIrNodeKind};
use super::super::store::TemplateIrStore;
use super::super::summary::TemplateIrSummary;
use crate::compiler_frontend::ast::templates::template::{
    Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

fn empty_location() -> SourceLocation {
    SourceLocation::default()
}

#[test]
fn store_starts_empty() {
    let store = TemplateIrStore::new();
    assert_eq!(store.template_count(), 0);
    assert_eq!(store.node_count(), 0);
}

#[test]
fn push_template_returns_sequential_ids() {
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

    let id_a = store.push_template(TemplateIr::new(
        node_id,
        Style::default(),
        TemplateType::StringFunction,
        TemplateIrSummary::default(),
        empty_location(),
    ));
    let id_b = store.push_template(TemplateIr::new(
        node_id,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    ));

    assert_eq!(id_a.index(), 0);
    assert_eq!(id_b.index(), 1);
    assert_eq!(store.template_count(), 2);
}

#[test]
fn push_node_returns_sequential_ids() {
    let mut store = TemplateIrStore::new();
    let mut string_table = StringTable::new();

    let id_a = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Text {
            text: string_table.intern("abc"),
            byte_len: 3,
            origin: TemplateSegmentOrigin::Body,
        },
        empty_location(),
    ));
    let id_b = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence { children: vec![] },
        empty_location(),
    ));

    assert_eq!(id_a.index(), 0);
    assert_eq!(id_b.index(), 1);
    assert_eq!(store.node_count(), 2);
}

#[test]
fn get_template_returns_stored_entry() {
    let mut store = TemplateIrStore::new();
    let mut string_table = StringTable::new();

    let node_id = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Text {
            text: string_table.intern(""),
            byte_len: 0,
            origin: TemplateSegmentOrigin::Body,
        },
        empty_location(),
    ));

    let id = store.push_template(TemplateIr::new(
        node_id,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    ));

    let retrieved = store.get_template(id).expect("template should exist");
    assert_eq!(retrieved.root, node_id);
}

#[test]
fn get_node_returns_stored_entry() {
    let mut store = TemplateIrStore::new();

    let id = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence { children: vec![] },
        empty_location(),
    ));

    let retrieved = store.get_node(id).expect("node should exist");
    assert!(matches!(
        retrieved.kind,
        TemplateIrNodeKind::Sequence { .. }
    ));
}

#[test]
fn out_of_bounds_lookup_returns_none() {
    let store = TemplateIrStore::new();
    assert!(
        store
            .get_template(super::super::ids::TemplateIrId::new(99))
            .is_none()
    );
    assert!(
        store
            .get_node(super::super::ids::TemplateIrNodeId::new(99))
            .is_none()
    );
}
