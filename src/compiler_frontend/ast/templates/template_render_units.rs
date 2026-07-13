//! Template render-unit preparation for linear and control-flow templates.
//!
//! WHAT: Prepares control-flow body roots in TIR and installs the formatted
//! TIR reference for linear templates. Linear templates format directly from
//! a TIR view, making TIR formatting the sole production authority.
//!
//! WHY: Normal templates, template `if` branches, and template `loop` bodies
//! all need the same composition and formatting rules. Keeping the render-unit
//! shaping here prevents control-flow support from growing a parallel template
//! pipeline.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::Style;
use crate::compiler_frontend::ast::templates::tir::{
    ControlFlowBodyKind, TemplateConstructionContext, TemplateIr, TemplateIrBranch,
    TemplateIrNodeId, TemplateIrNodeKind, TemplateIrStore, TemplateParserIrBuilderState,
    TemplateRef, TemplateTirPhase, TemplateTirReference, TemplateWrapperReference, TirView,
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
///      TIR formatting is the production authority for linear bodies.
pub(in crate::compiler_frontend::ast::templates) fn install_formatted_tir_reference_for_linear_template(
    tir_reference: &mut TemplateTirReference,
    has_control_flow: bool,
    style: &Style,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<(), TemplateError> {
    if has_control_flow {
        return Ok(());
    }

    let reference = tir_reference.clone();

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

    *tir_reference = TemplateTirReference {
        root: TemplateRef::new(context.template_ir_store_id, formatted_template_id),
        store_owner,
        is_composed: reference.is_composed,
        phase: TemplateTirPhase::Formatted,
        overlay_set_id: reference.overlay_set_id,
    };

    Ok(())
}

struct TirBodyRootInput<'a> {
    root_children: &'a [TemplateIrNodeId],
    style: &'a Style,
    child_wrappers: &'a [TemplateWrapperReference],
    body_root: TemplateIrNodeId,
    body_kind: ControlFlowBodyKind,
    control_flow_node_id: TemplateIrNodeId,
}

/// Prepares a branch/fallback body TIR root from parser-emitted head-prefix
/// nodes plus the parsed body root.
///
/// WHAT: reuses the owning template's parser-emitted head-prefix TIR nodes,
///       formats the parsed body root, applies inherited wrappers, builds a
///       temporary template combining head prefix and body, and composes it so
///       head-chain wrappers apply to the body.
/// WHY: with control-flow bodies emitted directly into TIR, body-root
///      preparation reuses the parser-emitted body sequence and the owning
///      template's head-prefix nodes. The prepared root is installed directly
///      onto the owning TIR control-flow node; a missing store/root or
///      impossible replacement is an internal `CompilerError`, not a silent
///      fallback.
fn prepare_branch_body_tir_root(
    input: TirBodyRootInput<'_>,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<(), TemplateError> {
    let TirBodyRootInput {
        root_children,
        style,
        child_wrappers,
        body_root,
        body_kind,
        control_flow_node_id,
    } = input;

    // Derive the head-prefix TIR nodes from the owning template's parser-emitted
    // root children. These are the same nodes the parser materialized from the
    // shared head-prefix atoms, so reusing them avoids rebuilding TIR from
    // the formatted TIR root.
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
    let body_root = format_tir_body_root(body_root, style, context, string_table)?;

    let mut store = context.template_ir_store.borrow_mut();
    let registry = context.template_ir_registry.borrow();

    let body_root = apply_inherited_child_wrappers_to_body_root(
        body_root,
        child_wrappers,
        &registry,
        &mut store,
        string_table,
    )?;

    let body_children = sequence_children(&store, body_root).ok_or_else(|| {
        CompilerError::compiler_error("Branch body preparation produced a non-sequence body root.")
    })?;

    // Build a temporary template combining the converted head-prefix nodes with
    // the body children, then compose so head-chain wrappers apply to the body
    // exactly as the atom-level path did.
    let candidate_id = build_branch_body_candidate_from_tir_nodes(
        &head_prefix_nodes,
        &body_children,
        &mut store,
        &registry,
    )?;
    let composed_root = compose_tir_head_chain(&mut store, candidate_id, string_table, true)?;

    replace_control_flow_body_tir_root(&mut store, control_flow_node_id, body_kind, composed_root)
        .map_err(TemplateError::from)
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
/// WHY: lets `prepare_branch_body_tir_root` keep inherited
///      `$children(...)` wrapper handling inside the TIR owner.
struct ControlFlowBodyPreparationContext<'a> {
    construction_context: &'a TemplateConstructionContext,
    style: &'a Style,
    child_wrappers: &'a [TemplateWrapperReference],
    context: &'a ScopeContext,
    string_table: &'a mut StringTable,
}

/// Prepares one branch or fallback body.
///
/// WHAT: reads the parsed body root node ID from the owning TIR control-flow
///       node, derives the prepared TIR root from parser-emitted head-prefix
///       nodes plus that body root, formats it via the TIR-native formatter,
///       then applies inherited wrappers and head-chain composition. The
///       prepared root is installed directly onto the TIR control-flow node.
/// WHY: branch and fallback bodies share the same head-prefix + body shape, so
///      one preparation owner keeps TIR formatting, wrapper application, and
///      root installation in sync without duplicating the flow per arm.
fn prepare_branch_or_fallback_body(
    ctx: ControlFlowBodyPreparationContext<'_>,
    control_flow_node_id: TemplateIrNodeId,
    body_root: TemplateIrNodeId,
    body_kind: ControlFlowBodyKind,
) -> Result<(), TemplateError> {
    let ControlFlowBodyPreparationContext {
        construction_context,
        style,
        child_wrappers,
        context,
        string_table,
    } = ctx;

    // Collect parser-emitted root children before the mutable store borrow so
    // the TIR-derived path can reuse same-store head-prefix nodes.
    let root_children = construction_context.builder().root_children().to_vec();

    prepare_branch_body_tir_root(
        TirBodyRootInput {
            root_children: &root_children,
            style,
            child_wrappers,
            body_root,
            body_kind,
            control_flow_node_id,
        },
        context,
        string_table,
    )
}

/// Prepares a loop body TIR root from the parsed body root.
///
/// WHAT: formats the parsed TIR body root, trims whitespace-only text nodes
///       before any top-level loop-control marker, applies inherited
///       `$children(..)` wrappers to direct child-template occurrences natively
///       in TIR, and installs the result as the loop's body root.
/// WHY: loop bodies do not carry the owning template's shared head prefix (that
///      wraps the aggregate output), so they can skip head-chain composition.
///      Loop-control boundary whitespace trimming is applied as a TIR-local
///      transform so the loop body root owns the behavior. The prepared root is
///      installed directly onto the TIR `Loop` node; a missing store/root or
///      impossible replacement is an internal `CompilerError`, not a silent
///      fallback.
fn prepare_loop_body_tir_root(
    control_flow_node_id: TemplateIrNodeId,
    style: &Style,
    child_wrappers: &[TemplateWrapperReference],
    body_root: TemplateIrNodeId,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<(), TemplateError> {
    // The loop body is already a parser-emitted TIR sequence node; formatting,
    // loop-control boundary trimming, and wrapper application operate on it
    // directly without content-to-TIR materialization.

    // Release the store borrow around the TIR formatter call; the formatter
    // authority mutates the registry-owned store through `TirView`.
    let body_root = format_tir_body_root(body_root, style, context, string_table)?;

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
    )?;

    replace_control_flow_body_tir_root(
        &mut store,
        control_flow_node_id,
        ControlFlowBodyKind::LoopBody,
        body_root,
    )
    .map_err(TemplateError::from)
}

/// Prepares a template `loop` body.
///
/// WHAT: reads the parsed body root node ID from the owning TIR `Loop` node,
///       formats that root via the TIR-native formatter, then applies inherited
///       wrappers. Loop bodies intentionally skip head-prefix composition
///       because the owning head wraps the aggregate output once, not each
///       iteration. Loop-control boundary whitespace trimming is applied as a
///       TIR-local transform.
/// WHY: the loop body root owns loop-control boundary whitespace trimming and
///      formatting natively in TIR.
fn prepare_loop_body(
    ctx: ControlFlowBodyPreparationContext<'_>,
    control_flow_node_id: TemplateIrNodeId,
    body_root: TemplateIrNodeId,
) -> Result<(), TemplateError> {
    let ControlFlowBodyPreparationContext {
        style,
        child_wrappers,
        context,
        string_table,
        ..
    } = ctx;

    prepare_loop_body_tir_root(
        control_flow_node_id,
        style,
        child_wrappers,
        body_root,
        context,
        string_table,
    )
}

/// Applies composition and formatting to a structured control-flow template.
///
/// For `if`, each branch is a complete TIR render unit that includes the shared
/// head prefix. For `loop`, the per-iteration body is finalized independently
/// and the parser-emitted head prefix becomes an aggregate-wrapper TIR subtree,
/// so later folding and lowering apply it once around the aggregate.
///
/// After each body is formatted, this installs the prepared body root directly
/// onto the owning parser TIR control-flow node. The control-flow node and its
/// body node IDs are read from the TIR store through the parser builder state,
/// not from a durable AST carrier.
pub(in crate::compiler_frontend::ast::templates) struct ControlFlowRenderUnitRequest<'a> {
    pub(in crate::compiler_frontend::ast::templates) style: &'a Style,
    pub(in crate::compiler_frontend::ast::templates) child_wrappers:
        &'a [TemplateWrapperReference],
    pub(in crate::compiler_frontend::ast::templates) context: &'a ScopeContext,
    pub(in crate::compiler_frontend::ast::templates) string_table: &'a mut StringTable,
}

