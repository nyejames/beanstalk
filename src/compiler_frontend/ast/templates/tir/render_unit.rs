//! TIR render-unit construction helpers.
//!
//! WHAT: owns TIR aggregate-wrapper subtree construction.
//!
//! WHY: localizes the link between AST aggregate placeholders and TIR-native
//! loop aggregate wrappers. Aggregate-wrapper construction consumes parser TIR
//! and store-qualified child references directly, keeping parser-emitted TIR
//! as the only structural authority during render-unit preparation.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::runtime_template_expression;
use crate::compiler_frontend::ast::templates::template::{Style, TemplateType};
use crate::compiler_frontend::ast::templates::tir::builder::TemplateIrBuilder;
use crate::compiler_frontend::ast::templates::tir::foreign_slot_insert_proxy::build_foreign_slot_insert_proxy;
use crate::compiler_frontend::ast::templates::tir::formatter_view::{
    TirFormatterResult, format_tir_template,
};
use crate::compiler_frontend::ast::templates::tir::ids::{TemplateIrId, TemplateIrNodeId};
use crate::compiler_frontend::ast::templates::tir::node::{
    TemplateIr, TemplateIrNode, TemplateIrNodeKind,
};
use crate::compiler_frontend::ast::templates::tir::overlays::{
    TemplateOverlaySet, TemplateOverlaySetId,
};
use crate::compiler_frontend::ast::templates::tir::refs::{
    TemplateRef, TemplateTirChildReference, TemplateWrapperReference,
};
use crate::compiler_frontend::ast::templates::tir::registry::TemplateIrRegistry;
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::ast::templates::tir::summary::{
    TemplateIrSummary, summarize_existing_nodes, summarize_existing_root,
};
use crate::compiler_frontend::ast::templates::tir::view::{TemplateTirPhase, TirView};

use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_messages::DiagnosticSeverity;
use crate::compiler_frontend::instrumentation::{
    AstCounter, add_ast_counter, increment_ast_counter,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use std::sync::Arc;

// ------------------------------
//  Aggregate-wrapper candidates
// ------------------------------

/// Builds a temporary TIR template for loop aggregate-wrapper composition.
///
/// WHAT: reuses the owning template's already-materialized head-prefix TIR
///       nodes, converting cross-store child templates recorded as dynamic
///       expressions into same-store `ChildTemplate` references, and appends a
///       compiler-internal `AggregateOutput` node as the body fill.
/// WHY: loop aggregate wrapping should consume the exact nodes that the parser
///      already emitted. Cross-store child templates
///      that the parser could only record as opaque dynamic expressions are
///      materialized into the current store so head-chain composition can
///      resolve their slots around the aggregate fill.
pub(in crate::compiler_frontend::ast::templates) fn build_aggregate_wrapper_candidate_from_tir_nodes(
    head_prefix_nodes: &[TemplateIrNodeId],
    store: &mut TemplateIrStore,
    registry: &TemplateIrRegistry,
) -> Result<TemplateIrId, TemplateError> {
    let mut children = Vec::with_capacity(head_prefix_nodes.len() + 1);
    let root_location =
        head_prefix_node_location(store, head_prefix_nodes).map_err(TemplateError::from)?;

    for &node_id in head_prefix_nodes {
        let candidate_node = convert_head_node_for_aggregate_wrapper(node_id, store, registry)?;
        children.push(candidate_node);
    }

    children.push(store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::AggregateOutput,
        root_location.to_owned(),
    )));

    let summary = summarize_existing_nodes(store, &children);

    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence { children },
        root_location.to_owned(),
    ));

    Ok(store.push_template(TemplateIr::new(
        root,
        Style::default(),
        TemplateType::String,
        summary,
        root_location,
    )))
}

