//! Compile-time template folding.
//!
//! WHAT: Converts fully-resolved template render plans and const control-flow
//! bodies into interned string IDs.
//!
//! WHY: Keeps compile-time folding on the same AST-prepared render-plan shapes
//! that runtime lowering consumes, without entangling parser or HIR code.

use std::borrow::Cow;

use crate::ast_log;
use crate::compiler_frontend::ast::const_eval::constant_fold;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::expression_rpn::{
    ExpressionRpn, ExpressionRpnItem,
};
use crate::compiler_frontend::ast::statements::match_patterns::MatchPattern;
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::{
    TemplateAtom, TemplateContent, TemplateSegment, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_composition::wrap_direct_child_atom;
use crate::compiler_frontend::ast::templates::template_control_flow::{
    ConstRangeCursor, TemplateAggregatePiece, TemplateAggregateRenderPlan, TemplateBranchChain,
    TemplateBranchSelector, TemplateConditionalBranch, TemplateControlFlow, TemplateFoldBinding,
    TemplateLoopControlFlow, TemplateLoopControlKind, TemplateLoopHeader,
    build_collection_iteration_bindings, build_range_iteration_bindings, const_collection_items,
};
use crate::compiler_frontend::ast::templates::template_formatting::apply_body_formatter;
use crate::compiler_frontend::ast::templates::template_render_plan::{
    RenderPiece, TemplateRenderPlan,
};
use crate::compiler_frontend::ast::templates::template_slots::SlotResolutionMode;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::tir::{
    TemplateIrStore, convert_template_to_tir, fold_tir_template,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DiagnosticSeverity, InvalidTemplateStructureReason,
};
use crate::compiler_frontend::instrumentation::{AstCounter, add_ast_counter};
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::type_coercion::string::{
    FoldedStringPiece, fold_expression_kind_to_string,
};
use crate::compiler_frontend::value_mode::ValueMode;

// -------------------------
//  Folding Context
// -------------------------

/// Required context for compile-time template folding.
///
/// WHAT: carries all project-aware state that folding can require.
/// WHY: folding must not rely on ad-hoc inherited-style placeholders or
///       resolver-less fallback branches.
pub struct TemplateFoldContext<'a> {
    pub string_table: &'a mut StringTable,
    pub(crate) project_path_resolver: &'a ProjectPathResolver,
    pub path_format_config: &'a PathStringFormatConfig,
    pub source_file_scope: &'a InternedPath,
    pub template_const_loop_iteration_limit: usize,
    pub(crate) bindings: Vec<TemplateFoldBinding>,
}

/// Compile-time template folding must keep structural no-output distinct from
/// output that happens to be an empty string, because parent wrappers apply only
/// to structurally emitted children.
pub(crate) enum TemplateEmission {
    NoOutput,
    Output(StringId),
    Break(Option<StringId>),
    Continue(Option<StringId>),
}

/// Borrow-first expression resolution result for template folding.
///
/// WHAT: distinguishes expressions that were not modified during fold-binding
///       resolution (borrowed reference to the original) from expressions that
///       were actually rewritten (owned).
/// WHY: most template expressions pass through folding unchanged because they
///      contain no foldable bindings. Returning a borrowed reference avoids
///      cloning the entire expression tree on the common no-substitution path,
///      which is the majority of expressions in template-heavy modules.
pub(crate) enum FoldResolvedExpression<'a> {
    /// The expression was not changed; fold sites can use the original.
    Borrowed(&'a Expression),
    /// The expression was actually rewritten; this is the owned result.
    Owned(Box<Expression>),
}

impl FoldResolvedExpression<'_> {
    /// Consumes the resolved expression and returns an owned `Expression`.
    ///
    /// WHAT: clones only when the resolved expression is borrowed (no substitution
    ///       happened), so callers that genuinely need an owned value still work.
    /// WHY: a few call sites (like RPN operand vectors) need owned values, but
    ///      this method makes the clone explicit and only happens when the
    ///      borrow-first path determined a rewrite is required.
    pub(crate) fn into_owned(self) -> Expression {
        match self {
            FoldResolvedExpression::Borrowed(expr) => expr.clone(),
            FoldResolvedExpression::Owned(expr) => *expr,
        }
    }
}

/// Maximum bytes to reserve for a single const-loop aggregate output buffer.
///
/// WHAT: caps the capacity hint for loop aggregates so adversarial or large
/// const loops cannot force enormous allocations.
/// WHY: estimates are cheap hints, not promises; correctness does not depend
/// on the buffer being large enough, only on avoiding repeated reallocations
/// for normal bounded loops.
const FOLD_LOOP_RESERVE_BYTE_CAP: usize = 64 * 1024;

/// Maximum iterations to use when estimating a streaming range loop.
///
/// Collection loops know their item count exactly after const evaluation, but
/// numeric ranges stream through `ConstRangeCursor` specifically to avoid an
/// eager counting pass. This cap keeps range-loop reservations useful without
/// turning the configured expansion limit into a large upfront allocation.
const FOLD_RANGE_LOOP_RESERVE_ITERATION_CAP: usize = 256;

/// Creates a fold output buffer with a cheap, safe capacity hint and records
/// how many bytes were reserved for profiling.
fn reserve_fold_output_buffer(estimated_bytes: usize) -> String {
    add_ast_counter(
        AstCounter::TemplateEstimatedFoldOutputBytes,
        estimated_bytes,
    );
    String::with_capacity(estimated_bytes)
}