pub(in crate::compiler_frontend::ast::templates) fn prepare_control_flow_render_units(
    construction_context: &mut TemplateConstructionContext,
    request: ControlFlowRenderUnitRequest<'_>,
) -> Result<(), TemplateError> {
    let ControlFlowRenderUnitRequest {
        style,
        child_wrappers,
        context,
        string_table,
    } = request;

    // Locate the owning TIR control-flow node through the parser builder state.
    // The body parser already constructed the BranchChain/Loop node and its
    // body node IDs in the TIR store; render-unit preparation reads and
    // updates them directly.
    let control_flow_node_id = {
        let store = context.template_ir_store.borrow();
        construction_context
            .builder()
            .control_flow_node_id(&store)
            .ok_or_else(|| {
                CompilerError::compiler_error(
                    "prepare_control_flow_render_units called on template without a TIR control-flow node",
                )
            })?
    };

    let kind = {
        let store = context.template_ir_store.borrow();
        let node = store.get_node(control_flow_node_id).ok_or_else(|| {
            CompilerError::compiler_error(
                "Control-flow node disappeared from the TIR store during render-unit preparation.",
            )
        })?;
        node.kind.clone()
    };

    match kind {
        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            prepare_branch_chain_render_units(
                control_flow_node_id,
                &branches,
                fallback,
                ControlFlowBodyPreparationContext {
                    construction_context,
                    style,
                    child_wrappers,
                    context,
                    string_table,
                },
            )?;
        }

        TemplateIrNodeKind::Loop { body, .. } => {
            prepare_loop_render_units(
                control_flow_node_id,
                body,
                construction_context,
                style,
                child_wrappers,
                context,
                string_table,
            )?;
        }

        _ => {
            return Err(CompilerError::compiler_error(
                "Control-flow node was neither a BranchChain nor a Loop during render-unit preparation.",
            )
            .into());
        }
    }

    Ok(())
}

