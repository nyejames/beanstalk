//! TIR-native compile-time template folding.
//!
//! WHAT: folds a `TemplateIr` tree directly into an interned string emission
//! without rebuilding `TemplateContent` or `TemplateRenderPlan`.
//!
//! WHY: removes the current representation ping-pong between content, render
//! plans, and rebuilt content for the folding stage while preserving the same
//! user-visible output semantics.
//!
//! ## Temporary bridges
//!
//! - Loop aggregate wrappers still use the AST `TemplateAggregateRenderPlan`
//!   carried on `TemplateIrNodeKind::Loop::aggregate_render_plan`. This field
//!   is deleted once Phase B4 introduces TIR-native render units.
//! - `RenderPiece` folding is reused for aggregate-wrapper pieces only.
//!
//! ## Deletion checkpoint
//!
//! When TIR-native formatter (B3) and render-unit (B4) phases land, the
//! `aggregate_render_plan` field and the `fold_render_piece` reuse should be
//! replaced by pure TIR nodes.

use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template_control_flow::{
    ConstRangeCursor, TemplateAggregatePiece, TemplateAggregateRenderPlan, TemplateBranchSelector,
    TemplateFoldBinding, TemplateLoopControlKind, TemplateLoopHeader,
    build_collection_iteration_bindings, build_range_iteration_bindings, const_collection_items,
};
use crate::compiler_frontend::ast::templates::template_folding::{
    TemplateEmission, TemplateFoldContext, apply_conditional_child_wrapper_templates,
    condition_location_or_loop_location, fold_bool_condition,
    fold_conditional_loop_const_condition, loop_body_not_const_error,
    resolve_fold_bindings_in_expression, selected_option_capture_payload,
};
use crate::compiler_frontend::ast::templates::tir::ids::{TemplateIrId, TemplateIrNodeId};
use crate::compiler_frontend::ast::templates::tir::node::{TemplateIrBranch, TemplateIrNodeKind};
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidTemplateStructureReason,
};
use crate::compiler_frontend::instrumentation::{AstCounter, add_ast_counter};
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::type_coercion::string::{
    FoldedStringPiece, fold_expression_kind_to_string,
};

// Reuse the old render-piece folder for the temporary aggregate-wrapper bridge.
// This is acceptable because aggregate plans are still AST-shaped until Phase B4.
use crate::compiler_frontend::ast::templates::template_folding::fold_render_piece;

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

/// Cheap estimate for an aggregate render plan that wraps a known aggregate
/// output plus already-resolved text pieces.
fn estimate_aggregate_render_plan_bytes(
    aggregate_plan: &TemplateAggregateRenderPlan,
    aggregate_output_len: usize,
    string_table: &crate::compiler_frontend::symbols::string_interning::StringTable,
) -> usize {
    aggregate_plan
        .pieces
        .iter()
        .map(|piece| match piece {
            TemplateAggregatePiece::Aggregate => aggregate_output_len,
            TemplateAggregatePiece::Render(render_piece) => {
                render_piece.estimate_output_bytes(string_table)
            }
        })
        .sum()
}

/// Records that a folded output string was interned.
fn record_tir_fold_output_intern(byte_len: usize) {
    add_ast_counter(AstCounter::TirFoldStringInternCalls, 1);
    add_ast_counter(AstCounter::TirFoldOutputBytes, byte_len);
    add_ast_counter(AstCounter::TemplateFoldStringInternCalls, 1);
    add_ast_counter(AstCounter::TemplateFoldOutputBytes, byte_len);
}

// -------------------------
//  Public entry point
// -------------------------

