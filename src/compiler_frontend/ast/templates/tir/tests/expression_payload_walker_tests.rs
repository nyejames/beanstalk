//! TIR expression-payload walker tests.
//!
//! WHAT: exercises the strict finalized-body mutation walker over every TIR
//! shape AST finalization must normalize.
//! WHY: the walker is the TIR-owned body-authority handoff. These tests prove
//! it follows control-flow body roots, child-template refs, insert
//! contributions, aggregate wrappers, and runtime slot-plan subtrees.

use super::super::builder::TemplateIrBuilder;
use super::super::expression_payload_walker::{
    TirExpressionPayloadMutator, collect_effective_tir_expression_overlay_payloads,
    collect_tir_expression_overlay_payloads, mutate_finalized_tir_body_root_expression_payloads,
    walk_expression_payloads_with_nested_tir_views, walk_tir_view_expression_payloads,
};
use super::super::ids::{ExpressionSiteId, TemplateIrId, TemplateIrNodeId};
use super::super::node::{
    TemplateIr, TemplateIrBranch, TemplateIrNode, TemplateIrNodeKind,
    TemplateLoopHeaderExpressionSites,
};
use super::super::overlays::TirExpressionOverlay;
use super::super::parser_builder_state::TemplateTirReference;
use super::super::refs::TemplateRef;
use super::super::registry::TemplateIrRegistry;
use super::super::slot_plan::{
    TemplateSlotContributionSourcePlan, TemplateSlotPlan, TemplateSlotSitePlan,
    TemplateSlotSiteRenderPiece, TemplateSlotSiteRenderPlan,
};
use super::super::store::TemplateIrStore;
use super::super::summary::TemplateIrSummary;
use super::super::view::TirView;
use super::super::{
    TemplateOverlaySet, TemplateOverlaySetId, TemplateTirChildReference, TemplateTirPhase,
};
use crate::compiler_frontend::ast::ast_nodes::{LoopBindings, RangeEndKind, RangeLoopSpec};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::expression_rpn::{
    ExpressionRpn, ExpressionRpnItem,
};
use crate::compiler_frontend::ast::templates::template::Template;
use crate::compiler_frontend::ast::templates::template::{
    SlotKey, Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchSelector, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_slots::{
    RuntimeSlotContributionSourceId, RuntimeSlotSiteId,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;

fn empty_location() -> SourceLocation {
    SourceLocation::default()
}

fn expression(value: i32) -> Expression {
    Expression::int(value, empty_location(), ValueMode::ImmutableOwned)
}

fn bool_expression(value: bool) -> Expression {
    Expression::bool(value, empty_location(), ValueMode::ImmutableOwned)
}

fn dynamic_expression_site_id(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
) -> ExpressionSiteId {
    let node = store.get_node(node_id).expect("node should exist");
    match &node.kind {
        TemplateIrNodeKind::DynamicExpression { site_id, .. } => *site_id,
        _ => panic!("expected dynamic expression node"),
    }
}

fn branch_selector_site_id(store: &TemplateIrStore, node_id: TemplateIrNodeId) -> ExpressionSiteId {
    let node = store.get_node(node_id).expect("node should exist");
    match &node.kind {
        TemplateIrNodeKind::BranchChain { branches, .. } => branches[0].selector_site_id,
        _ => panic!("expected branch chain node"),
    }
}

fn collect_view_expression_payloads(view: &TirView<'_>) -> Result<Vec<Expression>, CompilerError> {
    let mut payloads = Vec::new();
    walk_tir_view_expression_payloads(view, &mut |expression| {
        payloads.push(expression.clone());
        Ok(())
    })?;
    Ok(payloads)
}

fn dynamic_node(store: &mut TemplateIrStore, value: i32) -> TemplateIrNodeId {
    let site_id = store.next_expression_site_id();

    store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::DynamicExpression {
            expression: Box::new(expression(value)),
            origin: TemplateSegmentOrigin::Body,
            reactive_subscription: None,
            site_id,
        },
        empty_location(),
    ))
}

fn push_template(store: &mut TemplateIrStore, root: TemplateIrNodeId) -> TemplateIrId {
    store.push_template(TemplateIr::new(
        root,
        Style::default(),
        TemplateType::StringFunction,
        TemplateIrSummary::default(),
        empty_location(),
    ))
}

fn mutate_from_root(
    store: &mut TemplateIrStore,
    root: TemplateIrNodeId,
) -> Result<CountingMutator, CompilerError> {
    let mut mutator = CountingMutator::default();
    mutate_finalized_tir_body_root_expression_payloads(store, root, &mut mutator)?;
    Ok(mutator)
}

#[derive(Debug, Default)]
struct CountingMutator {
    count: usize,
}

impl TirExpressionPayloadMutator for CountingMutator {
    fn mutate_expression_payload(
        &mut self,
        expression: &mut Expression,
    ) -> Result<(), CompilerError> {
        self.count += 1;
        expression.contains_regular_division = true;
        Ok(())
    }
}

