//! Focused tests for final TIR overlay storage.
//!
//! WHAT: exercises overlay ID round trips/display, empty/default overlay set
//! behavior, overlay entry allocation, overlay set allocation and canonical
//! reuse, composition order, and missing-overlay-set rejection.
//!
//! WHY: overlay storage is a new registry-owned subsystem. These tests guard the
//! invariants later phases depend on: canonical reuse, the "last non-`None`
//! wins" composition rule, and stable ID/display behavior.

use super::super::ids::{ChildTemplateOccurrenceId, ExpressionSiteId, SlotOccurrenceId};
use super::super::overlays::{
    TemplateOverlaySet, TemplateOverlaySetId, TirExpressionOverlay, TirExpressionOverlayId,
    TirSlotResolution, TirSlotResolutionOverlay, TirSlotResolutionOverlayId, TirWrapperContext,
    TirWrapperContextOverlay, TirWrapperContextOverlayId,
};
use super::super::refs::{TemplateRef, TemplateStoreId, TemplateWrapperSetRef};
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, ExpressionValueShape,
};
use crate::compiler_frontend::ast::expressions::expression_types::ConstRecordState;
use crate::compiler_frontend::ast::templates::template::SlotKey;
use crate::compiler_frontend::ast::templates::tir::overlays::TirWrapperApplicationMode;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
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
use super::super::registry::TemplateIrRegistry;

#[test]
fn overlay_set_id_round_trips_through_index() {
    let id = TemplateOverlaySetId::new(3);
    assert_eq!(id.index(), 3);
}

#[test]
fn expression_overlay_id_round_trips_through_index() {
    let id = TirExpressionOverlayId::new(5);
    assert_eq!(id.index(), 5);
}

#[test]
fn slot_resolution_overlay_id_round_trips_through_index() {
    let id = TirSlotResolutionOverlayId::new(7);
    assert_eq!(id.index(), 7);
}

#[test]
fn wrapper_context_overlay_id_round_trips_through_index() {
    let id = TirWrapperContextOverlayId::new(11);
    assert_eq!(id.index(), 11);
}

#[test]
fn overlay_ids_display_with_final_system_names() {
    assert_eq!(
        TemplateOverlaySetId::new(1).to_string(),
        "TemplateOverlaySetId(1)"
    );
    assert_eq!(
        TirExpressionOverlayId::new(2).to_string(),
        "TirExpressionOverlayId(2)"
    );
    assert_eq!(
        TirSlotResolutionOverlayId::new(3).to_string(),
        "TirSlotResolutionOverlayId(3)"
    );
    assert_eq!(
        TirWrapperContextOverlayId::new(4).to_string(),
        "TirWrapperContextOverlayId(4)"
    );
}

#[test]
fn default_overlay_set_is_empty() {
    let set = TemplateOverlaySet::default();
    assert!(set.is_empty());
    assert!(set.expression_overrides.is_none());
    assert!(set.slot_resolution.is_none());
    assert!(set.wrapper_context.is_none());
}

#[test]
fn empty_constructor_matches_default() {
    let set = TemplateOverlaySet::empty();
    assert_eq!(set, TemplateOverlaySet::default());
    assert!(set.is_empty());
}

#[test]
fn non_empty_set_reports_not_empty() {
    let set = TemplateOverlaySet {
        expression_overrides: Some(TirExpressionOverlayId::new(0)),
        slot_resolution: None,
        wrapper_context: None,
    };
    assert!(!set.is_empty());
}

#[test]
fn allocate_expression_overlay_returns_sequential_ids() {
    let mut registry = TemplateIrRegistry::new();

    let a = registry.allocate_expression_overlay(TirExpressionOverlay::default());
    let b = registry.allocate_expression_overlay(TirExpressionOverlay::default());

    assert_eq!(a.index(), 0);
    assert_eq!(b.index(), 1);
}

#[test]
fn allocate_slot_resolution_overlay_returns_sequential_ids() {
    let mut registry = TemplateIrRegistry::new();

    let a = registry.allocate_slot_resolution_overlay(TirSlotResolutionOverlay::default());
    let b = registry.allocate_slot_resolution_overlay(TirSlotResolutionOverlay::default());

    assert_eq!(a.index(), 0);
    assert_eq!(b.index(), 1);
}

#[test]
fn allocate_wrapper_context_overlay_returns_sequential_ids() {
    let mut registry = TemplateIrRegistry::new();

    let a = registry.allocate_wrapper_context_overlay(TirWrapperContextOverlay::default());
    let b = registry.allocate_wrapper_context_overlay(TirWrapperContextOverlay::default());

    assert_eq!(a.index(), 0);
    assert_eq!(b.index(), 1);
}