/// Builds a temporary TIR template for branch/fallback body-root composition.
///
/// WHAT: reuses the owning template's parser-emitted head-prefix TIR nodes
///       (converting cross-store child templates into same-store references)
///       and appends the already-materialized body-only children as the body
///       fill so head-chain composition can wrap the body.
/// WHY: branch and fallback bodies carry the shared head prefix plus their own
///      body content. Deriving both portions from parser-emitted TIR keeps the
///      body root authoritative while `compose_tir_head_chain` preserves
///      wrapper semantics.
pub(in crate::compiler_frontend::ast::templates) fn build_branch_body_candidate_from_tir_nodes(
    head_prefix_nodes: &[TemplateIrNodeId],
    body_children: &[TemplateIrNodeId],
    store: &mut TemplateIrStore,
    registry: &TemplateIrRegistry,
) -> Result<TemplateIrId, TemplateError> {
    let mut children = Vec::with_capacity(head_prefix_nodes.len() + body_children.len());
    let root_location = branch_body_candidate_location(store, head_prefix_nodes, body_children)
        .map_err(TemplateError::from)?;

    // Convert each head-prefix node so same-store children are reused and
    // cross-store child templates (recorded by the parser as opaque
    // DynamicExpression nodes) are materialized into this store. This mirrors
    // the loop aggregate-wrapper conversion so head-chain composition sees
    // resolvable ChildTemplate references.
    for &node_id in head_prefix_nodes {
        let candidate_node = convert_head_node_for_aggregate_wrapper(node_id, store, registry)?;
        children.push(candidate_node);
    }

    // The body children are already materialized in this store, so append them
    // directly. They carry Body-origin metadata so `compose_tir_head_chain`
    // partitions them into the body partition and applies head-prefix wrappers
    // around them.
    children.extend_from_slice(body_children);

    let summary = summarize_existing_nodes(store, &children);

    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence { children },
        root_location.to_owned(),
    ));

    Ok(store.push_template(TemplateIr::new(
        root,
        Style::default(),
        TemplateType::String,
        summary,
        root_location,
    )))
}

/// Returns the source location for an aggregate-wrapper candidate root.
///
/// WHAT: uses the first head-prefix node's location so the temporary
///       aggregate wrapper template carries a meaningful source span.
fn head_prefix_node_location(
    store: &TemplateIrStore,
    head_prefix_nodes: &[TemplateIrNodeId],
) -> Result<SourceLocation, CompilerError> {
    // An empty candidate carries no provenance, so a default location is the
    // only honest span. A selected node that is missing is an internal
    // invariant failure.
    match head_prefix_nodes.first().copied() {
        None => Ok(SourceLocation::default()),
        Some(node_id) => store
            .get_node(node_id)
            .map(|node| node.location.clone())
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "TIR aggregate-wrapper candidate: selected head-prefix node {} was missing from the store.",
                    node_id
                ))
            }),
    }
}

/// Returns the source location for a branch/fallback body candidate root.
///
/// WHAT: prefers the first shared head-prefix node and otherwise uses the first
///       prepared body child.
/// WHY: branch templates without a head prefix still need a concrete body span
///      after their temporary composition root is built.
fn branch_body_candidate_location(
    store: &TemplateIrStore,
    head_prefix_nodes: &[TemplateIrNodeId],
    body_children: &[TemplateIrNodeId],
) -> Result<SourceLocation, CompilerError> {
    // An empty candidate (no head prefix and no body children) carries no
    // provenance, so a default location is the only honest span. A selected
    // node that is missing is an internal invariant failure.
    let selected = head_prefix_nodes
        .first()
        .or_else(|| body_children.first())
        .copied();
    match selected {
        None => Ok(SourceLocation::default()),
        Some(node_id) => store
            .get_node(node_id)
            .map(|node| node.location.clone())
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "TIR branch body candidate: selected node {} was missing from the store.",
                    node_id
                ))
            }),
    }
}

