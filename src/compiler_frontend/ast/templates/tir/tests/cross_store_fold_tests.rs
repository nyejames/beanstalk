//! Cross-store template folding through `Template::fold_to_emission`.
//!
//! WHAT: exercises the cross-store view-native fold path where a parent
//!       `Template` carries a `tir_reference` pointing to a foreign store. The
//!       fold borrows the owning store from the registry and folds through
//!       `fold_tir_view` directly, preserving root, phase, and overlay-set
//!       identity.
//!
//! WHY: the same-store fast path uses a conservative linear-fold safety gate.
//!      These tests prove the cross-store path handles expression overlays,
//!      control flow, and third-store descendants without rebuilding or
//!      flattening foreign identity.

use crate::compiler_frontend::ast::ast_nodes::{LoopBindings, RangeEndKind, RangeLoopSpec};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::{
    SlotKey, Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchSelector, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_folding::{
    TemplateEmission, TemplateFoldContext,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::tir::fold_cache::TirFoldCache;
use crate::compiler_frontend::ast::templates::tir::ids::{TemplateIrId, TemplateIrNodeId};
use crate::compiler_frontend::ast::templates::tir::node::{
    TemplateIr, TemplateIrBranch, TemplateIrNode, TemplateIrNodeKind,
};
use crate::compiler_frontend::ast::templates::tir::overlays::{
    TemplateOverlaySet, TemplateOverlaySetId, TirExpressionOverlay,
};
use crate::compiler_frontend::ast::templates::tir::registry::TemplateIrRegistry;
use crate::compiler_frontend::ast::templates::tir::store::{TemplateIrStore, TemplateIrStoreOwner};
use crate::compiler_frontend::ast::templates::tir::summary::TemplateIrSummary;
use crate::compiler_frontend::ast::templates::tir::view::TemplateTirPhase;
use crate::compiler_frontend::ast::templates::tir::{
    TemplateRef, TemplateStoreId, TemplateTirChildReference, TemplateTirReference,
};
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;
use crate::projects::settings::DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use super::assert_slot_insert_fold_error;

// ------------------------------------------------------------------
//  Test helpers
// ------------------------------------------------------------------

fn empty_location() -> SourceLocation {
    SourceLocation::default()
}

fn test_project_path_resolver() -> ProjectPathResolver {
    let cwd = std::env::temp_dir();
    ProjectPathResolver::new(
        cwd.clone(),
        cwd,
        crate::compiler_frontend::source_packages::root_file::PreparedSourcePackageRoots::empty(),
        &crate::builder_surface::SourceFileKindRegistry::default(),
    )
    .expect("test path resolver should be valid")
}

fn int_expression(value: i32) -> Expression {
    Expression::int(value, empty_location(), ValueMode::ImmutableOwned)
}

fn bool_expression(value: bool) -> Expression {
    Expression::bool(value, empty_location(), ValueMode::ImmutableOwned)
}

/// Converts a `TemplateEmission` into the folded string for assertion.
fn emission_to_string(emission: TemplateEmission, string_table: &StringTable) -> String {
    match emission {
        TemplateEmission::NoOutput => String::new(),
        TemplateEmission::Output(output) => string_table.resolve(output).to_owned(),
        TemplateEmission::Break(Some(output)) | TemplateEmission::Continue(Some(output)) => {
            string_table.resolve(output).to_owned()
        }
        TemplateEmission::Break(None) | TemplateEmission::Continue(None) => String::new(),
    }
}

/// Pushes a text node into `store` and returns its node ID.
fn text_node(
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

/// Wraps `child` in a sequence node and returns the sequence node ID.
fn sequence_with(store: &mut TemplateIrStore, child: TemplateIrNodeId) -> TemplateIrNodeId {
    store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence {
            children: vec![child],
        },
        empty_location(),
    ))
}

/// Pushes a template entry from a root node ID and returns the template ID.
fn push_template_entry(store: &mut TemplateIrStore, root: TemplateIrNodeId) -> TemplateIrId {
    store.push_template(TemplateIr::new(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        empty_location(),
    ))
}

/// Builds a single-text-node template entry and returns its ID.
fn push_text_template(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
    text: &str,
) -> TemplateIrId {
    let text = text_node(store, string_table, text);
    let root = sequence_with(store, text);
    push_template_entry(store, root)
}

/// Creates a parent `Template` whose `tir_reference` points to a foreign store.
fn foreign_parent_template(
    store_id: TemplateStoreId,
    template_id: TemplateIrId,
    store_owner: Arc<TemplateIrStoreOwner>,
    phase: TemplateTirPhase,
    overlay_set_id: TemplateOverlaySetId,
) -> Template {
    Template {
        kind: TemplateType::StringFunction,
        tir_reference: TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner,
            phase,
            overlay_set_id,
        },
        location: empty_location(),
    }
}

