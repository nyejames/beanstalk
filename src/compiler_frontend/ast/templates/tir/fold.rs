//! TIR-native compile-time template folding.
//!
//! WHAT: folds a `TemplateIr` tree directly into an interned string emission
//!
//! WHY: folding works directly on the authoritative TIR representation, keeping
//! the fold stage decoupled from intermediate content surfaces.
//!
//! ## Loop aggregate wrappers
//!
//! Loop aggregate wrappers are TIR-native subtrees rooted at
//! `TemplateIrNodeKind::Loop::aggregate_wrapper`. The `AggregateOutput` marker
//! node inside the wrapper is replaced at fold time with the already-folded
//! aggregate string.

use crate::compiler_frontend::ast::ast_nodes::RangeLoopSpec;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::SlotKey;
use crate::compiler_frontend::ast::templates::template::Template;
use crate::compiler_frontend::ast::templates::template::TemplateType;
use crate::compiler_frontend::ast::templates::template_control_flow::{
    ConstRangeCursor, TemplateBranchSelector, TemplateFoldBinding, TemplateLoopControlKind,
    TemplateLoopHeader, build_collection_iteration_bindings, build_range_iteration_bindings,
    const_collection_items,
};
use crate::compiler_frontend::ast::templates::template_folding::{
    TemplateEmission, TemplateFoldContext, condition_location_or_loop_location,
    fold_bool_condition, fold_conditional_loop_const_condition, loop_body_not_const_error,
    resolve_fold_bindings_in_expression, selected_option_capture_payload,
    template_emission_from_output_and_signal,
};
use crate::compiler_frontend::ast::templates::tir::fold_cache::TirFoldCacheKey;
use crate::compiler_frontend::ast::templates::tir::ids::{
    ExpressionSiteId, TemplateIrId, TemplateIrNodeId, TemplateWrapperSetId,
};
use crate::compiler_frontend::ast::templates::tir::node::{
    TemplateIr, TemplateIrBranch, TemplateIrNodeKind, TemplateLoopHeaderExpressionSites,
};
use crate::compiler_frontend::ast::templates::tir::overlays::{
    TirSlotResolutionKind, TirWrapperApplicationMode, TirWrapperContext,
};
use crate::compiler_frontend::ast::templates::tir::preparation::PreparedFold;
use crate::compiler_frontend::ast::templates::tir::refs::TemplateTirReference;
use crate::compiler_frontend::ast::templates::tir::refs::{
    TemplateTirChildReference, TemplateWrapperReference,
};
use crate::compiler_frontend::ast::templates::tir::slot_composition::collect_tir_slot_schema;
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::ast::templates::tir::view::{TemplateTirPhase, TirView};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidTemplateStructureReason,
};
use crate::compiler_frontend::instrumentation::{
    AstCounter, add_ast_counter, increment_ast_counter,
};
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::type_coercion::string::{
    FoldedStringPiece, fold_expression_kind_to_string,
};

// -------------------------
//  Capacity helpers
// -------------------------

/// Maximum bytes to reserve for a single const-loop aggregate output buffer.
const FOLD_LOOP_RESERVE_BYTE_CAP: usize = 64 * 1024;

/// Maximum iterations to use when estimating a streaming range loop.
const FOLD_RANGE_LOOP_RESERVE_ITERATION_CAP: usize = 256;

/// Creates a fold output buffer with a cheap, safe capacity hint and records
/// the reservation for TIR counters.
fn reserve_tir_fold_output_buffer(estimated_bytes: usize) -> String {
    add_ast_counter(
        AstCounter::TemplateEstimatedFoldOutputBytes,
        estimated_bytes,
    );
    String::with_capacity(estimated_bytes)
}

/// Records how many bytes the actual folded output exceeded the estimate by.
fn record_tir_fold_output_estimate_miss(actual_len: usize, estimated_bytes: usize) {
    if actual_len > estimated_bytes {
        add_ast_counter(
            AstCounter::TemplateFoldOutputEstimateMissBytes,
            actual_len - estimated_bytes,
        );
    }
}

/// Cheap estimate for a loop aggregate buffer given a per-iteration body
/// estimate and an iteration count, clamped to avoid huge reservations.
fn estimate_loop_aggregate_bytes(body_estimate: usize, iteration_count: usize) -> usize {
    body_estimate
        .saturating_mul(iteration_count)
        .min(FOLD_LOOP_RESERVE_BYTE_CAP)
}

/// Records that a folded output string was interned.
fn record_tir_fold_output_intern(byte_len: usize) {
    add_ast_counter(AstCounter::TirFoldStringInternCalls, 1);
    add_ast_counter(AstCounter::TirFoldOutputBytes, byte_len);
    add_ast_counter(AstCounter::TemplateFoldStringInternCalls, 1);
    add_ast_counter(AstCounter::TemplateFoldOutputBytes, byte_len);
}

/// Rejects `$insert(...)` helper templates at the exact fold boundary where
/// they would otherwise render as ordinary string content.
///
/// WHAT: every effective template source enters one of the fold-owned template
/// entry points before its root is walked, including slot-resolution sources,
/// wrapper-context wrappers and child-template references.
/// WHY: checking the selected template entry avoids scratch materialization,
/// stale-source reads and repeated whole-descendant prepasses. Raw
/// consumed `InsertContribution` nodes that aren't reachable from the effective
/// fold path remain correctly ignored.
fn reject_slot_insert_template(kind: &TemplateType) -> Result<(), TemplateError> {
    if matches!(kind, TemplateType::SlotInsert(_)) {
        return Err(CompilerError::compiler_error(
            "Invalid template content reached string folding: unresolved slot insertions cannot be rendered directly.",
        )
        .into());
    }

    Ok(())
}

/// Borrowed exact-view input shared by recursive fold walkers.
///
/// WHAT: couples overlay reads with the exact view for the store currently
///       being traversed.
/// WHY: recursive node, control-flow, and wrapper folds must preserve the
///      complete view identity without expanding `TemplateFoldContext`.
struct FoldTraversalInput<'view, 'store> {
    view: &'view TirView<'store>,
}

impl<'view, 'store> FoldTraversalInput<'view, 'store> {
    /// Resolves one expression site through the active exact view.
    ///
    /// WHAT: delegates expression lookup to the shared module-local view.
    /// WHY: structural transitions retain the complete expression authority
    ///       carried by the current view while changing other dimensions.
    fn effective_expression_for_site(
        &self,
        site_id: ExpressionSiteId,
    ) -> Result<Option<&'store Expression>, TemplateError> {
        Ok(self.view.effective_expression_for_site(site_id)?)
    }
}

struct PreparedWrapperReference {
    reference: TemplateWrapperReference,
}

fn prepare_wrapper_references(
    wrapper_references: &[TemplateWrapperReference],
) -> Result<Vec<PreparedWrapperReference>, TemplateError> {
    Ok(wrapper_references
        .iter()
        .copied()
        .map(|reference| PreparedWrapperReference { reference })
        .collect())
}

// -------------------------
//  Public entry point
// -------------------------

/// Folds one prepared, exact TIR view into an owned emission.
///
/// WHAT: consumes the completed preparation proof and enters the fold cache and
///      reducer without reclassifying or re-walking the template for authority.
/// WHY: preparation is the sole semantic classifier. The identity check must
///      happen before cache lookup so a stale proof can never authorize output.
pub(crate) fn fold_prepared_template(
    prepared: &PreparedFold,
    view: TirView<'_>,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<TemplateEmission, TemplateError> {
    // Keep the project-aware context fields part of the fold contract even
    // though the TIR reducer itself only consumes the string table, bindings,
    // loop limit, and cache.
    let _project_context = (
        fold_context.project_path_resolver,
        fold_context.path_format_config,
        fold_context.source_file_scope,
    );

    if prepared.identity != view.identity() {
        return Err(CompilerError::compiler_error(
            "TIR fold preparation root/phase/context identity does not match the supplied view.",
        )
        .into());
    }

    if !view.phase().is_at_least(TemplateTirPhase::Composed) {
        return Err(CompilerError::compiler_error(format!(
            "fold_prepared_template: root {} at phase {} has not reached Composed",
            view.root_ref(),
            view.phase()
        ))
        .into());
    }

    fold_exact_view(&view, fold_context)
}