/// Records how many bytes the actual folded output exceeded the estimate by.
///
/// WHAT: only positive misses are recorded. Over-estimates are ignored because
/// they do not indicate a bad allocation path.
fn record_fold_output_estimate_miss(actual_len: usize, estimated_bytes: usize) {
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
    string_table: &StringTable,
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

impl TemplateFoldContext<'_> {
    fn lookup_binding(&self, path: &InternedPath) -> Option<&Expression> {
        self.bindings
            .iter()
            .rev()
            .find(|binding| &binding.path == path)
            .map(|binding| &binding.value)
    }

    pub(crate) fn push_bindings(
        &mut self,
        bindings: impl IntoIterator<Item = TemplateFoldBinding>,
    ) -> usize {
        let previous_len = self.bindings.len();
        self.bindings.extend(bindings);
        previous_len
    }

    pub(crate) fn restore_bindings(&mut self, previous_len: usize) {
        self.bindings.truncate(previous_len);
    }
}

// -------------------------
//  Folding Implementation
// -------------------------

impl Template {
    /// Folds a fully-resolved template into an interned string ID.
    ///
    /// WHAT: routes the normal (non-formatting) fold path through the TIR-native
    /// folder. Templates that still need body formatting at fold time keep the
    /// old render-plan path until Phase B3 migrates the formatter view.
    /// WHY: Phase B2 proves TIR folding parity while leaving the formatter
    /// migration to the next phase.
    pub(crate) fn fold_into_stringid(
        &self,
        fold_context: &mut TemplateFoldContext<'_>,
    ) -> Result<StringId, TemplateError> {
        // Keep resolver/path/scope in the fold contract even when a specific template
        // only needs string interning today. Callers must propagate full project context.
        let _required_project_context = (
            fold_context.project_path_resolver,
            fold_context.path_format_config,
            fold_context.source_file_scope,
        );

        match self.fold_to_emission(fold_context)? {
            TemplateEmission::NoOutput => {
                let empty_id = fold_context.string_table.intern("");
                record_fold_output_intern(0);
                Ok(empty_id)
            }
            TemplateEmission::Output(output) => Ok(output),
            TemplateEmission::Break(_) | TemplateEmission::Continue(_) => Err(
                CompilerError::compiler_error(
                    "Template loop-control signal escaped the nearest template loop during folding.",
                )
                .into(),
            ),
        }
    }

    pub(crate) fn fold_to_emission(
        &self,
        fold_context: &mut TemplateFoldContext<'_>,
    ) -> Result<TemplateEmission, TemplateError> {
        // Phase B3 will remove this branch by folding formatters directly from TIR.
        // Until then, templates that still require body formatting at fold time use
        // the existing render-plan fold path to preserve formatter behavior.
        if self.content_needs_formatting {
            return self.fold_to_emission_via_render_plan(fold_context);
        }

        let mut tir_store = TemplateIrStore::new();
        let tir_id = convert_template_to_tir(self, &mut tir_store, fold_context.string_table);
        fold_tir_template(&tir_store, tir_id, fold_context)
    }

