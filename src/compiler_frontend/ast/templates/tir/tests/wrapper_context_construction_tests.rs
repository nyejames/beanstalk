//! TIR wrapper-context overlay construction tests.
//!
//! WHAT: exercises the TIR-owned `attach_wrapper_context_overlay` operation
//!       that records inherited wrapper sets and `$fresh` suppression for
//!       child-template occurrences on a template's structural root.
//!
//! WHY: this operation moved from `create_template_node.rs` into the TIR
//!      wrapper owner. The formerly local implementation silently skipped
//!      foreign child references and missing authority. These tests protect the
//!      registry-aware resolution and the now-propagated internal-error
//!      invariants that integration output cannot expose.

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::template::{
    SlotKey, Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::TemplateBranchSelector;
use crate::compiler_frontend::ast::templates::tir::builder::TemplateIrBuilder;
use crate::compiler_frontend::ast::templates::tir::ids::TemplateIrId;
use crate::compiler_frontend::ast::templates::tir::node::TemplateIrBranch;
use crate::compiler_frontend::ast::templates::tir::overlays::{
    TemplateOverlaySet, TemplateOverlaySetId, TirWrapperApplicationMode,
};
use crate::compiler_frontend::ast::templates::tir::parser_builder_state::TemplateTirReference;
use crate::compiler_frontend::ast::templates::tir::refs::{
    TemplateRef, TemplateTirChildReference, TemplateWrapperReference,
};
use crate::compiler_frontend::ast::templates::tir::registry::{
    RegisteredTemplateIrStore, TemplateIrRegistry,
};
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::ast::templates::tir::summary::TemplateIrSummary;
use crate::compiler_frontend::ast::templates::tir::view::TemplateTirPhase;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;
use std::cell::RefCell;
use std::rc::Rc;

use super::attach_wrapper_context_overlay;

fn empty_location() -> SourceLocation {
    SourceLocation::default()
}

fn build_text_template(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
    text: &str,
) -> TemplateIrId {
    let text_id = string_table.intern(text);
    let mut builder = TemplateIrBuilder::new(store);
    let text_node = builder.push_text_node(
        text_id,
        text.len() as u32,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );
    let root = builder.push_sequence_node(vec![text_node], empty_location());

    builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        empty_location(),
    )
}

/// Builds a text template whose style marks `$fresh` suppression.
fn build_fresh_text_template(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
    text: &str,
) -> TemplateIrId {
    let text_id = string_table.intern(text);
    let mut builder = TemplateIrBuilder::new(store);
    let text_node = builder.push_text_node(
        text_id,
        text.len() as u32,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );
    let root = builder.push_sequence_node(vec![text_node], empty_location());

    let fresh_style = Style {
        skip_parent_child_wrappers: true,
        ..Style::default()
    };

    builder.finish_template(
        root,
        fresh_style,
        TemplateType::String,
        TemplateIrSummary::empty(),
        empty_location(),
    )
}

/// Builds a template with a false-no-else branch chain so its summary carries
/// `has_control_flow = true`.
fn build_control_flow_template(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
) -> TemplateIrId {
    let body_text = string_table.intern("hidden");
    let mut builder = TemplateIrBuilder::new(store);
    let body_node = builder.push_text_node(
        body_text,
        "hidden".len() as u32,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );
    let branch = TemplateIrBranch::new(
        TemplateBranchSelector::Bool(Expression::bool(
            false,
            empty_location(),
            ValueMode::ImmutableOwned,
        )),
        body_node,
        empty_location(),
    );
    let root = builder.push_branch_chain_node(vec![branch], None, empty_location());

    builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary {
            has_control_flow: true,
            ..TemplateIrSummary::empty()
        },
        empty_location(),
    )
}

