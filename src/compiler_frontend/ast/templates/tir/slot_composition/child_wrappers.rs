//! TIR-native `$children(..)` wrapper application.
//!
//! WHAT: wraps direct body-origin child templates in inherited wrapper
//!       templates, resolving slot-bearing wrappers by routing the child as a
//!       single loose contribution and expanding the wrapper's slot
//!       placeholders.
//!
//! WHY: this is the TIR-native equivalent of the child-wrapper branches in the
//!      legacy template composition pipeline. Keeping it separate from
//!      head-chain composition reflects the two distinct composition sites:
//!      head-chain receivers are opened by head-origin wrappers, while child
//!      wrappers are inherited from enclosing control-flow or render-unit
//!      contexts.
//!
//! Design constraint: production direct-child wrapper application now uses
//! wrapper-context overlays. The store-local structural helpers in this module
//! remain only where a later transform needs an owned wrapper tree, such as
//! control-flow aggregate wrappers or focused structural tests.

use crate::compiler_frontend::ast::templates::template::{Style, TemplateType};
use crate::compiler_frontend::ast::templates::tir::overlays::TemplateOverlaySetId;
use crate::compiler_frontend::ast::templates::tir::refs::TemplateTirChildReference;
use crate::compiler_frontend::ast::templates::tir::summary::TemplateIrSummary;
use crate::compiler_frontend::ast::templates::tir::view::TemplateTirPhase;
use crate::compiler_frontend::ast::templates::tir::{
    TemplateIr, TemplateIrId, TemplateIrNode, TemplateIrNodeId, TemplateIrNodeKind, TemplateIrStore,
};
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

use super::helpers::{
    SlotResolutionComposition, build_composed_wrapper_template, build_tir_fill_template,
    copy_tir_wrapper_template_with_fresh_slot_occurrence_ids, internal_compiler_error,
};
use super::schema::expand_tir_slot_placeholders_into;

/// Boxed diagnostic result for child-wrapper application.
///
/// Sits behind the already-boxed template composition boundaries (for
/// example `TemplateError::Diagnostic` in `render_unit.rs`). Boxing here keeps
/// the `Err` variant small enough for Clippy's `result_large_err` lint
/// while preserving every diagnostic value, source location, and semantic
/// fact. The three test-only structural helpers and the three production
/// wrapper helpers all share this one file-local boundary.
type ChildWrapperResult<T> = Result<T, Box<CompilerDiagnostic>>;

#[cfg(test)]
#[cfg(test)]
use super::helpers::rebuild_root_sequence;
#[cfg(test)]
#[cfg(test)]
use super::schema::tir_template_has_unresolved_slots;
#[cfg(test)]
use crate::compiler_frontend::instrumentation::{
    AstCounter, add_ast_counter, increment_ast_counter,
};

/// Test-only store-local application of `$children(..)` wrapper templates to
/// direct child template nodes in a TIR tree.
///
/// WHAT: walks the template's TIR root `Sequence` children, wraps body-origin
///       `ChildTemplate` nodes that have no unresolved slots in the inherited
///       wrappers, and leaves control-flow nodes (`BranchChain` / `Loop`) and
///       head-origin children unchanged.
/// WHY: focused tests still need the structural wrapper result while production
///      direct-child wrapping moves through wrapper-context overlays.
#[cfg(test)]
pub(crate) fn apply_tir_child_wrappers(
    store: &mut TemplateIrStore,
    template_id: TemplateIrId,
    wrapper_template_ids: &[TemplateIrId],
    string_table: &StringTable,
) -> ChildWrapperResult<TemplateIrNodeId> {
    let mut slot_compositions = Vec::new();
    apply_tir_child_wrappers_into(
        store,
        template_id,
        wrapper_template_ids,
        string_table,
        &mut slot_compositions,
    )
}