/// Folds one exact Composed-or-later view, consulting the phase-local cache.
///
/// WHAT: validates the view's structural and overlay authority before looking
///       up its precise cache key, then reduces and caches the exact result
///       when no loop bindings are active.
/// WHY: the root and repeated structural child/source folds share one cache
///      owner without preparing or classifying recursively. Parsed structural
///      children and virtual injected-wrapper folds intentionally bypass this
///      helper because their reduction semantics are different.
fn fold_exact_view(
    view: &TirView<'_>,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<TemplateEmission, TemplateError> {
    if !view.phase().is_at_least(TemplateTirPhase::Composed) {
        return Err(CompilerError::compiler_error(format!(
            "fold_prepared_template: root {} at phase {} has not reached Composed",
            view.root_ref(),
            view.phase()
        ))
        .into());
    }

    let store = view.store();
    let root = view.root_ref();
    let template = view.root_template()?;
    if store.get_node(template.root).is_none() {
        return Err(CompilerError::compiler_error(format!(
            "TIR fold: node {} does not exist in the module store.",
            template.root
        ))
        .into());
    }
    view.expression_overlay()?;
    view.slot_resolution_overlay()?;
    view.wrapper_context_overlay()?;
    if let Some(slot_plan_id) = template.runtime_slot_plan
        && store.get_slot_plan(slot_plan_id).is_none()
    {
        return Err(CompilerError::compiler_error(format!(
            "TIR fold: slot plan {} does not exist in the module store.",
            slot_plan_id
        ))
        .into());
    }
    let bindings_empty = fold_context.bindings.is_empty();
    let cache_key = TirFoldCacheKey {
        identity: view.identity(),
        loop_iteration_limit: fold_context.template_const_loop_iteration_limit,
        bindings_empty,
    };

    // Attribute one prepared view fold per store-backed view, across
    // finalization, doc-fragment, and HIR-handoff callers.
    increment_ast_counter(AstCounter::TirViewFoldsAttempted);

    if bindings_empty && let Some(cached) = fold_context.fold_cache.get(&cache_key) {
        increment_ast_counter(AstCounter::TirFoldCacheHits);
        return Ok(*cached);
    }

    increment_ast_counter(AstCounter::TirFoldCacheMisses);

    let has_expression_overlay = view.expression_overlay()?.is_some();
    let has_slot_overlay = view.slot_resolution_overlay()?.is_some();
    let has_wrapper_context = view.context().wrapper_context.is_some();

    // Attribute the overlay shape so callers can rank which overlay combinations
    // drive the view-native fold path.
    match (has_expression_overlay, has_slot_overlay) {
        (false, false) => increment_ast_counter(AstCounter::TirViewFoldOverlayEmpty),
        (true, false) => increment_ast_counter(AstCounter::TirViewFoldOverlayExpressionOnly),
        (false, true) => increment_ast_counter(AstCounter::TirViewFoldOverlaySlotOnly),
        (true, true) => increment_ast_counter(AstCounter::TirViewFoldOverlayExpressionAndSlot),
    }
    if has_wrapper_context {
        increment_ast_counter(AstCounter::TirViewFoldWrapperContextPresent);
    }

    // View-native fold: pass the exact view to the reducer so it reads
    // effective expressions and slot resolutions without cloning the store.
    let fold_input = FoldTraversalInput { view };
    let result = fold_tir_template_with_view(store, root, fold_context, &fold_input)?;

    if bindings_empty {
        fold_context.fold_cache.insert(cache_key, result);
    }

    Ok(result)
}

/// Folds a TIR template through one required exact `TirView`.
///
/// WHAT: the fold walker reads structural nodes from `store` but consults `view`
///       for effective expressions (dynamic-expression sites, branch selectors,
///       loop headers) and slot resolutions.
/// WHY: view-native overlay reads let folding apply expression, slot, and
///      wrapper-context overrides without mutating or cloning the store.
fn fold_tir_template_with_view(
    store: &TemplateIrStore,
    template_id: TemplateIrId,
    fold_context: &mut TemplateFoldContext<'_>,
    fold_input: &FoldTraversalInput<'_, '_>,
) -> Result<TemplateEmission, TemplateError> {
    add_ast_counter(AstCounter::TirFoldTemplatesFolded, 1);

    let template = store
        .get_template(template_id)
        .cloned()
        .ok_or_else(|| missing_template_diagnostic(template_id))?;
    reject_slot_insert_template(&template.kind)?;

    if template.runtime_slot_plan.is_some() {
        return Err(CompilerError::compiler_error(
            "TIR fold: a runtime slot plan reached the fold reducer without a foldable preparation proof.",
        )
        .into());
    }

    let estimated_bytes = template.summary.estimated_output_bytes;
    let mut output_buffer = reserve_tir_fold_output_buffer(estimated_bytes);
    let mut emitted_output = false;

    let signal = fold_tir_node_into_buffer(
        store,
        template.root,
        &mut output_buffer,
        &mut emitted_output,
        fold_context,
        fold_input,
    )?;

    let emission = build_emission_from_buffer(
        output_buffer,
        estimated_bytes,
        signal,
        emitted_output,
        fold_context,
    )?;

    // Wrapper sets store `TemplateWrapperReference` values; extract the
    // store-local `TemplateIrId` for same-store folding lookups.
    let wrapper_references: Vec<TemplateWrapperReference> =
        match template.conditional_child_wrapper_set {
            Some(wrapper_set_id) => store
                .get_wrapper_set(wrapper_set_id)
                .ok_or_else(|| missing_wrapper_set_diagnostic(wrapper_set_id))?
                .wrappers
                .to_vec(),
            None => Vec::new(),
        };

    fold_conditional_child_wrappers_around_emission(
        store,
        &wrapper_references,
        emission,
        TirWrapperApplicationMode::IfChildEmits,
        fold_context,
        fold_input,
    )
}

// -------------------------
//  Node folding
// -------------------------

/// Folds a single TIR node into an independent emission.
///
/// WHAT: creates a fresh output buffer for the node and returns the full
/// `TemplateEmission`. This is the right shape for branch bodies and loop
/// bodies, which may produce break/continue signals.
fn fold_tir_node(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
    fold_context: &mut TemplateFoldContext<'_>,
    fold_input: &FoldTraversalInput<'_, '_>,
) -> Result<TemplateEmission, TemplateError> {
    let mut buffer = String::new();
    let mut emitted_output = false;

    let signal = fold_tir_node_into_buffer(
        store,
        node_id,
        &mut buffer,
        &mut emitted_output,
        fold_context,
        fold_input,
    )?;

    build_emission_from_buffer(buffer, 0, signal, emitted_output, fold_context)
}

/// Folds a single TIR node, appending any output to the caller's buffer.
///
/// WHAT: dispatches on node kind and appends output directly. Returns an
/// optional loop-control signal when the node (or a nested node) produced one.
fn fold_tir_node_into_buffer(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
    output_buffer: &mut String,
    emitted_output: &mut bool,
    fold_context: &mut TemplateFoldContext<'_>,
    fold_input: &FoldTraversalInput<'_, '_>,
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    add_ast_counter(AstCounter::TirFoldNodesVisited, 1);

    let node = store
        .get_node(node_id)
        .cloned()
        .ok_or_else(|| missing_node_diagnostic(node_id))?;

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            fold_tir_sequence(
                store,
                children,
                output_buffer,
                emitted_output,
                fold_context,
                fold_input,
            )
        }

        TemplateIrNodeKind::Text { text, .. } => {
            output_buffer.push_str(fold_context.string_table.resolve(*text));
            *emitted_output = true;
            Ok(None)
        }

        TemplateIrNodeKind::DynamicExpression { expression, site_id, .. } => {
            // When a view with an expression overlay is present, use the
            // effective expression for this site instead of the structural
            // expression stored on the node. This replaces the old clone-and-
            // mutate overlay application path with a direct view read.
            let effective_expression = fold_input.effective_expression_for_site(*site_id)?;
            let expression_to_fold = effective_expression.unwrap_or(expression);
            fold_tir_dynamic_expression(
                store,
                expression_to_fold,
                output_buffer,
                emitted_output,
                fold_context,
                &node.location,
                fold_input,
            )
        }

        TemplateIrNodeKind::ChildTemplate {
            reference,
            occurrence_id,
            ..
        } => {
            let occurrence_context = fold_input
                .view
                .effective_wrapper_context(*occurrence_id)?
                .cloned();
            let emission = fold_child_template_reference(
                store,
                reference,
                fold_context,
                fold_input,
            )?;
            let wrapped_emission = apply_wrapper_context_overlay_to_child_emission(
                store,
                emission,
                fold_context,
                fold_input,
                occurrence_context.as_ref(),
            )?;

            append_template_emission_to_buffer(
                wrapped_emission,
                output_buffer,
                emitted_output,
                fold_context,
            )
        }

        TemplateIrNodeKind::Slot { placeholder } => {
            // Fold resolved slot sources in deterministic source order. Missing,
            // unresolved, or overlay-absent slots fold to empty output, matching
            // the structural behavior when no overlay entry is present.
            if let Some(resolution) = fold_input
                .view
                .effective_slot_resolution(placeholder.occurrence_id)?
                && let TirSlotResolutionKind::Resolved { sources } = &resolution.kind
            {
                for source in sources {
                    let emission = fold_resolved_slot_source(
                        store,
                        *source,
                        fold_context,
                        fold_input,
                    )?;
                    append_template_emission_to_buffer(
                        emission,
                        output_buffer,
                        emitted_output,
                        fold_context,
                    )?;
                }
                return Ok(None);
            }
            // Missing, unresolved, or uncovered slots intentionally fold to no
            // output.
            Ok(None)
        }

        TemplateIrNodeKind::InsertContribution { .. } => Err(CompilerError::compiler_error(
            "Insert contribution reached TIR folding without being consumed by slot composition.",
        )
        .into()),

        TemplateIrNodeKind::BranchChain { branches, fallback } => fold_tir_branch_chain(
            store,
            branches,
            *fallback,
            output_buffer,
            emitted_output,
            fold_context,
            fold_input,
        ),

        TemplateIrNodeKind::Loop {
            header,
            header_sites,
            body,
            aggregate_wrapper,
        } => fold_tir_loop(
            store,
            header,
            *header_sites,
            *body,
            *aggregate_wrapper,
            output_buffer,
            emitted_output,
            fold_context,
            fold_input,
            &node.location,
            fold_tir_node,
        ),

        TemplateIrNodeKind::AggregateOutput => Err(CompilerError::compiler_error(
            "TIR fold: AggregateOutput marker reached a fold site outside a loop aggregate wrapper.",
        )
        .into()),

        TemplateIrNodeKind::LoopControl { kind } => Ok(Some(*kind)),

        TemplateIrNodeKind::RuntimeSlotSite { .. } => {
            // Runtime slot sites are resolved during AST planning, not folding.
            Ok(None)
        }
    }
}

