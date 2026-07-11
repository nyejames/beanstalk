//! Template render-unit preparation for linear and control-flow templates.
//!
//! WHAT: Refreshes control-flow body roots in TIR and installs the formatted
//! TIR reference for linear templates. Linear templates no longer use a
//! compatibility `TemplateContent` formatter mirror; TIR formatting is the sole
//! production authority.
//!
//! WHY: Normal templates, template `if` branches, and template `loop` bodies
//! all need the same composition and formatting rules. Keeping the render-unit
//! shaping here prevents control-flow support from growing a parallel template
//! pipeline.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::Style;
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateControlFlow, TemplateControlFlowBodyScratch, TemplateControlFlowTirReference,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::tir::{
    ControlFlowBodyKind, TemplateConstructionContext, TemplateIr, TemplateIrNodeId,
    TemplateIrStore, TemplateOverlaySetId, TemplateParserIrBuilderState, TemplateRef,
    TemplateTirPhase, TemplateTirReference, TemplateWrapperReference, TirView,
    apply_inherited_child_wrappers_to_body_root, build_branch_body_candidate_from_tir_nodes,
    compose_tir_head_chain, current_same_store_tir_roots_for_template, format_tir_body_root,
    head_prefix_tir_nodes, prepare_loop_aggregate_wrapper, replace_control_flow_body_tir_root,
    replace_loop_aggregate_wrapper_tir_root, run_tir_formatter_with_warnings, sequence_children,
    trim_whitespace_before_loop_control_boundary,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use std::sync::Arc;

/// Installs formatter output as the current same-store TIR reference.
///
/// WHAT: runs the TIR formatter adapter over the template's current referenced
///       root and stores the append-only formatted root as a new TIR template.
/// WHY: linear templates now carry their formatted root directly in TIR.
///      The legacy `TemplateContent` formatter mirror is no longer the
///      production authority for linear bodies.
pub(in crate::compiler_frontend::ast::templates) fn install_formatted_tir_reference_for_linear_template(
    template: &mut Template,
    style: &Style,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<(), TemplateError> {
    if template.control_flow.is_some() {
        return Ok(());
    }

    let Some(reference) = template.tir_reference.clone() else {
        return Ok(());
    };

    if reference.phase.is_at_least(TemplateTirPhase::Formatted) {
        return Ok(());
    }

    let store = context.template_ir_store.borrow();
    let store_owner = store.owner();
    if !Arc::ptr_eq(&reference.store_owner, &store_owner) {
        return Ok(());
    }

    let original_template = store
        .get_template(reference.root.template_id)
        .cloned()
        .ok_or_else(|| {
            CompilerError::compiler_error(
                "Template TIR reference pointed at a missing same-store template.",
            )
        })?;

    drop(store);

    let formatter_result = {
        let registry = context.template_ir_registry.borrow();
        let root_ref = reference.root;
        let view = TirView::new(
            &registry,
            root_ref,
            reference.phase,
            reference.overlay_set_id,
        )
        .map_err(TemplateError::from)?;

        run_tir_formatter_with_warnings(&view, style, context, string_table)?
    };

    let mut summary = original_template.summary;
    summary.has_formatter = false;

    let mut store = context.template_ir_store.borrow_mut();
    let formatted_template_id = store.push_template(TemplateIr::new(
        formatter_result.root,
        original_template.style,
        original_template.kind,
        summary,
        original_template.location,
    ));

    template.tir_reference = Some(TemplateTirReference {
        root: TemplateRef::new(context.template_ir_store_id, formatted_template_id),
        store_owner,
        is_composed: reference.is_composed,
        phase: TemplateTirPhase::Formatted,
        overlay_set_id: reference.overlay_set_id,
    });

    Ok(())
}

struct TirBodyRootInput<'a> {
    owning_template: &'a Template,
    root_children: &'a [TemplateIrNodeId],
    style: &'a Style,
    child_wrappers: &'a [TemplateWrapperReference],
    body_root: TemplateIrNodeId,
    body_kind: ControlFlowBodyKind,
    body_phase: TemplateTirPhase,
    builder: Option<&'a TemplateParserIrBuilderState>,
}

/// Tries to derive a branch/fallback body TIR root from parser-emitted
/// head-prefix nodes plus a parser-emitted body root.
///
/// WHAT: reuses the owning template's parser-emitted head-prefix TIR nodes,
///       formats the parser-emitted body root, applies inherited wrappers,
///       builds a temporary template combining head prefix and body, and
///       composes it so head-chain wrappers apply to the body. Returns the
///       installed body reference when the root was replaced.
/// WHY: with control-flow bodies emitted directly into TIR, body-root
///      preparation no longer needs a content mirror; it can reuse the
///      parser-emitted body sequence and the owning template's head-prefix nodes.
fn try_sync_control_flow_body_tir_from_tir(
    input: TirBodyRootInput<'_>,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Option<TemplateControlFlowTirReference> {
    let TirBodyRootInput {
        owning_template,
        root_children,
        style,
        child_wrappers,
        body_root,
        body_kind,
        body_phase,
        builder,
    } = input;

    // Derive the head-prefix TIR nodes from the owning template's parser-emitted
    // root children. These are the same nodes the parser materialized from the
    // shared head-prefix atoms, so reusing them avoids rebuilding TIR from
    // `TemplateContent`.
    let head_prefix_nodes = {
        let store = context.template_ir_store.borrow();
        head_prefix_tir_nodes(&store, root_children)
    };

    // The body is already a parser-emitted TIR sequence node. Formatting,
    // wrapper application, and head-chain composition operate on this root
    // directly without content-to-TIR materialization.

    // Format the body root before inherited wrappers and head-chain composition
    // so the final body tree carries formatted text while wrappers remain opaque
    // anchors. The store borrow is released around this call because the TIR
    // formatter mutates the registry-owned store through `TirView`.
    let body_root = format_tir_body_root(body_root, style, context, string_table).ok()?;

    let mut store = context.template_ir_store.borrow_mut();
    let registry = context.template_ir_registry.borrow();

    let body_root = apply_inherited_child_wrappers_to_body_root(
        body_root,
        child_wrappers,
        &registry,
        &mut store,
        string_table,
    )
    .ok()?;

    let body_children = sequence_children(&store, body_root)?;

    // Build a temporary template combining the converted head-prefix nodes with
    // the body children, then compose so head-chain wrappers apply to the body
    // exactly as the atom-level path did.
    let candidate_id = build_branch_body_candidate_from_tir_nodes(
        &head_prefix_nodes,
        &body_children,
        &mut store,
        &registry,
    )
    .ok()?;
    let composed_root =
        compose_tir_head_chain(&mut store, candidate_id, string_table, true).ok()?;

    replace_control_flow_body_tir_root(
        owning_template,
        &mut store,
        body_kind,
        composed_root,
        body_phase,
        builder,
    )
}

/// Applies inherited `$children(..)` wrapper templates to direct child-template
/// occurrences in a control-flow body root.
///
/// WHAT: TIR-native equivalent of `apply_inherited_child_templates_to_atoms` for
///       body-only TIR roots. Walks the top-level children of the body sequence,
///       skipping `$fresh` children and not recursing into grandchildren.
///       Non-control-flow direct children are wrapped through
///       `wrap_tir_node_in_wrappers`; control-flow direct children receive the
///       inherited wrappers through a derived wrapper template whose
///       `conditional_child_wrapper_set` carries the inherited wrappers, matching
///       the atom-level `attach_conditional_child_wrappers` behavior.
/// WHY: lets `try_sync_control_flow_body_tir_from_tir` cover bodies with inherited
///      `$children(...)` wrappers without falling back to the content mirror.
struct ControlFlowBodyPreparationContext<'a> {
    owning_template: &'a Template,
    construction_context: &'a TemplateConstructionContext,
    style: &'a Style,
    child_wrappers: &'a [TemplateWrapperReference],
    context: &'a ScopeContext,
    string_table: &'a mut StringTable,
}