/// Converts a head-prefix TIR node for reuse in an aggregate-wrapper candidate.
///
/// WHAT: reuses the existing node when it is already a structural TIR node
///       (`Text`, `ChildTemplate`, `InsertContribution`, or a non-template
///       `DynamicExpression`). When the parser could only record a cross-store
///       child template as a `DynamicExpression` carrying a `Template`
///       expression, materializes it into the current store as a
///       `ChildTemplate` (or `InsertContribution` for slot-insert helpers) so
///       head-chain composition can resolve its slots around the aggregate fill.
/// WHY: the parser records same-store head child templates as `ChildTemplate`
///      nodes, but cross-store templates are recorded as opaque
///      `DynamicExpression` nodes. This conversion reuses already-materialized
///      nodes for structural kinds and rebuilds the cross-store child as a
///      same-store-resolvable `ChildTemplate` (or `InsertContribution`) so
///      head-chain composition can resolve its slots.
fn convert_head_node_for_aggregate_wrapper(
    node_id: TemplateIrNodeId,
    store: &mut TemplateIrStore,
    registry: &TemplateIrRegistry,
) -> Result<TemplateIrNodeId, TemplateError> {
    let Some(node) = store.get_node(node_id) else {
        return Err(CompilerError::compiler_error(
            "TIR aggregate-wrapper preparation: head-prefix node ID was not present in the store.",
        )
        .into());
    };
    let node_kind = node.kind.to_owned();

    match node_kind {
        TemplateIrNodeKind::Text { .. } => Ok(node_id),

        TemplateIrNodeKind::ChildTemplate { .. } => Ok(node_id),

        TemplateIrNodeKind::InsertContribution { .. } => Ok(node_id),

        TemplateIrNodeKind::DynamicExpression { expression, .. } => {
            // Cross-store child templates are recorded by the parser as
            // DynamicExpression nodes carrying a Template expression. Convert
            // them to same-store ChildTemplate (or InsertContribution) nodes
            // so head-chain composition can resolve their slots around the
            // aggregate fill.
            if let Some(child_template) = runtime_template_expression(&expression) {
                let store_owner = store.owner();

                // For cross-store children that carry a registry-valid
                // tir_reference, preserve the foreign store-qualified identity
                // instead of rebuilding the template into the current store.
                // This keeps phase, overlay-set identity, and third-store
                // descendants resolvable through their own qualified refs.
                //
                // SlotInsert heads use a local proxy template that carries
                // the target slot key and mirrors the foreign SlotInsert body
                // as a Sequence of local routing nodes: nested $insert helpers
                // become local InsertContribution nodes, and non-insert content
                // becomes ChildTemplate references to narrow derived foreign
                // templates. This lets InsertContribution route by target key
                // and discover nested inserts recursively — like the same-store
                // contract — without deep-copying the foreign tree or reading
                // an intermediate content representation.
                let child_reference = &child_template.tir_reference;
                let is_foreign_reference = child_reference.root.store_id != store.store_id();

                if is_foreign_reference {
                    // Foreign store: require the registry store, a matching
                    // owner, the referenced template, and a registry-backed
                    // kind. The receiving registry is the authority for foreign
                    // child identity, so the durable kind cache is no longer a
                    // fallback.
                    let foreign_store_handle = registry
                        .store_handle(child_reference.root.store_id)
                        .ok_or_else(|| {
                            CompilerError::compiler_error(format!(
                                "TIR render-unit foreign child referenced store {} which is not in the module-local TIR registry.",
                                child_reference.root.store_id
                            ))
                        })?;
                    let child_kind = {
                        let foreign_store = foreign_store_handle.borrow();
                        if !Arc::ptr_eq(&foreign_store.owner(), &child_reference.store_owner) {
                            return Err(CompilerError::compiler_error(format!(
                                "TIR render-unit foreign child store {} owner did not match the registry store owner.",
                                child_reference.root.store_id
                            ))
                            .into());
                        }

                        foreign_store
                            .get_template(child_reference.root.template_id)
                            .ok_or_else(|| {
                                CompilerError::compiler_error(format!(
                                    "TIR render-unit foreign child template {} was not found in registry-backed store {}.",
                                    child_reference.root.template_id,
                                    child_reference.root.store_id
                                ))
                            })?
                            .kind
                            .clone()
                    };

                    let reference = TemplateTirChildReference::new(
                        child_reference.root,
                        child_reference.phase,
                        child_reference.overlay_set_id,
                    );

                    if matches!(child_kind, TemplateType::SlotInsert(_)) {
                        let proxy_id = build_foreign_slot_insert_proxy(
                            store,
                            registry,
                            &reference,
                            &child_kind,
                            &expression.location,
                        )?;
                        return Ok(store.push_node(TemplateIrNode::new(
                            TemplateIrNodeKind::InsertContribution { template: proxy_id },
                            expression.location.to_owned(),
                        )));
                    }

                    let occurrence_id = store.next_child_template_occurrence_id();
                    return Ok(store.push_node(TemplateIrNode::new(
                        TemplateIrNodeKind::ChildTemplate {
                            reference,
                            occurrence_id,
                        },
                        expression.location.to_owned(),
                    )));
                }

                // Same-store parser children already carry their complete TIR
                // identity. Preserve it exactly instead of reducing it to a
                // local ID and inventing Parsed/empty overlay metadata.
                if !Arc::ptr_eq(&child_reference.store_owner, &store_owner)
                    || child_reference.root.store_id != store.store_id()
                    || store
                        .get_template(child_reference.root.template_id)
                        .is_none()
                {
                    return Err(TemplateError::from(CompilerError::compiler_error(
                        "TIR render-unit child did not carry a same-store parser-emitted reference.",
                    )));
                }
                let child_reference = TemplateTirChildReference::new(
                    child_reference.root,
                    child_reference.phase,
                    child_reference.overlay_set_id,
                );

                let child_kind = child_template
                    .tir_kind_from_store(store)
                    .ok_or_else(|| {
                        TemplateError::from(CompilerError::compiler_error(
                            "TIR render-unit same-store child template kind was not found in its TIR store.",
                        ))
                    })?;

                if matches!(child_kind, TemplateType::SlotInsert(_)) {
                    return Ok(store.push_node(TemplateIrNode::new(
                        TemplateIrNodeKind::InsertContribution {
                            template: child_reference.root.template_id,
                        },
                        expression.location.to_owned(),
                    )));
                }

                let occurrence_id = store.next_child_template_occurrence_id();
                return Ok(store.push_node(TemplateIrNode::new(
                    TemplateIrNodeKind::ChildTemplate {
                        reference: child_reference,
                        occurrence_id,
                    },
                    expression.location.to_owned(),
                )));
            }

            // Non-template dynamic expressions (scalar/reference/reactive head
            // values) are reused as-is.
            Ok(node_id)
        }

        _ => Ok(node_id),
    }
}