/// Builds a slot wrapper template (before / $slot / after) finalized in the
/// current store, returning both its template ID and a wrapper reference.
fn build_wrapper_template(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
    before: &str,
    after: &str,
) -> (TemplateIrId, TemplateWrapperReference) {
    let before_id = string_table.intern(before);
    let after_id = string_table.intern(after);
    let mut builder = TemplateIrBuilder::new(store);
    let before_node = builder.push_text_node(
        before_id,
        before.len() as u32,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );
    let slot_node = builder.push_slot_node(SlotKey::Default, empty_location());
    let after_node = builder.push_text_node(
        after_id,
        after.len() as u32,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );
    let root =
        builder.push_sequence_node(vec![before_node, slot_node, after_node], empty_location());

    let wrapper_id = builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        empty_location(),
    );

    let wrapper_ref = TemplateWrapperReference::new(
        store.qualify_template_ref(wrapper_id),
        TemplateTirPhase::Finalized,
        TemplateOverlaySetId::empty(),
    );

    (wrapper_id, wrapper_ref)
}

/// Builds a parent template in the current store containing one child-template
/// occurrence referencing `child_reference`.
fn build_parent_with_child(
    store: &mut TemplateIrStore,
    child_reference: TemplateTirChildReference,
) -> TemplateIrId {
    let mut builder = TemplateIrBuilder::new(store);
    let child_node =
        builder.push_child_template_node_with_reference(child_reference, empty_location());
    let root = builder.push_sequence_node(vec![child_node], empty_location());

    builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        empty_location(),
    )
}

/// Creates a `TemplateTirReference` for `template_id` in the registered store,
/// at `Composed` phase with the empty overlay set.
fn make_tir_reference(
    registered_store: &RegisteredTemplateIrStore,
    template_id: TemplateIrId,
    overlay_set_id: TemplateOverlaySetId,
) -> TemplateTirReference {
    let store = registered_store.store().borrow();
    TemplateTirReference {
        root: TemplateRef::new(registered_store.store_id(), template_id),
        store_owner: store.owner(),
        phase: TemplateTirPhase::Composed,
        overlay_set_id,
    }
}

// -------------------------
//  Registry-aware foreign child resolution
// -------------------------

#[test]
fn attach_wrapper_context_resolves_foreign_fresh_child() {
    let mut string_table = StringTable::new();
    let registry = Rc::new(RefCell::new(TemplateIrRegistry::new()));
    let registered_store = RegisteredTemplateIrStore::allocate_in(Rc::clone(&registry));

    let empty_overlay_set_id = registry
        .borrow_mut()
        .allocate_overlay_set(TemplateOverlaySet::empty());

    // Build the wrapper template in the current store.
    let wrapper_ref = {
        let mut store = registered_store.store().borrow_mut();
        let (_wrapper_id, wrapper_ref) =
            build_wrapper_template(&mut store, &mut string_table, "before", "after");
        wrapper_ref
    };

    // Build a `$fresh` child template in a foreign store.
    let foreign_store_id = registry.borrow_mut().allocate_store();
    let foreign_child_id = {
        let registry_borrow = registry.borrow_mut();
        let mut foreign_store = registry_borrow
            .store_mut(foreign_store_id)
            .expect("foreign store should exist");
        build_fresh_text_template(&mut foreign_store, &mut string_table, "child")
    };

    // Build the parent in the current store with a foreign child reference.
    let parent_id = {
        let mut store = registered_store.store().borrow_mut();
        let child_reference = TemplateTirChildReference::new(
            TemplateRef::new(foreign_store_id, foreign_child_id),
            TemplateTirPhase::Composed,
            empty_overlay_set_id,
        );
        build_parent_with_child(&mut store, child_reference)
    };

    let mut tir_reference = make_tir_reference(&registered_store, parent_id, empty_overlay_set_id);

    attach_wrapper_context_overlay(&mut tir_reference, &[wrapper_ref], &registered_store)
        .expect("foreign $fresh child should resolve through the registry");

    // The overlay set should have changed from the empty set.
    assert_ne!(
        tir_reference.overlay_set_id, empty_overlay_set_id,
        "wrapper-context overlay should have been attached"
    );

    // Verify the wrapper context overlay records $fresh suppression for the
    // foreign child occurrence.
    let registry_borrow = registry.borrow();
    let overlay_set = registry_borrow
        .overlay_set(tir_reference.overlay_set_id)
        .expect("composed overlay set should exist");
    let wrapper_overlay_id = overlay_set
        .wrapper_context
        .expect("wrapper-context overlay should be set");
    let wrapper_overlay = registry_borrow
        .wrapper_context_overlay(wrapper_overlay_id)
        .expect("wrapper-context overlay entry should exist");

    assert_eq!(
        wrapper_overlay.contexts.len(),
        1,
        "one child occurrence should have context"
    );
    let (_, context) = &wrapper_overlay.contexts[0];
    assert!(
        context.skip_parent_child_wrappers,
        "foreign $fresh child should record suppression"
    );
    assert!(
        context.inherited_wrapper_set.is_none(),
        "$fresh child should not inherit wrappers"
    );
}