/// Folds a sequence node by folding each child in authored order.
fn fold_tir_sequence(
    store: &TemplateIrStore,
    children: &[TemplateIrNodeId],
    output_buffer: &mut String,
    emitted_output: &mut bool,
    fold_context: &mut TemplateFoldContext<'_>,
    fold_input: &FoldTraversalInput<'_, '_>,
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    for &child_id in children {
        let signal = fold_tir_node_into_buffer(
            store,
            child_id,
            output_buffer,
            emitted_output,
            fold_context,
            fold_input,
        )?;

        if signal.is_some() {
            return Ok(signal);
        }
    }

    Ok(None)
}

/// Folds a dynamic expression node after resolving fold bindings.
fn fold_tir_dynamic_expression(
    store: &TemplateIrStore,
    expression: &Expression,
    output_buffer: &mut String,
    emitted_output: &mut bool,
    fold_context: &mut TemplateFoldContext<'_>,
    location: &SourceLocation,
    fold_input: &FoldTraversalInput<'_, '_>,
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    let resolved = resolve_fold_bindings_in_expression(expression, fold_context)?;
    let expression_ref: &Expression = match &resolved {
        crate::compiler_frontend::ast::templates::template_folding::FoldResolvedExpression::Borrowed(
            expr,
        ) => expr,
        crate::compiler_frontend::ast::templates::template_folding::FoldResolvedExpression::Owned(
            expr,
        ) => expr,
    };

    if matches!(
        expression_ref.kind,
        ExpressionKind::RuntimeSlotApplicationHandoff(_)
    ) {
        // Runtime slot applications are helper-owned runtime payloads. The
        // previous stored-handoff path treated them as structural no-output
        // when a surrounding const fold proved the selected control-flow path
        // emits nothing; the owned expression variant preserves that contract.
        return Ok(None);
    }

    match fold_expression_kind_to_string(&expression_ref.kind, fold_context.string_table) {
        Some(FoldedStringPiece::Text(text)) => {
            output_buffer.push_str(&text);
            *emitted_output = true;
            Ok(None)
        }

        Some(FoldedStringPiece::Char(ch)) => {
            output_buffer.push(ch);
            *emitted_output = true;
            Ok(None)
        }

        Some(FoldedStringPiece::Skip) => Ok(None),

        Some(FoldedStringPiece::NestedTemplate) => {
            let ExpressionKind::Template(template) = &expression_ref.kind else {
                return Err(CompilerError::compiler_error(
                    "String coercion returned NestedTemplate for a non-Template expression kind.",
                )
                .into());
            };

            let template_kind = nested_template_kind(template, store, fold_context)?;
            reject_slot_insert_template(&template_kind)?;

            append_template_emission_to_buffer(
                fold_template_reference(
                    store,
                    FoldTemplateReference::Nested(&template.tir_reference),
                    fold_context,
                    fold_input,
                )?,
                output_buffer,
                emitted_output,
                fold_context,
            )
        }

        None => Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::NonFoldableConstTemplate,
            location.to_owned(),
        )
        .into()),
    }
}

/// Reads a nested AST template's kind from its authoritative TIR entry.
fn nested_template_kind(
    template: &Template,
    store: &TemplateIrStore,
    _fold_context: &TemplateFoldContext<'_>,
) -> Result<TemplateType, TemplateError> {
    template.tir_kind_from_store(store).ok_or_else(|| {
        CompilerError::compiler_error(format!(
            "TIR fold: nested template kind for {} was not found in the module store.",
            template.tir_reference.root
        ))
        .into()
    })
}

/// Folds a module-local child-template reference against the module store.
///
/// WHAT: uses the precise `root`/`phase`/`context` identity stored on the
///       `ChildTemplate` node to enter the named structural transition and fold
///       through the resulting exact view. Parsed references retain only the
///       parent expression authority; Composed references additionally carry
///       their slot and wrapper dimensions.
/// WHY: child-template nodes carry enough identity for precise view-based
///      folding. Reading the root from the active store keeps cache and
///      overlay identity intact.
fn fold_child_template_reference(
    store: &TemplateIrStore,
    reference: &TemplateTirChildReference,
    fold_context: &mut TemplateFoldContext<'_>,
    fold_input: &FoldTraversalInput<'_, '_>,
) -> Result<TemplateEmission, TemplateError> {
    fold_template_reference(
        store,
        FoldTemplateReference::Structural(reference),
        fold_context,
        fold_input,
    )
}