/// Prepares one branch or fallback body.
///
/// WHAT: derives the body TIR root from parser-emitted head-prefix nodes plus
///       the body-only scratch, formats that root via the TIR-native formatter,
///       then applies inherited wrappers and head-chain composition. Runtime
///       slot handoffs are recovered through body-only current-state
///       materialization before the head-prefix candidate is composed.
/// WHY: branch and fallback bodies share the same head-prefix + body shape, so
///      one preparation owner keeps TIR formatting, wrapper application, and
///      root refresh in sync without duplicating the flow per arm.
fn prepare_branch_or_fallback_body(
    ctx: ControlFlowBodyPreparationContext<'_>,
    body_root: TemplateIrNodeId,
    previous_body_tir_reference: Option<TemplateControlFlowTirReference>,
    body_kind: ControlFlowBodyKind,
) -> Result<(bool, Option<TemplateControlFlowTirReference>), TemplateError> {
    let ControlFlowBodyPreparationContext {
        owning_template,
        construction_context,
        style,
        child_wrappers,
        context,
        string_table,
    } = ctx;

    // Collect parser-emitted root children before the mutable store borrow so
    // the TIR-derived path can reuse same-store head-prefix nodes.
    let root_children = construction_context.builder().root_children().to_vec();
    let body_phase = control_flow_body_phase();

    // Derive the body TIR root from parser-emitted head-prefix nodes plus the
    // body scratch. Formatting happens on the TIR body root before inherited
    // wrappers and head-chain composition; runtime slot handoffs are recovered
    // through body-only current-state materialization.
    let refreshed_ref = try_sync_control_flow_body_tir_from_tir(
        TirBodyRootInput {
            owning_template,
            root_children: &root_children,
            style,
            child_wrappers,
            body_root,
            body_kind,
            body_phase,
            builder: Some(construction_context.builder()),
        },
        context,
        string_table,
    );

    let refreshed = refreshed_ref.is_some();
    let refreshed_ref = refreshed_ref.or(previous_body_tir_reference);

    Ok((refreshed, refreshed_ref))
}