// ------------------------------
//  Composed aggregate-wrapper output
// ------------------------------

/// Composed aggregate-wrapper output for TIR consumption.
pub(in crate::compiler_frontend::ast::templates) struct PreparedLoopAggregateWrapper {
    /// TIR root of the composed aggregate-wrapper subtree.
    ///
    /// WHAT: carries the internal `AggregateOutput` marker at its composed
    ///       position inside the wrapper tree.
    /// WHY: render-unit preparation installs this authoritative root directly
    ///      onto the owning `Loop` node for finalization and runtime handoff.
    pub(in crate::compiler_frontend::ast::templates) tir_root: TemplateIrNodeId,
}

// ------------------------------
//  Formatter helpers
// ------------------------------

/// Converts TIR formatter diagnostic messages into a single `TemplateError`.
///
/// WHAT: scans the formatter output for hard errors and returns the first one
///       as a `TemplateError`; when no error exists, fabricates a generic
///       compiler-error so the caller never receives an unexplained failure.
/// WHY: the formatter emits structured diagnostics; this helper provides the
///      narrow bridge from formatter messages back to template-stage errors.
fn tir_formatter_messages_to_template_error(messages: CompilerMessages) -> TemplateError {
    for diagnostic in messages.into_diagnostics() {
        if diagnostic.severity == DiagnosticSeverity::Error {
            return diagnostic.into();
        }
    }

    CompilerError::compiler_error("TIR formatter failed without returning a compiler error.").into()
}