#[test]
fn mutates_direct_dynamic_expression() {
    let mut store = TemplateIrStore::new();
    let root = dynamic_node(&mut store, 1);

    let mutator = mutate_from_root(&mut store, root).expect("walk should succeed");

    assert_eq!(mutator.count, 1);
    let node = store.get_node(root).expect("root node should exist");
    let TemplateIrNodeKind::DynamicExpression { expression, .. } = &node.kind else {
        panic!("expected dynamic expression");
    };
    assert!(expression.contains_regular_division);
}

#[test]
fn mutates_branch_selector_and_body_expression() {
    let mut store = TemplateIrStore::new();
    let body = dynamic_node(&mut store, 2);
    let selector_site_id = store.next_expression_site_id();
    let branch = TemplateIrBranch::new(
        TemplateBranchSelector::Bool(expression(1)),
        body,
        empty_location(),
    )
    .with_selector_site_id(selector_site_id);
    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::BranchChain {
            branches: vec![branch],
            fallback: None,
        },
        empty_location(),
    ));

    let mutator = mutate_from_root(&mut store, root).expect("walk should succeed");

    assert_eq!(mutator.count, 2);
}

#[test]
fn mutates_loop_header_body_and_aggregate_wrapper_expression() {
    let mut store = TemplateIrStore::new();
    let body = dynamic_node(&mut store, 2);
    let aggregate_wrapper = dynamic_node(&mut store, 3);
    let header = TemplateLoopHeader::Conditional {
        condition: Box::new(expression(1)),
    };
    let header_sites = store.allocate_loop_header_expression_sites(&header);
    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Loop {
            header,
            header_sites,
            body,
            aggregate_wrapper: Some(aggregate_wrapper),
        },
        empty_location(),
    ));

    let mutator = mutate_from_root(&mut store, root).expect("walk should succeed");

    assert_eq!(mutator.count, 3);
}

#[test]
fn mutates_range_loop_header_expression_positions() {
    let mut store = TemplateIrStore::new();
    let body = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence { children: vec![] },
        empty_location(),
    ));
    let header = TemplateLoopHeader::Range {
        bindings: Box::new(LoopBindings {
            item: None,
            index: None,
        }),
        range: Box::new(RangeLoopSpec {
            start: expression(1),
            end: expression(10),
            end_kind: RangeEndKind::Exclusive,
            step: Some(expression(2)),
        }),
    };
    let header_sites = store.allocate_loop_header_expression_sites(&header);
    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Loop {
            header,
            header_sites,
            body,
            aggregate_wrapper: None,
        },
        empty_location(),
    ));

    let mutator = mutate_from_root(&mut store, root).expect("walk should succeed");

    assert_eq!(mutator.count, 3);
}

#[test]
fn mutates_child_template_and_nested_child_template_expression() {
    let mut store = TemplateIrStore::new();
    let grandchild_root = dynamic_node(&mut store, 3);
    let grandchild_template = push_template(&mut store, grandchild_root);

    let nested_child_occurrence = store.next_child_template_occurrence_id();
    let nested_child_reference = TemplateTirChildReference::same_store(
        grandchild_template,
        store.store_id(),
        TemplateTirPhase::Parsed,
        TemplateOverlaySetId::empty(),
    );
    let nested_child = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::ChildTemplate {
            reference: nested_child_reference,
            occurrence_id: nested_child_occurrence,
        },
        empty_location(),
    ));
    let child_template = push_template(&mut store, nested_child);

    let root_occurrence = store.next_child_template_occurrence_id();
    let root_reference = TemplateTirChildReference::same_store(
        child_template,
        store.store_id(),
        TemplateTirPhase::Parsed,
        TemplateOverlaySetId::empty(),
    );
    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::ChildTemplate {
            reference: root_reference,
            occurrence_id: root_occurrence,
        },
        empty_location(),
    ));

    let mutator = mutate_from_root(&mut store, root).expect("walk should succeed");

    assert_eq!(mutator.count, 1);
}

#[test]
fn mutates_insert_contribution_child_expression() {
    let mut store = TemplateIrStore::new();
    let insert_root = dynamic_node(&mut store, 1);
    let insert_template = push_template(&mut store, insert_root);
    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::InsertContribution {
            template: insert_template,
        },
        empty_location(),
    ));

    let mutator = mutate_from_root(&mut store, root).expect("walk should succeed");

    assert_eq!(mutator.count, 1);
}

