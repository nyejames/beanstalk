//! Tests for final AST type-boundary validation of template expression payloads.
//!
//! WHAT: proves that the finalized `TirView` path in `validate_types.rs` validates
//!       the *effective* dynamic expression provided by expression overlays, not the
//!       stale structural expression stored in the TIR node.
//! WHY: Phase 12 type-boundary validation must catch orphan `TypeId`s on the payload
//!      that later phases actually consume; otherwise a valid structural expression
//!      could hide an invalid overlay expression from the AST→HIR boundary.

use super::*;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::template::{
    Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchSelector, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::tir::{
    ExpressionSiteId, TemplateIrBranch, TemplateIrBuilder, TemplateIrId, TemplateIrNodeKind,
    TemplateIrRegistry, TemplateIrStore, TemplateIrSummary, TemplateLoopHeaderExpressionSites,
    TemplateOverlaySet, TemplateRef, TemplateTirPhase, TemplateTirReference, TirExpressionOverlay,
};
use crate::compiler_frontend::compiler_errors::ErrorType;
use crate::compiler_frontend::compiler_messages::source_location::CharPosition;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::{TypeId, builtin_type_ids};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;
use std::cell::RefCell;
use std::rc::Rc;

/// Builds a deterministic source location for test assertions.
fn location_at(line: i32, column: i32) -> SourceLocation {
    SourceLocation::new(
        InternedPath::default(),
        CharPosition {
            line_number: line,
            char_column: column,
        },
        CharPosition {
            line_number: line,
            char_column: column,
        },
    )
}

fn finalized_template_with_expression_overlay(
    template_ir_store: &Rc<RefCell<TemplateIrStore>>,
    registry: &mut TemplateIrRegistry,
    template_id: TemplateIrId,
    site_id: ExpressionSiteId,
    overlay_expression: Expression,
) -> Template {
    let store_id = registry.adopt_store(Rc::clone(template_ir_store));
    let store_owner = template_ir_store.borrow().owner();
    let overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![(site_id, Box::new(overlay_expression))],
    });
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    });

    Template {
        kind: TemplateType::StringFunction,
        tir_reference: TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner,
            is_composed: true,
            phase: TemplateTirPhase::Finalized,
            overlay_set_id,
        },
        id: String::new(),
        location: SourceLocation::default(),
    }
}

fn invalid_bool_expression(value: bool, location: SourceLocation) -> Expression {
    Expression::new(
        ExpressionKind::Bool(value),
        location,
        TypeId(9999),
        DataType::Bool,
        ValueMode::ImmutableOwned,
    )
}

#[test]
fn finalized_tir_view_dynamic_expression_payload_validates_effective_overlay_expression() {
    let mut string_table = StringTable::new();
    let type_environment = TypeEnvironment::new();
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut registry = TemplateIrRegistry::new();

    let structural_text = string_table.intern("structural payload");
    let structural_location = location_at(10, 5);
    let structural_expression = Expression::string_slice(
        structural_text,
        structural_location.clone(),
        ValueMode::ImmutableOwned,
    );

    // The structural expression carries a valid builtin TypeId. If validation were
    // reading the stored payload instead of the finalized view, this template would
    // pass and the orphan TypeId on the overlay would be missed.
    assert_eq!(structural_expression.type_id, builtin_type_ids::STRING);

    let overlay_location = location_at(20, 7);
    let orphan_type_id = TypeId(9999);
    let overlay_expression = Expression::new(
        ExpressionKind::StringSlice(structural_text),
        overlay_location.clone(),
        orphan_type_id,
        DataType::StringSlice,
        ValueMode::ImmutableOwned,
    );

    let (template_id, site_id) = {
        let mut store = template_ir_store.borrow_mut();
        let (template_id, dynamic_node_id) = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            let dynamic_node_id = builder.push_dynamic_expression_node(
                structural_expression,
                TemplateSegmentOrigin::Body,
                None,
                structural_location,
            );
            let template_id = builder.finish_template(
                dynamic_node_id,
                Style::default(),
                TemplateType::StringFunction,
                TemplateIrSummary::default(),
                SourceLocation::default(),
            );
            (template_id, dynamic_node_id)
        };

        let site_id = match &store
            .get_node(dynamic_node_id)
            .expect("dynamic expression node should exist")
            .kind
        {
            TemplateIrNodeKind::DynamicExpression { site_id, .. } => *site_id,
            other => panic!("expected dynamic expression node, got {other:?}"),
        };

        (template_id, site_id)
    };

    let template = finalized_template_with_expression_overlay(
        &template_ir_store,
        &mut registry,
        template_id,
        site_id,
        overlay_expression,
    );

    let store_borrow = template_ir_store.borrow();
    let context = TypeValidationContext {
        type_environment: &type_environment,
        template_ir_store: &store_borrow,
        template_ir_registry: &registry,
    };

    let error = validate_template_expression_payloads(&template, &context).expect_err(
        "finalized TirView path should detect orphan TypeId on effective overlay expression",
    );

    assert!(
        matches!(error.error_type, ErrorType::Compiler),
        "type-boundary failure should be reported as an internal compiler invariant, not a user diagnostic"
    );
    assert_eq!(
        error.location, overlay_location,
        "error location must point to the effective overlay expression, not the structural payload"
    );
    assert!(
        error.msg.contains("9999"),
        "error should name the orphan TypeId: {error:?}"
    );
}