#[test]
fn attach_wrapper_context_resolves_foreign_control_flow_child_as_if_child_emits() {
    let mut string_table = StringTable::new();
    let registry = Rc::new(RefCell::new(TemplateIrRegistry::new()));
    let registered_store = RegisteredTemplateIrStore::allocate_in(Rc::clone(&registry));

    let empty_overlay_set_id = registry
        .borrow_mut()
        .allocate_overlay_set(TemplateOverlaySet::empty());

    let wrapper_ref = {
        let mut store = registered_store.store().borrow_mut();
        let (_wrapper_id, wrapper_ref) =
            build_wrapper_template(&mut store, &mut string_table, "before", "after");
        wrapper_ref
    };

    // Build a control-flow child in a foreign store.
    let foreign_store_id = registry.borrow_mut().allocate_store();
    let foreign_child_id = {
        let registry_borrow = registry.borrow_mut();
        let mut foreign_store = registry_borrow
            .store_mut(foreign_store_id)
            .expect("foreign store should exist");
        build_control_flow_template(&mut foreign_store, &mut string_table)
    };

    let parent_id = {
        let mut store = registered_store.store().borrow_mut();
        let child_reference = TemplateTirChildReference::new(
            TemplateRef::new(foreign_store_id, foreign_child_id),
            TemplateTirPhase::Composed,
            empty_overlay_set_id,
        );
        build_parent_with_child(&mut store, child_reference)
    };

    let mut tir_reference = make_tir_reference(&registered_store, parent_id, empty_overlay_set_id);

    attach_wrapper_context_overlay(&mut tir_reference, &[wrapper_ref], &registered_store)
        .expect("foreign control-flow child should resolve through the registry");

    let registry_borrow = registry.borrow();
    let overlay_set = registry_borrow
        .overlay_set(tir_reference.overlay_set_id)
        .expect("composed overlay set should exist");
    let wrapper_overlay_id = overlay_set
        .wrapper_context
        .expect("wrapper-context overlay should be set");
    let wrapper_overlay = registry_borrow
        .wrapper_context_overlay(wrapper_overlay_id)
        .expect("wrapper-context overlay entry should exist");

    assert_eq!(wrapper_overlay.contexts.len(), 1);
    let (_, context) = &wrapper_overlay.contexts[0];
    assert!(
        !context.skip_parent_child_wrappers,
        "non-fresh foreign child should not suppress wrappers"
    );
    assert!(
        context.inherited_wrapper_set.is_some(),
        "non-fresh foreign child should inherit wrappers"
    );
    assert!(
        matches!(
            context.application_mode,
            TirWrapperApplicationMode::IfChildEmits
        ),
        "foreign control-flow child should use IfChildEmits"
    );
}

// -------------------------
//  Formerly silent failure propagation
// -------------------------

#[test]
fn attach_wrapper_context_errors_on_missing_owning_template() {
    let registry = Rc::new(RefCell::new(TemplateIrRegistry::new()));
    let registered_store = RegisteredTemplateIrStore::allocate_in(Rc::clone(&registry));

    let empty_overlay_set_id = registry
        .borrow_mut()
        .allocate_overlay_set(TemplateOverlaySet::empty());

    // Reference a template ID that was never allocated.
    let mut tir_reference = make_tir_reference(
        &registered_store,
        TemplateIrId::new(999),
        empty_overlay_set_id,
    );

    let result = attach_wrapper_context_overlay(
        &mut tir_reference,
        &[TemplateWrapperReference::new(
            TemplateRef::new(registered_store.store_id(), TemplateIrId::new(0)),
            TemplateTirPhase::Finalized,
            empty_overlay_set_id,
        )],
        &registered_store,
    );

    assert!(
        result.is_err(),
        "missing owning template should return an internal error"
    );
    let error = result.unwrap_err();
    assert!(
        error.msg.contains("owning template"),
        "error should mention the missing owning template, got: {}",
        error.msg
    );
}