#[test]
fn mutates_runtime_slot_plan_wrapper_source_and_site_render_piece() {
    let mut store = TemplateIrStore::new();
    let wrapper_root = dynamic_node(&mut store, 1);
    let source_root = dynamic_node(&mut store, 2);
    let site_render_root = dynamic_node(&mut store, 3);
    let contribution_source = RuntimeSlotContributionSourceId(0);
    let slot_plan_id = store.push_slot_plan(TemplateSlotPlan {
        location: empty_location(),
        contribution_sources: vec![TemplateSlotContributionSourcePlan {
            source: contribution_source,
            target: SlotKey::Default,
            render_root: source_root,
            renders_wrapper_unconditionally: true,
            location: empty_location(),
        }],
        slot_sites: vec![TemplateSlotSitePlan {
            site: RuntimeSlotSiteId(0),
            key: SlotKey::Default,
            render_plan: TemplateSlotSiteRenderPlan {
                pieces: vec![
                    TemplateSlotSiteRenderPiece::Render(site_render_root),
                    TemplateSlotSiteRenderPiece::ContributionSource(contribution_source),
                ],
            },
            location: empty_location(),
        }],
    });

    let mut runtime_template = TemplateIr::new(
        wrapper_root,
        Style::default(),
        TemplateType::StringFunction,
        TemplateIrSummary::default(),
        empty_location(),
    );
    runtime_template.runtime_slot_plan = Some(slot_plan_id);
    let runtime_template_id = store.push_template(runtime_template);
    let occurrence_id = store.next_child_template_occurrence_id();
    let runtime_reference = TemplateTirChildReference::same_store(
        runtime_template_id,
        store.store_id(),
        TemplateTirPhase::Parsed,
        TemplateOverlaySetId::empty(),
    );
    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::ChildTemplate {
            reference: runtime_reference,
            occurrence_id,
        },
        empty_location(),
    ));

    let mutator = mutate_from_root(&mut store, root).expect("walk should succeed");

    assert_eq!(mutator.count, 3);
}

#[test]
fn collects_runtime_slot_plan_wrapper_source_and_site_render_piece_dynamic_payloads() {
    let mut store = TemplateIrStore::new();
    let wrapper_root = dynamic_node(&mut store, 1);
    let source_root = dynamic_node(&mut store, 2);
    let site_render_root = dynamic_node(&mut store, 3);
    let contribution_source = RuntimeSlotContributionSourceId(0);
    let slot_plan_id = store.push_slot_plan(TemplateSlotPlan {
        location: empty_location(),
        contribution_sources: vec![TemplateSlotContributionSourcePlan {
            source: contribution_source,
            target: SlotKey::Default,
            render_root: source_root,
            renders_wrapper_unconditionally: true,
            location: empty_location(),
        }],
        slot_sites: vec![TemplateSlotSitePlan {
            site: RuntimeSlotSiteId(0),
            key: SlotKey::Default,
            render_plan: TemplateSlotSiteRenderPlan {
                pieces: vec![
                    TemplateSlotSiteRenderPiece::Render(site_render_root),
                    TemplateSlotSiteRenderPiece::ContributionSource(contribution_source),
                ],
            },
            location: empty_location(),
        }],
    });

    let mut runtime_template = TemplateIr::new(
        wrapper_root,
        Style::default(),
        TemplateType::StringFunction,
        TemplateIrSummary::default(),
        empty_location(),
    );
    runtime_template.runtime_slot_plan = Some(slot_plan_id);
    let runtime_template_id = store.push_template(runtime_template);

    let payloads = collect_tir_expression_overlay_payloads(&store, runtime_template_id)
        .expect("expression overlay collection should succeed");

    assert_eq!(payloads.len(), 3);
}

#[test]
fn effective_collection_preserves_same_store_child_expression_overlay() {
    let mut store = TemplateIrStore::new();
    let child_root = dynamic_node(&mut store, 1);
    let child_site_id = dynamic_expression_site_id(&store, child_root);
    let child_template_id = push_template(&mut store, child_root);

    let mut registry = TemplateIrRegistry::new();
    let child_expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![(child_site_id, Box::new(expression(9)))],
    });
    let child_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(child_expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    });
    let root_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let child_reference = TemplateTirChildReference::same_store(
        child_template_id,
        store.store_id(),
        TemplateTirPhase::Composed,
        child_overlay_set_id,
    );
    let child_occurrence_id = store.next_child_template_occurrence_id();
    let parent_root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::ChildTemplate {
            reference: child_reference,
            occurrence_id: child_occurrence_id,
        },
        empty_location(),
    ));
    let parent_template_id = push_template(&mut store, parent_root);

    let payloads = collect_effective_tir_expression_overlay_payloads(
        &store,
        &registry,
        parent_template_id,
        root_overlay_set_id,
    )
    .expect("effective payload collection should succeed");

    assert!(payloads.iter().any(|(site_id, expression)| {
        *site_id == child_site_id && matches!(expression.kind, ExpressionKind::Int(9))
    }));
}