enum FoldTemplateReference<'reference> {
    Structural(&'reference TemplateTirChildReference),
    Nested(&'reference TemplateTirReference),
}

/// Resolves one effective template reference through the module store, then
/// enters the canonical template fold path.
///
/// Structural child and nested AST references use their named `TirView`
/// transitions. Every Composed-or-later exact child view uses the shared cache;
/// Parsed structural children use the direct reducer because their referenced
/// overlay dimensions are not active yet.
fn fold_template_reference(
    store: &TemplateIrStore,
    reference: FoldTemplateReference<'_>,
    fold_context: &mut TemplateFoldContext<'_>,
    fold_input: &FoldTraversalInput<'_, '_>,
) -> Result<TemplateEmission, TemplateError> {
    let (child_view, child_root) = {
        let parent_view = fold_input.view;

        match reference {
            FoldTemplateReference::Structural(reference) => {
                let child_view = parent_view.structural_child(*reference)?;
                (child_view, reference.root)
            }
            FoldTemplateReference::Nested(reference) => {
                if !reference.phase.is_at_least(TemplateTirPhase::Composed) {
                    return Err(CompilerError::compiler_error(format!(
                        "TIR fold: nested template {} at phase {} has not reached Composed.",
                        reference.root, reference.phase
                    ))
                    .into());
                }

                let child_view = parent_view.nested_template_value(*reference)?;
                (child_view, reference.root)
            }
        }
    };

    let child_fold_input = FoldTraversalInput { view: &child_view };
    if child_view.phase().is_at_least(TemplateTirPhase::Composed) {
        fold_exact_view(&child_view, fold_context)
    } else {
        fold_tir_template_with_view(store, child_root, fold_context, &child_fold_input)
    }
}

fn fold_resolved_slot_source(
    store: &TemplateIrStore,
    source: TemplateIrId,
    fold_context: &mut TemplateFoldContext<'_>,
    fold_input: &FoldTraversalInput<'_, '_>,
) -> Result<TemplateEmission, TemplateError> {
    let parent_view = fold_input.view;
    let source_view = parent_view.resolved_slot_source(source)?;
    let source_fold_input = FoldTraversalInput { view: &source_view };
    if source_view.phase().is_at_least(TemplateTirPhase::Composed) {
        fold_exact_view(&source_view, fold_context)
    } else {
        fold_tir_template_with_view(store, source, fold_context, &source_fold_input)
    }
}

/// Applies the wrapper-context overlay for a child-template occurrence, if any.
///
/// WHAT: resolves the effective `TirWrapperContext` for `occurrence_id` and folds
///       any inherited wrapper templates around the already-folded child emission.
///       `$fresh` suppression is honored by treating a suppressed context as empty,
///       and no-output/signal emissions pass through unchanged so skipped branches
///       and zero-iteration loops do not receive wrappers.
/// WHY: wrapper-context overlays replace the structural mutation of
///      `conditional_child_wrapper_set`. Applying them at the child occurrence
///      boundary lets the same structural child template be shared under different
///      wrapper contexts without store mutation.
fn apply_wrapper_context_overlay_to_child_emission(
    store: &TemplateIrStore,
    emission: TemplateEmission,
    fold_context: &mut TemplateFoldContext<'_>,
    fold_input: &FoldTraversalInput<'_, '_>,
    context: Option<&TirWrapperContext>,
) -> Result<TemplateEmission, TemplateError> {
    let Some(context) = context else {
        return Ok(emission);
    };

    // `$fresh` suppresses parent-applied wrappers at this occurrence. The
    // inherited wrapper set is omitted from the overlay when suppressed, but
    // honor the flag explicitly in case it coexists with a wrapper set ref.
    if context.skip_parent_child_wrappers {
        return Ok(emission);
    }

    let wrapper_set_ref = match context.inherited_wrapper_set {
        Some(wrapper_set_ref) => wrapper_set_ref,
        None => return Ok(emission),
    };

    let wrapper_set = store.get_wrapper_set(wrapper_set_ref).ok_or_else(|| {
        CompilerError::compiler_error(
            "TIR fold: inherited wrapper set referenced by overlay is missing.",
        )
    })?;

    let wrapper_references: Vec<TemplateWrapperReference> = wrapper_set.wrappers.clone();

    fold_conditional_child_wrappers_around_emission(
        store,
        &wrapper_references,
        emission,
        context.application_mode,
        fold_context,
        fold_input,
    )
}

/// Appends a child-template emission to the caller's output buffer.
fn append_template_emission_to_buffer(
    emission: TemplateEmission,
    output_buffer: &mut String,
    emitted_output: &mut bool,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    match emission {
        TemplateEmission::NoOutput => Ok(None),
        TemplateEmission::Output(output) => {
            output_buffer.push_str(fold_context.string_table.resolve(output));
            *emitted_output = true;
            Ok(None)
        }
        TemplateEmission::Break(output) => {
            if let Some(output) = output {
                output_buffer.push_str(fold_context.string_table.resolve(output));
                *emitted_output = true;
            }
            Ok(Some(TemplateLoopControlKind::Break))
        }
        TemplateEmission::Continue(output) => {
            if let Some(output) = output {
                output_buffer.push_str(fold_context.string_table.resolve(output));
                *emitted_output = true;
            }
            Ok(Some(TemplateLoopControlKind::Continue))
        }
    }
}

/// Applies conditional child wrappers to an already-folded emission using
/// a virtual wrapper fold that does not push synthetic nodes into the store.
///
/// WHAT: folds each inherited wrapper template around the already-folded child
///       output string, injecting the child output at the slot that the fill
///       content would route to (or appending it after slot-less wrapper
///       content). No-output and empty-signal cases pass through unchanged so
///       skipped branches or zero-iteration loops do not receive wrappers.
///
/// WHY: this replaces the structural wrap-then-fold path that pushed synthetic
///      `Text`/`Sequence` nodes and composed templates into the module
///      `TemplateIrStore`. The virtual child output is carried through the fold
///      walk and injected at slot positions, so the live store is never mutated
///      during view-native folding.
fn fold_conditional_child_wrappers_around_emission(
    store: &TemplateIrStore,
    wrapper_references: &[TemplateWrapperReference],
    emission: TemplateEmission,
    application_mode: TirWrapperApplicationMode,
    fold_context: &mut TemplateFoldContext<'_>,
    fold_input: &FoldTraversalInput<'_, '_>,
) -> Result<TemplateEmission, TemplateError> {
    let prepared_wrappers = prepare_wrapper_references(wrapper_references)?;

    let (output, signal_kind) = match emission {
        TemplateEmission::NoOutput => {
            if matches!(application_mode, TirWrapperApplicationMode::IfChildEmits)
                || prepared_wrappers.is_empty()
            {
                return Ok(TemplateEmission::NoOutput);
            }

            (fold_context.string_table.intern(""), None)
        }
        TemplateEmission::Output(output) => (output, None),
        TemplateEmission::Break(Some(output)) => (output, Some(TemplateLoopControlKind::Break)),
        TemplateEmission::Continue(Some(output)) => {
            (output, Some(TemplateLoopControlKind::Continue))
        }
        TemplateEmission::Break(None) => {
            if matches!(application_mode, TirWrapperApplicationMode::IfChildEmits)
                || prepared_wrappers.is_empty()
            {
                return Ok(TemplateEmission::Break(None));
            }

            (
                fold_context.string_table.intern(""),
                Some(TemplateLoopControlKind::Break),
            )
        }
        TemplateEmission::Continue(None) => {
            if matches!(application_mode, TirWrapperApplicationMode::IfChildEmits)
                || prepared_wrappers.is_empty()
            {
                return Ok(TemplateEmission::Continue(None));
            }

            (
                fold_context.string_table.intern(""),
                Some(TemplateLoopControlKind::Continue),
            )
        }
    };

    if prepared_wrappers.is_empty() {
        return Ok(template_emission_from_output_and_signal(
            output,
            signal_kind,
        ));
    }

    add_ast_counter(
        AstCounter::TemplateWrapperApplications,
        prepared_wrappers.len(),
    );

    // Iterate wrappers in reverse (outermost-first), folding each around the
    // current child output. The output of one wrapper becomes the input to the
    // next, matching the nesting order of the structural wrap path.
    let mut current_output = output;
    for wrapper_reference in prepared_wrappers.iter().rev() {
        current_output = fold_tir_wrapper_around_child_output(
            store,
            wrapper_reference,
            current_output,
            fold_context,
            fold_input,
        )?;
    }

    Ok(template_emission_from_output_and_signal(
        current_output,
        signal_kind,
    ))
}

/// Folds a single wrapper template around an already-folded child output string
/// without pushing synthetic nodes into the store.
///
/// WHAT: folds the wrapper template's root, injecting the child output at the
///       slot that the fill content would route to. For slot-less wrappers the
///       child output is appended after the wrapper content. The wrapper's own
///       `conditional_child_wrapper_set` is not applied, matching the structural
///       composed/prepended template which always carried `None`.
///
/// WHY: this is the virtual replacement for `wrap_tir_node_in_wrappers` +
///      `fold_tir_node` on a synthetic subtree.
fn fold_tir_wrapper_around_child_output(
    store: &TemplateIrStore,
    prepared_wrapper: &PreparedWrapperReference,
    child_output: StringId,
    fold_context: &mut TemplateFoldContext<'_>,
    fold_input: &FoldTraversalInput<'_, '_>,
) -> Result<StringId, TemplateError> {
    let wrapper_reference = prepared_wrapper.reference;
    let wrapper_store = store;
    let wrapper_template = wrapper_store
        .get_template(wrapper_reference.root)
        .cloned()
        .ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "TIR wrapper fold: wrapper template {} not found in the module store.",
                wrapper_reference.root
            ))
        })?;

    let parent_view = fold_input.view;
    let wrapper_view = parent_view.wrapper(wrapper_reference)?;
    let wrapper_fold_input = FoldTraversalInput {
        view: &wrapper_view,
    };

    fold_tir_wrapper_with_input(
        wrapper_store,
        wrapper_reference.root,
        &wrapper_template,
        child_output,
        fold_context,
        &wrapper_fold_input,
    )
}

/// Folds one resolved wrapper template around an already-folded child output.
///
/// WHAT: applies the wrapper's effective slot routing and preserves injected
///      child precedence at the loose-fill target.
/// WHY: the same wrapper identity is shared across entry paths, so the
///      output walk must not discard its exact view.
fn fold_tir_wrapper_with_input(
    wrapper_store: &TemplateIrStore,
    wrapper_template_id: TemplateIrId,
    wrapper_template: &TemplateIr,
    child_output: StringId,
    fold_context: &mut TemplateFoldContext<'_>,
    wrapper_fold_input: &FoldTraversalInput<'_, '_>,
) -> Result<StringId, TemplateError> {
    reject_slot_insert_template(&wrapper_template.kind)?;

    if wrapper_template.runtime_slot_plan.is_some() {
        return Err(CompilerError::compiler_error(
            "TIR wrapper fold: a runtime slot plan reached the fold reducer without a foldable preparation proof.",
        )
        .into());
    }

    let child_output_len = fold_context.string_table.resolve(child_output).len();
    let estimated_bytes = wrapper_template.summary.estimated_output_bytes + child_output_len;
    let mut output_buffer = reserve_tir_fold_output_buffer(estimated_bytes);
    let mut emitted_output = false;

    let schema = collect_tir_slot_schema(wrapper_store, wrapper_template_id)?;

    if !schema.has_any_slots() {
        // Slot-less wrapper: fold the wrapper content, then append the child
        // output. This matches `build_tir_prepended_wrapper_template` which
        // creates a sequence [wrapper, child] and folds it.
        fold_tir_node_into_buffer(
            wrapper_store,
            wrapper_template.root,
            &mut output_buffer,
            &mut emitted_output,
            fold_context,
            wrapper_fold_input,
        )?;

        output_buffer.push_str(fold_context.string_table.resolve(child_output));
    } else {
        // Slot-bearing wrappers inject at the loose-fill target first. Named-
        // only wrappers have no target, so their resolved slots are folded and
        // the child is appended after the wrapper content.
        let fill_target_key = schema.loose_fill_target_key();
        fold_tir_wrapper_node_with_child_output(
            wrapper_store,
            wrapper_template.root,
            child_output,
            fill_target_key.as_ref(),
            &mut output_buffer,
            &mut emitted_output,
            fold_context,
            wrapper_fold_input,
        )?;

        if fill_target_key.is_none() {
            output_buffer.push_str(fold_context.string_table.resolve(child_output));
        }
    }

    let actual_len = output_buffer.len();
    record_tir_fold_output_estimate_miss(actual_len, estimated_bytes);
    let output_id = fold_context.string_table.intern(&output_buffer);
    record_tir_fold_output_intern(actual_len);

    Ok(output_id)
}