#[test]
fn overlay_entry_lookup_returns_allocated_payload() {
    let mut registry = TemplateIrRegistry::new();
    let id = registry.allocate_expression_overlay(TirExpressionOverlay::default());

    let overlay = registry
        .expression_overlay(id)
        .expect("expression overlay should exist");
    assert!(overlay.overrides.is_empty());
}

#[test]
fn overlay_entry_lookup_returns_none_for_missing_id() {
    let registry = TemplateIrRegistry::new();

    assert!(
        registry
            .expression_overlay(TirExpressionOverlayId::new(99))
            .is_none()
    );
    assert!(
        registry
            .slot_resolution_overlay(TirSlotResolutionOverlayId::new(99))
            .is_none()
    );
    assert!(
        registry
            .wrapper_context_overlay(TirWrapperContextOverlayId::new(99))
            .is_none()
    );
}

#[test]
fn allocate_overlay_set_returns_sequential_ids_for_distinct_sets() {
    let mut registry = TemplateIrRegistry::new();

    let expression_id = registry.allocate_expression_overlay(TirExpressionOverlay::default());
    let first = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let second = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(expression_id),
        slot_resolution: None,
        wrapper_context: None,
    });

    assert_eq!(first.index(), 0);
    assert_eq!(second.index(), 1);
    assert_eq!(registry.overlay_set_count(), 2);
}

#[test]
fn allocate_overlay_set_canonicalizes_empty_sets() {
    let mut registry = TemplateIrRegistry::new();

    let first = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let second = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    assert_eq!(first, second);
    assert_eq!(registry.overlay_set_count(), 1);
}

#[test]
fn allocate_overlay_set_canonicalizes_equivalent_sets() {
    let mut registry = TemplateIrRegistry::new();

    let expression_id = registry.allocate_expression_overlay(TirExpressionOverlay::default());
    let slot_id = registry.allocate_slot_resolution_overlay(TirSlotResolutionOverlay::default());
    let wrapper_id = registry.allocate_wrapper_context_overlay(TirWrapperContextOverlay::default());

    let set = TemplateOverlaySet {
        expression_overrides: Some(expression_id),
        slot_resolution: Some(slot_id),
        wrapper_context: Some(wrapper_id),
    };

    let first = registry.allocate_overlay_set(set.clone());
    let second = registry.allocate_overlay_set(set);

    assert_eq!(first, second);
    assert_eq!(registry.overlay_set_count(), 1);
}

#[test]
fn allocate_overlay_set_does_not_canonicalize_distinct_sets() {
    let mut registry = TemplateIrRegistry::new();

    let expression_id = registry.allocate_expression_overlay(TirExpressionOverlay::default());
    let slot_id = registry.allocate_slot_resolution_overlay(TirSlotResolutionOverlay::default());

    let first = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(expression_id),
        slot_resolution: None,
        wrapper_context: None,
    });
    let second = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: Some(slot_id),
        wrapper_context: None,
    });

    assert_ne!(first, second);
    assert_eq!(registry.overlay_set_count(), 2);
}

#[test]
fn overlay_set_lookup_returns_allocated_set() {
    let mut registry = TemplateIrRegistry::new();

    let expression_id = registry.allocate_expression_overlay(TirExpressionOverlay::default());
    let set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(expression_id),
        slot_resolution: None,
        wrapper_context: None,
    });

    let set = registry
        .overlay_set(set_id)
        .expect("overlay set should exist");
    assert_eq!(set.expression_overrides, Some(expression_id));
    assert!(set.slot_resolution.is_none());
    assert!(set.wrapper_context.is_none());
}

#[test]
fn overlay_set_lookup_returns_none_for_missing_id() {
    let registry = TemplateIrRegistry::new();
    assert!(
        registry
            .overlay_set(TemplateOverlaySetId::new(99))
            .is_none()
    );
}

