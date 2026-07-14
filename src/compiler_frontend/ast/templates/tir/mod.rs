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
//! ├── control_flow_roots.rs            Install prepared control-flow body roots
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

// -------------------------
//  Submodules
// -------------------------
//
// Test-only surfaces are gated with `#[cfg(test)]`. Production submodules keep
// their item-level dead-code exceptions local to reserved fields or narrow
// forward-parity hooks with explicit reasons.

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

// `control_flow_roots` installs and resolves finalized control-flow body
// roots after render-unit preparation.
mod control_flow_roots;

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
    ExpressionSiteId, TemplateIrId, TemplateIrNodeId, TemplateSlotPlanId, TemplateWrapperSetId,
};

// Focused cross-module tests need to construct overlay payloads directly. Keep
// the extra occurrence/resolution types out of the normal production surface.
#[cfg(test)]
pub(crate) use ids::SlotOccurrenceId;

// Read-only effective-view expression-payload walker shared by final
// type-boundary validation and debug TypeId validation. The walker is
// TIR-authoritative and removes the duplicated local traversal helpers from
// AST finalization.
pub(crate) use expression_payload_walker::walk_tir_view_expression_payloads;
// Expression-payload overlay APIs consumed by production AST finalization.
// Mutation is retained only for focused TIR walker tests.
pub(crate) use expression_payload_walker::{
    collect_effective_tir_expression_overlay_payloads,
    walk_expression_payloads_with_nested_tir_views,
};

// Store and node types are re-exported so HIR handoff construction and later
// phases can construct TIR data without deep import paths.
pub(crate) use node::TemplateLoopHeaderExpressionSites;
pub(crate) use node::{
    TemplateIr, TemplateIrBranch, TemplateIrNode, TemplateIrNodeKind, TirSlotPlaceholder,
};
pub(crate) use store::TemplateIrStore;
// `TemplateIrStoreOwner` is only needed by focused TIR tests through this
// re-export; production code imports it directly from `store`.
#[cfg(test)]
pub(crate) use store::TemplateIrStoreOwner;
pub(crate) use summary::TemplateIrSummary;

// Registry-qualified handles used by production TIR registry and view consumers.
#[cfg(test)]
pub(crate) use refs::TemplateTirChildReference;
pub(crate) use refs::{TemplateNodeRef, TemplateRef, TemplateStoreId, TemplateWrapperReference};
pub(crate) use wrapper_sets::{attach_wrapper_context_overlay, wrapper_reference_for_template};

// Extra store-qualified ref types needed by focused TIR tests. Keep them off
// the normal production surface.
#[cfg(test)]
pub(crate) use refs::TemplateStringDomainId;

// Module-local TIR registry: owns all stores and validates cross-store references.
pub(crate) use registry::{RegisteredTemplateIrStore, TemplateIrRegistry};

// Final overlay set and expression-overlay types consumed by production
// template creation and finalization.
pub(crate) use overlays::{TemplateOverlaySet, TemplateOverlaySetId, TirExpressionOverlay};
#[cfg(test)]
pub(crate) use overlays::{TirSlotResolution, TirSlotResolutionOverlay};

// Builder: narrow parser-facing facade for direct TIR emission.
pub(crate) use builder::TemplateIrBuilder;

// Control-flow root installation after render-unit preparation.
pub(crate) use control_flow_roots::{
    ControlFlowBodyKind, replace_control_flow_body_tir_root,
    replace_loop_aggregate_wrapper_tir_root,
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
pub(crate) use parser_builder_state::TemplateTirReference;

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
    TirTemplateClassification, classify_effective_tir_view_template,
    effective_branch_selector_for_view, effective_loop_header_for_view,
    refresh_kind_from_classification, tir_node_is_const_evaluable_value,
    tir_subtree_has_unresolved_slots, tir_view_expression_is_const_evaluable_value_with_bindings,
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

// Central read API over registry-owned template roots and overlay sets.
pub(crate) use view::{TemplateTirPhase, TirView, finalized_tir_view_for_template};