/// Tries to derive a loop body TIR root from the body-only scratch.
///
/// WHAT: materializes the body scratch into a flat TIR node list, formats that
///       TIR root, trims whitespace-only text nodes before any top-level
///       loop-control marker, applies inherited `$children(..)` wrappers to
///       direct child-template occurrences natively in TIR, and installs the
///       result as the loop's body root.
/// WHY: loop bodies do not carry the owning template's shared head prefix (that
///      wraps the aggregate output), so they can skip head-chain composition.
///      Runtime slot handoffs and loop-control markers are recovered through
///      body-only current-state materialization, and loop-control boundary
///      whitespace trimming is applied as a TIR-local transform so the loop
///      body root owns the behavior without a finalized-content mirror
///      fallback.
fn try_sync_loop_body_tir_from_tir(
    owning_template: &Template,
    style: &Style,
    child_wrappers: &[TemplateWrapperReference],
    body_root: TemplateIrNodeId,
    builder: Option<&TemplateParserIrBuilderState>,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Option<TemplateControlFlowTirReference> {
    // The loop body is already a parser-emitted TIR sequence node; formatting,
    // loop-control boundary trimming, and wrapper application operate on it
    // directly without content-to-TIR materialization.

    // Release the store borrow around the TIR formatter call; the formatter
    // authority mutates the registry-owned store through `TirView`.
    let body_root = format_tir_body_root(body_root, style, context, string_table).ok()?;

    let mut store = context.template_ir_store.borrow_mut();
    let body_root =
        trim_whitespace_before_loop_control_boundary(body_root, &mut store, string_table);

    let registry = context.template_ir_registry.borrow();
    let body_root = apply_inherited_child_wrappers_to_body_root(
        body_root,
        child_wrappers,
        &registry,
        &mut store,
        string_table,
    )
    .ok()?;

    replace_control_flow_body_tir_root(
        owning_template,
        &mut store,
        ControlFlowBodyKind::LoopBody,
        body_root,
        control_flow_body_phase(),
        builder,
    )
}

fn control_flow_body_phase() -> TemplateTirPhase {
    // Every control-flow body root that is successfully refreshed goes through
    // `format_tir_body_root`, which applies the same TIR formatter adapter used
    // by linear templates. For explicit-formatter bodies that produces formatter
    // output; for no-formatter bodies it performs default-whitespace
    // normalization and `$raw` preservation. In both cases the resulting body
    // root has passed formatter preparation, so the phase is recorded as
    // `Formatted`.
    TemplateTirPhase::Formatted
}

/// Prepares a template `loop` body.
///
/// WHAT: derives the body TIR root from the body-only scratch, formats that root
///       via the TIR-native formatter, then applies inherited wrappers. Loop
///       bodies intentionally skip head-prefix composition because the owning
///       head wraps the aggregate output once, not each iteration. Runtime slot
///       handoffs and loop-control markers are routed through body-only
///       current-state TIR materialization, and loop-control boundary whitespace
///       trimming is applied as a TIR-local transform.
/// WHY: the loop body root owns loop-control boundary whitespace trimming
///      natively in TIR; atom-level content mirror formatting is no longer
///      part of the production path.
fn prepare_loop_body(
    ctx: ControlFlowBodyPreparationContext<'_>,
    body_root: TemplateIrNodeId,
    previous_body_tir_reference: Option<TemplateControlFlowTirReference>,
) -> Result<(bool, Option<TemplateControlFlowTirReference>), TemplateError> {
    let ControlFlowBodyPreparationContext {
        owning_template,
        construction_context,
        style,
        child_wrappers,
        context,
        string_table,
        ..
    } = ctx;

    let refreshed_ref = try_sync_loop_body_tir_from_tir(
        owning_template,
        style,
        child_wrappers,
        body_root,
        Some(construction_context.builder()),
        context,
        string_table,
    );

    let refreshed = refreshed_ref.is_some();
    let refreshed_ref = refreshed_ref.or(previous_body_tir_reference);

    Ok((refreshed, refreshed_ref))
}

/// Applies composition and formatting to a structured control-flow template in
/// place.
///
/// For `if`, each branch is a complete TIR render unit that includes the shared
/// head prefix. For `loop`, the per-iteration body is finalized independently
/// and the parser-emitted head prefix becomes an aggregate-wrapper TIR subtree,
/// so later folding and lowering apply it once around the aggregate.
///
/// After each body is formatted, this refreshes the corresponding
/// TIR-authoritative body root and drops the parsed content scratch.
pub(in crate::compiler_frontend::ast::templates) struct ControlFlowRenderUnitRequest<'a> {
    pub(in crate::compiler_frontend::ast::templates) control_flow_body_scratch:
        TemplateControlFlowBodyScratch,
    pub(in crate::compiler_frontend::ast::templates) style: &'a Style,
    pub(in crate::compiler_frontend::ast::templates) child_wrappers:
        &'a [TemplateWrapperReference],
    pub(in crate::compiler_frontend::ast::templates) context: &'a ScopeContext,
    pub(in crate::compiler_frontend::ast::templates) string_table: &'a mut StringTable,
}