/// Runs the TIR formatter and forwards any warnings to the active diagnostic
/// context.
///
/// WHAT: thin wrapper around `format_tir_template` that returns formatter
///       warnings with the formatted TIR result.
/// WHY: warnings are user-visible formatter behavior and must reach the caller.
pub(in crate::compiler_frontend::ast::templates) fn run_tir_formatter_with_warnings(
    view: &TirView<'_>,
    style: &Style,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<TirFormatterResult, TemplateError> {
    let result = format_tir_template(view, style, string_table)
        .map_err(tir_formatter_messages_to_template_error)?;

    for warning in &result.warnings {
        context.emit_warning(warning.clone());
    }

    Ok(result)
}

/// Formats a control-flow body TIR root using the template's style formatter.
///
/// WHAT: wraps the body root in a short-lived temporary template so the
///       TIR-native formatter can recursively format body text and nested
///       child templates, then returns the formatted root node ID.
/// WHY: control-flow body roots are nodes, not top-level templates, but they
///      still need the same formatter treatment as linear template bodies.
pub(in crate::compiler_frontend::ast::templates) fn format_tir_body_root(
    body_root: TemplateIrNodeId,
    style: &Style,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<TemplateIrNodeId, TemplateError> {
    let store_id = context.registered_template_ir_store.store_id();

    let temp_template_id = {
        let mut store = context.registered_template_ir_store.store().borrow_mut();
        let location = store
            .get_node(body_root)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "TIR body-root formatting: body root node {} was missing from the store.",
                    body_root
                ))
            })?
            .location
            .clone();
        let summary = summarize_existing_root(&store, body_root);

        store.push_template(TemplateIr::new(
            body_root,
            style.clone(),
            TemplateType::String,
            summary,
            location,
        ))
    };

    let empty_overlay_set_id = {
        let mut registry = context.registered_template_ir_store.registry().borrow_mut();
        registry.allocate_overlay_set(TemplateOverlaySet::empty())
    };

    let registry = context.registered_template_ir_store.registry().borrow();
    let temp_ref = TemplateRef::new(store_id, temp_template_id);
    let view = TirView::new(
        &registry,
        temp_ref,
        TemplateTirPhase::Parsed,
        empty_overlay_set_id,
    )
    .map_err(TemplateError::from)?;

    let result = run_tir_formatter_with_warnings(&view, style, context, string_table)?;

    Ok(result.root)
}

// ------------------------------
//  Sequence and whitespace helpers
// ------------------------------

/// Returns the children of a `Sequence` node, or an internal `CompilerError`
/// when the node is missing or not a sequence.
///
/// WHAT: extracts the flat child list so it can be appended after head-prefix
///       nodes in a branch/fallback body candidate.
/// WHY: parser-emitted body roots are sealed under a `Sequence` node; flattening
///      that wrapper keeps the body nodes at the same level as the head-prefix
///      nodes so head-chain composition can partition them correctly.
pub(in crate::compiler_frontend::ast::templates) fn sequence_children(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
) -> Result<Vec<TemplateIrNodeId>, CompilerError> {
    let node = store.get_node(node_id).ok_or_else(|| {
        CompilerError::compiler_error(format!(
            "TIR sequence-children lookup: node {} was missing from the store.",
            node_id
        ))
    })?;
    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => Ok(children.clone()),
        _ => Err(CompilerError::compiler_error(format!(
            "TIR sequence-children lookup: node {} was not a Sequence root.",
            node_id
        ))),
    }
}

/// Returns true when a TIR node is whitespace-only literal text.
///
/// WHAT: checks the interned text of a Text node and reports whether it
///       contains only whitespace.
/// WHY: loop-control boundary trimming needs the same whitespace test used by
///      the parser TIR builder state without depending on that module's
///      private helpers.
fn tir_node_is_whitespace_only_text(
    node_id: TemplateIrNodeId,
    store: &TemplateIrStore,
    string_table: &StringTable,
) -> Result<bool, CompilerError> {
    let node = store.get_node(node_id).ok_or_else(|| {
        CompilerError::compiler_error(format!(
            "TIR loop-control trim: whitespace candidate node {} was missing from the store.",
            node_id
        ))
    })?;

    let TemplateIrNodeKind::Text { text, .. } = &node.kind else {
        return Ok(false);
    };

    Ok(string_table.resolve(*text).trim().is_empty())
}