/// Prepares every branch and fallback body in a branch chain.
///
/// WHAT: reads branch and fallback body node IDs from the TIR `BranchChain`
///       node, prepares each body, and installs the prepared root directly onto
///       the TIR node. Body node IDs are read before preparation starts so the
///       store can be mutated during each body's format/compose/install cycle
///       without holding a borrow across the mutable phase.
fn prepare_branch_chain_render_units(
    control_flow_node_id: TemplateIrNodeId,
    branches: &[TemplateIrBranch],
    fallback: Option<TemplateIrNodeId>,
    ctx: ControlFlowBodyPreparationContext<'_>,
) -> Result<(), TemplateError> {
    let ControlFlowBodyPreparationContext {
        construction_context,
        style,
        child_wrappers,
        context,
        string_table,
    } = ctx;

    for (index, branch) in branches.iter().enumerate() {
        prepare_branch_or_fallback_body(
            ControlFlowBodyPreparationContext {
                construction_context,
                style,
                child_wrappers,
                context,
                string_table: &mut *string_table,
            },
            control_flow_node_id,
            branch.body,
            ControlFlowBodyKind::Branch { index },
        )?;
    }

    if let Some(fallback_body) = fallback {
        prepare_branch_or_fallback_body(
            ControlFlowBodyPreparationContext {
                construction_context,
                style,
                child_wrappers,
                context,
                string_table: &mut *string_table,
            },
            control_flow_node_id,
            fallback_body,
            ControlFlowBodyKind::Fallback,
        )?;
    }

    Ok(())
}

/// Prepares the loop body and installs the aggregate-wrapper subtree.
///
/// WHAT: reads the loop body node ID from the TIR `Loop` node, prepares the
///       body, then builds and installs the aggregate wrapper directly onto the
///       TIR `Loop` node. The aggregate wrapper root no longer needs to be
///       cached on a durable carrier because the TIR node owns it and reactive
///       metadata walks the TIR root directly.
fn prepare_loop_render_units(
    control_flow_node_id: TemplateIrNodeId,
    body: TemplateIrNodeId,
    construction_context: &mut TemplateConstructionContext,
    style: &Style,
    child_wrappers: &[TemplateWrapperReference],
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<(), TemplateError> {
    // Prepare the loop body with the same format-once + TIR pattern used
    // by branch/fallback bodies. Loop bodies skip the shared head prefix
    // because the owning head wraps the aggregate output once, not each
    // iteration.
    prepare_loop_body(
        ControlFlowBodyPreparationContext {
            construction_context,
            style,
            child_wrappers,
            context,
            string_table,
        },
        control_flow_node_id,
        body,
    )?;

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

    // Install the composed TIR aggregate-wrapper subtree onto the owning
    // `Loop` node.
    replace_loop_aggregate_wrapper_tir_root(
        &mut template_ir_store,
        control_flow_node_id,
        aggregate_wrapper.tir_root,
    )?;

    Ok(())
}

pub(crate) fn template_contains_control_flow(
    template: &crate::compiler_frontend::ast::templates::template_types::Template,
    template_ir_store: &TemplateIrStore,
    builder: Option<&TemplateParserIrBuilderState>,
) -> bool {
    let Some(roots) =
        current_same_store_tir_roots_for_template(template, template_ir_store, builder)
    else {
        return false;
    };

    template_ir_store.subtree_contains_control_flow_from_roots(&roots.roots)
}