#[test]
fn attach_wrapper_context_errors_on_missing_child_template() {
    let mut string_table = StringTable::new();
    let registry = Rc::new(RefCell::new(TemplateIrRegistry::new()));
    let registered_store = RegisteredTemplateIrStore::allocate_in(Rc::clone(&registry));

    let empty_overlay_set_id = registry
        .borrow_mut()
        .allocate_overlay_set(TemplateOverlaySet::empty());

    let wrapper_ref = {
        let mut store = registered_store.store().borrow_mut();
        let (_wrapper_id, wrapper_ref) =
            build_wrapper_template(&mut store, &mut string_table, "before", "after");
        wrapper_ref
    };

    // Build a parent with a same-store child reference to a non-existent
    // template ID. The former local implementation silently skipped this.
    let parent_id = {
        let mut store = registered_store.store().borrow_mut();
        let child_reference = TemplateTirChildReference::new(
            TemplateRef::new(registered_store.store_id(), TemplateIrId::new(999)),
            TemplateTirPhase::Composed,
            empty_overlay_set_id,
        );
        build_parent_with_child(&mut store, child_reference)
    };

    let mut tir_reference = make_tir_reference(&registered_store, parent_id, empty_overlay_set_id);

    let result =
        attach_wrapper_context_overlay(&mut tir_reference, &[wrapper_ref], &registered_store);

    assert!(
        result.is_err(),
        "missing child template should return an internal error, not silently skip"
    );
    let error = result.unwrap_err();
    assert!(
        error.msg.contains("child template"),
        "error should mention the missing child template, got: {}",
        error.msg
    );
}

#[test]
fn attach_wrapper_context_no_contexts_leaves_overlay_set_unchanged() {
    let mut string_table = StringTable::new();
    let registry = Rc::new(RefCell::new(TemplateIrRegistry::new()));
    let registered_store = RegisteredTemplateIrStore::allocate_in(Rc::clone(&registry));

    let empty_overlay_set_id = registry
        .borrow_mut()
        .allocate_overlay_set(TemplateOverlaySet::empty());

    // Build a parent with no child-template occurrences.
    let parent_id = {
        let mut store = registered_store.store().borrow_mut();
        build_text_template(&mut store, &mut string_table, "plain text")
    };

    let mut tir_reference = make_tir_reference(&registered_store, parent_id, empty_overlay_set_id);
    let original_overlay_set_id = tir_reference.overlay_set_id;

    attach_wrapper_context_overlay(
        &mut tir_reference,
        &[TemplateWrapperReference::new(
            TemplateRef::new(registered_store.store_id(), TemplateIrId::new(0)),
            TemplateTirPhase::Finalized,
            empty_overlay_set_id,
        )],
        &registered_store,
    )
    .expect("template with no child occurrences should succeed");

    assert_eq!(
        tir_reference.overlay_set_id, original_overlay_set_id,
        "no contexts means no wrapper overlay should be attached"
    );
}

#[test]
fn attach_wrapper_context_rejects_wrong_registered_store_id() {
    let mut string_table = StringTable::new();
    let registry = Rc::new(RefCell::new(TemplateIrRegistry::new()));
    let registered_store = RegisteredTemplateIrStore::allocate_in(Rc::clone(&registry));
    let foreign_store_id = registry.borrow_mut().allocate_store();
    let empty_overlay_set_id = registry
        .borrow_mut()
        .allocate_overlay_set(TemplateOverlaySet::empty());

    let parent_id = {
        let mut store = registered_store.store().borrow_mut();
        build_text_template(&mut store, &mut string_table, "plain text")
    };
    let mut tir_reference = make_tir_reference(&registered_store, parent_id, empty_overlay_set_id);
    tir_reference.root = TemplateRef::new(foreign_store_id, parent_id);

    let error = attach_wrapper_context_overlay(&mut tir_reference, &[], &registered_store)
        .expect_err("store-qualified identity mismatch should fail");

    assert!(error.msg.contains("different registered store"));
}

