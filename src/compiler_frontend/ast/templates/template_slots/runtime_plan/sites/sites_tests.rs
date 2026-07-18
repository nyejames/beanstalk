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
use crate::compiler_frontend::ast::templates::template_slots::TemplateSlotError;
use crate::compiler_frontend::ast::templates::tir::refs::TemplateTirChildReference;
use crate::compiler_frontend::ast::templates::tir::{
    TemplateIrBuilder, TemplateIrId, TemplateIrNodeId, TemplateIrStore, TemplateSlotPlanId,
    TemplateSlotSiteRenderPiece, TemplateSlotSiteRenderPlan, TemplateTirPhase, TemplateViewContext,
    TirCopyState,
};
use crate::compiler_frontend::compiler_errors::ErrorType;
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
    let same_store_missing_template = TemplateTirChildReference::new(
        TemplateIrId::new(99),
        TemplateTirPhase::Parsed,
        TemplateViewContext::default(),
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
fn missing_structural_authority_propagates_through_runtime_slot_site_planner() {
    let mut store = TemplateIrStore::new();
    let mut copy_state = TirCopyState::new();
    let inner_plan = TemplateSlotSiteRenderPlan::default();
    let mut planner = super::RuntimeWrapperSitePlanBuilder {
        sources: &[],
        slot_plan_id: TemplateSlotPlanId::new(0),
        store: &mut store,
        copy_state: &mut copy_state,
    };

    let result = planner.try_build_child_wrapper_site_pieces_from_tir_id(
        TemplateIrId::new(0),
        TemplateIrNodeId::new(7),
        &inner_plan,
    );
    let error = result.expect_err(
        "runtime slot planning must propagate missing structural authority as an error",
    );

    match error {
        TemplateSlotError::Infrastructure(error) => {
            assert_eq!(error.error_type, ErrorType::Compiler);
        }
        TemplateSlotError::Diagnostic(_) => {
            panic!("missing structural authority must not become a user diagnostic");
        }
    }
}