/// Trims whitespace-only text nodes that sit immediately before a top-level
/// LoopControl node in a loop body root sequence.
///
/// WHAT: applies the parser-level cleanup that strips trailing whitespace
///       before `[break]`/`[continue]` markers directly to the TIR body root.
/// WHY: loop-control boundary whitespace trimming belongs to the authoritative
///      loop body root and must reject malformed child references.
pub(in crate::compiler_frontend::ast::templates) fn trim_whitespace_before_loop_control_boundary(
    body_root: TemplateIrNodeId,
    store: &mut TemplateIrStore,
    string_table: &StringTable,
) -> Result<TemplateIrNodeId, CompilerError> {
    let node = store
        .get_node(body_root)
        .ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "TIR loop-control trim: body root node {} was missing from the store.",
                body_root
            ))
        })?
        .clone();

    let children = match node.kind {
        TemplateIrNodeKind::Sequence { children } => children,
        _ => {
            return Err(CompilerError::compiler_error(format!(
                "TIR loop-control trim: body root node {} was not a Sequence.",
                body_root
            )));
        }
    };

    let mut new_children = Vec::with_capacity(children.len());
    let original_children_count = children.len();

    for child_id in &children {
        let child_id = *child_id;
        let child = store.get_node(child_id).ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "TIR loop-control trim: child node {} was missing from the store.",
                child_id
            ))
        })?;
        let is_loop_control = matches!(child.kind, TemplateIrNodeKind::LoopControl { .. });

        if is_loop_control {
            // Drop whitespace-only text nodes that immediately precede this
            // loop-control marker, preserving any preceding non-whitespace output.
            while let Some(last) = new_children.last().copied() {
                if tir_node_is_whitespace_only_text(last, store, string_table)? {
                    new_children.pop();
                } else {
                    break;
                }
            }
        }

        new_children.push(child_id);
    }

    if new_children.len() == original_children_count {
        return Ok(body_root);
    }

    let location = node.location.clone();
    let mut builder = TemplateIrBuilder::new(store);
    Ok(builder.push_sequence_node(new_children, location))
}

// ------------------------------
//  Inherited child-wrapper application
// ------------------------------

/// Store-independent facts needed to classify a direct child before mutating
/// the current composition store.
struct ChildWrapperClassification {
    skip_parent_child_wrappers: bool,
    has_unresolved_slots: bool,
    has_control_flow: bool,
}

