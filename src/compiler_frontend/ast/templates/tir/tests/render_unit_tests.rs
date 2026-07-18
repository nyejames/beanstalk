//! TIR render-unit construction tests.
//
// WHAT: exercises same-store wrapper-reference normalization, aggregate-wrapper
// candidate construction, and required render-unit node authority.
// WHY: these focused tests protect the current module-local TIR owner and
// render-unit identity invariants.

use crate::compiler_frontend::ast::templates::template::{
    Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::tir::ids::{TemplateIrId, TemplateIrNodeId};
use crate::compiler_frontend::ast::templates::tir::node::{
    TemplateIr, TemplateIrNode, TemplateIrNodeKind,
};
use crate::compiler_frontend::ast::templates::tir::overlays::{
    TemplateOverlaySet, TemplateOverlaySetId,
};
use crate::compiler_frontend::ast::templates::tir::refs::{
    TemplateTirChildReference, TemplateTirReference,
};
use crate::compiler_frontend::ast::templates::tir::render_unit::{
    build_aggregate_wrapper_candidate_from_tir_nodes, build_branch_body_candidate_from_tir_nodes,
};
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::ast::templates::tir::summary::TemplateIrSummary;
use crate::compiler_frontend::ast::templates::tir::view::TemplateTirPhase;
use crate::compiler_frontend::ast::templates::tir::wrapper_sets::wrapper_reference_for_template;
use crate::compiler_frontend::ast::templates::tir::{
    head_prefix_tir_nodes, sequence_children, trim_whitespace_before_loop_control_boundary,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

fn empty_location() -> SourceLocation {
    SourceLocation::default()
}

fn push_text_node(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
    text: &str,
) -> TemplateIrNodeId {
    let text_id = string_table.intern(text);
    store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Text {
            text: text_id,
            byte_len: text.len() as u32,
            origin: TemplateSegmentOrigin::Body,
        },
        empty_location(),
    ))
}

fn push_template_entry(
    store: &mut TemplateIrStore,
    root: TemplateIrNodeId,
    kind: TemplateType,
) -> TemplateIrId {
    store.push_template(TemplateIr::new(
        root,
        Style::default(),
        kind,
        TemplateIrSummary::default(),
        empty_location(),
    ))
}

fn candidate_children(store: &TemplateIrStore, template_id: TemplateIrId) -> Vec<TemplateIrNodeId> {
    let template = store
        .get_template(template_id)
        .expect("candidate template should exist");
    let TemplateIrNodeKind::Sequence { children } = &store
        .get_node(template.root)
        .expect("candidate root should exist")
        .kind
    else {
        panic!("candidate root should be a sequence");
    };

    children.to_owned()
}

#[test]
fn same_store_wrapper_reference_is_normalized_without_materialization() {
    let mut store = TemplateIrStore::new();
    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence { children: vec![] },
        empty_location(),
    ));
    let template_id = push_template_entry(&mut store, root, TemplateType::String);
    let overlay_set_id = store.allocate_overlay_set(TemplateOverlaySet::empty());

    let template = crate::compiler_frontend::ast::templates::template::Template {
        kind: TemplateType::String,
        tir_reference: TemplateTirReference {
            root: template_id,
            phase: TemplateTirPhase::Parsed,
            overlay_set_id,
        },
        location: empty_location(),
    };

    let reference = wrapper_reference_for_template(&template, &store)
        .expect("same-store wrapper should normalize");

    assert_eq!(reference.root, template_id);
    assert_eq!(reference.phase, TemplateTirPhase::Parsed);
    assert_eq!(reference.overlay_set_id, overlay_set_id);
}

#[test]
fn wrapper_with_missing_overlay_set_returns_error() {
    let mut store = TemplateIrStore::new();
    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence { children: vec![] },
        empty_location(),
    ));
    let template_id = push_template_entry(&mut store, root, TemplateType::String);
    let template = crate::compiler_frontend::ast::templates::template::Template {
        kind: TemplateType::String,
        tir_reference: TemplateTirReference {
            root: template_id,
            phase: TemplateTirPhase::Parsed,
            overlay_set_id: TemplateOverlaySetId::new(999),
        },
        location: empty_location(),
    };

    assert!(wrapper_reference_for_template(&template, &store).is_err());
}

