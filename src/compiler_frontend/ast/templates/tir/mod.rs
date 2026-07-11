//! Template IR (TIR) — AST-local intermediate representation for parsed templates.
//!
//! WHAT: TIR is a tree-structured representation of template content that
//! stores all template data in a single `TemplateIrStore` with typed IDs.
//! It is the authoritative source of template semantics during AST processing.
//!
//! WHY: TIR gives composition, formatting, folding, metadata, and HIR handoff
//! a single stable representation, making the data flow explicit and avoiding
//! repeated rebuilding of intermediate template content.
//!
//! ## Ownership contract
//!
//! TIR is owned by the AST template subsystem. It does not own HIR, backend,
//! or public API data. The store is module-scoped and dropped after AST
//! template processing for that module completes. HIR and backends never see
//! TIR IDs, stores, views, overlays, or registry values.
//!
//! ## Phase model
//!
//! `TemplateTirPhase` tracks how far a structural root has progressed through
//! the AST template pipeline:
//!
//! ```text
//! Parsed -> Composed -> Formatted -> Finalized
//! ```
//!
//! Consumers require a minimum phase: folding needs `Composed`, and HIR handoff
//! needs `Finalized`.
//!
//! ## Registry, views, and overlays
//!
//! `TemplateIrRegistry` owns all stores and overlay side tables for one module.
//! `TirView` is the single production read API: it pairs a store-qualified root
//! with a phase and an overlay-set ID, and resolves effective expressions,
//! slot resolutions, and wrapper contexts without mutating shared structural
//! roots.
//!
//! ## Module layout
//!
//! ```text
//! tir/
//! ├── mod.rs                           Module entry and narrow re-exports
//! ├── ids.rs                           Typed store IDs (template, node, wrapper set, slot plan)
//! ├── refs.rs                          Store-qualified final TIR references
//! ├── registry.rs                      Module-local registry for stores, refs, and overlays
//! ├── overlays.rs                      Final overlay set and overlay dimension handles
//! ├── store.rs                         TemplateIrStore — central owned storage
//! ├── node.rs                          TemplateIr, TemplateIrNode, TemplateIrNodeKind
//! ├── summary.rs                       TemplateIrSummary — shape metadata for capacity planning
//! ├── validation.rs                    Structural validation after conversion
//! ├── builder.rs                       Parser-facing mutable facade for direct TIR emission
//! ├── parser_builder_state.rs          In-progress parser TIR accumulator
//! ├── expression_payload_walker.rs     Shared read-only expression-payload traversal
//! ├── construction.rs                  TIR construction helpers (atom-to-node, summary)
//! ├── subtree_copy.rs                  TIR-native active-context subtree copying
//! ├── finalize_sync.rs                 Install finalized TIR roots after render-unit preparation
//! ├── classification.rs                Store-aware TIR shape queries for classification
//! ├── fold.rs                          TIR-native compile-time folding
//! ├── formatter_view.rs                TIR-native formatter feed
//! ├── render_unit.rs                   Render-unit and aggregate-wrapper preparation
//! ├── foreign_slot_insert_proxy.rs      Cross-store SlotInsert proxy construction
//! ├── handoff_materialization.rs       Build owned runtime-template trees for HIR lowering
//! ├── slot_plan.rs                     Runtime slot route handoff side tables
//! ├── slot_composition/                TIR-native slot schema and contribution routing
//! ├── wrapper_sets.rs                  Wrapper set equivalence and reuse
//! └── tests/                           TIR-focused tests
//! ```
//!
//! Only `mod.rs` controls what is re-exported. Submodules keep their internals
//! `pub(crate)` and `mod.rs` selects a narrow API surface.

#[cfg(test)]
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template_types::Template;
#[cfg(test)]
use crate::compiler_frontend::compiler_errors::CompilerError;
#[cfg(test)]
use crate::compiler_frontend::symbols::string_interning::StringTable;
use std::sync::Arc;

// -------------------------
//  Submodules
// -------------------------
//
// Test-only surfaces are gated with `#[cfg(test)]`. Production submodules keep
// their item-level dead-code exceptions local to reserved fields or narrow
// forward-parity hooks with explicit reasons.