#[test]
fn compose_overlay_sets_last_non_none_wins_per_dimension() {
    let mut registry = TemplateIrRegistry::new();

    let outer_expression = registry.allocate_expression_overlay(TirExpressionOverlay::default());
    let inner_expression = registry.allocate_expression_overlay(TirExpressionOverlay::default());
    let inner_slot = registry.allocate_slot_resolution_overlay(TirSlotResolutionOverlay::default());

    let outer = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(outer_expression),
        slot_resolution: None,
        wrapper_context: None,
    });
    let inner = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(inner_expression),
        slot_resolution: Some(inner_slot),
        wrapper_context: None,
    });

    let composed = registry
        .compose_overlay_sets(&[outer, inner])
        .expect("composition should succeed");
    let set = registry
        .overlay_set(composed)
        .expect("composed overlay set should exist");

    // Later non-`None` values win per dimension, so the inner expression
    // override replaces the outer one while its slot resolution fills the
    // dimension the outer left empty.
    assert_eq!(set.expression_overrides, Some(inner_expression));
    assert_eq!(set.slot_resolution, Some(inner_slot));
    assert!(set.wrapper_context.is_none());
}

#[test]
fn compose_overlay_sets_fills_in_none_dimensions_from_later_sets() {
    let mut registry = TemplateIrRegistry::new();

    let expression_id = registry.allocate_expression_overlay(TirExpressionOverlay::default());
    let slot_id = registry.allocate_slot_resolution_overlay(TirSlotResolutionOverlay::default());
    let wrapper_id = registry.allocate_wrapper_context_overlay(TirWrapperContextOverlay::default());

    let first = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(expression_id),
        slot_resolution: None,
        wrapper_context: None,
    });
    let second = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: Some(slot_id),
        wrapper_context: None,
    });
    let third = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: None,
        wrapper_context: Some(wrapper_id),
    });

    let composed = registry
        .compose_overlay_sets(&[first, second, third])
        .expect("composition should succeed");
    let set = registry
        .overlay_set(composed)
        .expect("composed overlay set should exist");

    assert_eq!(set.expression_overrides, Some(expression_id));
    assert_eq!(set.slot_resolution, Some(slot_id));
    assert_eq!(set.wrapper_context, Some(wrapper_id));
}

#[test]
fn compose_overlay_sets_canonicalizes_to_existing_equivalent_set() {
    let mut registry = TemplateIrRegistry::new();

    let expression_id = registry.allocate_expression_overlay(TirExpressionOverlay::default());

    let set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(expression_id),
        slot_resolution: None,
        wrapper_context: None,
    });

    // Composing a single set yields an equivalent set, so canonicalization
    // reuses the existing ID instead of allocating a duplicate.
    let composed = registry
        .compose_overlay_sets(&[set_id])
        .expect("composition should succeed");

    assert_eq!(composed, set_id);
    assert_eq!(registry.overlay_set_count(), 1);
}

#[test]
fn compose_overlay_sets_with_empty_input_yields_canonical_empty_set() {
    let mut registry = TemplateIrRegistry::new();

    let empty_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let composed = registry
        .compose_overlay_sets(&[])
        .expect("empty composition should succeed");

    assert_eq!(composed, empty_id);
    assert_eq!(registry.overlay_set_count(), 1);
}

#[test]
fn compose_overlay_sets_rejects_missing_set_ids() {
    let mut registry = TemplateIrRegistry::new();

    let expression_id = registry.allocate_expression_overlay(TirExpressionOverlay::default());
    let real_set = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(expression_id),
        slot_resolution: None,
        wrapper_context: None,
    });
    let missing_set = TemplateOverlaySetId::new(99);

    let error = registry
        .compose_overlay_sets(&[missing_set, real_set])
        .expect_err("missing overlay set IDs should be internal errors");

    assert!(
        error
            .msg
            .contains("compose_overlay_sets: TemplateOverlaySetId(99) does not exist")
    );
}

// -------------------------
//  Occurrence-keyed overlay payload tests
// -------------------------

#[test]
fn slot_resolution_carries_source_template_ref() {
    let source = TemplateRef::new(
        TemplateStoreId::new(0),
        super::super::ids::TemplateIrId::new(3),
    );
    let resolution = TirSlotResolution::resolved(SlotKey::Default, vec![source]);
    assert_eq!(resolution.key, SlotKey::Default);
    assert_eq!(resolution.sources(), &[source]);
}

#[test]
fn slot_resolution_records_multiple_sources_for_replay() {
    let first = TemplateRef::new(
        TemplateStoreId::new(0),
        super::super::ids::TemplateIrId::new(3),
    );
    let second = TemplateRef::new(
        TemplateStoreId::new(0),
        super::super::ids::TemplateIrId::new(4),
    );

    let resolution = TirSlotResolution::resolved(SlotKey::Positional(2), vec![first, second]);

    assert_eq!(resolution.sources(), &[first, second]);
}