/// Folds a TIR template into an emission result.
///
/// WHAT: recursively walks the template's TIR node tree and produces the same
/// `TemplateEmission` shape that the old render-plan fold path produced.
/// WHY: this is the production folding entry point for templates that have
/// been converted to TIR.
///
/// Conditional child wrappers are applied here from the `TemplateIr` entry so
/// nested TIR child templates preserve the same maybe-empty wrapper semantics
/// as the legacy `Template` fold path.
pub(crate) fn fold_tir_template(
    store: &TemplateIrStore,
    template_id: TemplateIrId,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<TemplateEmission, TemplateError> {
    add_ast_counter(AstCounter::TirFoldTemplatesFolded, 1);

    let template = store
        .get_template(template_id)
        .ok_or_else(|| missing_template_diagnostic(template_id))?;

    let estimated_bytes = template.summary.estimated_output_bytes;
    let mut output_buffer = reserve_tir_fold_output_buffer(estimated_bytes);
    let mut emitted_output = false;

    let signal = fold_tir_node_into_buffer(
        store,
        template.root,
        &mut output_buffer,
        &mut emitted_output,
        fold_context,
    )?;

    let emission = build_emission_from_buffer(
        output_buffer,
        estimated_bytes,
        signal,
        emitted_output,
        fold_context,
    )?;

    apply_conditional_child_wrapper_templates(
        &template.location,
        &template.conditional_child_wrappers,
        emission,
        fold_context,
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
) -> Result<TemplateEmission, TemplateError> {
    let mut buffer = String::new();
    let mut emitted_output = false;

    let signal = fold_tir_node_into_buffer(
        store,
        node_id,
        &mut buffer,
        &mut emitted_output,
        fold_context,
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
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    add_ast_counter(AstCounter::TirFoldNodesVisited, 1);

    let node = store
        .get_node(node_id)
        .ok_or_else(|| missing_node_diagnostic(node_id))?;

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            fold_tir_sequence(store, children, output_buffer, emitted_output, fold_context)
        }

        TemplateIrNodeKind::Text { text, .. } => {
            output_buffer.push_str(fold_context.string_table.resolve(*text));
            *emitted_output = true;
            Ok(None)
        }

        TemplateIrNodeKind::DynamicExpression { expression, .. } => {
            fold_tir_dynamic_expression(expression, output_buffer, emitted_output, fold_context)
        }

        TemplateIrNodeKind::ChildTemplate { template } => fold_tir_child_template(
            store,
            *template,
            output_buffer,
            emitted_output,
            fold_context,
        ),

        TemplateIrNodeKind::Slot { .. } => {
            // Unfilled slots intentionally fold to no output.
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
        ),

        TemplateIrNodeKind::Loop {
            header,
            body,
            aggregate_wrapper,
            aggregate_render_plan,
        } => fold_tir_loop(
            store,
            header,
            *body,
            *aggregate_wrapper,
            aggregate_render_plan.as_ref(),
            output_buffer,
            emitted_output,
            fold_context,
            &node.location,
        ),

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
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    for &child_id in children {
        let signal = fold_tir_node_into_buffer(
            store,
            child_id,
            output_buffer,
            emitted_output,
            fold_context,
        )?;

        if signal.is_some() {
            return Ok(signal);
        }
    }

    Ok(None)
}

/// Folds a dynamic expression node after resolving fold bindings.
fn fold_tir_dynamic_expression(
    expression: &Expression,
    output_buffer: &mut String,
    emitted_output: &mut bool,
    fold_context: &mut TemplateFoldContext<'_>,
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

            if matches!(template.kind, crate::compiler_frontend::ast::templates::template::TemplateType::SlotInsert(_))
                || template.contains_slot_insertions()
            {
                return Err(CompilerError::compiler_error(
                    "Invalid template content reached string folding: unresolved slot insertions cannot be rendered directly.",
                )
                .into());
            }

            match template.fold_to_emission(fold_context)? {
                TemplateEmission::NoOutput => Ok(None),
                TemplateEmission::Output(folded_nested) => {
                    output_buffer.push_str(fold_context.string_table.resolve(folded_nested));
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

        None => Err(CompilerError::compiler_error(
            "Invalid Expression Used Inside template when trying to fold into a string. The compiler_frontend should not be trying to fold this template.",
        )
        .into()),
    }
}

/// Folds a TIR child-template reference.
fn fold_tir_child_template(
    store: &TemplateIrStore,
    template_id: TemplateIrId,
    output_buffer: &mut String,
    emitted_output: &mut bool,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    match fold_tir_template(store, template_id, fold_context)? {
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
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    for branch in branches {
        let selected = match &branch.selector {
            TemplateBranchSelector::Bool(condition) => {
                fold_bool_condition(condition, &branch.location, fold_context)?
            }

            TemplateBranchSelector::OptionPresentCapture { scrutinee, pattern } => {
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
            );
        }
    }

    fold_tir_fallback_branch(store, fallback, output_buffer, emitted_output, fold_context)
}

/// Folds a selected branch body after pushing option-capture bindings.
fn fold_tir_branch_with_bindings<const N: usize>(
    store: &TemplateIrStore,
    branch: &TemplateIrBranch,
    bindings: [TemplateFoldBinding; N],
    output_buffer: &mut String,
    emitted_output: &mut bool,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    let previous_bindings_len = fold_context.push_bindings(bindings);
    let result = fold_tir_branch_body(
        store,
        branch.body,
        output_buffer,
        emitted_output,
        fold_context,
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
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    fold_tir_node_into_buffer(store, body_id, output_buffer, emitted_output, fold_context)
}

/// Folds the fallback branch, if any.
fn fold_tir_fallback_branch(
    store: &TemplateIrStore,
    fallback: Option<TemplateIrNodeId>,
    output_buffer: &mut String,
    emitted_output: &mut bool,
    fold_context: &mut TemplateFoldContext<'_>,
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
    )
}

// -------------------------
//  Loop folding
// -------------------------

/// Folds a TIR loop node, including its aggregate wrapper.
///
/// This helper intentionally mirrors the legacy `fold_template_loop` signature:
/// each parameter represents a distinct responsibility (store, header, body,
/// aggregate plan, output sink, fold context, source location). Grouping them
/// would not improve readability, so the argument count is allowed.
#[allow(clippy::too_many_arguments)]
fn fold_tir_loop(
    store: &TemplateIrStore,
    header: &TemplateLoopHeader,
    body_id: TemplateIrNodeId,
    _aggregate_wrapper: Option<TemplateIrNodeId>,
    aggregate_render_plan: Option<&TemplateAggregateRenderPlan>,
    output_buffer: &mut String,
    emitted_output: &mut bool,
    fold_context: &mut TemplateFoldContext<'_>,
    loop_location: &SourceLocation,
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    // The body estimate seeds the aggregate buffer reservation.
    let body_estimate = estimate_tir_node_output_bytes(store, body_id, fold_context.string_table);

    let (aggregate, estimated_aggregate, did_emit_body) = match header {
        TemplateLoopHeader::Conditional { condition } => {
            let condition_value = fold_conditional_loop_const_condition(condition, loop_location)?;
            if !condition_value {
                return Ok(None);
            }

            return Err(CompilerDiagnostic::invalid_template_structure(
                InvalidTemplateStructureReason::TemplateConditionalLoopConstTrue,
                condition_location_or_loop_location(condition, loop_location),
            )
            .into());
        }

        TemplateLoopHeader::Range { bindings, range } => {
            let estimated_iterations = std::cmp::min(
                fold_context.template_const_loop_iteration_limit,
                FOLD_RANGE_LOOP_RESERVE_ITERATION_CAP,
            );
            let estimated_aggregate =
                estimate_loop_aggregate_bytes(body_estimate, estimated_iterations);
            let mut aggregate = reserve_tir_fold_output_buffer(estimated_aggregate);
            let mut did_emit = false;

            let mut cursor = ConstRangeCursor::new(
                range,
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
            let items = const_collection_items(iterable)?;
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

    let Some(aggregate_plan) = aggregate_render_plan else {
        // No wrapper plan: the aggregate output is the loop's output.
        output_buffer.push_str(fold_context.string_table.resolve(aggregate_id));
        *emitted_output = true;
        return Ok(None);
    };

    fold_tir_aggregate_render_plan(
        aggregate_plan,
        aggregate_id,
        output_buffer,
        emitted_output,
        fold_context,
    )
}

/// Folds one loop-body iteration into the aggregate buffer.
fn fold_tir_loop_iteration(
    store: &TemplateIrStore,
    body_id: TemplateIrNodeId,
    iteration_bindings: Vec<TemplateFoldBinding>,
    fold_context: &mut TemplateFoldContext<'_>,
    loop_location: &SourceLocation,
    aggregate: &mut String,
) -> Result<(bool, Option<TemplateLoopControlKind>), TemplateError> {
    let previous_bindings_len = fold_context.push_bindings(iteration_bindings);
    let folded_result = fold_tir_node(store, body_id, fold_context);
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

/// Folds an aggregate render plan around a loop aggregate output.
fn fold_tir_aggregate_render_plan(
    aggregate_plan: &TemplateAggregateRenderPlan,
    aggregate_output: StringId,
    output_buffer: &mut String,
    emitted_output: &mut bool,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    let aggregate_output_len = fold_context.string_table.resolve(aggregate_output).len();
    let estimated_bytes = estimate_aggregate_render_plan_bytes(
        aggregate_plan,
        aggregate_output_len,
        fold_context.string_table,
    );
    let mut wrapper_buffer = reserve_tir_fold_output_buffer(estimated_bytes);
    let mut wrapper_emitted_output = false;

    for piece in &aggregate_plan.pieces {
        match piece {
            TemplateAggregatePiece::Aggregate => {
                wrapper_buffer.push_str(fold_context.string_table.resolve(aggregate_output));
                wrapper_emitted_output = true;
            }
            TemplateAggregatePiece::Render(render_piece) => {
                if fold_render_piece(
                    render_piece,
                    &mut wrapper_buffer,
                    &mut wrapper_emitted_output,
                    fold_context,
                )?
                .is_some()
                {
                    return Err(CompilerError::compiler_error(
                        "Loop-control signal reached aggregate render plan folding; aggregate wrapper plans should not contain loop control.",
                    )
                    .into());
                }
            }
        }
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
) -> usize {
    let Some(node) = store.get_node(node_id) else {
        return 0;
    };

    match &node.kind {
        TemplateIrNodeKind::Text { text, .. } => string_table.resolve(*text).len(),
        TemplateIrNodeKind::Sequence { children } => children
            .iter()
            .map(|child| estimate_tir_node_output_bytes(store, *child, string_table))
            .sum(),
        _ => 0,
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