#[test]
fn collects_dynamic_payloads_branch_selectors_and_loop_headers() {
    let mut store = TemplateIrStore::new();
    let branch_body = dynamic_node(&mut store, 2);
    let selector_site_id = store.next_expression_site_id();
    let branch = TemplateIrBranch::new(
        TemplateBranchSelector::Bool(expression(1)),
        branch_body,
        empty_location(),
    )
    .with_selector_site_id(selector_site_id);
    let branch_chain = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::BranchChain {
            branches: vec![branch],
            fallback: None,
        },
        empty_location(),
    ));

    let loop_body = dynamic_node(&mut store, 4);
    let aggregate_wrapper = dynamic_node(&mut store, 5);
    let header = TemplateLoopHeader::Conditional {
        condition: Box::new(expression(3)),
    };
    let header_sites = store.allocate_loop_header_expression_sites(&header);
    let header_condition_site_id = match header_sites {
        TemplateLoopHeaderExpressionSites::Conditional { condition } => condition,
        _ => panic!("expected conditional loop header sites"),
    };
    let loop_node = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Loop {
            header,
            header_sites,
            body: loop_body,
            aggregate_wrapper: Some(aggregate_wrapper),
        },
        empty_location(),
    ));

    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence {
            children: vec![branch_chain, loop_node],
        },
        empty_location(),
    ));
    let template_id = push_template(&mut store, root);

    let payloads = collect_tir_expression_overlay_payloads(&store, template_id)
        .expect("expression overlay collection should succeed");

    assert_eq!(
        payloads.len(),
        5,
        "collector should include dynamic nodes, branch selectors, and loop header expressions"
    );
    assert!(
        payloads
            .iter()
            .any(|(site_id, expression)| *site_id == selector_site_id
                && matches!(
                    expression.kind,
                    crate::compiler_frontend::ast::expressions::expression::ExpressionKind::Int(1)
                )),
        "branch selector payload should be keyed by the branch selector site ID"
    );
    assert!(
        payloads
            .iter()
            .any(|(site_id, expression)| *site_id == header_condition_site_id
                && matches!(
                    expression.kind,
                    crate::compiler_frontend::ast::expressions::expression::ExpressionKind::Int(3)
                )),
        "loop header payload should be keyed by the allocated header expression-site ID"
    );
}

#[test]
fn collects_range_loop_header_payloads_by_allocated_site_ids() {
    let mut store = TemplateIrStore::new();
    let body = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence { children: vec![] },
        empty_location(),
    ));
    let header = TemplateLoopHeader::Range {
        bindings: Box::new(LoopBindings {
            item: None,
            index: None,
        }),
        range: Box::new(RangeLoopSpec {
            start: expression(1),
            end: expression(10),
            end_kind: RangeEndKind::Exclusive,
            step: Some(expression(2)),
        }),
    };
    let header_sites = store.allocate_loop_header_expression_sites(&header);
    let (start_site_id, end_site_id, step_site_id) = match header_sites {
        TemplateLoopHeaderExpressionSites::Range { start, end, step } => (
            start,
            end,
            step.expect("range step site should be allocated"),
        ),
        _ => panic!("expected range loop header sites"),
    };
    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Loop {
            header,
            header_sites,
            body,
            aggregate_wrapper: None,
        },
        empty_location(),
    ));
    let template_id = push_template(&mut store, root);

    let payloads = collect_tir_expression_overlay_payloads(&store, template_id)
        .expect("expression overlay collection should succeed");

    assert_eq!(payloads.len(), 3);
    assert!(
        payloads
            .iter()
            .any(|(site_id, expression)| *site_id == start_site_id
                && matches!(
                    expression.kind,
                    crate::compiler_frontend::ast::expressions::expression::ExpressionKind::Int(1)
                ))
    );
    assert!(
        payloads
            .iter()
            .any(|(site_id, expression)| *site_id == end_site_id
                && matches!(
                    expression.kind,
                    crate::compiler_frontend::ast::expressions::expression::ExpressionKind::Int(10)
                ))
    );
    assert!(
        payloads
            .iter()
            .any(|(site_id, expression)| *site_id == step_site_id
                && matches!(
                    expression.kind,
                    crate::compiler_frontend::ast::expressions::expression::ExpressionKind::Int(2)
                ))
    );
}

#[test]
fn reports_missing_child_template_as_compiler_error() {
    let mut store = TemplateIrStore::new();
    let occurrence_id = store.next_child_template_occurrence_id();
    let missing_reference = TemplateTirChildReference::same_store(
        TemplateIrId::new(99),
        store.store_id(),
        TemplateTirPhase::Parsed,
        TemplateOverlaySetId::empty(),
    );
    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::ChildTemplate {
            reference: missing_reference,
            occurrence_id,
        },
        empty_location(),
    ));

    let error = mutate_from_root(&mut store, root).expect_err("missing ref should fail");

    assert!(error.msg.contains("missing child template"));
}

#[test]
fn reports_missing_runtime_slot_site_as_compiler_error() {
    let mut store = TemplateIrStore::new();
    let slot_plan_id = store.push_slot_plan(TemplateSlotPlan {
        location: empty_location(),
        contribution_sources: vec![],
        slot_sites: vec![],
    });
    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::RuntimeSlotSite {
            plan: slot_plan_id,
            site: RuntimeSlotSiteId(0),
        },
        empty_location(),
    ));

    let error = mutate_from_root(&mut store, root).expect_err("missing slot site should fail");

    assert!(error.msg.contains("missing runtime slot site"));
}