mod body_root_ref;
mod classification;
mod construction;
mod contribution_shape;
mod expression_payload_walker;
mod ids;
mod subtree_copy;

// `refs` defines the active store-qualified handle types consumed by the
// registry, view, folding, formatting, metadata, validation, and handoff paths.
mod refs;

// `registry` owns AST-local TIR stores plus overlay storage. Production
// construction, finalization, view, folding, formatting, metadata, validation,
// and slot-composition paths use the active registry surface, while focused
// tests keep currently-unused freeze/domain helpers gated to test builds.
mod registry;

// `overlays` defines active overlay-set and overlay-dimension payloads. The
// production registry/view paths consume expression and slot-resolution
// overlays; focused tests keep the wrapper-context payload surface covered
// until production allocates that overlay dimension.
mod overlays;

// `view` is the central AST-local read API over registry-owned template roots
// and body/root subtrees plus overlay sets. It is consumed by production final
// type-boundary and debug validation as well as tests.
mod view;

// `node` defines the core `TemplateIr`, `TemplateIrNode`, and `TemplateIrNodeKind`
// types consumed by construction, view, folding, formatting, metadata, validation,
// slot composition, and HIR handoff.
mod node;

// `store` owns every TIR template, node, wrapper set, and side-table entry. It is
// the central storage consumed by construction, finalization, view, folding,
// formatting, metadata, validation, slot composition, and HIR handoff.
mod store;

mod summary;

// `validation` is exercised only by focused TIR tests via direct submodule
// imports and is not called in production builds. Gate it under cfg(test) so
// the unused helpers no longer trigger dead-code warnings in normal builds.
#[cfg(test)]
mod validation;

mod builder;
mod construction_context;
mod fold;
mod fold_safety;

// Fold-cache types are production plumbing for `fold_tir_view`.
mod fold_cache;

mod parser_builder_state;

// `formatter_view` produces the TIR-native formatted tree and any formatter
// warnings. The result type is re-exported so production callers can forward
// warnings without reaching into the private formatter-view module.
mod formatter_view;

// `hir_handoff` builds the owned runtime-template and runtime-slot handoff
// payloads consumed by HIR lowering. All re-exported items are consumed by
// AST finalization, reactive metadata, validation, or HIR lowering.
mod handoff_materialization;

mod render_unit;

// `foreign_slot_insert_proxy` builds local proxy templates for cross-store
// SlotInsert heads. Render-unit conversion calls it as orchestration; the
// module owns proxy construction and derived-template creation, routing all
// foreign-store mutation through `TemplateIrRegistry::store_mut`.
mod foreign_slot_insert_proxy;

// Runtime slot-plan handoff side-table types consumed by reactive metadata and HIR lowering.
mod slot_plan;

// `finalize_sync` installs finalized TIR roots after render-unit preparation.
mod finalize_sync;

mod slot_composition;

mod wrapper_sets;

#[cfg(test)]
mod tests;

// -------------------------
//  Re-exports
// -------------------------

// IDs are the primary external interface — consumers use them to reference
// store entries without reaching into the store module directly.
pub(crate) use ids::{
    ChildTemplateOccurrenceId, ExpressionSiteId, TemplateIrId, TemplateIrNodeId,
    TemplateSlotPlanId, TemplateWrapperSetId,
};

// Focused cross-module tests need to construct overlay payloads directly. Keep
// the extra occurrence/resolution types out of the normal production surface.
#[cfg(test)]
pub(crate) use ids::SlotOccurrenceId;

// Read-only expression-payload walker shared by final type-boundary validation
// and debug TypeId validation. The walker is TIR-authoritative and removes the
// duplicated local traversal helpers from AST finalization.
pub(crate) use expression_payload_walker::walk_tir_view_expression_payloads;
// Body-root expression-payload overlay APIs consumed by production AST
// finalization (template expression normalization and reactive annotation).
// Mutation is retained only for focused TIR walker tests.
pub(crate) use expression_payload_walker::{
    TirExpressionPayloadVisitor, collect_effective_tir_body_root_expression_overlay_payloads,
    collect_tir_body_root_expression_overlay_payloads, collect_tir_expression_overlay_payloads,
    walk_tir_expression_payloads,
};