/// Folds a parent template through `fold_to_emission` with a fold context that
/// carries the registry. The fold-context store access is `Unavailable` so the
/// same-store check fails and the cross-store path borrows the owning store
/// from the registry.
fn fold_cross_store_parent(
    registry: TemplateIrRegistry,
    parent: &Template,
    string_table: &mut StringTable,
) -> Result<TemplateEmission, TemplateError> {
    let registry = Rc::new(RefCell::new(registry));
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut fold_context = TemplateFoldContext {
        string_table,
        project_path_resolver: &resolver,
        path_format_config: &path_format,
        source_file_scope: &source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        template_ir_registry: Some(Rc::clone(&registry)),
        bindings: vec![],
        fold_cache: TirFoldCache::new(),
    };
    parent.fold_to_emission(&mut fold_context)
}

/// Returns the store-owner token for `store_id` in `registry`.
fn store_owner(
    registry: &TemplateIrRegistry,
    store_id: TemplateStoreId,
) -> Arc<TemplateIrStoreOwner> {
    registry
        .store_handle(store_id)
        .expect("store handle should exist")
        .borrow()
        .owner()
}

// ------------------------------------------------------------------
//  Tests: basic cross-store fold
// ------------------------------------------------------------------

#[test]
fn cross_store_fold_folds_foreign_text_template() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_b_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let foreign_template_id = {
        let mut store_b = registry
            .store_mut(store_b_id)
            .expect("store B should be mutable");
        push_text_template(&mut store_b, &mut string_table, "from B")
    };

    let owner = store_owner(&registry, store_b_id);
    let parent = foreign_parent_template(
        store_b_id,
        foreign_template_id,
        owner,
        TemplateTirPhase::Composed,
        overlay_set_id,
    );

    let emission = fold_cross_store_parent(registry, &parent, &mut string_table)
        .expect("cross-store fold should succeed");

    assert_eq!(
        emission_to_string(emission, &string_table),
        "from B",
        "foreign text template should fold through the owning store"
    );
}

#[test]
fn same_store_fold_rejects_nested_slot_insert_from_authoritative_tir() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let owner = store_owner(&registry, store_id);

    let outer_template_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let slot_insert_root = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Sequence {
                children: Vec::new(),
            },
            empty_location(),
        ));
        let slot_insert_template = store.push_template(TemplateIr::new(
            slot_insert_root,
            Style::default(),
            TemplateType::SlotInsert(SlotKey::Default),
            TemplateIrSummary::empty(),
            empty_location(),
        ));

        let occurrence_id = store.next_child_template_occurrence_id();
        let nested_root = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::ChildTemplate {
                reference: TemplateTirChildReference::same_store(
                    slot_insert_template,
                    store_id,
                    TemplateTirPhase::Composed,
                    overlay_set_id,
                ),
                occurrence_id,
            },
            empty_location(),
        ));
        let nested_template_id = push_template_entry(&mut store, nested_root);

        // Compatibility content stays empty. Only the composed TIR reference
        // exposes the escaped insertion.
        let nested_template = foreign_parent_template(
            store_id,
            nested_template_id,
            Arc::clone(&owner),
            TemplateTirPhase::Composed,
            overlay_set_id,
        );
        let expression_site = store.next_expression_site_id();
        let outer_root = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::DynamicExpression {
                expression: Box::new(Expression::template(
                    nested_template,
                    ValueMode::ImmutableOwned,
                )),
                origin: TemplateSegmentOrigin::Body,
                reactive_subscription: None,
                site_id: expression_site,
            },
            empty_location(),
        ));
        push_template_entry(&mut store, outer_root)
    };
    let outer_template = foreign_parent_template(
        store_id,
        outer_template_id,
        owner,
        TemplateTirPhase::Composed,
        overlay_set_id,
    );

    assert_slot_insert_fold_error(fold_cross_store_parent(
        registry,
        &outer_template,
        &mut string_table,
    ));
}