/// Internal child-wrapper application that also collects slot-bearing
/// wrapper/fill pairs into `slot_compositions` for later overlay allocation.
///
/// WHY: the slot-composition entry point (`wrap_tir_node_in_wrappers_into`)
///      needs the pairs to allocate slot-resolution overlays after the store
///      borrow is released. The public `apply_tir_child_wrappers` discards them.
#[cfg(test)]
fn apply_tir_child_wrappers_into(
    store: &mut TemplateIrStore,
    template_id: TemplateIrId,
    wrapper_template_ids: &[TemplateIrId],
    string_table: &StringTable,
    slot_compositions: &mut Vec<SlotResolutionComposition>,
) -> ChildWrapperResult<TemplateIrNodeId> {
    increment_ast_counter(AstCounter::TemplateTirChildWrapperCalls);

    let template = store.get_template(template_id).ok_or_else(|| {
        internal_compiler_error(
            "TIR child wrapper application: template ID was not present in the store.",
        )
    })?;

    let root_node_id = template.root;

    // No wrappers means no transformation is possible.
    if wrapper_template_ids.is_empty() {
        return Ok(root_node_id);
    }

    // Child wrappers only apply to templates whose body is a sequence of children.
    let root_children: Vec<TemplateIrNodeId> =
        super::head_chain::root_sequence_children(store, root_node_id)?
            .map(|children| children.to_vec())
            .unwrap_or_default();

    if root_children.is_empty() {
        return Ok(root_node_id);
    }

    // The parser records how many head-origin nodes precede the body. Body
    // direct children appear at or after this index, so we must not wrap
    // head-origin child templates (for example, template-valued head expressions).
    let body_start_index = (template.summary.head_node_count as usize).min(root_children.len());

    let mut new_children = Vec::with_capacity(root_children.len());
    let mut any_wrapped = false;
    let mut wrapped_count: usize = 0;

    for (index, child_id) in root_children.iter().enumerate() {
        let child_node = store.get_node(*child_id).ok_or_else(|| {
            internal_compiler_error(
                "TIR child wrapper application: child node ID was not present in the store.",
            )
        })?;

        if index >= body_start_index
            && let TemplateIrNodeKind::ChildTemplate { reference, .. } = &child_node.kind
        {
            let child_template_id = reference.root;

            // Direct children are templates that produce output, not wrappers
            // that still need to receive content. A child with unresolved slots
            // is a wrapper receiver and must be left for head-chain composition.
            if !tir_template_has_unresolved_slots(store, child_template_id)? {
                let wrapped_child_id = wrap_tir_node_in_wrappers_into(
                    store,
                    *child_id,
                    wrapper_template_ids,
                    string_table,
                    slot_compositions,
                )?;

                new_children.push(wrapped_child_id);
                any_wrapped = true;
                wrapped_count += 1;
                continue;
            }
        }

        // Head-origin children and control-flow nodes pass through unchanged.
        // Control-flow wrappers are attached conditionally during folding, not
        // during composition, because a skipped branch or zero-iteration loop
        // must not receive wrappers merely for existing.
        new_children.push(*child_id);
    }

    add_ast_counter(AstCounter::TemplateTirChildWrapperHits, wrapped_count);

    if !any_wrapped {
        return Ok(root_node_id);
    }

    rebuild_root_sequence(store, root_node_id, new_children)
}

/// Wraps a single direct child `ChildTemplate` node in all inherited wrappers.
///
/// WHAT: iterates the wrapper list in reverse (outermost-first), composing each
///       wrapper around the current wrapped child.
/// WHY: the legacy pipeline applies wrappers outermost-first so the first
///      wrapper in the inherited list becomes the innermost layer around the
///      child; reverse iteration preserves that nesting order.
pub(crate) fn wrap_tir_node_in_wrappers(
    store: &mut TemplateIrStore,
    child_node_id: TemplateIrNodeId,
    wrapper_template_ids: &[TemplateIrId],
    string_table: &StringTable,
) -> ChildWrapperResult<TemplateIrNodeId> {
    let mut slot_compositions = Vec::new();
    wrap_tir_node_in_wrappers_into(
        store,
        child_node_id,
        wrapper_template_ids,
        string_table,
        &mut slot_compositions,
    )
}