#[test]
fn wrapper_with_missing_template_returns_error() {
    let store = TemplateIrStore::new();
    let template = crate::compiler_frontend::ast::templates::template::Template {
        kind: TemplateType::String,
        tir_reference: TemplateTirReference {
            root: TemplateIrId::new(99),
            phase: TemplateTirPhase::Parsed,
            overlay_set_id: TemplateOverlaySetId::empty(),
        },
        location: empty_location(),
    };

    assert!(wrapper_reference_for_template(&template, &store).is_err());
}

#[test]
fn wrapper_candidates_reuse_parser_structural_child_template() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let overlay_set_id = store.allocate_overlay_set(TemplateOverlaySet::empty());

    let child_root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence { children: vec![] },
        empty_location(),
    ));
    let child_template_id = push_template_entry(&mut store, child_root, TemplateType::String);
    let parser_reference = TemplateTirChildReference::new(
        child_template_id,
        TemplateTirPhase::Finalized,
        overlay_set_id,
    );
    let parser_occurrence_id = store.next_child_template_occurrence_id();
    let parser_child_node = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::ChildTemplate {
            reference: parser_reference,
            occurrence_id: parser_occurrence_id,
        },
        empty_location(),
    ));
    let body_node = push_text_node(&mut store, &mut string_table, "body");

    let aggregate_template_id =
        build_aggregate_wrapper_candidate_from_tir_nodes(&[parser_child_node], &mut store)
            .expect("aggregate candidate should reuse parser structural child");
    let aggregate_children = candidate_children(&store, aggregate_template_id);
    assert_eq!(aggregate_children[0], parser_child_node);
    assert!(matches!(
        store.get_node(aggregate_children[0]).expect("aggregate child should exist").kind,
        TemplateIrNodeKind::ChildTemplate {
            reference,
            occurrence_id,
        } if reference == parser_reference && occurrence_id == parser_occurrence_id
    ));
    assert!(matches!(
        store
            .get_node(
                *aggregate_children
                    .last()
                    .expect("aggregate marker should exist")
            )
            .expect("aggregate marker should exist")
            .kind,
        TemplateIrNodeKind::AggregateOutput
    ));

    let branch_template_id =
        build_branch_body_candidate_from_tir_nodes(&[parser_child_node], &[body_node], &mut store)
            .expect("branch candidate should reuse parser structural child");
    let branch_children = candidate_children(&store, branch_template_id);
    assert_eq!(branch_children, vec![parser_child_node, body_node]);
}

#[test]
fn sequence_children_rejects_missing_node() {
    let store = TemplateIrStore::new();
    assert!(sequence_children(&store, TemplateIrNodeId::new(99)).is_err());
}

#[test]
fn sequence_children_rejects_non_sequence_root() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let text_node = push_text_node(&mut store, &mut string_table, "leaf");

    assert!(sequence_children(&store, text_node).is_err());
}

#[test]
fn head_prefix_tir_nodes_rejects_missing_root_child() {
    let store = TemplateIrStore::new();
    assert!(head_prefix_tir_nodes(&store, &[TemplateIrNodeId::new(99)]).is_err());
}

#[test]
fn head_prefix_tir_nodes_accepts_empty_prefix() {
    let store = TemplateIrStore::new();
    assert!(head_prefix_tir_nodes(&store, &[]).unwrap().is_empty());
}

#[test]
fn trim_whitespace_rejects_missing_body_root() {
    let string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    assert!(
        trim_whitespace_before_loop_control_boundary(
            TemplateIrNodeId::new(99),
            &mut store,
            &string_table,
        )
        .is_err()
    );
}

#[test]
fn trim_whitespace_rejects_non_sequence_body_root() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let text_node = push_text_node(&mut store, &mut string_table, "leaf");

    assert!(
        trim_whitespace_before_loop_control_boundary(text_node, &mut store, &string_table).is_err()
    );
}

#[test]
fn trim_whitespace_rejects_missing_child_in_sequence() {
    let string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let body_root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence {
            children: vec![TemplateIrNodeId::new(99)],
        },
        empty_location(),
    ));

    assert!(
        trim_whitespace_before_loop_control_boundary(body_root, &mut store, &string_table).is_err()
    );
}