#[test]
fn same_store_fold_preserves_nested_authoritative_control_flow() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let owner = store_owner(&registry, store_id);

    let outer_template_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let selected_text = text_node(&mut store, &mut string_table, "selected");
        let selected_body = sequence_with(&mut store, selected_text);
        let fallback_text = text_node(&mut store, &mut string_table, "fallback");
        let fallback_body = sequence_with(&mut store, fallback_text);
        let selector_site = store.next_expression_site_id();
        let branch = TemplateIrBranch::new(
            TemplateBranchSelector::Bool(bool_expression(true)),
            selected_body,
            empty_location(),
        )
        .with_selector_site_id(selector_site);
        let nested_root = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::BranchChain {
                branches: vec![branch],
                fallback: Some(fallback_body),
            },
            empty_location(),
        ));
        let nested_template_id = push_template_entry(&mut store, nested_root);
        let nested_template = foreign_parent_template(
            store_id,
            nested_template_id,
            Arc::clone(&owner),
            TemplateTirPhase::Composed,
            overlay_set_id,
        );

        let expression_site = store.next_expression_site_id();
        let outer_root = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::DynamicExpression {
                expression: Box::new(Expression::template(
                    nested_template,
                    ValueMode::ImmutableOwned,
                )),
                origin: TemplateSegmentOrigin::Body,
                reactive_subscription: None,
                site_id: expression_site,
            },
            empty_location(),
        ));
        push_template_entry(&mut store, outer_root)
    };
    let outer_template = foreign_parent_template(
        store_id,
        outer_template_id,
        owner,
        TemplateTirPhase::Composed,
        overlay_set_id,
    );

    let emission = fold_cross_store_parent(registry, &outer_template, &mut string_table)
        .expect("ordinary nested control flow should keep folding through its authoritative view");
    assert_eq!(emission_to_string(emission, &string_table), "selected");
}

#[test]
fn cross_store_fold_rejects_nested_foreign_slot_insert_from_tir() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let outer_store_id = registry.allocate_store();
    let nested_store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let nested_template_id = {
        let mut nested_store = registry
            .store_mut(nested_store_id)
            .expect("nested store should be mutable");
        let root = nested_store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Sequence {
                children: Vec::new(),
            },
            empty_location(),
        ));
        nested_store.push_template(TemplateIr::new(
            root,
            Style::default(),
            TemplateType::SlotInsert(SlotKey::Default),
            TemplateIrSummary::empty(),
            empty_location(),
        ))
    };
    let nested_template = foreign_parent_template(
        nested_store_id,
        nested_template_id,
        store_owner(&registry, nested_store_id),
        TemplateTirPhase::Composed,
        overlay_set_id,
    );

    let outer_template_id = {
        let mut outer_store = registry
            .store_mut(outer_store_id)
            .expect("outer store should be mutable");
        let expression_site = outer_store.next_expression_site_id();
        let root = outer_store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::DynamicExpression {
                expression: Box::new(Expression::template(
                    nested_template,
                    ValueMode::ImmutableOwned,
                )),
                origin: TemplateSegmentOrigin::Body,
                reactive_subscription: None,
                site_id: expression_site,
            },
            empty_location(),
        ));
        push_template_entry(&mut outer_store, root)
    };
    let outer_template = foreign_parent_template(
        outer_store_id,
        outer_template_id,
        store_owner(&registry, outer_store_id),
        TemplateTirPhase::Composed,
        overlay_set_id,
    );

    assert_slot_insert_fold_error(fold_cross_store_parent(
        registry,
        &outer_template,
        &mut string_table,
    ));
}

// ------------------------------------------------------------------
//  Tests: expression overlay preserved
// ------------------------------------------------------------------

#[test]
fn cross_store_fold_preserves_expression_overlay() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_b_id = registry.allocate_store();
    let _overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    // Build a foreign template whose root is a DynamicExpression carrying
    // int(1). An expression overlay will override it with int(99).
    let (foreign_template_id, expression_site_id) = {
        let mut store_b = registry
            .store_mut(store_b_id)
            .expect("store B should be mutable");
        let site_id = store_b.next_expression_site_id();
        let dynamic = store_b.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::DynamicExpression {
                expression: Box::new(int_expression(1)),
                origin: TemplateSegmentOrigin::Body,
                reactive_subscription: None,
                site_id,
            },
            empty_location(),
        ));
        let root = sequence_with(&mut store_b, dynamic);
        (push_template_entry(&mut store_b, root), site_id)
    };

    // Allocate an expression overlay that overrides the structural int(1)
    // with int(99).
    let expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![(expression_site_id, Box::new(int_expression(99)))],
    });
    let expr_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    });

    let owner = store_owner(&registry, store_b_id);
    let parent = foreign_parent_template(
        store_b_id,
        foreign_template_id,
        owner,
        TemplateTirPhase::Finalized,
        expr_overlay_set_id,
    );

    let emission = fold_cross_store_parent(registry, &parent, &mut string_table)
        .expect("cross-store fold with expression overlay should succeed");

    assert_eq!(
        emission_to_string(emission, &string_table),
        "99",
        "expression overlay should override the structural expression"
    );
}

// ------------------------------------------------------------------
//  Tests: control-flow body preserved
// ------------------------------------------------------------------