/// Recursively folds a wrapper template node, injecting the already-folded
/// child output at an optional loose-fill target and resolving other slots.
///
/// WHAT: walks the wrapper template's root, folding text, dynamic expressions,
///       and child templates normally. When a `Slot` node's key matches the fill
///       target, the child output is pushed directly into the buffer. Other
///       slots use the wrapper view's effective resolution when available.
///       Branch chains and loops inside the wrapper are handled by evaluating
///       the same conditions and recursing with the same child injection.
///
/// WHY: this is analogous to `fold_tir_aggregate_wrapper_node` but injects at
///      `Slot` nodes instead of `AggregateOutput` markers. No synthetic nodes
///      are pushed into the store, so the live module store is never mutated.
#[allow(clippy::too_many_arguments)]
fn fold_tir_wrapper_node_with_child_output(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
    child_output: StringId,
    fill_target_key: Option<&SlotKey>,
    output_buffer: &mut String,
    emitted_output: &mut bool,
    fold_context: &mut TemplateFoldContext<'_>,
    fold_input: &FoldTraversalInput<'_, '_>,
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    let node = store
        .get_node(node_id)
        .cloned()
        .ok_or_else(|| missing_node_diagnostic(node_id))?;

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            for &child_id in children {
                let signal = fold_tir_wrapper_node_with_child_output(
                    store,
                    child_id,
                    child_output,
                    fill_target_key,
                    output_buffer,
                    emitted_output,
                    fold_context,
                    fold_input,
                )?;
                if signal.is_some() {
                    return Ok(signal);
                }
            }
            Ok(None)
        }

        TemplateIrNodeKind::Text { text, .. } => {
            output_buffer.push_str(fold_context.string_table.resolve(*text));
            *emitted_output = true;
            Ok(None)
        }

        TemplateIrNodeKind::DynamicExpression { expression, site_id, .. } => {
            let effective_expression = fold_input.effective_expression_for_site(*site_id)?;
            let expression_to_fold = effective_expression.unwrap_or(expression);
            fold_tir_dynamic_expression(
                store,
                expression_to_fold,
                output_buffer,
                emitted_output,
                fold_context,
                &node.location,
                fold_input,
            )
        }

        TemplateIrNodeKind::ChildTemplate {
            reference,
            occurrence_id,
            ..
        } => {
            // Resolve the occurrence context while the parent wrapper view is
            // still active. The nested child then enters its exact view for
            // expression, slot, and wrapper dimensions before the parent
            // occurrence context is applied to its completed emission.
            let occurrence_context = fold_input
                .view
                .effective_wrapper_context(*occurrence_id)?
                .cloned();
            let child_template_id = reference.root;

            let child_template = store
                .get_template(child_template_id)
                .cloned()
                .ok_or_else(|| missing_template_diagnostic(child_template_id))?;
            reject_slot_insert_template(&child_template.kind)?;

            if child_template.runtime_slot_plan.is_some() {
                return Err(CompilerError::compiler_error(
                    "TIR wrapper fold: a runtime child slot plan reached the fold reducer without a foldable preparation proof.",
                )
                .into());
            }

            let parent_view = fold_input.view;
            let child_view = parent_view.structural_child(*reference)?;
            let child_fold_input = FoldTraversalInput { view: &child_view };
            let child_emission = fold_tir_wrapper_node_to_emission(
                store,
                child_template.root,
                child_output,
                fill_target_key,
                fold_context,
                &child_fold_input,
            )?;

            let wrapped_emission = apply_wrapper_context_overlay_to_child_emission(
                store,
                child_emission,
                fold_context,
                fold_input,
                occurrence_context.as_ref(),
            )?;

            append_template_emission_to_buffer(
                wrapped_emission,
                output_buffer,
                emitted_output,
                fold_context,
            )
        }

        TemplateIrNodeKind::Slot { placeholder } => {
            if fill_target_key.is_some_and(|key| placeholder.key == *key) {
                output_buffer.push_str(fold_context.string_table.resolve(child_output));
                *emitted_output = true;
                // Injection has precedence over any overlay-resolved sources
                // for this slot, matching HIR handoff materialization.
                return Ok(None);
            }

            if let Some(resolution) = fold_input
                .view
                .effective_slot_resolution(placeholder.occurrence_id)?
                && let TirSlotResolutionKind::Resolved { sources } = &resolution.kind
            {
                for source in sources {
                    let emission = fold_resolved_slot_source(
                        store,
                        *source,
                        fold_context,
                        fold_input,
                    )?;
                    append_template_emission_to_buffer(
                        emission,
                        output_buffer,
                        emitted_output,
                        fold_context,
                    )?;
                }
            }

            // Unresolved or uncovered slots remain empty.
            Ok(None)
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            fold_tir_wrapper_branch_chain(
                store,
                branches,
                *fallback,
                child_output,
                fill_target_key,
                output_buffer,
                emitted_output,
                fold_context,
                fold_input,
            )
        }

        TemplateIrNodeKind::Loop {
            header,
            header_sites,
            body,
            aggregate_wrapper,
        } => fold_tir_loop(
            store,
            header,
            *header_sites,
            *body,
            *aggregate_wrapper,
            output_buffer,
            emitted_output,
            fold_context,
            fold_input,
            &node.location,
            |store, body_id, fold_ctx, fold_input| {
                fold_tir_wrapper_node_to_emission(
                    store,
                    body_id,
                    child_output,
                    fill_target_key,
                    fold_ctx,
                    fold_input,
                )
            },
        ),

        TemplateIrNodeKind::LoopControl { kind } => Ok(Some(*kind)),

        // AggregateOutput markers are only valid inside aggregate wrapper
        // subtrees, not inside conditional child wrapper templates.
        TemplateIrNodeKind::AggregateOutput => Err(CompilerError::compiler_error(
            "TIR wrapper fold: AggregateOutput marker reached a wrapper fold site outside an aggregate wrapper.",
        )
        .into()),

        // Insert contributions should have been consumed by slot composition.
        TemplateIrNodeKind::InsertContribution { .. } => Err(CompilerError::compiler_error(
            "Insert contribution reached TIR wrapper folding without being consumed by slot composition.",
        )
        .into()),

        // Runtime slot sites are resolved during AST planning, not folding.
        TemplateIrNodeKind::RuntimeSlotSite { .. } => Ok(None),
    }
}

/// Folds a wrapper template node into an independent emission, carrying the
/// child output for slot injection.
///
/// WHAT: creates a fresh output buffer, folds the node with child output
///       injection, and returns the full `TemplateEmission`. This is the
///       wrapper-fold equivalent of `fold_tir_node`.
fn fold_tir_wrapper_node_to_emission(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
    child_output: StringId,
    fill_target_key: Option<&SlotKey>,
    fold_context: &mut TemplateFoldContext<'_>,
    fold_input: &FoldTraversalInput<'_, '_>,
) -> Result<TemplateEmission, TemplateError> {
    let child_output_len = fold_context.string_table.resolve(child_output).len();
    let mut buffer = reserve_tir_fold_output_buffer(child_output_len);
    let mut emitted_output = false;

    let signal = fold_tir_wrapper_node_with_child_output(
        store,
        node_id,
        child_output,
        fill_target_key,
        &mut buffer,
        &mut emitted_output,
        fold_context,
        fold_input,
    )?;

    build_emission_from_buffer(
        buffer,
        child_output_len,
        signal,
        emitted_output,
        fold_context,
    )
}

/// Evaluates a branch chain inside a wrapper template, folding the selected
/// branch body with child output injection.
///
/// WHAT: matches `fold_tir_branch_chain` but folds the selected branch body
///       through `fold_tir_wrapper_node_with_child_output` instead of the main
///       fold walker, so slot injection remains active inside branch bodies.
#[allow(clippy::too_many_arguments)]
fn fold_tir_wrapper_branch_chain(
    store: &TemplateIrStore,
    branches: &[TemplateIrBranch],
    fallback: Option<TemplateIrNodeId>,
    child_output: StringId,
    fill_target_key: Option<&SlotKey>,
    output_buffer: &mut String,
    emitted_output: &mut bool,
    fold_context: &mut TemplateFoldContext<'_>,
    fold_input: &FoldTraversalInput<'_, '_>,
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    for branch in branches {
        let effective_expression =
            fold_input.effective_expression_for_site(branch.selector_site_id)?;

        let selected = match (&branch.selector, effective_expression) {
            (TemplateBranchSelector::Bool(condition), None) => {
                fold_bool_condition(condition, &branch.location, fold_context)?
            }
            (TemplateBranchSelector::Bool(_), Some(effective)) => {
                fold_bool_condition(effective, &branch.location, fold_context)?
            }
            (TemplateBranchSelector::OptionPresentCapture { scrutinee, pattern }, None) => {
                if let Some(payload) =
                    selected_option_capture_payload(scrutinee, pattern, fold_context)?
                {
                    return fold_tir_wrapper_branch_with_bindings(
                        store,
                        branch,
                        [payload],
                        child_output,
                        fill_target_key,
                        output_buffer,
                        emitted_output,
                        fold_context,
                        fold_input,
                    );
                }

                false
            }
            (TemplateBranchSelector::OptionPresentCapture { pattern, .. }, Some(effective)) => {
                if let Some(payload) =
                    selected_option_capture_payload(effective, pattern, fold_context)?
                {
                    return fold_tir_wrapper_branch_with_bindings(
                        store,
                        branch,
                        [payload],
                        child_output,
                        fill_target_key,
                        output_buffer,
                        emitted_output,
                        fold_context,
                        fold_input,
                    );
                }

                false
            }
        };

        if selected {
            return fold_tir_wrapper_node_with_child_output(
                store,
                branch.body,
                child_output,
                fill_target_key,
                output_buffer,
                emitted_output,
                fold_context,
                fold_input,
            );
        }
    }

    let Some(fallback_id) = fallback else {
        return Ok(None);
    };

    fold_tir_wrapper_node_with_child_output(
        store,
        fallback_id,
        child_output,
        fill_target_key,
        output_buffer,
        emitted_output,
        fold_context,
        fold_input,
    )
}