pub(in crate::compiler_frontend::ast::templates) fn prepare_control_flow_render_units(
    owning_template: &mut Template,
    construction_context: &mut TemplateConstructionContext,
    request: ControlFlowRenderUnitRequest<'_>,
) -> Result<(), TemplateError> {
    // Take the control-flow value out of the owning template so the inner
    // preparation work can mutably borrow both the control-flow value and the
    // owning template (for parser-TIR body sync) without a simultaneous borrow
    // of the same field.
    let Some(mut control_flow) = owning_template.control_flow.take() else {
        return Err(CompilerError::compiler_error(
            "prepare_control_flow_render_units called on template without control flow",
        )
        .into());
    };

    let result = prepare_control_flow_render_units_inner(
        &mut control_flow,
        owning_template,
        construction_context,
        request,
    );

    // Always restore the control-flow value, even if inner preparation failed.
    owning_template.control_flow = Some(control_flow);
    result
}

/// Inner body of [`prepare_control_flow_render_units`].
///
fn prepare_control_flow_render_units_inner(
    control_flow: &mut TemplateControlFlow,
    owning_template: &mut Template,
    construction_context: &mut TemplateConstructionContext,
    request: ControlFlowRenderUnitRequest<'_>,
) -> Result<(), TemplateError> {
    let ControlFlowRenderUnitRequest {
        control_flow_body_scratch,
        style,
        child_wrappers,
        context,
        string_table,
    } = request;

    let mut body_sync_attempted = false;
    let mut all_bodies_refreshed = true;

    match (control_flow, control_flow_body_scratch) {
        (
            TemplateControlFlow::BranchChain(branch_chain),
            TemplateControlFlowBodyScratch::BranchChain(scratch),
        ) => {
            if branch_chain.branches.len() != scratch.branches.len() {
                return Err(CompilerError::compiler_error(
                    "Template branch body scratch count did not match parsed branch count.",
                )
                .into());
            }

            for (index, (branch, body_content)) in branch_chain
                .branches
                .iter_mut()
                .zip(scratch.branches)
                .enumerate()
            {
                let (refreshed, refreshed_ref) = prepare_branch_or_fallback_body(
                    ControlFlowBodyPreparationContext {
                        owning_template,
                        construction_context,
                        style,
                        child_wrappers,
                        context,
                        string_table,
                    },
                    body_content,
                    branch.body_tir_reference.clone(),
                    ControlFlowBodyKind::Branch { index },
                )?;
                body_sync_attempted = true;
                all_bodies_refreshed &= refreshed;
                branch.body_tir_reference = refreshed_ref;
            }

            match (&mut branch_chain.fallback, scratch.fallback) {
                (Some(fallback), Some(fallback_body_content)) => {
                    let (refreshed, refreshed_ref) = prepare_branch_or_fallback_body(
                        ControlFlowBodyPreparationContext {
                            owning_template,
                            construction_context,
                            style,
                            child_wrappers,
                            context,
                            string_table,
                        },
                        fallback_body_content,
                        fallback.body_tir_reference.clone(),
                        ControlFlowBodyKind::Fallback,
                    )?;
                    body_sync_attempted = true;
                    all_bodies_refreshed &= refreshed;
                    fallback.body_tir_reference = refreshed_ref;
                }

                (None, None) => {}

                _ => {
                    return Err(CompilerError::compiler_error(
                        "Template fallback body scratch did not match parsed fallback state.",
                    )
                    .into());
                }
            }
        }

        (
            TemplateControlFlow::Loop(template_loop),
            TemplateControlFlowBodyScratch::Loop(scratch),
        ) => {
            // Prepare the loop body with the same format-once + TIR pattern used
            // by branch/fallback bodies. Loop bodies skip the shared head prefix
            // because the owning head wraps the aggregate output once, not each
            // iteration.
            let (refreshed, refreshed_ref) = prepare_loop_body(
                ControlFlowBodyPreparationContext {
                    owning_template,
                    construction_context,
                    style,
                    child_wrappers,
                    context,
                    string_table,
                },
                scratch.body,
                template_loop.body_tir_reference.clone(),
            )?;
            body_sync_attempted = true;
            all_bodies_refreshed &= refreshed;
            template_loop.body_tir_reference = refreshed_ref;

            // Collect the parser-emitted root children before the mutable store
            // borrow so the aggregate wrapper can reuse existing same-store TIR
            // head-prefix nodes instead of rebuilding from content atoms.
            let root_children = construction_context.builder().root_children().to_vec();

            let mut template_ir_store = context.template_ir_store.borrow_mut();
            let registry = context.template_ir_registry.borrow();
            let aggregate_wrapper = prepare_loop_aggregate_wrapper(
                &root_children,
                string_table,
                &registry,
                &mut template_ir_store,
            )?;
            let aggregate_wrapper_tir_reference =
                TemplateControlFlowTirReference::with_full_identity(
                    template_ir_store.owner(),
                    template_ir_store.store_id(),
                    aggregate_wrapper.tir_root,
                    TemplateTirPhase::Composed,
                    TemplateOverlaySetId::empty(),
                    template_loop.location.to_owned(),
                );
            template_loop.aggregate_wrapper_tir_reference = Some(aggregate_wrapper_tir_reference);

            // Install the composed TIR aggregate-wrapper subtree onto the owning
            // `Loop` node.
            let _aggregate_wrapper_installed = replace_loop_aggregate_wrapper_tir_root(
                owning_template,
                &mut template_ir_store,
                aggregate_wrapper.tir_root,
                Some(construction_context.builder()),
            );
        }

        _ => {
            return Err(CompilerError::compiler_error(
                "Template control-flow body scratch did not match parsed control-flow kind.",
            )
            .into());
        }
    }

    if body_sync_attempted && all_bodies_refreshed {
        // All bodies were refreshed to finalized parser-TIR roots, so the owner
        // summary no longer represents a formatter-pending surface. Keep
        // `has_control_flow` intact; that is the correct blocker for the owner.
        //
        // The builder-state suppression flag is the active owner: `finish` reads
        // `suppress_formatter_summary_on_finish` to decide whether a single
        // control-flow child should become the direct root instead of a
        // Sequence-wrapped root. The old pre-finish store mutation and
        // sequence-unwrap helper were ineffective because `tir_reference` does
        // not exist until `finish` runs.
        construction_context
            .builder_mut()
            .clear_has_formatter_summary();
    }

    Ok(())
}

pub(crate) fn template_contains_control_flow(
    template: &crate::compiler_frontend::ast::templates::template_types::Template,
    template_ir_store: &TemplateIrStore,
    builder: Option<&TemplateParserIrBuilderState>,
) -> bool {
    if template.control_flow.is_some() {
        return true;
    }

    let Some(roots) =
        current_same_store_tir_roots_for_template(template, template_ir_store, builder)
    else {
        return false;
    };

    template_ir_store.subtree_contains_control_flow_from_roots(&roots.roots)
}
