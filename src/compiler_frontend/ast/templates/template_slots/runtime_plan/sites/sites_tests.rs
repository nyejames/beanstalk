//! Focused unit tests for runtime slot-site node authority.
//!
//! WHAT: protects the TIR-authority invariants of `slot_key_for_node` and
//!       `build_tir_wrapper_render_pieces` that integration output cannot
//!       inspect: a missing node is an internal error, a present non-slot is
//!       optional, and a same-store child template must exist before recursion.
//! WHY: these are broken-TIR-authority paths, not user-facing behaviour, so they
//!      belong beside the owner rather than in integration cases.

use super::{build_tir_wrapper_render_pieces, slot_key_for_node};
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::SlotKey;
use crate::compiler_frontend::ast::templates::template::TemplateSegmentOrigin;
use crate::compiler_frontend::ast::templates::tir::{
    TemplateIrBuilder, TemplateIrId, TemplateIrNodeId, TemplateIrStore, TemplateOverlaySetId,
    TemplateRef, TemplateSlotSiteRenderPiece, TemplateSlotSiteRenderPlan, TemplateStoreId,
    TemplateTirChildReference, TemplateTirPhase, TirCopyState,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

fn empty_location() -> SourceLocation {
    SourceLocation::default()
}

fn assert_authority_error(
    result: Result<Vec<TemplateSlotSiteRenderPiece>, TemplateError>,
    context: &str,
    owner_marker: &str,
) {
    let error =
        result.expect_err(format!("{context} must surface as a broken-authority error").as_str());
    match error {
        TemplateError::Infrastructure(error) => {
            assert!(
                error.msg.contains(owner_marker),
                "{context} should be rejected by the {owner_marker} owner, got: {}",
                error.msg,
            );
        }
        TemplateError::Diagnostic(_) => {
            panic!("{context} must be an infrastructure error, not a user diagnostic",);
        }
    }
}

#[test]
fn missing_node_is_an_authority_error() {
    let store = TemplateIrStore::new();
    let missing_node = TemplateIrNodeId::new(7);

    let result = slot_key_for_node(&store, missing_node);

    assert!(
        result.is_err(),
        "a missing node must be an internal error, not an implicit non-slot classification"
    );
}

#[test]
fn present_non_slot_node_is_optional() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let mut builder = TemplateIrBuilder::new(&mut store);
    let text_id = string_table.intern("body");
    let byte_len = u32::try_from(string_table.resolve(text_id).len()).unwrap_or(u32::MAX);
    let text_node = builder.push_text_node(
        text_id,
        byte_len,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );

    let result = slot_key_for_node(&store, text_node);

    assert_eq!(result.expect("present non-slot node should classify"), None);
}

#[test]
fn present_slot_node_carries_its_key() {
    let mut store = TemplateIrStore::new();
    let mut builder = TemplateIrBuilder::new(&mut store);
    let slot_node = builder.push_slot_node(SlotKey::Default, empty_location());

    let result = slot_key_for_node(&store, slot_node);

    assert_eq!(
        result.expect("present slot node should classify"),
        Some(SlotKey::Default)
    );
}

#[test]
fn missing_same_store_child_template_is_an_authority_error() {
    let mut store = TemplateIrStore::new();
    let same_store_missing_template = TemplateTirChildReference::same_store(
        TemplateIrId::new(99),
        store.store_id(),
        TemplateTirPhase::Parsed,
        TemplateOverlaySetId::empty(),
    );
    let mut builder = TemplateIrBuilder::new(&mut store);
    let child_node = builder
        .push_child_template_node_with_reference(same_store_missing_template, empty_location());

    let mut copy_state = TirCopyState::new();
    let inner_plan = TemplateSlotSiteRenderPlan::default();

    let result = build_tir_wrapper_render_pieces(
        child_node,
        &inner_plan,
        SlotKey::Default,
        &mut store,
        &mut copy_state,
    );

    assert_authority_error(
        result,
        "missing same-store child template",
        "Runtime slot site planning",
    );
}

#[test]
fn foreign_child_reference_stays_on_subtree_copy_authority() {
    let mut store = TemplateIrStore::new();
    let foreign_store_id = TemplateStoreId::new(store.store_id().index() + 1);
    let foreign_reference = TemplateTirChildReference::new(
        TemplateRef::new(foreign_store_id, TemplateIrId::new(0)),
        TemplateTirPhase::Parsed,
        TemplateOverlaySetId::empty(),
    );
    let mut builder = TemplateIrBuilder::new(&mut store);
    let child_node =
        builder.push_child_template_node_with_reference(foreign_reference, empty_location());

    let mut copy_state = TirCopyState::new();
    let inner_plan = TemplateSlotSiteRenderPlan::default();

    let result = build_tir_wrapper_render_pieces(
        child_node,
        &inner_plan,
        SlotKey::Default,
        &mut store,
        &mut copy_state,
    );

    assert_authority_error(result, "foreign child reference", "active-slot copy");
}