#[test]
fn view_walker_reads_dynamic_expression_overlay() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    let (template_id, site_id) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let mut builder = TemplateIrBuilder::new(&mut store);
        let root = builder.push_dynamic_expression_node(
            expression(1),
            TemplateSegmentOrigin::Body,
            None,
            empty_location(),
        );
        let template_id = builder.finish_template(
            root,
            Style::default(),
            TemplateType::StringFunction,
            TemplateIrSummary::default(),
            empty_location(),
        );
        let site_id = dynamic_expression_site_id(&store, root);
        (template_id, site_id)
    };

    let overlay_set_id = {
        let overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
            overrides: vec![(site_id, Box::new(expression(42)))],
        });
        registry.allocate_overlay_set(TemplateOverlaySet {
            expression_overrides: Some(overlay_id),
            slot_resolution: None,
            wrapper_context: None,
        })
    };

    let root_ref = TemplateRef::new(store_id, template_id);
    let view = TirView::with_minimum_phase(
        &registry,
        root_ref,
        TemplateTirPhase::Finalized,
        TemplateTirPhase::Finalized,
        overlay_set_id,
    )
    .expect("view should construct");

    let payloads = collect_view_expression_payloads(&view).expect("walk should succeed");

    assert_eq!(payloads.len(), 1);
    assert!(
        matches!(payloads[0].kind, ExpressionKind::Int(42)),
        "walker should see the overlay override, not the structural expression"
    );
}

#[test]
fn view_walker_follows_cross_store_child_expression_payloads() {
    let mut registry = TemplateIrRegistry::new();
    let empty_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let parent_store_id = registry.allocate_store();
    let child_store_id = registry.allocate_store();

    let child_template_id = {
        let mut child_store = registry
            .store_mut(child_store_id)
            .expect("child store should be mutable");
        let child_root = dynamic_node(&mut child_store, 42);
        push_template(&mut child_store, child_root)
    };

    let parent_template_id = {
        let mut parent_store = registry
            .store_mut(parent_store_id)
            .expect("parent store should be mutable");
        let occurrence_id = parent_store.next_child_template_occurrence_id();
        let child_reference = TemplateTirChildReference::new(
            TemplateRef::new(child_store_id, child_template_id),
            TemplateTirPhase::Finalized,
            empty_overlay_set_id,
        );
        let parent_root = parent_store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::ChildTemplate {
                reference: child_reference,
                occurrence_id,
            },
            empty_location(),
        ));
        push_template(&mut parent_store, parent_root)
    };

    let view = TirView::new(
        &registry,
        TemplateRef::new(parent_store_id, parent_template_id),
        TemplateTirPhase::Finalized,
        empty_overlay_set_id,
    )
    .expect("parent view should construct");

    let payloads = collect_view_expression_payloads(&view).expect("walk should succeed");

    assert_eq!(payloads.len(), 1);
    assert!(matches!(payloads[0].kind, ExpressionKind::Int(42)));
}

#[test]
fn view_walker_distinguishes_overlay_contexts_for_the_same_child_root() {
    let mut registry = TemplateIrRegistry::new();
    let empty_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let store_id = registry.allocate_store();

    let (child_template_id, child_site_id) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let child_root = dynamic_node(&mut store, 1);
        let child_site_id = dynamic_expression_site_id(&store, child_root);
        let child_template_id = push_template(&mut store, child_root);
        (child_template_id, child_site_id)
    };

    let override_overlay_set_id = {
        let expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
            overrides: vec![(child_site_id, Box::new(expression(42)))],
        });
        registry.allocate_overlay_set(TemplateOverlaySet {
            expression_overrides: Some(expression_overlay_id),
            slot_resolution: None,
            wrapper_context: None,
        })
    };

    let parent_template_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let child_root = TemplateRef::new(store_id, child_template_id);
        let structural_occurrence_id = store.next_child_template_occurrence_id();
        let structural_child = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::ChildTemplate {
                reference: TemplateTirChildReference::new(
                    child_root,
                    TemplateTirPhase::Finalized,
                    empty_overlay_set_id,
                ),
                occurrence_id: structural_occurrence_id,
            },
            empty_location(),
        ));
        let overlaid_occurrence_id = store.next_child_template_occurrence_id();
        let overlaid_child = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::ChildTemplate {
                reference: TemplateTirChildReference::new(
                    child_root,
                    TemplateTirPhase::Finalized,
                    override_overlay_set_id,
                ),
                occurrence_id: overlaid_occurrence_id,
            },
            empty_location(),
        ));
        let parent_root = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Sequence {
                children: vec![structural_child, overlaid_child],
            },
            empty_location(),
        ));
        push_template(&mut store, parent_root)
    };

    let view = TirView::new(
        &registry,
        TemplateRef::new(store_id, parent_template_id),
        TemplateTirPhase::Finalized,
        empty_overlay_set_id,
    )
    .expect("parent view should construct");

    let payloads = collect_view_expression_payloads(&view).expect("walk should succeed");

    assert_eq!(payloads.len(), 2);
    assert!(
        payloads
            .iter()
            .any(|expression| matches!(expression.kind, ExpressionKind::Int(1)))
    );
    assert!(
        payloads
            .iter()
            .any(|expression| matches!(expression.kind, ExpressionKind::Int(42)))
    );
}