// Store and node types are re-exported so HIR handoff construction and later
// phases can construct TIR data without deep import paths.
pub(crate) use node::TemplateLoopHeaderExpressionSites;
pub(crate) use node::{
    TemplateIr, TemplateIrBranch, TemplateIrNode, TemplateIrNodeKind, TirSlotPlaceholder,
};
pub(crate) use store::{TemplateIrStore, TemplateIrStoreOwner};
pub(crate) use summary::TemplateIrSummary;

// Registry-qualified handles used by production TIR registry and view consumers.
#[cfg(test)]
pub(crate) use refs::TemplateTirChildReference;
pub(crate) use refs::{
    TemplateNodeRef, TemplateRef, TemplateStoreId, TemplateWrapperReference, TemplateWrapperSetRef,
};
pub(crate) use wrapper_sets::wrapper_reference_for_template;

// Body/root reference that carries the view-system identity (store, phase,
// overlay set, source location, and same-store proof) for control-flow bodies.
pub(crate) use body_root_ref::TemplateTirBodyReference;
// Extra store-qualified ref types needed by focused TIR tests. Keep them off
// the normal production surface.
#[cfg(test)]
pub(crate) use refs::TemplateStringDomainId;

// Module-local TIR registry: owns all stores and validates cross-store references.
pub(crate) use registry::TemplateIrRegistry;

// Final overlay set and expression-overlay types consumed by production
// template creation and finalization.
pub(crate) use overlays::{
    TemplateOverlaySet, TemplateOverlaySetId, TirExpressionOverlay, TirWrapperApplicationMode,
    TirWrapperContext, TirWrapperContextOverlay, TirWrapperContextOverlayId,
};
#[cfg(test)]
pub(crate) use overlays::{TirSlotResolution, TirSlotResolutionOverlay};

// Builder: narrow parser-facing facade for direct TIR emission.
pub(crate) use builder::TemplateIrBuilder;

// Finalized TIR state: owns the logic that installs finalized TIR roots after
// render-unit preparation.
pub(crate) use finalize_sync::{
    ControlFlowBodyKind, finalized_control_flow_body_tir_reference,
    replace_control_flow_body_tir_root, replace_loop_aggregate_wrapper_tir_root,
};
#[cfg(test)]
pub(crate) use finalize_sync::{
    build_finalized_tir_root_from_content, build_finalized_tir_root_with_control_flow,
};

// TIR-native child-contribution classification shared by slot composition and
// the atom-based runtime slot planner.
pub(crate) use contribution_shape::{ContributionShape, classify_tir_contribution_node};

// TIR-native slot composition and runtime slot-site planning entry points used
// outside `tir/`. The schema query is shared with atom-level routing; routing
// internals stay in `slot_composition`.
pub(crate) use slot_composition::{
    RoutedTirSlotContributions, TirSlotContributions, TirSlotSchema,
    collect_tir_slot_placeholders_in_order, collect_tir_slot_schema, compose_tir_head_chain,
    compose_tir_head_chain_with_overlays, merge_tir_slot_resolution_overlay_sets,
    wrap_tir_node_in_wrappers,
};

// Parser builder state: the in-progress parser TIR accumulator.
pub(crate) use parser_builder_state::{TemplateParserIrBuilderState, TemplateTirReference};

// Parser-local construction context: owns the in-progress builder state while
// a template is being parsed and shaped, keeping parse-time accumulator state
// off the long-lived `Template` struct.
pub(crate) use construction_context::TemplateConstructionContext;

// TIR construction and active-slot-plan helpers: atom-to-node conversion,
// summary tracking, and subtree copying used by runtime slot planning and
// finalize-sync body roots.
pub(crate) use construction::{
    CurrentStateMaterializationSummary, record_materialization_counters,
};
pub(crate) use subtree_copy::copy_tir_subtree_with_active_slot_plan;