/// Folds a selected wrapper branch body after pushing option-capture bindings.
#[allow(clippy::too_many_arguments)]
fn fold_tir_wrapper_branch_with_bindings<const N: usize>(
    store: &TemplateIrStore,
    branch: &TemplateIrBranch,
    bindings: [TemplateFoldBinding; N],
    child_output: StringId,
    fill_target_key: Option<&SlotKey>,
    output_buffer: &mut String,
    emitted_output: &mut bool,
    fold_context: &mut TemplateFoldContext<'_>,
    fold_input: &FoldTraversalInput<'_, '_>,
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    let previous_bindings_len = fold_context.push_bindings(bindings);
    let result = fold_tir_wrapper_node_with_child_output(
        store,
        branch.body,
        child_output,
        fill_target_key,
        output_buffer,
        emitted_output,
        fold_context,
        fold_input,
    );
    fold_context.restore_bindings(previous_bindings_len);

    result
}

// -------------------------
//  Branch-chain folding
// -------------------------

/// Folds a branch chain by selecting the first true branch or the fallback.
fn fold_tir_branch_chain(
    store: &TemplateIrStore,
    branches: &[TemplateIrBranch],
    fallback: Option<TemplateIrNodeId>,
    output_buffer: &mut String,
    emitted_output: &mut bool,
    fold_context: &mut TemplateFoldContext<'_>,
    fold_input: &FoldTraversalInput<'_, '_>,
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    for branch in branches {
        // Check for a view-effective expression for this branch's selector
        // site. When present, it replaces the structural selector expression
        // for condition evaluation through the same view-effective semantics as
        // the old clone-and-apply path.
        let effective_expression =
            fold_input.effective_expression_for_site(branch.selector_site_id)?;

        let selected = match (&branch.selector, effective_expression) {
            (TemplateBranchSelector::Bool(condition), None) => {
                fold_bool_condition(condition, &branch.location, fold_context)?
            }
            (TemplateBranchSelector::Bool(_), Some(effective)) => {
                fold_bool_condition(effective, &branch.location, fold_context)?
            }
            (TemplateBranchSelector::OptionPresentCapture { scrutinee, pattern }, None) => {
                if let Some(payload) =
                    selected_option_capture_payload(scrutinee, pattern, fold_context)?
                {
                    return fold_tir_branch_with_bindings(
                        store,
                        branch,
                        [payload],
                        output_buffer,
                        emitted_output,
                        fold_context,
                        fold_input,
                    );
                }

                false
            }
            (TemplateBranchSelector::OptionPresentCapture { pattern, .. }, Some(effective)) => {
                if let Some(payload) =
                    selected_option_capture_payload(effective, pattern, fold_context)?
                {
                    return fold_tir_branch_with_bindings(
                        store,
                        branch,
                        [payload],
                        output_buffer,
                        emitted_output,
                        fold_context,
                        fold_input,
                    );
                }

                false
            }
        };

        if selected {
            return fold_tir_branch_body(
                store,
                branch.body,
                output_buffer,
                emitted_output,
                fold_context,
                fold_input,
            );
        }
    }

    fold_tir_fallback_branch(
        store,
        fallback,
        output_buffer,
        emitted_output,
        fold_context,
        fold_input,
    )
}

/// Folds a selected branch body after pushing option-capture bindings.
fn fold_tir_branch_with_bindings<const N: usize>(
    store: &TemplateIrStore,
    branch: &TemplateIrBranch,
    bindings: [TemplateFoldBinding; N],
    output_buffer: &mut String,
    emitted_output: &mut bool,
    fold_context: &mut TemplateFoldContext<'_>,
    fold_input: &FoldTraversalInput<'_, '_>,
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    let previous_bindings_len = fold_context.push_bindings(bindings);
    let result = fold_tir_branch_body(
        store,
        branch.body,
        output_buffer,
        emitted_output,
        fold_context,
        fold_input,
    );
    fold_context.restore_bindings(previous_bindings_len);

    result
}

/// Folds a branch body node.
fn fold_tir_branch_body(
    store: &TemplateIrStore,
    body_id: TemplateIrNodeId,
    output_buffer: &mut String,
    emitted_output: &mut bool,
    fold_context: &mut TemplateFoldContext<'_>,
    fold_input: &FoldTraversalInput<'_, '_>,
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    fold_tir_node_into_buffer(
        store,
        body_id,
        output_buffer,
        emitted_output,
        fold_context,
        fold_input,
    )
}

/// Folds the fallback branch, if any.
fn fold_tir_fallback_branch(
    store: &TemplateIrStore,
    fallback: Option<TemplateIrNodeId>,
    output_buffer: &mut String,
    emitted_output: &mut bool,
    fold_context: &mut TemplateFoldContext<'_>,
    fold_input: &FoldTraversalInput<'_, '_>,
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    let Some(fallback_id) = fallback else {
        return Ok(None);
    };

    fold_tir_node_into_buffer(
        store,
        fallback_id,
        output_buffer,
        emitted_output,
        fold_context,
        fold_input,
    )
}

// -------------------------
//  Loop folding
// -------------------------