#[test]
fn view_walker_reads_branch_selector_overlay() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    let (template_id, selector_site_id) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let mut builder = TemplateIrBuilder::new(&mut store);
        let body = builder.push_dynamic_expression_node(
            expression(2),
            TemplateSegmentOrigin::Body,
            None,
            empty_location(),
        );
        let branch = TemplateIrBranch::new(
            TemplateBranchSelector::Bool(bool_expression(false)),
            body,
            empty_location(),
        );
        let root = builder.push_branch_chain_node(vec![branch], None, empty_location());
        let template_id = builder.finish_template(
            root,
            Style::default(),
            TemplateType::StringFunction,
            TemplateIrSummary::default(),
            empty_location(),
        );
        let selector_site_id = branch_selector_site_id(&store, root);
        (template_id, selector_site_id)
    };

    let overlay_set_id = {
        let overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
            overrides: vec![(selector_site_id, Box::new(bool_expression(true)))],
        });
        registry.allocate_overlay_set(TemplateOverlaySet {
            expression_overrides: Some(overlay_id),
            slot_resolution: None,
            wrapper_context: None,
        })
    };

    let root_ref = TemplateRef::new(store_id, template_id);
    let view = TirView::with_minimum_phase(
        &registry,
        root_ref,
        TemplateTirPhase::Finalized,
        TemplateTirPhase::Finalized,
        overlay_set_id,
    )
    .expect("view should construct");

    let payloads = collect_view_expression_payloads(&view).expect("walk should succeed");

    assert_eq!(payloads.len(), 2);
    assert!(
        payloads
            .iter()
            .any(|e| matches!(e.kind, ExpressionKind::Bool(true))),
        "overlay branch selector should be visited"
    );
    assert!(
        payloads
            .iter()
            .any(|e| matches!(e.kind, ExpressionKind::Int(2))),
        "body expression should be visited"
    );
    assert!(
        !payloads
            .iter()
            .any(|e| matches!(e.kind, ExpressionKind::Bool(false))),
        "structural branch selector should not be visited"
    );
}

#[test]
fn view_walker_reads_loop_header_overlay() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    let (template_id, start_site_id, step_site_id) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let body = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Sequence { children: vec![] },
            empty_location(),
        ));
        let header = TemplateLoopHeader::Range {
            bindings: Box::new(LoopBindings {
                item: None,
                index: None,
            }),
            range: Box::new(RangeLoopSpec {
                start: expression(1),
                end: expression(10),
                end_kind: RangeEndKind::Exclusive,
                step: Some(expression(2)),
            }),
        };
        let header_sites = store.allocate_loop_header_expression_sites(&header);
        let (start_site_id, step_site_id) = match header_sites {
            TemplateLoopHeaderExpressionSites::Range {
                start,
                end: _,
                step,
            } => (start, step.expect("range step site should be allocated")),
            _ => panic!("expected range loop header sites"),
        };
        let root = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Loop {
                header,
                header_sites,
                body,
                aggregate_wrapper: None,
            },
            empty_location(),
        ));
        let template_id = push_template(&mut store, root);
        (template_id, start_site_id, step_site_id)
    };

    let overlay_set_id = {
        let overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
            overrides: vec![
                (start_site_id, Box::new(expression(100))),
                (step_site_id, Box::new(expression(50))),
            ],
        });
        registry.allocate_overlay_set(TemplateOverlaySet {
            expression_overrides: Some(overlay_id),
            slot_resolution: None,
            wrapper_context: None,
        })
    };

    let root_ref = TemplateRef::new(store_id, template_id);
    let view = TirView::with_minimum_phase(
        &registry,
        root_ref,
        TemplateTirPhase::Finalized,
        TemplateTirPhase::Finalized,
        overlay_set_id,
    )
    .expect("view should construct");

    let payloads = collect_view_expression_payloads(&view).expect("walk should succeed");

    assert_eq!(payloads.len(), 3);
    assert!(
        payloads
            .iter()
            .any(|e| matches!(e.kind, ExpressionKind::Int(100))),
        "overlay range start should be visited"
    );
    assert!(
        payloads
            .iter()
            .any(|e| matches!(e.kind, ExpressionKind::Int(10))),
        "structural range end should be visited"
    );
    assert!(
        payloads
            .iter()
            .any(|e| matches!(e.kind, ExpressionKind::Int(50))),
        "overlay range step should be visited"
    );
    assert!(
        !payloads
            .iter()
            .any(|e| matches!(e.kind, ExpressionKind::Int(1))),
        "structural range start should not be visited"
    );
    assert!(
        !payloads
            .iter()
            .any(|e| matches!(e.kind, ExpressionKind::Int(2))),
        "structural range step should not be visited"
    );
}

//  --------------------------
//  Nested expression-and-TIR-view walker
//  --------------------------