/// Same as `finalized_tir_view_dynamic_expression_payload_validates_effective_overlay_expression`,
/// but for a `BranchChain` selector site. The error must point at the overlay selector expression,
/// not at the stored structural selector.
#[test]
fn finalized_tir_view_branch_selector_payload_validates_effective_overlay_expression_location() {
    let _string_table = StringTable::new();
    let type_environment = TypeEnvironment::new();
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut registry = TemplateIrRegistry::new();

    let structural_location = location_at(10, 5);
    let structural_selector =
        Expression::bool(true, structural_location.clone(), ValueMode::ImmutableOwned);

    let overlay_location = location_at(20, 7);
    let overlay_selector = invalid_bool_expression(true, overlay_location.clone());

    let (template_id, selector_site_id) = {
        let mut store = template_ir_store.borrow_mut();
        let (template_id, branch_chain_node_id) = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            let branch_body = builder.push_sequence_node(vec![], SourceLocation::default());
            let branch = TemplateIrBranch::new(
                TemplateBranchSelector::Bool(structural_selector),
                branch_body,
                structural_location,
            );
            let branch_chain_node_id =
                builder.push_branch_chain_node(vec![branch], None, SourceLocation::default());
            let template_id = builder.finish_template(
                branch_chain_node_id,
                Style::default(),
                TemplateType::StringFunction,
                TemplateIrSummary::default(),
                SourceLocation::default(),
            );
            (template_id, branch_chain_node_id)
        };

        let selector_site_id = match &store
            .get_node(branch_chain_node_id)
            .expect("branch chain node should exist")
            .kind
        {
            TemplateIrNodeKind::BranchChain { branches, .. } => branches[0].selector_site_id,
            other => panic!("expected branch chain node, got {other:?}"),
        };

        (template_id, selector_site_id)
    };

    let template = finalized_template_with_expression_overlay(
        &template_ir_store,
        &mut registry,
        template_id,
        selector_site_id,
        overlay_selector,
    );

    let store_borrow = template_ir_store.borrow();
    let context = TypeValidationContext {
        type_environment: &type_environment,
        template_ir_store: &store_borrow,
        template_ir_registry: &registry,
    };

    let error = validate_template_expression_payloads(&template, &context).expect_err(
        "finalized TirView path should detect orphan TypeId on effective overlay selector",
    );

    assert_eq!(
        error.location, overlay_location,
        "error location must point to the effective overlay selector, not the structural selector"
    );
    assert!(error.msg.contains("9999"));
}

/// Same as the dynamic-expression and branch-selector cases, but for a `Loop`
/// header condition site. The error must point at the overlay header expression.
#[test]
fn finalized_tir_view_loop_header_payload_validates_effective_overlay_expression_location() {
    let _string_table = StringTable::new();
    let type_environment = TypeEnvironment::new();
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut registry = TemplateIrRegistry::new();

    let structural_location = location_at(10, 5);
    let structural_condition = Expression::bool(
        false,
        structural_location.clone(),
        ValueMode::ImmutableOwned,
    );

    let overlay_location = location_at(30, 9);
    let overlay_condition = invalid_bool_expression(false, overlay_location.clone());

    let (template_id, condition_site_id) = {
        let mut store = template_ir_store.borrow_mut();
        let (template_id, loop_node_id) = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            let loop_body = builder.push_sequence_node(vec![], SourceLocation::default());
            let header = TemplateLoopHeader::Conditional {
                condition: Box::new(structural_condition),
            };
            let loop_node_id = builder.push_loop_node(header, loop_body, None, structural_location);
            let template_id = builder.finish_template(
                loop_node_id,
                Style::default(),
                TemplateType::StringFunction,
                TemplateIrSummary::default(),
                SourceLocation::default(),
            );
            (template_id, loop_node_id)
        };

        let condition_site_id = match &store
            .get_node(loop_node_id)
            .expect("loop node should exist")
            .kind
        {
            TemplateIrNodeKind::Loop { header_sites, .. } => match header_sites {
                TemplateLoopHeaderExpressionSites::Conditional { condition } => *condition,
                other => panic!("expected conditional loop header sites, got {other:?}"),
            },
            other => panic!("expected loop node, got {other:?}"),
        };

        (template_id, condition_site_id)
    };

    let template = finalized_template_with_expression_overlay(
        &template_ir_store,
        &mut registry,
        template_id,
        condition_site_id,
        overlay_condition,
    );

    let store_borrow = template_ir_store.borrow();
    let context = TypeValidationContext {
        type_environment: &type_environment,
        template_ir_store: &store_borrow,
        template_ir_registry: &registry,
    };

    let error = validate_template_expression_payloads(&template, &context).expect_err(
        "finalized TirView path should detect orphan TypeId on effective overlay loop header",
    );

    assert_eq!(
        error.location, overlay_location,
        "error location must point to the effective overlay loop header, not the structural header"
    );
    assert!(error.msg.contains("9999"));
}