/// Reads only the foreign facts needed by the body walker, releasing the
/// owning-store borrow before the caller creates local derived nodes.
fn resolve_foreign_child_classification(
    registry: &TemplateIrRegistry,
    reference: &TemplateTirChildReference,
) -> Result<ChildWrapperClassification, TemplateError> {
    let foreign_store_rc = registry.store_handle(reference.root.store_id).ok_or_else(|| {
        CompilerError::compiler_error(format!(
            "Inherited-wrapper child referenced store {} which is not in the module-local TIR registry.",
            reference.root.store_id
        ))
    })?;

    let foreign_store = foreign_store_rc.borrow();
    let child_template = foreign_store
        .get_template(reference.root.template_id)
        .ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "Inherited-wrapper child referenced template {} which is missing in store {}.",
                reference.root.template_id, reference.root.store_id
            ))
        })?;

    Ok(ChildWrapperClassification {
        skip_parent_child_wrappers: child_template.style.skip_parent_child_wrappers,
        has_unresolved_slots: child_template.summary.has_slots
            || child_template.summary.slot_count > 0,
        has_control_flow: child_template.summary.has_control_flow,
    })
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
///       `conditional_child_wrapper_set` carries the inherited wrappers.
/// Same-store children are resolved from the current mutable store as a fast
/// path. Foreign children have their style/summary facts resolved through the
/// module-local registry without holding a borrow across local mutation, so
/// derived output stays local and the original child reference is preserved.
/// WHY: lets `prepare_branch_body_tir_root` cover bodies with inherited
///      `$children(...)` wrappers while preserving foreign child identities.
pub(in crate::compiler_frontend::ast::templates) fn apply_inherited_child_wrappers_to_body_root(
    body_root: TemplateIrNodeId,
    wrapper_refs: &[TemplateWrapperReference],
    registry: &TemplateIrRegistry,
    store: &mut TemplateIrStore,
    string_table: &StringTable,
) -> Result<TemplateIrNodeId, TemplateError> {
    // Validate body-root authority before any short-circuit. The body root must
    // exist and be a `Sequence` even when there are no inherited wrappers, so a
    // malformed render unit surfaces as an internal error instead of a silent
    // unchanged-body fallback.
    let body_node = store
        .get_node(body_root)
        .ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "Inherited-wrapper application: body root node {} was missing from the store.",
                body_root
            ))
        })?
        .clone();

    let children = match body_node.kind {
        TemplateIrNodeKind::Sequence { children } => children,
        _ => {
            return Err(CompilerError::compiler_error(format!(
                "Inherited-wrapper application: body root node {} was not a Sequence.",
                body_root
            ))
            .into());
        }
    };

    let body_location = body_node.location;

    if wrapper_refs.is_empty() {
        return Ok(body_root);
    }

    increment_ast_counter(AstCounter::TemplateTirChildWrapperCalls);

    let mut new_children = Vec::with_capacity(children.len());
    let mut any_changed = false;
    let mut wrapped_count: usize = 0;

    for child_id in children {
        let child_node = match store.get_node(child_id) {
            Some(node) => node.clone(),
            None => {
                return Err(CompilerError::compiler_error(
                    "Control-flow body root referenced a missing child TIR node.",
                )
                .into());
            }
        };

        let TemplateIrNodeKind::ChildTemplate { reference, .. } = child_node.kind else {
            new_children.push(child_id);
            continue;
        };

        // Same-store children are resolved directly from the current mutable
        // store as a fast path; foreign children have their style/summary
        // facts resolved through the registry so the borrow on the owning
        // store is released before any derived output is written locally.
        let classification = match reference.template_id_in_store(store.store_id()) {
            Some(child_template_id) => {
                let child_template = match store.get_template(child_template_id) {
                    Some(template) => template,
                    None => {
                        return Err(CompilerError::compiler_error(
                            "ChildTemplate node referenced a missing TIR template.",
                        )
                        .into());
                    }
                };

                ChildWrapperClassification {
                    skip_parent_child_wrappers: child_template.style.skip_parent_child_wrappers,
                    has_unresolved_slots: child_template.summary.has_slots
                        || child_template.summary.slot_count > 0,
                    has_control_flow: child_template.summary.has_control_flow,
                }
            }
            None => resolve_foreign_child_classification(registry, &reference)?,
        };

        if classification.skip_parent_child_wrappers {
            new_children.push(child_id);
            continue;
        }

        if classification.has_unresolved_slots {
            // Slot-bearing children are wrapper receivers; leave them for
            // head-chain composition.
            new_children.push(child_id);
            continue;
        }

        let wrapped_child_id = if classification.has_control_flow {
            wrap_control_flow_child_in_inherited_wrappers(
                store,
                &reference,
                wrapper_refs,
                &child_node.location,
            )?
        } else {
            let wrapper_ids: Vec<TemplateIrId> = wrapper_refs
                .iter()
                .map(|wrapper_ref| wrapper_ref.root.template_id)
                .collect();
            super::wrap_tir_node_in_wrappers(store, child_id, &wrapper_ids, string_table)?
        };

        new_children.push(wrapped_child_id);
        any_changed = true;
        wrapped_count += 1;
    }

    add_ast_counter(AstCounter::TemplateTirChildWrapperHits, wrapped_count);

    if !any_changed {
        return Ok(body_root);
    }

    Ok(store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence {
            children: new_children,
        },
        body_location,
    )))
}