/// Constructs a `TemplateTirReference` for a same-store template at Finalized phase.
fn finalized_tir_reference(
    store: &TemplateIrStore,
    template_id: TemplateIrId,
    overlay_set_id: TemplateOverlaySetId,
) -> TemplateTirReference {
    TemplateTirReference {
        root: TemplateRef::new(store.store_id(), template_id),
        store_owner: store.owner(),
        phase: TemplateTirPhase::Finalized,
        overlay_set_id,
    }
}

/// Constructs a `Template` value carrying only the durable TIR reference.
fn template_with_reference(reference: TemplateTirReference) -> Template {
    Template {
        kind: TemplateType::StringFunction,
        tir_reference: reference,
        location: empty_location(),
    }
}

/// Wraps a `Template` in a string-typed `ExpressionKind::Template` expression.
fn template_expression(template: Template) -> Expression {
    Expression::new(
        ExpressionKind::Template(Box::new(template)),
        empty_location(),
        builtin_type_ids::STRING,
        DataType::StringSlice,
        ValueMode::ImmutableOwned,
    )
}

/// Wraps an operand in a single-operand `ExpressionKind::Runtime` RPN expression.
fn runtime_expression(operand: Expression) -> Expression {
    Expression::new(
        ExpressionKind::Runtime(ExpressionRpn {
            items: vec![ExpressionRpnItem::Operand(operand)],
        }),
        empty_location(),
        builtin_type_ids::INT,
        DataType::Int,
        ValueMode::ImmutableOwned,
    )
}

/// Wraps a value in a `Coerced` expression with a placeholder target type.
fn coerced_expression(value: Expression) -> Expression {
    Expression::new(
        ExpressionKind::Coerced {
            value: Box::new(value),
            to_type: builtin_type_ids::STRING,
        },
        empty_location(),
        builtin_type_ids::STRING,
        DataType::StringSlice,
        ValueMode::ImmutableOwned,
    )
}

/// Collects all expression payloads visited by the nested-expression walker.
fn collect_nested_expression_payloads(
    expression: &Expression,
    registry: &TemplateIrRegistry,
) -> Result<Vec<Expression>, CompilerError> {
    let mut payloads = Vec::new();
    let result =
        walk_expression_payloads_with_nested_tir_views(expression, registry, &mut |payload| {
            payloads.push(payload.clone());
            Ok(())
        });
    result.map(|()| payloads)
}

#[test]
fn nested_walker_enters_template_expression_tir_view() {
    let mut registry = TemplateIrRegistry::new();
    let empty_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let store_id = registry.allocate_store();

    let template_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let root = dynamic_node(&mut store, 42);
        push_template(&mut store, root)
    };

    let reference = {
        let store = registry.store(store_id).expect("store should exist");
        finalized_tir_reference(&store, template_id, empty_overlay_set_id)
    };
    let template = template_with_reference(reference);
    let expression = template_expression(template);

    let payloads =
        collect_nested_expression_payloads(&expression, &registry).expect("walk should succeed");

    assert_eq!(payloads.len(), 1);
    assert!(
        matches!(payloads[0].kind, ExpressionKind::Int(42)),
        "TIR dynamic expression inside the template should be visited"
    );
}

#[test]
fn nested_walker_inspects_runtime_and_coerced_operands() {
    let mut registry = TemplateIrRegistry::new();
    let empty_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let store_id = registry.allocate_store();

    let template_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let root = dynamic_node(&mut store, 99);
        push_template(&mut store, root)
    };

    let reference = {
        let store = registry.store(store_id).expect("store should exist");
        finalized_tir_reference(&store, template_id, empty_overlay_set_id)
    };
    let template = template_with_reference(reference);
    let template_expr = template_expression(template);

    // Wrap the template expression in Coerced, then in Runtime, so the walker
    // must descend through both wrappers to reach the template-valued TIR view.
    let coerced = coerced_expression(template_expr);
    let expression = runtime_expression(coerced);

    let payloads =
        collect_nested_expression_payloads(&expression, &registry).expect("walk should succeed");

    assert_eq!(payloads.len(), 1);
    assert!(
        matches!(payloads[0].kind, ExpressionKind::Int(99)),
        "TIR dynamic expression behind Coerced and Runtime should be visited"
    );
}

#[test]
fn nested_walker_fails_on_store_owner_mismatch() {
    let mut registry = TemplateIrRegistry::new();
    let empty_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let store_id = registry.allocate_store();
    let other_store_id = registry.allocate_store();

    let template_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let root = dynamic_node(&mut store, 1);
        push_template(&mut store, root)
    };

    // Build a reference whose root points at store_id but whose store_owner
    // token belongs to a different store. The walker must reject this.
    let mismatched_reference = {
        let other_store = registry
            .store(other_store_id)
            .expect("other store should exist");
        TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner: other_store.owner(),
            phase: TemplateTirPhase::Finalized,
            overlay_set_id: empty_overlay_set_id,
        }
    };

    let template = template_with_reference(mismatched_reference);
    let expression = template_expression(template);

    let result = collect_nested_expression_payloads(&expression, &registry);

    assert!(
        result.is_err(),
        "store-owner mismatch should produce a conservative error"
    );
}