    /// Legacy render-plan fold path used only when body formatting is still
    /// pending at fold time.
    ///
    /// WHAT: builds or reuses the render plan and folds it, then applies any
    /// conditional child wrappers.
    /// WHY: kept as a temporary bridge until Phase B3 provides a TIR formatter
    /// view.
    fn fold_to_emission_via_render_plan(
        &self,
        fold_context: &mut TemplateFoldContext<'_>,
    ) -> Result<TemplateEmission, TemplateError> {
        let Some(control_flow) = &self.control_flow else {
            let plan = if self.content_needs_formatting {
                apply_body_formatter(
                    &self.unformatted_content,
                    &self.style,
                    fold_context.string_table,
                )
                .map(|result| Cow::Owned(result.plan))
                .map_err(|messages| {
                    messages
                        .into_diagnostics()
                        .into_iter()
                        .find(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
                        .map(TemplateError::from)
                        .unwrap_or_else(|| {
                            CompilerError::compiler_error(
                                "Template formatter failed without returning a compiler error.",
                            )
                            .into()
                        })
                })?
            } else {
                render_plan_for_folding(self.render_plan.as_ref(), &self.content)
            };

            let output = fold_plan(plan.as_ref(), fold_context)?;
            return apply_conditional_child_wrappers(
                self,
                TemplateEmission::Output(output),
                fold_context,
            );
        };

        let emission = fold_control_flow(control_flow, fold_context)?;
        apply_conditional_child_wrappers(self, emission, fold_context)
    }
}

/// Folds AST template control flow through the legacy render-plan path.
///
/// # Temporary test access
///
/// Exposed as `pub(crate)` so Phase B2 TIR parity tests can compare the old
/// control-flow fold output against the new TIR fold output. This function is
/// deleted once the TIR fold route fully replaces the render-plan path.
pub(crate) fn fold_control_flow(
    control_flow: &TemplateControlFlow,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<TemplateEmission, TemplateError> {
    match control_flow {
        TemplateControlFlow::BranchChain(branch_chain) => {
            fold_template_branch_chain(branch_chain, fold_context)
        }

        TemplateControlFlow::Loop(template_loop) => fold_template_loop(template_loop, fold_context),

        TemplateControlFlow::LoopControl(signal) => match signal.kind {
            TemplateLoopControlKind::Break => Ok(TemplateEmission::Break(None)),
            TemplateLoopControlKind::Continue => Ok(TemplateEmission::Continue(None)),
        },
    }
}

fn fold_template_branch_chain(
    branch_chain: &TemplateBranchChain,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<TemplateEmission, TemplateError> {
    fold_if_branch_chain(branch_chain, fold_context)
}

fn fold_if_branch_chain(
    branch_chain: &TemplateBranchChain,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<TemplateEmission, TemplateError> {
    for branch in &branch_chain.branches {
        let selected = match &branch.selector {
            TemplateBranchSelector::Bool(condition) => {
                fold_bool_condition(condition, &branch.location, fold_context)?
            }

            TemplateBranchSelector::OptionPresentCapture { scrutinee, pattern } => {
                if let Some(payload) =
                    selected_option_capture_payload(scrutinee, pattern, fold_context)?
                {
                    return fold_selected_branch_with_bindings(branch, [payload], fold_context);
                }

                false
            }
        };

        if selected {
            return fold_conditional_branch(branch, fold_context);
        }
    }

    fold_fallback_branch(branch_chain, fold_context)
}

fn fold_conditional_branch(
    branch: &TemplateConditionalBranch,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<TemplateEmission, TemplateError> {
    let branch_plan = render_plan_for_folding(branch.render_plan.as_ref(), &branch.content);

    fold_plan_to_emission(branch_plan.as_ref(), fold_context)
}

fn fold_selected_branch_with_bindings<const N: usize>(
    branch: &TemplateConditionalBranch,
    bindings: [TemplateFoldBinding; N],
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<TemplateEmission, TemplateError> {
    let previous_bindings_len = fold_context.push_bindings(bindings);
    let folded_branch = fold_conditional_branch(branch, fold_context);
    fold_context.restore_bindings(previous_bindings_len);

    folded_branch
}

pub(crate) fn selected_option_capture_payload(
    scrutinee: &Expression,
    pattern: &MatchPattern,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<Option<TemplateFoldBinding>, TemplateError> {
    match const_option_presence(scrutinee, fold_context)? {
        ConstOptionPresence::Present(value) => Ok(Some(TemplateFoldBinding {
            path: option_capture_binding_path(pattern)?,
            value: *value,
        })),

        ConstOptionPresence::Absent => Ok(None),
    }
}

fn fold_fallback_branch(
    branch_chain: &TemplateBranchChain,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<TemplateEmission, TemplateError> {
    let Some(fallback) = &branch_chain.fallback else {
        return Ok(TemplateEmission::NoOutput);
    };

    let fallback_plan = render_plan_for_folding(fallback.render_plan.as_ref(), &fallback.content);
    fold_plan_to_emission(fallback_plan.as_ref(), fold_context)
}

enum ConstOptionPresence {
    Present(Box<Expression>),
    Absent,
}

fn const_option_presence(
    scrutinee: &Expression,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<ConstOptionPresence, TemplateError> {
    let resolved = resolve_fold_bindings_in_expression(scrutinee, fold_context)?;

    // Work with the resolved expression by reference to avoid an extra clone
    // when the resolver returned a borrowed reference (no binding was substituted).
    let resolved_ref: &Expression = match &resolved {
        FoldResolvedExpression::Borrowed(expr) => expr,
        FoldResolvedExpression::Owned(expr) => expr,
    };

    match &resolved_ref.kind {
        ExpressionKind::OptionNone => Ok(ConstOptionPresence::Absent),

        ExpressionKind::Coerced { value, .. } => {
            let payload = (**value).clone();
            if payload.is_compile_time_constant() {
                Ok(ConstOptionPresence::Present(Box::new(payload)))
            } else {
                Err(option_capture_const_deferred_error(resolved_ref).into())
            }
        }

        _ => Err(option_capture_const_deferred_error(resolved_ref).into()),
    }
}

fn option_capture_binding_path(pattern: &MatchPattern) -> Result<InternedPath, TemplateError> {
    let MatchPattern::OptionPresentCapture { binding_path, .. } = pattern else {
        return Err(CompilerError::compiler_error(
            "Template option-capture folding received a non-capture pattern.",
        )
        .into());
    };

    Ok(binding_path.clone())
}

fn option_capture_const_deferred_error(expression: &Expression) -> CompilerDiagnostic {
    CompilerDiagnostic::invalid_template_structure(
        InvalidTemplateStructureReason::TemplateOptionCaptureConstDeferred,
        expression.location.clone(),
    )
}

fn fold_template_loop(
    template_loop: &TemplateLoopControlFlow,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<TemplateEmission, TemplateError> {
    let body_plan = render_plan_for_folding(
        template_loop.body_render_plan.as_ref(),
        &template_loop.body_content,
    );
    let body_estimate = body_plan.estimate_output_bytes(fold_context.string_table);

    // The match returns the filled aggregate, its estimate, and whether any
    // output was emitted. The conditional header always returns early.
    let (aggregate, estimated_aggregate, emitted_output) = match &template_loop.header {
        TemplateLoopHeader::Conditional { condition } => {
            let condition_value =
                fold_conditional_loop_const_condition(condition, &template_loop.location)?;
            if !condition_value {
                return Ok(TemplateEmission::NoOutput);
            }

            return Err(CompilerDiagnostic::invalid_template_structure(
                InvalidTemplateStructureReason::TemplateConditionalLoopConstTrue,
                condition_location_or_loop_location(condition, &template_loop.location),
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
            let mut aggregate = reserve_fold_output_buffer(estimated_aggregate);
            let mut emitted_output = false;

            let mut cursor = ConstRangeCursor::new(
                range,
                fold_context.template_const_loop_iteration_limit,
                template_loop.location.clone(),
            )?;

            while let Some(counter) = cursor.next_counter()? {
                add_ast_counter(AstCounter::TemplateFoldLoopIterations, 1);
                let iteration_bindings =
                    build_range_iteration_bindings(bindings, counter, cursor.iteration_count() - 1);
                let (did_emit, signal) = fold_template_loop_iteration(
                    body_plan.as_ref(),
                    iteration_bindings,
                    fold_context,
                    &template_loop.location,
                    &mut aggregate,
                )?;

                emitted_output |= did_emit;

                match signal {
                    Some(TemplateLoopControlKind::Break) => break,
                    Some(TemplateLoopControlKind::Continue) => continue,
                    None => {}
                }
            }

            (aggregate, estimated_aggregate, emitted_output)
        }

        TemplateLoopHeader::Collection { bindings, iterable } => {
            let items = const_collection_items(iterable)?;
            let estimated_iterations = std::cmp::min(
                items.len(),
                fold_context.template_const_loop_iteration_limit,
            );
            let estimated_aggregate =
                estimate_loop_aggregate_bytes(body_estimate, estimated_iterations);
            let mut aggregate = reserve_fold_output_buffer(estimated_aggregate);
            let mut emitted_output = false;

            for (index, item) in items.iter().enumerate() {
                add_ast_counter(AstCounter::TemplateFoldLoopIterations, 1);
                if index >= fold_context.template_const_loop_iteration_limit {
                    return Err(CompilerDiagnostic::invalid_template_structure(
                        InvalidTemplateStructureReason::TemplateConstLoopExpansionLimitExceeded {
                            limit: fold_context.template_const_loop_iteration_limit,
                        },
                        template_loop.location.clone(),
                    )
                    .into());
                }

                let iteration_bindings = build_collection_iteration_bindings(bindings, item, index);
                let (did_emit, signal) = fold_template_loop_iteration(
                    body_plan.as_ref(),
                    iteration_bindings,
                    fold_context,
                    &template_loop.location,
                    &mut aggregate,
                )?;

                emitted_output |= did_emit;

                match signal {
                    Some(TemplateLoopControlKind::Break) => break,
                    Some(TemplateLoopControlKind::Continue) => continue,
                    None => {}
                }
            }

            (aggregate, estimated_aggregate, emitted_output)
        }
    };

    if !emitted_output {
        return Ok(TemplateEmission::NoOutput);
    }

    let actual_len = aggregate.len();
    record_fold_output_estimate_miss(actual_len, estimated_aggregate);
    let aggregate_id = fold_context.string_table.intern(&aggregate);
    record_fold_output_intern(actual_len);
    let aggregate_plan = template_loop
        .aggregate_render_plan
        .as_ref()
        .ok_or_else(|| {
            CompilerError::compiler_error(
                "Const loop folding missing aggregate render plan; prepare_control_flow_render_units should have populated it.",
            )
        })?;
    fold_aggregate_render_plan(aggregate_plan, aggregate_id, fold_context)
}

pub(crate) fn fold_conditional_loop_const_condition(
    condition: &Expression,
    location: &SourceLocation,
) -> Result<bool, TemplateError> {
    match &condition.kind {
        ExpressionKind::Bool(value) => Ok(*value),

        ExpressionKind::Coerced { value, .. } => {
            fold_conditional_loop_const_condition(value, location)
        }

        _ => Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::TemplateLoopConditionNotConst,
            condition_location_or_loop_location(condition, location),
        )
        .into()),
    }
}

pub(crate) fn condition_location_or_loop_location(
    condition: &Expression,
    loop_location: &SourceLocation,
) -> SourceLocation {
    if condition.location == Default::default() {
        loop_location.clone()
    } else {
        condition.location.clone()
    }
}

/// Single const loop body fold that appends output to the aggregate and
/// reports whether any output was emitted and whether a break/continue
/// signal terminated the iteration.
fn fold_template_loop_iteration(
    body_plan: &TemplateRenderPlan,
    iteration_bindings: Vec<TemplateFoldBinding>,
    fold_context: &mut TemplateFoldContext<'_>,
    loop_location: &SourceLocation,
    aggregate: &mut String,
) -> Result<(bool, Option<TemplateLoopControlKind>), TemplateError> {
    let previous_bindings_len = fold_context.push_bindings(iteration_bindings);
    let folded_result = fold_plan_to_emission(body_plan, fold_context);
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

pub(crate) fn loop_body_not_const_error(
    error: TemplateError,
    diagnostic_location: &SourceLocation,
) -> TemplateError {
    match error {
        TemplateError::Diagnostic(diagnostic) => TemplateError::Diagnostic(diagnostic),
        TemplateError::Infrastructure(_) => CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::TemplateLoopBodyNotConst,
            diagnostic_location.clone(),
        )
        .into(),
    }
}

fn fold_aggregate_render_plan(
    aggregate_plan: &TemplateAggregateRenderPlan,
    aggregate_output: StringId,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<TemplateEmission, TemplateError> {
    let aggregate_output_len = fold_context.string_table.resolve(aggregate_output).len();
    let estimated_bytes = estimate_aggregate_render_plan_bytes(
        aggregate_plan,
        aggregate_output_len,
        fold_context.string_table,
    );
    let mut output_buffer = reserve_fold_output_buffer(estimated_bytes);
    let mut emitted_output = false;

    for piece in &aggregate_plan.pieces {
        match piece {
            TemplateAggregatePiece::Aggregate => {
                output_buffer.push_str(fold_context.string_table.resolve(aggregate_output));
                emitted_output = true;
            }
            TemplateAggregatePiece::Render(render_piece) => {
                if fold_render_piece(
                    render_piece,
                    &mut output_buffer,
                    &mut emitted_output,
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

    if !emitted_output {
        return Ok(TemplateEmission::NoOutput);
    }

    let actual_len = output_buffer.len();
    record_fold_output_estimate_miss(actual_len, estimated_bytes);
    let aggregate_output_id = fold_context.string_table.intern(&output_buffer);
    record_fold_output_intern(actual_len);
    Ok(TemplateEmission::Output(aggregate_output_id))
}

pub(crate) fn fold_bool_condition(
    condition: &Expression,
    fallback_location: &SourceLocation,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<bool, TemplateError> {
    let resolved = resolve_fold_bindings_in_expression(condition, fold_context)?;

    // Borrow the resolved expression by reference to avoid cloning when no
    // binding was substituted (the common path for const template conditions).
    let resolved_ref: &Expression = match &resolved {
        FoldResolvedExpression::Borrowed(expr) => expr,
        FoldResolvedExpression::Owned(expr) => expr,
    };

    fold_resolved_bool_condition(resolved_ref, fallback_location)
}

fn fold_resolved_bool_condition(
    condition: &Expression,
    fallback_location: &SourceLocation,
) -> Result<bool, TemplateError> {
    match &condition.kind {
        ExpressionKind::Bool(value) => Ok(*value),
        ExpressionKind::Coerced { value, .. } => {
            fold_resolved_bool_condition(value, fallback_location)
        }
        _ => Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::TemplateIfConditionNotConst,
            if condition.location == Default::default() {
                fallback_location.clone()
            } else {
                condition.location.clone()
            },
        )
        .into()),
    }
}

/// Applies parent `$children(..)` wrappers around control-flow output.
///
/// # Temporary test access
///
/// Exposed as `pub(crate)` so Phase B2 TIR parity tests can compare the old
/// wrapper application against the new TIR-routed path. This function is
/// deleted once TIR owns wrapper sets natively.
pub(crate) fn apply_conditional_child_wrappers(
    template: &Template,
    emission: TemplateEmission,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<TemplateEmission, TemplateError> {
    apply_conditional_child_wrapper_templates(
        &template.location,
        &template.conditional_child_wrappers,
        emission,
        fold_context,
    )
}

/// Applies parent `$children(..)` wrappers to an already-folded control-flow
/// emission.
///
/// WHAT: takes the source location and wrapper templates directly so both the
/// legacy `Template` path and the TIR fold path can use the same wrapper
/// semantics.
/// WHY: TIR stores conditional child wrappers on `TemplateIr`; sharing this
/// helper prevents the migration from duplicating wrapper composition logic.
pub(crate) fn apply_conditional_child_wrapper_templates(
    template_location: &SourceLocation,
    conditional_child_wrappers: &[Template],
    emission: TemplateEmission,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<TemplateEmission, TemplateError> {
    let (output, signal_kind) = match emission {
        TemplateEmission::NoOutput => return Ok(TemplateEmission::NoOutput),
        TemplateEmission::Output(output) => (output, None),
        TemplateEmission::Break(Some(output)) => (output, Some(TemplateLoopControlKind::Break)),
        TemplateEmission::Continue(Some(output)) => {
            (output, Some(TemplateLoopControlKind::Continue))
        }
        TemplateEmission::Break(None) => return Ok(TemplateEmission::Break(None)),
        TemplateEmission::Continue(None) => return Ok(TemplateEmission::Continue(None)),
    };

    if conditional_child_wrappers.is_empty() {
        return Ok(template_emission_from_output_and_signal(
            output,
            signal_kind,
        ));
    }
    add_ast_counter(
        AstCounter::TemplateWrapperApplications,
        conditional_child_wrappers.len(),
    );

    let output_expression =
        crate::compiler_frontend::ast::expressions::expression::Expression::string_slice(
            output,
            template_location.clone(),
            ValueMode::ImmutableOwned,
        );
    let mut wrapped_atom = TemplateAtom::Content(TemplateSegment::new(
        output_expression,
        TemplateSegmentOrigin::Body,
    ));

    for wrapper in conditional_child_wrappers.iter().rev() {
        wrapped_atom = wrap_direct_child_atom(
            &wrapped_atom,
            std::slice::from_ref(wrapper),
            fold_context.string_table,
            SlotResolutionMode::ComposeOnly,
        )
        .map_err(TemplateError::from)?;
    }

    let wrapped_plan = TemplateRenderPlan::from_content(&TemplateContent {
        atoms: vec![wrapped_atom],
    });
    let wrapped_output = fold_plan(&wrapped_plan, fold_context)?;

    Ok(template_emission_from_output_and_signal(
        wrapped_output,
        signal_kind,
    ))
}

fn template_emission_from_output_and_signal(
    output: StringId,
    signal_kind: Option<TemplateLoopControlKind>,
) -> TemplateEmission {
    match signal_kind {
        None => TemplateEmission::Output(output),
        Some(TemplateLoopControlKind::Break) => TemplateEmission::Break(Some(output)),
        Some(TemplateLoopControlKind::Continue) => TemplateEmission::Continue(Some(output)),
    }
}

/// Resolves fold bindings in an expression using a borrow-first strategy.
///
/// WHAT: examines an expression and returns either a borrowed reference to the
///       original (when no substitution was needed) or an owned rewritten expression.
/// WHY: most template expressions contain no foldable bindings. Cloning the
///      entire expression tree on every fold call is wasted work when the common
///      path simply passes the expression through unchanged. The borrow-first
///      approach avoids allocation on the no-substitution path entirely.
pub(crate) fn resolve_fold_bindings_in_expression<'a>(
    expression: &'a Expression,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<FoldResolvedExpression<'a>, TemplateError> {
    match &expression.kind {
        ExpressionKind::Reference(path) => {
            if let Some(bound_value) = fold_context.lookup_binding(path) {
                // Binding found: produce an owned clone of the bound value.
                // This is the actual substitution that justifies an allocation.
                add_ast_counter(AstCounter::TemplateFoldBindingSubstitutions, 1);
                add_ast_counter(AstCounter::TemplateFoldExpressionCloneRequests, 1);
                add_ast_counter(AstCounter::TemplateFoldExpressionOwnedRewrites, 1);
                Ok(FoldResolvedExpression::Owned(Box::new(bound_value.clone())))
            } else {
                // No binding: borrow the original expression unchanged.
                Ok(FoldResolvedExpression::Borrowed(expression))
            }
        }

        ExpressionKind::Coerced { value, to_type } => {
            let resolved = resolve_fold_bindings_in_expression(value, fold_context)?;

            // If the inner value was not substituted, the coerced wrapper is
            // also unchanged — borrow the original expression.
            if matches!(resolved, FoldResolvedExpression::Borrowed(_)) {
                return Ok(FoldResolvedExpression::Borrowed(expression));
            }

            // Inner value was rewritten: rebuild the coerced wrapper with the
            // resolved inner value. Only allocate because the inner actually changed.
            let resolved_owned = resolved.into_owned();
            add_ast_counter(AstCounter::TemplateFoldExpressionCloneRequests, 1);
            add_ast_counter(AstCounter::TemplateFoldExpressionOwnedRewrites, 1);
            Ok(FoldResolvedExpression::Owned(Box::new(Expression {
                kind: ExpressionKind::Coerced {
                    value: Box::new(resolved_owned),
                    to_type: *to_type,
                },
                ..expression.clone()
            })))
        }

        ExpressionKind::Runtime(rpn) => {
            fold_runtime_expression_with_bindings(expression, rpn, fold_context)
        }

        // All other expression kinds have no foldable bindings — borrow unchanged.
        _ => Ok(FoldResolvedExpression::Borrowed(expression)),
    }
}

/// Resolves fold bindings in a runtime RPN expression.
///
/// WHAT: substitutes foldable bindings inside RPN operand expressions and
///       attempts constant folding on the substituted result. Returns a borrowed
///       reference when no operand was substituted and folding did not produce
///       a new value.
/// WHY: RPN expressions in const template loops are the other main allocation
///      hot spot. When all operands are non-binding references or literals,
///      the expression passes through unchanged and should not be cloned.
fn fold_runtime_expression_with_bindings<'a>(
    expression: &'a Expression,
    rpn: &ExpressionRpn,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<FoldResolvedExpression<'a>, TemplateError> {
    let mut substituted = Vec::with_capacity(rpn.items.len());
    let mut any_substituted = false;

    for item in &rpn.items {
        let new_item = match item {
            ExpressionRpnItem::Operand(value) => {
                let resolved = resolve_fold_bindings_in_expression(value, fold_context)?;
                match resolved {
                    FoldResolvedExpression::Borrowed(_) => {
                        // Operand unchanged — push the original clone (operator
                        // nodes need owned items in the substituted Vec).
                        item.clone()
                    }
                    FoldResolvedExpression::Owned(owned) => {
                        any_substituted = true;
                        add_ast_counter(AstCounter::TemplateFoldExpressionCloneRequests, 1);
                        ExpressionRpnItem::Operand(*owned)
                    }
                }
            }
            ExpressionRpnItem::Operator { .. } => item.clone(),
        };
        substituted.push(new_item);
    }

    // No operand was substituted and constant folding has nothing new to
    // evaluate — borrow the original expression unchanged.
    if !any_substituted {
        return Ok(FoldResolvedExpression::Borrowed(expression));
    }

    // At least one operand was substituted; attempt constant folding on the
    // updated RPN to see if the expression can be simplified further.
    match constant_fold(&substituted, fold_context.string_table) {
        Ok(stack) => {
            if stack.len() == 1
                && let ExpressionRpnItem::Operand(folded) = &stack[0]
            {
                add_ast_counter(AstCounter::TemplateFoldExpressionCloneRequests, 1);
                add_ast_counter(AstCounter::TemplateFoldExpressionOwnedRewrites, 1);
                return Ok(FoldResolvedExpression::Owned(Box::new(folded.to_owned())));
            }
            // Folding did not simplify to a single value; build a new Runtime
            // expression from the substituted RPN.
            add_ast_counter(AstCounter::TemplateFoldExpressionCloneRequests, 1);
            add_ast_counter(AstCounter::TemplateFoldExpressionOwnedRewrites, 1);
            Ok(FoldResolvedExpression::Owned(Box::new(Expression {
                kind: ExpressionKind::Runtime(ExpressionRpn { items: substituted }),
                ..expression.clone()
            })))
        }

        Err(_) => {
            // Constant folding failed; build a new Runtime expression from the
            // substituted RPN so downstream sees the substituted operands.
            add_ast_counter(AstCounter::TemplateFoldExpressionCloneRequests, 1);
            add_ast_counter(AstCounter::TemplateFoldExpressionOwnedRewrites, 1);
            Ok(FoldResolvedExpression::Owned(Box::new(Expression {
                kind: ExpressionKind::Runtime(ExpressionRpn { items: substituted }),
                ..expression.clone()
            })))
        }
    }
}

/// Recursively folds a render plan into a single interned string ID.
///
/// # Temporary test access
///
/// Exposed as `pub(crate)` so Phase B2 TIR parity tests can compare the old
/// render-plan fold output against the new TIR fold output. This function is
/// deleted once the TIR fold route fully replaces the render-plan path.
pub(crate) fn fold_plan(
    plan: &TemplateRenderPlan,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<StringId, TemplateError> {
    match fold_plan_to_emission(plan, fold_context)? {
        TemplateEmission::NoOutput => {
            let empty_id = fold_context.string_table.intern("");
            record_fold_output_intern(0);
            Ok(empty_id)
        }
        TemplateEmission::Output(output) => Ok(output),
        TemplateEmission::Break(_) | TemplateEmission::Continue(_) => {
            Err(CompilerError::compiler_error(
                "Template loop-control signal escaped the nearest template loop during folding.",
            )
            .into())
        }
    }
}

/// Folds a single render piece, appending any output to the buffer.
///
/// Returns `Some(signal_kind)` when the piece (or a nested template within it)
/// produced a loop-control signal. The caller must intern the buffer and build
/// the appropriate `TemplateEmission`.
///
/// # Temporary TIR bridge
///
/// This helper is reused by the Phase B2 TIR aggregate-wrapper fold path
/// because aggregate render plans are still AST-shaped. It will be removed
/// once Phase B4 replaces aggregate wrappers with TIR-native render units.
pub(crate) fn fold_render_piece(
    piece: &RenderPiece,
    output_buffer: &mut String,
    emitted_output: &mut bool,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    // Map each render piece to an optional expression to fold. Head and body text
    // are treated identically during folding — the distinction only matters for
    // formatter boundary detection, which already ran before this stage.
    //
    // Text and head-content pieces always produce `StringSlice` expressions with
    // no foldable bindings, so they bypass the borrow-first resolver and fold
    // directly. Child template and dynamic expression pieces pass a reference
    // to the resolver to avoid cloning the expression tree on the common
    // no-substitution path.
    match piece {
        // Text atoms are always string slices with no foldable bindings —
        // fold them directly without going through the resolver.
        RenderPiece::Text(p) => {
            let text_expr =
                Expression::string_slice(p.text, p.location.clone(), ValueMode::ImmutableOwned);
            fold_expression_piece_to_buffer(&text_expr, output_buffer, emitted_output, fold_context)
        }
        RenderPiece::HeadContent(p) => {
            let head_expr =
                Expression::string_slice(p.text, p.location.clone(), ValueMode::ImmutableOwned);
            fold_expression_piece_to_buffer(&head_expr, output_buffer, emitted_output, fold_context)
        }
        RenderPiece::ChildTemplate(p) => {
            let resolved = resolve_fold_bindings_in_expression(&p.expression, fold_context)?;
            fold_resolved_expression_piece_to_buffer(
                &resolved,
                output_buffer,
                emitted_output,
                fold_context,
            )
        }
        RenderPiece::DynamicExpression(p) => {
            let resolved = resolve_fold_bindings_in_expression(&p.expression, fold_context)?;
            fold_resolved_expression_piece_to_buffer(
                &resolved,
                output_buffer,
                emitted_output,
                fold_context,
            )
        }
        RenderPiece::LoopControl(signal) => Ok(Some(signal.kind)),
        // Unfilled slots intentionally fold to empty; the surrounding authored
        // content still renders.
        RenderPiece::Slot(_) => Ok(None),
        RenderPiece::RuntimeSlotSite(_) => Ok(None),
    }
}

/// Folds an expression reference directly into the output buffer.
///
/// WHAT: used for text and head-content pieces that are always string slices
///       with no foldable bindings.
/// WHY: these pieces never need binding resolution, so we can fold them directly
///      without going through the borrow-first resolver.
fn fold_expression_piece_to_buffer(
    expression: &Expression,
    output_buffer: &mut String,
    emitted_output: &mut bool,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    match fold_expression_kind_to_string(&expression.kind, fold_context.string_table) {
        Some(FoldedStringPiece::Text(text)) => {
            *emitted_output = true;
            output_buffer.push_str(&text);
        }
        Some(FoldedStringPiece::Char(ch)) => {
            *emitted_output = true;
            output_buffer.push(ch);
        }
        Some(FoldedStringPiece::Skip) => {}
        Some(FoldedStringPiece::NestedTemplate) => {
            return Err(CompilerError::compiler_error(
                "String coercion returned NestedTemplate for a text/head-content expression.",
            )
            .into());
        }
        None => {
            return Err(CompilerError::compiler_error(
                "Invalid Expression Used Inside template when trying to fold into a string. The compiler_frontend should not be trying to fold this template.",
            )
            .into());
        }
    }
    Ok(None)
}

/// Folds a resolved expression (from the borrow-first resolver) into the output buffer.
///
/// WHAT: handles child template and dynamic expression pieces after binding resolution.
/// WHY: these pieces may contain foldable bindings, so they go through the resolver first.
///      The resolved expression is borrowed by reference when no substitution occurred.
fn fold_resolved_expression_piece_to_buffer(
    resolved: &FoldResolvedExpression<'_>,
    output_buffer: &mut String,
    emitted_output: &mut bool,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<Option<TemplateLoopControlKind>, TemplateError> {
    // Borrow the resolved expression by reference. If the resolver returned
    // a borrowed reference, this avoids any allocation entirely.
    let expression_ref: &Expression = match resolved {
        FoldResolvedExpression::Borrowed(expr) => expr,
        FoldResolvedExpression::Owned(expr) => expr,
    };

    // Delegate the "what can become string content" policy to the coercion module.
    match fold_expression_kind_to_string(&expression_ref.kind, fold_context.string_table) {
        Some(FoldedStringPiece::Text(text)) => {
            *emitted_output = true;
            output_buffer.push_str(&text);
        }

        Some(FoldedStringPiece::Char(ch)) => {
            *emitted_output = true;
            output_buffer.push(ch);
        }

        Some(FoldedStringPiece::Skip) => {}

        Some(FoldedStringPiece::NestedTemplate) => {
            // The expression kind was a Template — retrieve the template from the
            // resolved expression. When the resolver returned a borrowed reference,
            // the template inside the expression is still intact.
            let ExpressionKind::Template(template) = &expression_ref.kind else {
                return Err(CompilerError::compiler_error(
                    "String coercion returned NestedTemplate for a non-Template expression kind.",
                )
                .into());
            };

            if matches!(template.kind, TemplateType::SlotInsert(_))
                || template.contains_slot_insertions()
            {
                return Err(CompilerError::compiler_error(
                    "Invalid template content reached string folding: unresolved slot insertions cannot be rendered directly.",
                )
                .into());
            }

            // Nested templates that became fully resolved only after wrapper
            // composition are folded here to preserve authored nesting order.
            match template.fold_to_emission(fold_context)? {
                TemplateEmission::NoOutput => {}
                TemplateEmission::Output(folded_nested) => {
                    *emitted_output = true;
                    output_buffer.push_str(fold_context.string_table.resolve(folded_nested));
                }
                TemplateEmission::Break(output) => {
                    if let Some(output) = output {
                        *emitted_output = true;
                        output_buffer.push_str(fold_context.string_table.resolve(output));
                    }
                    return Ok(Some(TemplateLoopControlKind::Break));
                }
                TemplateEmission::Continue(output) => {
                    if let Some(output) = output {
                        *emitted_output = true;
                        output_buffer.push_str(fold_context.string_table.resolve(output));
                    }
                    return Ok(Some(TemplateLoopControlKind::Continue));
                }
            }
        }

        // Anything else can't be folded and should not get to this stage.
        None => {
            return Err(CompilerError::compiler_error(
                "Invalid Expression Used Inside template when trying to fold into a string. The compiler_frontend should not be trying to fold this template.",
            )
            .into());
        }
    }

    Ok(None)
}

/// Recursively folds a render plan while preserving structural loop-control signals.
fn fold_plan_to_emission(
    plan: &TemplateRenderPlan,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<TemplateEmission, TemplateError> {
    add_ast_counter(AstCounter::TemplateFoldPlanPiecesVisited, plan.pieces.len());

    let estimated_bytes = plan.estimate_output_bytes(fold_context.string_table);
    let mut output_buffer = reserve_fold_output_buffer(estimated_bytes);
    let mut emitted_output = false;

    for piece in &plan.pieces {
        if let Some(kind) =
            fold_render_piece(piece, &mut output_buffer, &mut emitted_output, fold_context)?
        {
            let actual_len = output_buffer.len();
            let output = emitted_output.then(|| {
                record_fold_output_estimate_miss(actual_len, estimated_bytes);
                let output_id = fold_context.string_table.intern(&output_buffer);
                record_fold_output_intern(actual_len);
                output_id
            });
            return Ok(match kind {
                TemplateLoopControlKind::Break => TemplateEmission::Break(output),
                TemplateLoopControlKind::Continue => TemplateEmission::Continue(output),
            });
        }
    }

    ast_log!("Folded template into: ", output_buffer);

    if !emitted_output {
        return Ok(TemplateEmission::NoOutput);
    }

    let actual_len = output_buffer.len();
    record_fold_output_estimate_miss(actual_len, estimated_bytes);
    let output_id = fold_context.string_table.intern(&output_buffer);
    record_fold_output_intern(actual_len);
    Ok(TemplateEmission::Output(output_id))
}

fn record_fold_output_intern(byte_len: usize) {
    add_ast_counter(AstCounter::TemplateFoldStringInternCalls, 1);
    add_ast_counter(AstCounter::TemplateFoldOutputBytes, byte_len);
}

fn render_plan_for_folding<'a>(
    existing_plan: Option<&'a TemplateRenderPlan>,
    content: &'a TemplateContent,
) -> Cow<'a, TemplateRenderPlan> {
    if let Some(plan) = existing_plan {
        return Cow::Borrowed(plan);
    }

    add_ast_counter(AstCounter::TemplateFoldFallbackPlanBuilds, 1);
    Cow::Owned(TemplateRenderPlan::from_content(content))
}