#[test]
fn slot_resolution_records_missing_and_unresolved_states() {
    let missing = TirSlotResolution::missing(SlotKey::Default);
    assert!(missing.is_missing());
    assert!(!missing.is_unresolved());
    assert!(missing.sources().is_empty());

    let unresolved = TirSlotResolution::unresolved(SlotKey::Positional(0));
    assert!(unresolved.is_unresolved());
    assert!(!unresolved.is_missing());
    assert!(unresolved.sources().is_empty());
}

#[test]
fn expression_overlay_lookup_returns_override_for_known_site() {
    let overlay = TirExpressionOverlay {
        overrides: vec![
            (ExpressionSiteId::new(0), Box::new(bool_expression())),
            (ExpressionSiteId::new(2), Box::new(bool_expression())),
        ],
    };

    assert!(
        overlay
            .expression_for_site(ExpressionSiteId::new(0))
            .is_some()
    );
    assert!(
        overlay
            .expression_for_site(ExpressionSiteId::new(2))
            .is_some()
    );
}

#[test]
fn expression_overlay_lookup_returns_none_for_unknown_site() {
    let overlay = TirExpressionOverlay {
        overrides: vec![(ExpressionSiteId::new(0), Box::new(bool_expression()))],
    };

    assert!(
        overlay
            .expression_for_site(ExpressionSiteId::new(99))
            .is_none()
    );
}

#[test]
fn expression_overlay_default_has_no_overrides() {
    let overlay = TirExpressionOverlay::default();
    assert!(overlay.overrides.is_empty());
    assert!(
        overlay
            .expression_for_site(ExpressionSiteId::new(0))
            .is_none()
    );
}

#[test]
fn slot_resolution_overlay_lookup_returns_resolution_for_known_occurrence() {
    let source = TemplateRef::new(
        TemplateStoreId::new(0),
        super::super::ids::TemplateIrId::new(1),
    );
    let overlay = TirSlotResolutionOverlay {
        resolutions: vec![(
            SlotOccurrenceId::new(0),
            TirSlotResolution::resolved(SlotKey::Default, vec![source]),
        )],
    };

    let found = overlay
        .resolution_for_occurrence(SlotOccurrenceId::new(0))
        .expect("resolution for known occurrence should exist");
    assert_eq!(found.sources(), &[source]);
}

#[test]
fn slot_resolution_overlay_lookup_returns_none_for_unknown_occurrence() {
    let overlay = TirSlotResolutionOverlay::default();
    assert!(
        overlay
            .resolution_for_occurrence(SlotOccurrenceId::new(99))
            .is_none()
    );
}

#[test]
fn wrapper_context_records_inherited_wrapper_set() {
    let wrapper_set = TemplateWrapperSetRef::new(
        TemplateStoreId::new(0),
        super::super::ids::TemplateWrapperSetId::new(1),
    );

    let context = TirWrapperContext::inherited(wrapper_set);

    assert_eq!(context.inherited_wrapper_set, Some(wrapper_set));
    assert!(!context.skip_parent_child_wrappers);
    assert!(matches!(
        context.application_mode,
        TirWrapperApplicationMode::Always
    ));
    assert!(!context.is_empty());
}

#[test]
fn wrapper_context_records_fresh_suppression_and_application_mode() {
    let context = TirWrapperContext {
        inherited_wrapper_set: None,
        skip_parent_child_wrappers: true,
        application_mode: TirWrapperApplicationMode::IfChildEmits,
    };

    assert!(context.skip_parent_child_wrappers);
    assert!(matches!(
        context.application_mode,
        TirWrapperApplicationMode::IfChildEmits
    ));
    assert!(!context.is_empty());
}

#[test]
fn wrapper_context_empty_has_no_effective_context() {
    assert!(TirWrapperContext::empty().is_empty());
}

#[test]
fn wrapper_context_overlay_lookup_returns_context_for_known_occurrence() {
    let context = TirWrapperContext {
        inherited_wrapper_set: None,
        skip_parent_child_wrappers: true,
        application_mode: TirWrapperApplicationMode::IfChildEmits,
    };
    let overlay = TirWrapperContextOverlay {
        contexts: vec![(ChildTemplateOccurrenceId::new(0), context.clone())],
    };

    let found = overlay
        .context_for_occurrence(ChildTemplateOccurrenceId::new(0))
        .expect("context for known occurrence should exist");
    assert_eq!(found, &context);
}

#[test]
fn wrapper_context_overlay_lookup_returns_none_for_unknown_occurrence() {
    let overlay = TirWrapperContextOverlay::default();
    assert!(
        overlay
            .context_for_occurrence(ChildTemplateOccurrenceId::new(99))
            .is_none()
    );
}
