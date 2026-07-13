//! Tests for the final TIR reference contract.
//!
//! WHAT: exercises store-qualified `TemplateTirReference` ownership, cloning
//!       and remapping of the live `Template` payload.
//! WHY: these handle invariants remain useful for AST-owned TIR payloads.

use super::super::ids::{TemplateIrId, TemplateIrNodeId};
use super::super::overlays::TemplateOverlaySetId;
use super::super::store::TemplateIrStore;
use super::super::{
    TemplateRef, TemplateTirBodyReference, TemplateTirPhase, TemplateTirReference,
    TemplateWrapperReference,
};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::template::{SlotKey, TemplateType};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchChain, TemplateBranchSelector, TemplateConditionalBranch, TemplateControlFlow,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;

use std::{path::Path, sync::Arc};

fn make_reference(template_id: TemplateIrId, store: &TemplateIrStore) -> TemplateTirReference {
    TemplateTirReference {
        root: TemplateRef::new(store.store_id(), template_id),
        store_owner: store.owner(),
        is_composed: false,
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
fn tir_reference_matches_correct_store_owner_and_rejects_wrong_owner() {
    let correct_store = TemplateIrStore::new();
    let wrong_store = TemplateIrStore::new();
    let reference = make_reference(TemplateIrId::new(0), &correct_store);

    assert!(Arc::ptr_eq(&reference.store_owner, &correct_store.owner()));
    assert!(!Arc::ptr_eq(&reference.store_owner, &wrong_store.owner()));
}

#[test]
fn template_clone_preserves_tir_reference() {
    let store = TemplateIrStore::new();
    let mut original = Template::empty();
    original.tir_reference = Some(make_reference(TemplateIrId::new(5), &store));

    let cloned = original.clone();

    let cloned_reference = cloned
        .tir_reference
        .expect("clone must preserve finalized TIR reference");
    assert_eq!(cloned_reference.root.template_id, TemplateIrId::new(5));
    assert!(Arc::ptr_eq(&cloned_reference.store_owner, &store.owner()));
}

#[test]
fn template_remap_updates_live_fields_and_preserves_tir_reference() {
    let mut source_table = StringTable::new();
    let source_location = SourceLocation::from_path(Path::new("path"), &mut source_table);
    let text = source_table.intern("text");
    let slot_name = source_table.intern("slot");

    let mut target_table = StringTable::new();
    target_table.intern("slot");
    target_table.intern("text");
    target_table.intern("path");

    let remap = target_table.merge_from(&source_table);
    assert!(!remap.is_identity());

    let store = TemplateIrStore::new();
    let mut template = Template::empty();
    template.id = "stable-template-id".to_owned();
    template.location = source_location.clone();
    template.kind = TemplateType::SlotDefinition(SlotKey::Named(slot_name));
    template.style.id = "test-style";
    template.style.skip_parent_child_wrappers = true;
    template.style.suppress_child_templates = true;
    template.control_flow = Some(TemplateControlFlow::BranchChain(Box::new(
        TemplateBranchChain {
            branches: vec![TemplateConditionalBranch {
                selector: TemplateBranchSelector::Bool(Expression::string_slice(
                    text,
                    source_location.clone(),
                    ValueMode::ImmutableOwned,
                )),
                body_tir_reference: TemplateTirBodyReference::with_store_local_identity(
                    &store,
                    TemplateIrNodeId::new(0),
                    TemplateTirPhase::Parsed,
                ),
                location: source_location.clone(),
            }],
            fallback: None,
            location: source_location.clone(),
        },
    )));

    template.child_wrappers.push(TemplateWrapperReference::new(
        TemplateRef::new(store.store_id(), TemplateIrId::new(4)),
        crate::compiler_frontend::ast::templates::tir::TemplateTirPhase::Parsed,
        TemplateOverlaySetId::empty_for_test(),
    ));
    template.tir_reference = Some(make_reference(TemplateIrId::new(3), &store));

    let before = template
        .tir_reference
        .clone()
        .expect("reference should exist before remap");

    template.remap_string_ids(&remap);

    assert_eq!(template.id, "stable-template-id");
    assert_eq!(
        template.location.scope.name_str(&target_table),
        Some("path")
    );
    assert!(matches!(
        template.kind,
        TemplateType::SlotDefinition(SlotKey::Named(id))
            if target_table.resolve(id) == "slot"
    ));
    assert_eq!(template.style.id, "test-style");
    assert!(template.style.skip_parent_child_wrappers);
    assert!(template.style.suppress_child_templates);

    let Some(TemplateControlFlow::BranchChain(branch_chain)) = &template.control_flow else {
        panic!("expected branch-chain control flow");
    };
    assert_eq!(
        branch_chain.location.scope.name_str(&target_table),
        Some("path")
    );
    let TemplateBranchSelector::Bool(condition) = &branch_chain.branches[0].selector else {
        panic!("expected boolean branch selector");
    };
    assert_eq!(condition.as_string(&target_table), "text");

    let child_wrapper = template
        .child_wrappers
        .first()
        .expect("child wrapper should remain attached");
    assert_eq!(child_wrapper.root.template_id, TemplateIrId::new(4));

    let after = template
        .tir_reference
        .expect("reference should exist after remap");

    assert_eq!(after.root, before.root);
    assert_eq!(after.phase, before.phase);
    assert_eq!(after.overlay_set_id, before.overlay_set_id);
    assert!(Arc::ptr_eq(&after.store_owner, &before.store_owner));
}