// Classification: store-aware TIR shape queries for template classification.
pub(crate) use classification::{
    MaterializedTirTemplateClassification, classify_effective_tir_view_template,
    classify_materialized_current_tir_template, effective_branch_selector_for_view,
    effective_loop_header_for_view, tir_node_is_const_evaluable_value,
    tir_subtree_contains_slot_insertions, tir_subtree_has_unresolved_slots,
    tir_view_expression_is_const_evaluable_value_with_bindings,
    tir_view_option_capture_presence_is_const_decidable, tir_view_subtree_is_const_evaluable_value,
};
// TirView-aware fold entrypoint is used internally by recursive child folding
// and by production template folding paths.
pub(crate) use fold::fold_tir_view;
pub(crate) use fold::fold_tir_view_read_only;
pub(crate) use fold_safety::tir_view_is_empty_overlay_linear_fold_safe;
#[cfg(test)]
pub(crate) use fold_safety::tir_view_is_expression_overlay_linear_fold_safe;
pub(crate) use fold_safety::tir_view_is_read_only_fold_safe;

// Fold cache: AST-phase-local cache for TIR fold results. The primary cache is
// production state used by template folding and HIR handoff.
pub(crate) use fold_cache::TirFoldCache;

// Fold-cache key/result types are imported directly from `fold_cache` by
// focused TIR tests and are not part of the production re-export surface.

// Formatter view: TIR-native formatter feed.

#[cfg(test)]
pub(crate) use formatter_view::format_tir_template;

// Render-unit helpers: prepare control-flow render roots from TIR-owned nodes.
pub(in crate::compiler_frontend::ast::templates) use render_unit::{
    apply_inherited_child_wrappers_to_body_root, build_branch_body_candidate_from_tir_nodes,
    format_tir_body_root, head_prefix_tir_nodes, prepare_loop_aggregate_wrapper,
    run_tir_formatter_with_warnings, sequence_children,
    trim_whitespace_before_loop_control_boundary,
};

pub(crate) use slot_plan::{
    TemplateSlotContributionSourcePlan, TemplateSlotPlan, TemplateSlotSitePlan,
    TemplateSlotSiteRenderPiece, TemplateSlotSiteRenderPlan, convert_tir_tree_to_active_slot_plan,
};

// Central read API: borrowed views over registry-owned template roots and
// body/root subtrees plus overlay sets. Production consumers and tests
// construct views through the narrow API instead of reaching into raw stores.
pub(crate) use view::{
    FinalizedTirViewAttempt, TemplateTirPhase, TirSubtreeView, TirView,
    finalized_tir_view_for_template,
};

// -------------------------
//  Finalized TIR root access
// -------------------------