#[test]
fn attach_wrapper_context_validates_current_overlay_before_allocating_wrapper_set() {
    let mut string_table = StringTable::new();
    let registry = Rc::new(RefCell::new(TemplateIrRegistry::new()));
    let registered_store = RegisteredTemplateIrStore::allocate_in(Rc::clone(&registry));
    let empty_overlay_set_id = registry
        .borrow_mut()
        .allocate_overlay_set(TemplateOverlaySet::empty());

    let (parent_id, wrapper_ref) = {
        let mut store = registered_store.store().borrow_mut();
        let child_id = build_text_template(&mut store, &mut string_table, "child");
        let child_reference = TemplateTirChildReference::new(
            TemplateRef::new(registered_store.store_id(), child_id),
            TemplateTirPhase::Composed,
            empty_overlay_set_id,
        );
        let parent_id = build_parent_with_child(&mut store, child_reference);
        let (_, wrapper_ref) =
            build_wrapper_template(&mut store, &mut string_table, "before", "after");
        (parent_id, wrapper_ref)
    };

    let missing_overlay_set_id = TemplateOverlaySetId::new(999);
    let mut tir_reference =
        make_tir_reference(&registered_store, parent_id, missing_overlay_set_id);
    let wrapper_set_count_before = registered_store.store().borrow().wrapper_sets.len();

    let error =
        attach_wrapper_context_overlay(&mut tir_reference, &[wrapper_ref], &registered_store)
            .expect_err("missing current overlay should fail before allocation");

    assert!(error.msg.contains("current overlay set"));
    assert_eq!(
        registered_store.store().borrow().wrapper_sets.len(),
        wrapper_set_count_before,
        "failed composition must not allocate a wrapper set"
    );
    assert_eq!(tir_reference.overlay_set_id, missing_overlay_set_id);
}

#[test]
fn attach_wrapper_context_validates_child_overlay_before_allocating_wrapper_set() {
    let mut string_table = StringTable::new();
    let registry = Rc::new(RefCell::new(TemplateIrRegistry::new()));
    let registered_store = RegisteredTemplateIrStore::allocate_in(Rc::clone(&registry));
    let empty_overlay_set_id = registry
        .borrow_mut()
        .allocate_overlay_set(TemplateOverlaySet::empty());
    let missing_overlay_set_id = TemplateOverlaySetId::new(999);

    let (parent_id, wrapper_ref) = {
        let mut store = registered_store.store().borrow_mut();
        let child_id = build_text_template(&mut store, &mut string_table, "child");
        let child_reference = TemplateTirChildReference::new(
            TemplateRef::new(registered_store.store_id(), child_id),
            TemplateTirPhase::Composed,
            missing_overlay_set_id,
        );
        let parent_id = build_parent_with_child(&mut store, child_reference);
        let (_, wrapper_ref) =
            build_wrapper_template(&mut store, &mut string_table, "before", "after");
        (parent_id, wrapper_ref)
    };

    let mut tir_reference = make_tir_reference(&registered_store, parent_id, empty_overlay_set_id);
    let wrapper_set_count_before = registered_store.store().borrow().wrapper_sets.len();

    let error =
        attach_wrapper_context_overlay(&mut tir_reference, &[wrapper_ref], &registered_store)
            .expect_err("missing child overlay should fail before allocation");

    assert!(
        error
            .msg
            .contains("child reference uses missing overlay set")
    );
    assert_eq!(
        registered_store.store().borrow().wrapper_sets.len(),
        wrapper_set_count_before,
        "failed child resolution must not allocate a wrapper set"
    );
    assert_eq!(tir_reference.overlay_set_id, empty_overlay_set_id);
}