#[test]
fn nested_walker_shares_visited_set_between_tir_child_and_expression_template() {
    let mut registry = TemplateIrRegistry::new();
    let empty_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let store_id = registry.allocate_store();

    // Child template B has one dynamic expression (value 42).
    let child_template_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let child_root = dynamic_node(&mut store, 42);
        push_template(&mut store, child_root)
    };

    let child_template_for_expr = {
        let store = registry.store(store_id).expect("store should exist");
        let reference = finalized_tir_reference(&store, child_template_id, empty_overlay_set_id);
        template_with_reference(reference)
    };

    // Parent template A has a sequence with:
    //   1. a ChildTemplate TIR node referencing B
    //   2. a DynamicExpression whose payload is ExpressionKind::Template(B)
    // Both reference the same effective identity. The shared visited set
    // ensures B is walked only once.
    let parent_template_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");

        let occurrence_id = store.next_child_template_occurrence_id();
        let child_ref = TemplateTirChildReference::new(
            TemplateRef::new(store_id, child_template_id),
            TemplateTirPhase::Finalized,
            empty_overlay_set_id,
        );
        let child_node = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::ChildTemplate {
                reference: child_ref,
                occurrence_id,
            },
            empty_location(),
        ));

        let site_id = store.next_expression_site_id();
        let expr_node = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::DynamicExpression {
                expression: Box::new(template_expression(child_template_for_expr)),
                origin: TemplateSegmentOrigin::Body,
                reactive_subscription: None,
                site_id,
            },
            empty_location(),
        ));

        let parent_root = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Sequence {
                children: vec![child_node, expr_node],
            },
            empty_location(),
        ));
        push_template(&mut store, parent_root)
    };

    let parent_template = {
        let store = registry.store(store_id).expect("store should exist");
        let reference = finalized_tir_reference(&store, parent_template_id, empty_overlay_set_id);
        template_with_reference(reference)
    };
    let expression = template_expression(parent_template);

    let payloads =
        collect_nested_expression_payloads(&expression, &registry).expect("walk should succeed");

    let int_42_count = payloads
        .iter()
        .filter(|e| matches!(e.kind, ExpressionKind::Int(42)))
        .count();
    assert_eq!(
        int_42_count, 1,
        "shared visited set should visit child B only once across TIR child and expression template paths"
    );
}

/// Insert contributions recurse through a child `TirView` that inherits the
/// parent phase and overlay set, so an expression overlay keyed by the insert
/// template's site is read through the effective view rather than the raw
/// structural payload.
#[test]
fn view_walker_reads_insert_contribution_effective_overlay() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    let (parent_template_id, insert_site_id) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");

        let insert_root = dynamic_node(&mut store, 1);
        let insert_template_id = push_template(&mut store, insert_root);
        let insert_site_id = dynamic_expression_site_id(&store, insert_root);

        let parent_root = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            builder.push_insert_contribution_node(insert_template_id, empty_location())
        };
        let parent_template_id = push_template(&mut store, parent_root);
        (parent_template_id, insert_site_id)
    };

    let overlay_set_id = {
        let overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
            overrides: vec![(insert_site_id, Box::new(expression(42)))],
        });
        registry.allocate_overlay_set(TemplateOverlaySet {
            expression_overrides: Some(overlay_id),
            slot_resolution: None,
            wrapper_context: None,
        })
    };

    let view = TirView::with_minimum_phase(
        &registry,
        TemplateRef::new(store_id, parent_template_id),
        TemplateTirPhase::Finalized,
        TemplateTirPhase::Finalized,
        overlay_set_id,
    )
    .expect("parent view should construct");

    let payloads = collect_view_expression_payloads(&view).expect("walk should succeed");

    assert_eq!(payloads.len(), 1);
    assert!(
        matches!(payloads[0].kind, ExpressionKind::Int(42)),
        "insert contribution should recurse through a child view and read the inherited overlay override, not the structural payload"
    );
}

/// A missing insert contribution template is reported as an explicit internal
/// error instead of silently skipped, because insert contributions now recurse
/// through a required child `TirView`.
#[test]
fn view_walker_reports_missing_insert_contribution_template() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    let parent_template_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let parent_root = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            builder.push_insert_contribution_node(TemplateIrId::new(99), empty_location())
        };
        push_template(&mut store, parent_root)
    };

    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let view = TirView::new(
        &registry,
        TemplateRef::new(store_id, parent_template_id),
        TemplateTirPhase::Finalized,
        overlay_set_id,
    )
    .expect("parent view should construct");

    let error = collect_view_expression_payloads(&view)
        .expect_err("a missing insert contribution template must fail explicitly");

    assert!(
        error.msg.contains("root_template"),
        "error must come from the required insert view root resolution: {}",
        error.msg,
    );
    assert!(
        error.msg.contains("missing"),
        "error must report the missing insert template: {}",
        error.msg
    );
}