/// Returns a same-store `TemplateIrId` for `template`, preferring its finalized
/// TIR reference and building a fresh TIR tree from its stored body content
/// when no same-store reference exists.
///
/// WHAT: production paths should prefer the finalized TIR reference.
///       When the reference is missing or cross-store, this helper builds
///       TIR from the template's stored body content via
///       `build_finalized_tir_root_from_content`.
/// WHY: the `Template` struct carries both a finalized TIR reference and the
///      stored body content used for parser-local shapes. This helper lets
///      callers obtain a same-store TIR root without reaching into those
///      fields directly.
#[cfg(test)]
pub(crate) fn finalized_template_tir_id(
    template: &Template,
    store: &mut TemplateIrStore,
    string_table: &StringTable,
) -> Result<TemplateIrId, TemplateError> {
    // -------------------------
    //  Same-store reference check
    // -------------------------

    // Fast path: reuse the existing same-store TIR reference when it already
    // carries the authoritative root. For templates without control flow the
    // parser-built root is authoritative only when the TIR shape is safe for
    // direct reuse — child templates, slots, insert contributions, wrappers, and
    // formatter-bearing shapes need composition or body-content construction
    // to produce the correct fold/handoff output, so they must fall through to
    // the body-content path. Roots at phase Composed or higher are already
    // safe for reuse; Parsed roots are excluded because they may carry
    // pre-format or pre-composition structure.
    //
    // For templates with control flow, the parser-built root may be a Sequence
    // that also carries the shared head prefix — folding that Sequence would
    // emit the prefix unconditionally (e.g. when no branch is selected). Only
    // reuse the reference when the root is the control-flow node itself, not a
    // Sequence wrapping it alongside head-prefix content.
    if let Some(reference) = &template.tir_reference
        && Arc::ptr_eq(&reference.store_owner, &store.owner())
    {
        let can_reuse = if template.control_flow.is_some() {
            store
                .get_template(reference.root.template_id)
                .is_some_and(|template_ir| {
                    matches!(
                        store.get_node(template_ir.root),
                        Some(node) if matches!(
                            node.kind,
                            TemplateIrNodeKind::BranchChain { .. }
                                | TemplateIrNodeKind::Loop { .. }
                                | TemplateIrNodeKind::LoopControl { .. }
                        )
                    )
                })
        } else {
            // Linear templates can carry two different TIR-derived roots while
            // the content mirror still exists:
            //
            // - TIR-composed roots are the only current structural authority for
            //   head-chain slot routing after content composition was removed.
            // - Formatted and Finalized roots are authoritative once render-unit
            //   preparation has run; they are reusable by phase alone.
            //
            // Phase and same-store identity gate current-state reuse. Parsed roots
            // are excluded because they may still carry pre-format or
            // pre-composition structure that body-content construction must
            // resolve.
            reference.can_reuse_as_linear_current_state()
        };

        if can_reuse {
            return Ok(reference.root.template_id);
        }
    }

    // -------------------------
    //  Construct TIR root
    // -------------------------

    // This path is only reached when no same-store finalized reference is
    // available. When the template carries control flow, build the TIR root
    // from the control-flow body references (which render-unit preparation
    // installs as same-store TIR roots). Otherwise, build from the stored
    // body content.
    let (root, summary) = if let Some(control_flow) = &template.control_flow {
        build_finalized_tir_root_with_control_flow(template, control_flow, store, string_table)
            .map_err(|reason| {
                TemplateError::from(CompilerError::compiler_error(format!(
                    "finalized_template_tir_id: control-flow TIR construction failed: {reason:?}"
                )))
            })?
    } else {
        let content = &template.content;
        build_finalized_tir_root_from_content(
            template,
            store,
            string_table,
            content,
            template.location.to_owned(),
        )
        .map_err(|reason| {
            TemplateError::from(CompilerError::compiler_error(format!(
                "finalized_template_tir_id: content TIR construction failed: {reason:?}"
            )))
        })?
    };

    // -------------------------
    //  Collect child wrappers
    // -------------------------

    // -------------------------
    //  Finalize template record
    // -------------------------

    let mut builder = TemplateIrBuilder::new(store);
    let template_id = builder.finish_template(
        root,
        template.style.to_owned(),
        template.kind.to_owned(),
        summary,
        template.location.to_owned(),
    );

    Ok(template_id)
}

// -------------------------
//  Same-store TIR root resolution
// -------------------------

/// Current same-store TIR roots for a `Template`.
///
/// WHAT: carries the root node IDs that callers should walk, plus the
///       finalized `TemplateIrId` to seed visited-template recursion when one
///       exists.
/// WHY: validation and debug walkers need both the starting nodes and a way
///      to avoid re-walking the seed template when recursing through
///      `ChildTemplate`/`InsertContribution` references.
pub(crate) struct SameStoreTirRoots {
    pub(crate) roots: Vec<TemplateIrNodeId>,
    pub(crate) seed_template_id: Option<TemplateIrId>,
}

/// WHAT: returns the finalized template root when `template.tir_reference`
///       belongs to `store`, or the in-progress parser builder children passed
///       via `builder` while parsing is still in progress. A cross-store or
///       missing TIR proof yields `None`.
/// WHY: centralizes the store-owner proof for finalized TIR references and
///      active parser construction contexts so every validation/debug walker
///      uses the same current TIR authority.
pub(crate) fn current_same_store_tir_roots_for_template(
    template: &Template,
    store: &TemplateIrStore,
    builder: Option<&TemplateParserIrBuilderState>,
) -> Option<SameStoreTirRoots> {
    let store_owner = store.owner();

    if let Some(reference) = &template.tir_reference
        && Arc::ptr_eq(&reference.store_owner, &store_owner)
    {
        let root = store.get_template(reference.root.template_id)?.root;
        return Some(SameStoreTirRoots {
            roots: vec![root],
            seed_template_id: Some(reference.root.template_id),
        });
    }

    if let Some(builder) = builder {
        let builder_owner = builder.store_owner.as_ref()?;
        if Arc::ptr_eq(builder_owner, &store_owner) {
            return Some(SameStoreTirRoots {
                roots: builder.root_children().to_owned(),
                seed_template_id: None,
            });
        }
    }

    None
}