/// Folds a TIR loop node, including its aggregate wrapper.
///
/// This helper matches the `fold_template_loop` signature: each parameter
/// represents a distinct responsibility (store, header, body, aggregate plan,
/// output sink, fold context, source location). Grouping them would not improve
/// readability, so the argument count is allowed.
#[allow(clippy::too_many_arguments)]
fn fold_tir_loop<F>(
    store: &TemplateIrStore,
    header: &TemplateLoopHeader,
    header_sites: TemplateLoopHeaderExpressionSites,
    body_id: TemplateIrNodeId,
    aggregate_wrapper: Option<TemplateIrNodeId>,
    output_buffer: &mut String,
    emitted_output: &mut bool,
    fold_context: &mut TemplateFoldContext<'_>,
    fold_input: &FoldTraversalInput<'_, '_>,
    loop_location: &SourceLocation,
    mut fold_body: F,
) -> Result<Option<TemplateLoopControlKind>, TemplateError>
where
    F: FnMut(
        &TemplateIrStore,
        TemplateIrNodeId,
        &mut TemplateFoldContext<'_>,
        &FoldTraversalInput<'_, '_>,
    ) -> Result<TemplateEmission, TemplateError>,
{
    // The body estimate seeds the aggregate buffer reservation.
    let body_estimate = estimate_tir_node_output_bytes(store, body_id, fold_context.string_table)?;

    let (aggregate, estimated_aggregate, did_emit_body) = match header {
        TemplateLoopHeader::Conditional { condition } => {
            let site_id = match header_sites {
                TemplateLoopHeaderExpressionSites::Conditional { condition } => condition,
                _ => {
                    return Err(CompilerError::compiler_error(
                        "TIR fold: loop header/header_sites shape mismatch (Conditional).",
                    )
                    .into());
                }
            };

            // Use the view-effective condition when an expression overlay
            // covers the site, otherwise fall back to the structural condition.
            let effective_condition = fold_input.effective_expression_for_site(site_id)?;
            let condition_ref = effective_condition.unwrap_or(condition.as_ref());

            let condition_value =
                fold_conditional_loop_const_condition(condition_ref, loop_location)?;
            if !condition_value {
                return Ok(None);
            }

            return Err(CompilerDiagnostic::invalid_template_structure(
                InvalidTemplateStructureReason::TemplateConditionalLoopConstTrue,
                condition_location_or_loop_location(condition_ref, loop_location),
            )
            .into());
        }

        TemplateLoopHeader::Range { bindings, range } => {
            let (start_site, end_site, step_site) = match header_sites {
                TemplateLoopHeaderExpressionSites::Range { start, end, step } => (start, end, step),
                _ => {
                    return Err(CompilerError::compiler_error(
                        "TIR fold: loop header/header_sites shape mismatch (Range).",
                    )
                    .into());
                }
            };

            // Check for view-effective overrides on range expressions. When an
            // overlay covers a range site, the effective expression replaces the
            // structural value for cursor construction. Only overridden
            // expressions are cloned; the rest use structural references.
            let effective_start = fold_input.effective_expression_for_site(start_site)?;
            let effective_end = fold_input.effective_expression_for_site(end_site)?;
            let effective_step = step_site
                .map(|site_id| fold_input.effective_expression_for_site(site_id))
                .transpose()?
                .flatten();

            let has_override =
                effective_start.is_some() || effective_end.is_some() || effective_step.is_some();

            let estimated_iterations = std::cmp::min(
                fold_context.template_const_loop_iteration_limit,
                FOLD_RANGE_LOOP_RESERVE_ITERATION_CAP,
            );
            let estimated_aggregate =
                estimate_loop_aggregate_bytes(body_estimate, estimated_iterations);
            let mut aggregate = reserve_tir_fold_output_buffer(estimated_aggregate);
            let mut did_emit = false;

            // Build the cursor from either the effective range (when overrides
            // exist) or the structural range directly. The effective range
            // clones only the overridden expressions, which is cheap compared
            // to cloning the entire store.
            let effective_range;
            let range_ref: &RangeLoopSpec = if has_override {
                let mut r = range.as_ref().clone();
                if let Some(expr) = effective_start {
                    r.start = expr.clone();
                }
                if let Some(expr) = effective_end {
                    r.end = expr.clone();
                }
                if let Some(expr) = effective_step {
                    r.step = Some(expr.clone());
                }
                effective_range = r;
                &effective_range
            } else {
                range.as_ref()
            };

            let mut cursor = ConstRangeCursor::new(
                range_ref,
                fold_context.template_const_loop_iteration_limit,
                loop_location.clone(),
            )?;

            while let Some(counter) = cursor.next_counter()? {
                add_ast_counter(AstCounter::TemplateFoldLoopIterations, 1);
                let iteration_bindings =
                    build_range_iteration_bindings(bindings, counter, cursor.iteration_count() - 1);
                let (iteration_did_emit, iteration_signal) = fold_tir_loop_iteration(
                    store,
                    body_id,
                    iteration_bindings,
                    fold_context,
                    loop_location,
                    &mut aggregate,
                    fold_input,
                    &mut fold_body,
                )?;

                did_emit |= iteration_did_emit;

                match iteration_signal {
                    Some(TemplateLoopControlKind::Break) => break,
                    Some(TemplateLoopControlKind::Continue) => continue,
                    None => {}
                }
            }

            (aggregate, estimated_aggregate, did_emit)
        }

        TemplateLoopHeader::Collection { bindings, iterable } => {
            let site_id = match header_sites {
                TemplateLoopHeaderExpressionSites::Collection { iterable } => iterable,
                _ => {
                    return Err(CompilerError::compiler_error(
                        "TIR fold: loop header/header_sites shape mismatch (Collection).",
                    )
                    .into());
                }
            };

            // Use the view-effective iterable when an expression overlay covers
            // the site, otherwise fall back to the structural iterable.
            let effective_iterable = fold_input.effective_expression_for_site(site_id)?;
            let iterable_ref = effective_iterable.unwrap_or(iterable.as_ref());

            let items = const_collection_items(iterable_ref)?;
            let estimated_iterations = std::cmp::min(
                items.len(),
                fold_context.template_const_loop_iteration_limit,
            );
            let estimated_aggregate =
                estimate_loop_aggregate_bytes(body_estimate, estimated_iterations);
            let mut aggregate = reserve_tir_fold_output_buffer(estimated_aggregate);
            let mut did_emit = false;

            for (index, item) in items.iter().enumerate() {
                add_ast_counter(AstCounter::TemplateFoldLoopIterations, 1);
                if index >= fold_context.template_const_loop_iteration_limit {
                    return Err(CompilerDiagnostic::invalid_template_structure(
                        InvalidTemplateStructureReason::TemplateConstLoopExpansionLimitExceeded {
                            limit: fold_context.template_const_loop_iteration_limit,
                        },
                        loop_location.clone(),
                    )
                    .into());
                }

                let iteration_bindings = build_collection_iteration_bindings(bindings, item, index);
                let (iteration_did_emit, iteration_signal) = fold_tir_loop_iteration(
                    store,
                    body_id,
                    iteration_bindings,
                    fold_context,
                    loop_location,
                    &mut aggregate,
                    fold_input,
                    &mut fold_body,
                )?;

                did_emit |= iteration_did_emit;

                match iteration_signal {
                    Some(TemplateLoopControlKind::Break) => break,
                    Some(TemplateLoopControlKind::Continue) => continue,
                    None => {}
                }
            }

            (aggregate, estimated_aggregate, did_emit)
        }
    };

    if !did_emit_body {
        return Ok(None);
    }

    let actual_aggregate_len = aggregate.len();
    record_tir_fold_output_estimate_miss(actual_aggregate_len, estimated_aggregate);
    let aggregate_id = fold_context.string_table.intern(&aggregate);
    record_tir_fold_output_intern(actual_aggregate_len);

    let Some(wrapper_node_id) = aggregate_wrapper else {
        // No wrapper plan: the aggregate output is the loop's output.
        output_buffer.push_str(fold_context.string_table.resolve(aggregate_id));
        *emitted_output = true;
        return Ok(None);
    };

    fold_tir_aggregate_wrapper(
        store,
        wrapper_node_id,
        aggregate_id,
        output_buffer,
        emitted_output,
        fold_context,
        fold_input,
    )
}

/// Folds one loop-body iteration into the aggregate buffer.
///
/// WHAT: pushes the iteration bindings, invokes `fold_body` to fold the body
///       node into an emission, restores the bindings, and appends the emission
///       output to the aggregate buffer.
/// WHY: parameterizing the body fold lets both the main fold walker (which
///      passes `fold_tir_node`) and the virtual wrapper fold walker (which
///      passes a child-output-injecting fold) reuse the same iteration logic
///      without duplicating the cursor, binding, or aggregate emission handling.
#[allow(clippy::too_many_arguments)]
fn fold_tir_loop_iteration<F>(
    store: &TemplateIrStore,
    body_id: TemplateIrNodeId,
    iteration_bindings: Vec<TemplateFoldBinding>,
    fold_context: &mut TemplateFoldContext<'_>,
    loop_location: &SourceLocation,
    aggregate: &mut String,
    fold_input: &FoldTraversalInput<'_, '_>,
    fold_body: F,
) -> Result<(bool, Option<TemplateLoopControlKind>), TemplateError>
where
    F: FnOnce(
        &TemplateIrStore,
        TemplateIrNodeId,
        &mut TemplateFoldContext<'_>,
        &FoldTraversalInput<'_, '_>,
    ) -> Result<TemplateEmission, TemplateError>,
{
    let previous_bindings_len = fold_context.push_bindings(iteration_bindings);
    let folded_result = fold_body(store, body_id, fold_context, fold_input);
    fold_context.restore_bindings(previous_bindings_len);

    let emission =
        folded_result.map_err(|error| loop_body_not_const_error(error, loop_location))?;

    match emission {
        TemplateEmission::NoOutput => Ok((false, None)),
        TemplateEmission::Output(output) => {
            aggregate.push_str(fold_context.string_table.resolve(output));
            Ok((true, None))
        }
        TemplateEmission::Break(output) => {
            let did_emit = output.is_some();
            if let Some(output) = output {
                aggregate.push_str(fold_context.string_table.resolve(output));
            }
            Ok((did_emit, Some(TemplateLoopControlKind::Break)))
        }
        TemplateEmission::Continue(output) => {
            let did_emit = output.is_some();
            if let Some(output) = output {
                aggregate.push_str(fold_context.string_table.resolve(output));
            }
            Ok((did_emit, Some(TemplateLoopControlKind::Continue)))
        }
    }
}

/// Folds an aggregate wrapper subtree around a loop aggregate output.
///
/// WHAT: walks the TIR subtree that the converter built from the AST aggregate
/// render plan, replacing the `AggregateOutput` marker with the already-folded
/// aggregate string.
/// WHY: this is the TIR-native replacement for the old AST render-plan wrapper
/// fold path.
fn fold_tir_aggregate_wrapper(
    store: &TemplateIrStore,
    wrapper_node_id: TemplateIrNodeId,
    aggregate_output: StringId,
    output_buffer: &mut String,
    emitted_output: &mut bool,
    fold_context: &mut TemplateFoldContext<'_>,
    fold_input: &FoldTraversalInput<'_, '_>,
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    let aggregate_output_len = fold_context.string_table.resolve(aggregate_output).len();
    let estimated_bytes = estimate_aggregate_wrapper_bytes(
        store,
        wrapper_node_id,
        aggregate_output_len,
        fold_context.string_table,
    )?;
    let mut wrapper_buffer = reserve_tir_fold_output_buffer(estimated_bytes);
    let mut wrapper_emitted_output = false;

    let signal = fold_tir_aggregate_wrapper_node(
        store,
        wrapper_node_id,
        aggregate_output,
        &mut wrapper_buffer,
        &mut wrapper_emitted_output,
        fold_context,
        fold_input,
    )?;

    if signal.is_some() {
        return Err(CompilerError::compiler_error(
            "Loop-control signal reached aggregate wrapper folding; aggregate wrappers should not contain loop control.",
        )
        .into());
    }

    if !wrapper_emitted_output {
        return Ok(None);
    }

    let actual_len = wrapper_buffer.len();
    record_tir_fold_output_estimate_miss(actual_len, estimated_bytes);
    let wrapper_id = fold_context.string_table.intern(&wrapper_buffer);
    record_tir_fold_output_intern(actual_len);

    output_buffer.push_str(fold_context.string_table.resolve(wrapper_id));
    *emitted_output = true;

    Ok(None)
}