/// Internal wrapper application that also collects slot-bearing wrapper/fill
/// pairs into `slot_compositions` for later overlay allocation.
///
/// WHY: the slot-composition child-wrapper entry point needs the pairs to
///      allocate slot-resolution overlays. The public
///      `wrap_tir_node_in_wrappers` discards them so callers outside
///      `slot_composition.rs` (fold, render-unit preparation) keep the same
///      signature.
pub(super) fn wrap_tir_node_in_wrappers_into(
    store: &mut TemplateIrStore,
    child_node_id: TemplateIrNodeId,
    wrapper_template_ids: &[TemplateIrId],
    string_table: &StringTable,
    slot_compositions: &mut Vec<SlotResolutionComposition>,
) -> ChildWrapperResult<TemplateIrNodeId> {
    let child_location = store
        .get_node(child_node_id)
        .map(|node| node.location.to_owned())
        .ok_or_else(|| {
            internal_compiler_error(
                "TIR child wrapper application: child node ID was not present in the store.",
            )
        })?;

    let mut current_child_node_id = child_node_id;

    for wrapper_template_id in wrapper_template_ids.iter().rev() {
        let wrapper_has_slots = {
            let wrapper_template = store.get_template(*wrapper_template_id).ok_or_else(|| {
                internal_compiler_error(
                    "TIR child wrapper application: wrapper template ID was not present in the store.",
                )
            })?;

            wrapper_template.summary.has_slots || wrapper_template.summary.slot_count > 0
        };

        if wrapper_has_slots {
            // The current wrapped child is the fill content for this wrapper's
            // slots. Routing treats it as a single loose contribution chunk.
            let fill_template_id =
                build_tir_fill_template(store, vec![current_child_node_id], child_node_id)?;

            // Copy the wrapper template so this child gets its own fresh
            // SlotOccurrenceIds. Without the copy, applying the same wrapper to
            // multiple body children would place identical occurrence IDs into
            // the parent's merged slot-resolution overlay, causing overlay merge
            // to fail.
            let copied_wrapper_template_id =
                copy_tir_wrapper_template_with_fresh_slot_occurrence_ids(
                    store,
                    *wrapper_template_id,
                )?;

            let routed = super::contributions::route_tir_slot_contributions(
                store,
                copied_wrapper_template_id,
                fill_template_id,
                string_table,
            )?;

            let expanded_root = expand_tir_slot_placeholders_into(
                store,
                copied_wrapper_template_id,
                &routed,
                string_table,
                slot_compositions,
            )?;

            let composed_template_id =
                build_composed_wrapper_template(store, copied_wrapper_template_id, expanded_root)?;

            // Record the wrapper/fill pair so the slot-composition entry point
            // can allocate a slot-resolution overlay after the store borrow is
            // released. The fill template persists in the store, so the overlay
            // path can re-route against it without re-discovering the wrappers.
            let wrapper_reference = TemplateTirChildReference::new(
                copied_wrapper_template_id,
                TemplateTirPhase::Parsed,
                TemplateOverlaySetId::empty(),
            );
            let fill_reference = fill_template_id;
            slot_compositions.push(SlotResolutionComposition::new(
                wrapper_reference,
                fill_reference,
            ));

            let occurrence_id = store.next_child_template_occurrence_id();
            let reference = TemplateTirChildReference::new(
                composed_template_id,
                TemplateTirPhase::Parsed,
                TemplateOverlaySetId::empty(),
            );
            current_child_node_id = store.push_node(TemplateIrNode::new(
                TemplateIrNodeKind::ChildTemplate {
                    reference,
                    occurrence_id,
                },
                child_location.to_owned(),
            ));
        } else {
            // A wrapper without slots simply prepends its own content before the
            // child. Build a combined template whose root sequence is the wrapper
            // followed by the already-wrapped child.
            let combined_template_id = build_tir_prepended_wrapper_template(
                store,
                *wrapper_template_id,
                current_child_node_id,
                child_location.to_owned(),
            )?;

            let occurrence_id = store.next_child_template_occurrence_id();
            let reference = TemplateTirChildReference::new(
                combined_template_id,
                TemplateTirPhase::Parsed,
                TemplateOverlaySetId::empty(),
            );
            current_child_node_id = store.push_node(TemplateIrNode::new(
                TemplateIrNodeKind::ChildTemplate {
                    reference,
                    occurrence_id,
                },
                child_location.to_owned(),
            ));
        }
    }

    Ok(current_child_node_id)
}

/// Builds a template that prepends a slot-less wrapper before an existing child.
///
/// WHAT: creates a new `String` template whose root sequence contains a
///       `ChildTemplate` reference to the wrapper followed by the current child.
/// WHY: this mirrors the legacy `wrap_atom_in_child_template` branch for
///      slot-less wrappers, which puts the wrapper content immediately before
///      the wrapped atom.
fn build_tir_prepended_wrapper_template(
    store: &mut TemplateIrStore,
    wrapper_template_id: TemplateIrId,
    child_node_id: TemplateIrNodeId,
    child_location: SourceLocation,
) -> ChildWrapperResult<TemplateIrId> {
    let wrapper_location = store
        .get_template(wrapper_template_id)
        .map(|wrapper_template| wrapper_template.location.to_owned())
        .ok_or_else(|| {
            internal_compiler_error(
                "TIR child wrapper application: wrapper template ID was not present in the store.",
            )
        })?;

    let occurrence_id = store.next_child_template_occurrence_id();
    let wrapper_reference = TemplateTirChildReference::new(
        wrapper_template_id,
        TemplateTirPhase::Parsed,
        TemplateOverlaySetId::empty(),
    );
    let wrapper_node_id = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::ChildTemplate {
            reference: wrapper_reference,
            occurrence_id,
        },
        wrapper_location.to_owned(),
    ));

    let combined_root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence {
            children: vec![wrapper_node_id, child_node_id],
        },
        child_location,
    ));

    // The combined template contains two child references, so it is not a plain
    // const-evaluable string even if both constituents happen to fold.
    let mut summary = TemplateIrSummary::default();
    summary.record_child_template();
    summary.record_child_template();
    summary.is_const_evaluable_shape = false;

    Ok(store.push_template(TemplateIr::new(
        combined_root,
        Style::default(),
        TemplateType::String,
        summary,
        wrapper_location,
    )))
}