#[test]
fn cross_store_fold_folds_foreign_branch_chain() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_b_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let foreign_template_id = {
        let mut store_b = registry
            .store_mut(store_b_id)
            .expect("store B should be mutable");

        // True branch: text "if-true"
        let true_text = text_node(&mut store_b, &mut string_table, "if-true");
        let true_body = sequence_with(&mut store_b, true_text);

        // Fallback: text "if-false"
        let false_text = text_node(&mut store_b, &mut string_table, "if-false");
        let false_body = sequence_with(&mut store_b, false_text);

        let branch = TemplateIrBranch::new(
            TemplateBranchSelector::Bool(bool_expression(true)),
            true_body,
            empty_location(),
        )
        .with_selector_site_id(store_b.next_expression_site_id());

        let branch_chain = store_b.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::BranchChain {
                branches: vec![branch],
                fallback: Some(false_body),
            },
            empty_location(),
        ));
        let root = sequence_with(&mut store_b, branch_chain);
        push_template_entry(&mut store_b, root)
    };

    let owner = store_owner(&registry, store_b_id);
    let parent = foreign_parent_template(
        store_b_id,
        foreign_template_id,
        owner,
        TemplateTirPhase::Composed,
        overlay_set_id,
    );

    let emission = fold_cross_store_parent(registry, &parent, &mut string_table)
        .expect("cross-store fold of branch chain should succeed");

    assert_eq!(
        emission_to_string(emission, &string_table),
        "if-true",
        "foreign branch chain should fold the true branch"
    );
}

// ------------------------------------------------------------------
//  Tests: range loop preserved
// ------------------------------------------------------------------

#[test]
fn cross_store_fold_folds_foreign_range_loop() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_b_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let foreign_template_id = {
        let mut store_b = registry
            .store_mut(store_b_id)
            .expect("store B should be mutable");

        // Loop body: text "x" per iteration.
        let body_text = text_node(&mut store_b, &mut string_table, "x");
        let body = sequence_with(&mut store_b, body_text);

        let header = TemplateLoopHeader::Range {
            bindings: Box::new(LoopBindings {
                item: None,
                index: None,
            }),
            range: Box::new(RangeLoopSpec {
                start: int_expression(0),
                end: int_expression(3),
                end_kind: RangeEndKind::Exclusive,
                step: None,
            }),
        };
        let header_sites = store_b.allocate_loop_header_expression_sites(&header);

        let loop_node = store_b.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Loop {
                header,
                header_sites,
                body,
                aggregate_wrapper: None,
            },
            empty_location(),
        ));
        let root = sequence_with(&mut store_b, loop_node);
        push_template_entry(&mut store_b, root)
    };

    let owner = store_owner(&registry, store_b_id);
    let parent = foreign_parent_template(
        store_b_id,
        foreign_template_id,
        owner,
        TemplateTirPhase::Composed,
        overlay_set_id,
    );

    let emission = fold_cross_store_parent(registry, &parent, &mut string_table)
        .expect("cross-store fold of range loop should succeed");

    assert_eq!(
        emission_to_string(emission, &string_table),
        "xxx",
        "foreign range loop should fold 3 iterations of 'x'"
    );
}

// ------------------------------------------------------------------
//  Tests: third-store qualified descendant
// ------------------------------------------------------------------

/// Builds a three-store chain: parent → store B → store C. The parent carries
/// a `tir_reference` to store B; store B's template contains a `ChildTemplate`
/// node referencing store C. Folding the parent must resolve the third-store
/// descendant through its own qualified ref.
#[test]
fn cross_store_fold_resolves_third_store_descendant() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_b_id = registry.allocate_store();
    let store_c_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    // Store C: a simple text template "from C".
    let store_c_template_id = {
        let mut store_c = registry
            .store_mut(store_c_id)
            .expect("store C should be mutable");
        push_text_template(&mut store_c, &mut string_table, "from C")
    };

    // Store B: a template containing a ChildTemplate referencing store C.
    let store_b_template_id = {
        let mut store_b = registry
            .store_mut(store_b_id)
            .expect("store B should be mutable");
        let child_reference = TemplateTirChildReference::new(
            TemplateRef::new(store_c_id, store_c_template_id),
            TemplateTirPhase::Composed,
            overlay_set_id,
        );
        let occurrence_id = store_b.next_child_template_occurrence_id();
        let child_node = store_b.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::ChildTemplate {
                reference: child_reference,
                occurrence_id,
            },
            empty_location(),
        ));
        let root = sequence_with(&mut store_b, child_node);
        push_template_entry(&mut store_b, root)
    };

    let owner = store_owner(&registry, store_b_id);
    let parent = foreign_parent_template(
        store_b_id,
        store_b_template_id,
        owner,
        TemplateTirPhase::Composed,
        overlay_set_id,
    );

    let emission = fold_cross_store_parent(registry, &parent, &mut string_table)
        .expect("three-store fold should succeed");

    assert_eq!(
        emission_to_string(emission, &string_table),
        "from C",
        "third-store descendant should be resolved through its own qualified ref"
    );
}