/// Wraps a control-flow direct child by creating a derived template that carries
/// the inherited wrappers as conditional child wrappers.
///
/// WHAT: the derived template root is a single `ChildTemplate` reference to the
///       effective child identity, and its `conditional_child_wrapper_set`
///       stores the inherited wrappers. When folded, the child produces its
///       (possibly conditional) output and the derived template applies the
///       inherited wrappers around that output without mutating the shared
///       child template.
/// WHY: control-flow output is conditional, so inherited wrappers must apply
///      around the emission rather than being baked into the child structure.
///      Threading the effective child reference (root, phase, overlay set)
///      instead of reconstructing a same-store reference from a local ID
///      preserves the exact identity of cross-store children inside the
///      derived node.
fn wrap_control_flow_child_in_inherited_wrappers(
    store: &mut TemplateIrStore,
    child_reference: &TemplateTirChildReference,
    wrapper_refs: &[TemplateWrapperReference],
    location: &SourceLocation,
) -> Result<TemplateIrNodeId, TemplateError> {
    let wrapper_set_id = store.push_or_reuse_wrapper_set(wrapper_refs.to_vec());

    let child_occurrence_id = store.next_child_template_occurrence_id();
    // Preserve the original child reference exact root, phase, and overlay
    // set instead of reconstructing a same-store reference from a local ID, so
    // cross-store children keep their owning-store identity inside the derived
    // wrapper node.
    let child_node_id = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::ChildTemplate {
            reference: *child_reference,
            occurrence_id: child_occurrence_id,
        },
        location.to_owned(),
    ));

    let mut summary = TemplateIrSummary::default();
    summary.record_child_template();
    summary.record_control_flow();

    let wrapper_template = TemplateIr {
        root: child_node_id,
        style: Style::default(),
        kind: TemplateType::String,
        summary,
        location: location.to_owned(),
        conditional_child_wrapper_set: Some(wrapper_set_id),
        runtime_slot_plan: None,
    };

    let wrapper_template_id = store.push_template(wrapper_template);
    let wrapper_occurrence_id = store.next_child_template_occurrence_id();
    let wrapper_reference = TemplateTirChildReference::same_store(
        wrapper_template_id,
        store.store_id(),
        TemplateTirPhase::Parsed,
        TemplateOverlaySetId::empty(),
    );

    Ok(store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::ChildTemplate {
            reference: wrapper_reference,
            occurrence_id: wrapper_occurrence_id,
        },
        location.to_owned(),
    )))
}

// ------------------------------
//  Loop aggregate-wrapper preparation
// ------------------------------

/// Prepares the composed aggregate-wrapper for a template `loop`.
///
/// WHAT: derives the head-prefix TIR nodes from the owning template's
///       parser-emitted root children, builds a temporary aggregate-wrapper
///       candidate, composes it through `compose_tir_head_chain`, and
///       returns its authoritative composed root.
/// WHY: loop aggregate wrapping should consume the exact parser-emitted TIR
///      nodes so one structural authority owns aggregate preparation.
pub(in crate::compiler_frontend::ast::templates) fn prepare_loop_aggregate_wrapper(
    root_children: &[TemplateIrNodeId],
    string_table: &StringTable,
    registry: &TemplateIrRegistry,
    template_ir_store: &mut TemplateIrStore,
) -> Result<PreparedLoopAggregateWrapper, TemplateError> {
    // Derive the head-prefix TIR nodes from the owning template's parser-emitted
    // root children. Reusing those exact nodes preserves parser identity and
    // avoids rebuilding an equivalent head structure.
    let head_prefix_nodes =
        head_prefix_tir_nodes(template_ir_store, root_children).map_err(TemplateError::from)?;

    let aggregate_template_id = build_aggregate_wrapper_candidate_from_tir_nodes(
        &head_prefix_nodes,
        template_ir_store,
        registry,
    )?;
    let composed_root = super::compose_tir_head_chain(
        template_ir_store,
        aggregate_template_id,
        string_table,
        true,
    )?;
    Ok(PreparedLoopAggregateWrapper {
        tir_root: composed_root,
    })
}

/// Extracts the head-prefix TIR nodes from the owning template's parser-emitted
/// root children.
///
/// WHAT: returns all root children before the first control-flow node (`Loop`
///       or `BranchChain`). These are the same TIR nodes the parser
///       materialized from the shared head-prefix atoms, so reusing them
///       avoids rebuilding TIR from content.
/// WHY: loop aggregate wrappers and branch/fallback body roots both wrap the
///      shared head prefix around their respective body output. The
///      head-prefix nodes are structurally everything before the control-flow
///      node in the owning template's root sequence, so one extractor serves
///      both control-flow kinds.
pub(in crate::compiler_frontend::ast::templates) fn head_prefix_tir_nodes(
    store: &TemplateIrStore,
    root_children: &[TemplateIrNodeId],
) -> Result<Vec<TemplateIrNodeId>, CompilerError> {
    let mut prefix = Vec::new();
    for &node_id in root_children {
        let node = store.get_node(node_id).ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "TIR head-prefix extraction: root child node {} was missing from the store.",
                node_id
            ))
        })?;
        if matches!(
            node.kind,
            TemplateIrNodeKind::Loop { .. } | TemplateIrNodeKind::BranchChain { .. }
        ) {
            break;
        }
        prefix.push(node_id);
    }
    Ok(prefix)
}