/// Folds a child-template reference that appears inside an aggregate wrapper.
///
/// WHAT: the referenced template is a wrapper template (for example from a
///       `$children(..)` directive) whose body contains the `AggregateOutput`
///       marker. The marker must be replaced with the already-folded aggregate
///       string, just like direct aggregate-wrapper siblings. The helper recurses
///       into the child template's root so nested wrapper layers are expanded.
///
/// WHY: the normal `fold_tir_child_template` entry treats the child as an
///      independent template and rejects `AggregateOutput` as an internal error.
///      Preserving aggregate context across the child-template boundary lets
///      composed wrapper TIR shapes fold without losing aggregate context.
fn fold_tir_aggregate_wrapper_child_template(
    store: &TemplateIrStore,
    reference: &TemplateTirChildReference,
    aggregate_output: StringId,
    wrapper_buffer: &mut String,
    wrapper_emitted_output: &mut bool,
    fold_context: &mut TemplateFoldContext<'_>,
    fold_input: &FoldTraversalInput<'_, '_>,
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    let template_id = reference.root;
    let template = store
        .get_template(template_id)
        .cloned()
        .ok_or_else(|| missing_template_diagnostic(template_id))?;
    reject_slot_insert_template(&template.kind)?;

    if template.runtime_slot_plan.is_some() {
        return Err(CompilerError::compiler_error(
            "TIR aggregate-wrapper fold: a runtime child slot plan reached the fold reducer without a foldable preparation proof.",
        )
        .into());
    }

    let parent_view = fold_input.view;
    let child_view = parent_view.structural_child(*reference)?;
    let child_fold_input = FoldTraversalInput { view: &child_view };
    fold_tir_aggregate_wrapper_node(
        store,
        template.root,
        aggregate_output,
        wrapper_buffer,
        wrapper_emitted_output,
        fold_context,
        &child_fold_input,
    )
}

/// Recursively folds one node inside an aggregate wrapper subtree.
fn fold_tir_aggregate_wrapper_node(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
    aggregate_output: StringId,
    wrapper_buffer: &mut String,
    wrapper_emitted_output: &mut bool,
    fold_context: &mut TemplateFoldContext<'_>,
    fold_input: &FoldTraversalInput<'_, '_>,
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    let node = store
        .get_node(node_id)
        .cloned()
        .ok_or_else(|| missing_node_diagnostic(node_id))?;

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            for &child_id in children {
                let signal = fold_tir_aggregate_wrapper_node(
                    store,
                    child_id,
                    aggregate_output,
                    wrapper_buffer,
                    wrapper_emitted_output,
                    fold_context,
                    fold_input,
                )?;

                if signal.is_some() {
                    return Ok(signal);
                }
            }

            Ok(None)
        }

        TemplateIrNodeKind::Text { text, .. } => {
            wrapper_buffer.push_str(fold_context.string_table.resolve(*text));
            *wrapper_emitted_output = true;
            Ok(None)
        }

        TemplateIrNodeKind::DynamicExpression { expression, site_id, .. } => {
            // Use the view-effective expression when an overlay covers this
            // site, matching the view-native fold walker behavior.
            let effective_expression = fold_input.effective_expression_for_site(*site_id)?;
            let expression_to_fold = effective_expression.unwrap_or(expression);

            let signal = fold_tir_dynamic_expression(
                store,
                expression_to_fold,
                wrapper_buffer,
                wrapper_emitted_output,
                fold_context,
                &node.location,
                fold_input,
            )?;

            if signal.is_some() {
                return Ok(signal);
            }

            Ok(None)
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            fold_tir_aggregate_wrapper_child_template(
                store,
                reference,
                aggregate_output,
                wrapper_buffer,
                wrapper_emitted_output,
                fold_context,
                fold_input,
            )
        }

        TemplateIrNodeKind::AggregateOutput => {
            wrapper_buffer.push_str(fold_context.string_table.resolve(aggregate_output));
            *wrapper_emitted_output = true;
            Ok(None)
        }

        _ => Err(CompilerError::compiler_error(
            "TIR fold: malformed aggregate wrapper subtree contains a node kind that cannot be folded inside a wrapper.",
        )
        .into()),
    }
}

/// Cheap byte estimate for an aggregate wrapper subtree.
fn estimate_aggregate_wrapper_bytes(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
    aggregate_output_len: usize,
    string_table: &crate::compiler_frontend::symbols::string_interning::StringTable,
) -> Result<usize, TemplateError> {
    let node = store
        .get_node(node_id)
        .cloned()
        .ok_or_else(|| missing_node_diagnostic(node_id))?;

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => children
            .iter()
            .map(|child| {
                estimate_aggregate_wrapper_bytes(store, *child, aggregate_output_len, string_table)
            })
            .sum::<Result<usize, TemplateError>>(),

        TemplateIrNodeKind::Text { text, .. } => Ok(string_table.resolve(*text).len()),

        TemplateIrNodeKind::AggregateOutput => Ok(aggregate_output_len),

        // Child templates and dynamic expressions contribute an unknown amount
        // of output at this stage; estimating them would require recursive
        // folding. Leave them as zero and let the estimate-miss counter record
        // the difference.
        TemplateIrNodeKind::ChildTemplate { .. } | TemplateIrNodeKind::DynamicExpression { .. } => {
            Ok(0)
        }

        _ => Err(CompilerError::compiler_error(
            "TIR fold: malformed aggregate wrapper subtree contains a node kind that cannot be estimated inside a wrapper.",
        )
        .into()),
    }
}

// -------------------------
//  Output helpers
// -------------------------

/// Builds a `TemplateEmission` from a filled output buffer.
fn build_emission_from_buffer(
    buffer: String,
    estimated_bytes: usize,
    signal: Option<TemplateLoopControlKind>,
    emitted_output: bool,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<TemplateEmission, TemplateError> {
    if signal.is_some() && !emitted_output {
        return Ok(match signal {
            Some(TemplateLoopControlKind::Break) => TemplateEmission::Break(None),
            Some(TemplateLoopControlKind::Continue) => TemplateEmission::Continue(None),
            None => unreachable!(),
        });
    }

    if !emitted_output {
        return Ok(TemplateEmission::NoOutput);
    }

    let actual_len = buffer.len();
    record_tir_fold_output_estimate_miss(actual_len, estimated_bytes);
    let output_id = fold_context.string_table.intern(&buffer);
    record_tir_fold_output_intern(actual_len);

    Ok(match signal {
        None => TemplateEmission::Output(output_id),
        Some(TemplateLoopControlKind::Break) => TemplateEmission::Break(Some(output_id)),
        Some(TemplateLoopControlKind::Continue) => TemplateEmission::Continue(Some(output_id)),
    })
}

/// Cheap estimate of how many bytes a TIR node will contribute if folded.
///
/// WHAT: sums text bytes for the current node and its direct sequence children.
/// WHY: gives loop bodies a cheap capacity hint without traversing the whole
/// tree or recursively folding nested templates.
fn estimate_tir_node_output_bytes(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
    string_table: &crate::compiler_frontend::symbols::string_interning::StringTable,
) -> Result<usize, TemplateError> {
    let node = store
        .get_node(node_id)
        .cloned()
        .ok_or_else(|| missing_node_diagnostic(node_id))?;

    match &node.kind {
        TemplateIrNodeKind::Text { text, .. } => Ok(string_table.resolve(*text).len()),
        TemplateIrNodeKind::Sequence { children } => children
            .iter()
            .map(|child| estimate_tir_node_output_bytes(store, *child, string_table))
            .sum(),
        _ => Ok(0),
    }
}

// -------------------------
//  Internal diagnostics
// -------------------------

fn missing_template_diagnostic(template_id: TemplateIrId) -> CompilerError {
    CompilerError::compiler_error(format!(
        "TIR fold referenced template {} that is not present in the store.",
        template_id
    ))
}

fn missing_node_diagnostic(node_id: TemplateIrNodeId) -> CompilerError {
    CompilerError::compiler_error(format!(
        "TIR fold referenced node {} that is not present in the store.",
        node_id
    ))
}

fn missing_wrapper_set_diagnostic(wrapper_set_id: TemplateWrapperSetId) -> CompilerError {
    CompilerError::compiler_error(format!(
        "TIR fold referenced wrapper set {} that is not present in the store.",
        wrapper_set_id
    ))
}
