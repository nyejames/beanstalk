//! Compile-time template folding.
//!
//! WHAT: Converts fully-resolved template content into interned string IDs
//! by recursively folding atoms (text, nested templates, head/body segments).
//!
//! WHY: Separates folding logic from parsing and composition so it can later
//! be rebuilt on top of the render-plan IR without entangling parser code.

use crate::ast_log;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, RangeEndKind};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::match_patterns::MatchPattern;
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::{
    TemplateAtom, TemplateContent, TemplateSegment, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_composition::{
    compose_template_head_chain, wrap_direct_child_atom,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchChain, TemplateBranchSelector, TemplateConditionalBranch, TemplateControlFlow,
    TemplateLoopControlFlow, TemplateLoopControlKind, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_formatting::apply_body_formatter;
use crate::compiler_frontend::ast::templates::template_render_plan::{
    RenderPiece, TemplateRenderPlan,
};
use crate::compiler_frontend::ast::templates::template_slots::SlotResolutionMode;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DiagnosticSeverity, InvalidTemplateStructureReason,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::optimizers::constant_folding::{ConstantFoldResult, constant_fold};
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
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

#[derive(Clone)]
pub(crate) struct TemplateFoldBinding {
    pub(crate) path: InternedPath,
    pub(crate) value: Expression,
}

impl TemplateFoldContext<'_> {
    fn lookup_binding(&self, path: &InternedPath) -> Option<&Expression> {
        self.bindings
            .iter()
            .rev()
            .find(|binding| &binding.path == path)
            .map(|binding| &binding.value)
    }

    fn push_bindings(&mut self, bindings: impl IntoIterator<Item = TemplateFoldBinding>) -> usize {
        let previous_len = self.bindings.len();
        self.bindings.extend(bindings);
        previous_len
    }

    fn restore_bindings(&mut self, previous_len: usize) {
        self.bindings.truncate(previous_len);
    }
}

// -------------------------
//  Folding Implementation
// -------------------------

impl Template {
    /// Folds a fully-resolved template into an interned string ID.
    /// Applies deferred formatting if needed, then recursively folds all pieces.
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

        if self.control_flow.is_some() {
            return match self.fold_to_emission(fold_context)? {
                TemplateEmission::NoOutput => Ok(fold_context.string_table.intern("")),
                TemplateEmission::Output(output) => Ok(output),
                TemplateEmission::Break(_) | TemplateEmission::Continue(_) => Err(
                    CompilerError::compiler_error(
                        "Template loop-control signal escaped the nearest template loop during folding.",
                    )
                    .into(),
                ),
            };
        }

        // 1. Resolve the render plan.
        let plan = if self.content_needs_formatting {
            apply_body_formatter(
                &self.unformatted_content,
                &self.style,
                fold_context.string_table,
            )
            .map(|result| result.plan)
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
            self.render_plan
                .clone()
                .unwrap_or_else(|| TemplateRenderPlan::from_content(&self.content))
        };

        // 2. Recursively fold the plan into a final string.
        fold_plan(&plan, fold_context)
    }

    pub(crate) fn fold_to_emission(
        &self,
        fold_context: &mut TemplateFoldContext<'_>,
    ) -> Result<TemplateEmission, TemplateError> {
        let Some(control_flow) = &self.control_flow else {
            let output = self.fold_into_stringid(fold_context)?;
            return Ok(TemplateEmission::Output(output));
        };

        let emission = fold_control_flow(self, control_flow, fold_context)?;
        apply_conditional_child_wrappers(self, emission, fold_context)
    }
}

fn fold_control_flow(
    template: &Template,
    control_flow: &TemplateControlFlow,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<TemplateEmission, TemplateError> {
    match control_flow {
        TemplateControlFlow::BranchChain(branch_chain) => {
            fold_template_branch_chain(branch_chain, fold_context)
        }

        TemplateControlFlow::Loop(template_loop) => {
            fold_template_loop(template, template_loop, fold_context)
        }

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
    let branch_plan = branch
        .render_plan
        .clone()
        .unwrap_or_else(|| TemplateRenderPlan::from_content(&branch.content));

    fold_plan_to_emission(&branch_plan, fold_context)
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

fn selected_option_capture_payload(
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

    let fallback_plan = fallback
        .render_plan
        .clone()
        .unwrap_or_else(|| TemplateRenderPlan::from_content(&fallback.content));
    fold_plan_to_emission(&fallback_plan, fold_context)
}

enum ConstOptionPresence {
    Present(Box<Expression>),
    Absent,
}

fn const_option_presence(
    scrutinee: &Expression,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<ConstOptionPresence, TemplateError> {
    let resolved = resolve_fold_bindings_in_expression(scrutinee.to_owned(), fold_context)?;

    match &resolved.kind {
        ExpressionKind::OptionNone => Ok(ConstOptionPresence::Absent),

        ExpressionKind::Coerced { value, .. } => {
            let payload = (**value).clone();
            if payload.is_compile_time_constant() {
                Ok(ConstOptionPresence::Present(Box::new(payload)))
            } else {
                Err(option_capture_const_deferred_error(&resolved).into())
            }
        }

        _ => Err(option_capture_const_deferred_error(&resolved).into()),
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
    template: &Template,
    template_loop: &TemplateLoopControlFlow,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<TemplateEmission, TemplateError> {
    let body_plan = template_loop
        .body_render_plan
        .clone()
        .unwrap_or_else(|| TemplateRenderPlan::from_content(&template_loop.body_content));

    let mut aggregate = String::new();
    let mut emitted_iterations = 0usize;
    let mut emitted_output = false;

    match &template_loop.header {
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
            let range_values = const_range_values(range)?;
            for counter in range_values.iterations(
                fold_context.template_const_loop_iteration_limit,
                &template_loop.location,
            )? {
                emitted_iterations += 1;
                enforce_loop_iteration_limit(
                    emitted_iterations,
                    fold_context.template_const_loop_iteration_limit,
                    &template_loop.location,
                )?;

                let iteration_bindings =
                    build_range_iteration_bindings(bindings, counter, emitted_iterations - 1);
                match fold_loop_body_iteration(
                    &body_plan,
                    iteration_bindings,
                    fold_context,
                    &template_loop.location,
                    &mut aggregate,
                )? {
                    TemplateEmission::NoOutput => {}
                    TemplateEmission::Output(_) => emitted_output = true,
                    TemplateEmission::Break(output) => {
                        emitted_output |= append_optional_signal_output(
                            output,
                            fold_context.string_table,
                            &mut aggregate,
                        );
                        break;
                    }
                    TemplateEmission::Continue(output) => {
                        emitted_output |= append_optional_signal_output(
                            output,
                            fold_context.string_table,
                            &mut aggregate,
                        );
                        continue;
                    }
                }
            }
        }

        TemplateLoopHeader::Collection { bindings, iterable } => {
            let items = const_collection_items(iterable)?;
            for (index, item) in items.iter().enumerate() {
                emitted_iterations += 1;
                enforce_loop_iteration_limit(
                    emitted_iterations,
                    fold_context.template_const_loop_iteration_limit,
                    &template_loop.location,
                )?;

                let iteration_bindings = build_collection_iteration_bindings(bindings, item, index);
                match fold_loop_body_iteration(
                    &body_plan,
                    iteration_bindings,
                    fold_context,
                    &template_loop.location,
                    &mut aggregate,
                )? {
                    TemplateEmission::NoOutput => {}
                    TemplateEmission::Output(_) => emitted_output = true,
                    TemplateEmission::Break(output) => {
                        emitted_output |= append_optional_signal_output(
                            output,
                            fold_context.string_table,
                            &mut aggregate,
                        );
                        break;
                    }
                    TemplateEmission::Continue(output) => {
                        emitted_output |= append_optional_signal_output(
                            output,
                            fold_context.string_table,
                            &mut aggregate,
                        );
                        continue;
                    }
                }
            }
        }
    }

    if !emitted_output {
        return Ok(TemplateEmission::NoOutput);
    }

    let aggregate_id = fold_context.string_table.intern(&aggregate);
    apply_loop_shared_head(template, aggregate_id, fold_context)
}

fn fold_conditional_loop_const_condition(
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

fn condition_location_or_loop_location(
    condition: &Expression,
    loop_location: &SourceLocation,
) -> SourceLocation {
    if condition.location == Default::default() {
        loop_location.clone()
    } else {
        condition.location.clone()
    }
}

fn fold_loop_body_iteration(
    body_plan: &TemplateRenderPlan,
    iteration_bindings: Vec<TemplateFoldBinding>,
    fold_context: &mut TemplateFoldContext<'_>,
    diagnostic_location: &crate::compiler_frontend::tokenizer::tokens::SourceLocation,
    aggregate: &mut String,
) -> Result<TemplateEmission, TemplateError> {
    let previous_bindings_len = fold_context.push_bindings(iteration_bindings);
    let folded_result = fold_plan_to_emission(body_plan, fold_context);
    fold_context.restore_bindings(previous_bindings_len);

    let emission =
        folded_result.map_err(|error| loop_body_not_const_error(error, diagnostic_location))?;

    match emission {
        TemplateEmission::NoOutput => Ok(TemplateEmission::NoOutput),
        TemplateEmission::Output(output) => {
            aggregate.push_str(fold_context.string_table.resolve(output));
            Ok(TemplateEmission::Output(output))
        }
        TemplateEmission::Break(output) => Ok(TemplateEmission::Break(output)),
        TemplateEmission::Continue(output) => Ok(TemplateEmission::Continue(output)),
    }
}

fn append_optional_signal_output(
    output: Option<StringId>,
    string_table: &StringTable,
    aggregate: &mut String,
) -> bool {
    let Some(output) = output else {
        return false;
    };

    aggregate.push_str(string_table.resolve(output));
    true
}

fn loop_body_not_const_error(
    error: TemplateError,
    diagnostic_location: &crate::compiler_frontend::tokenizer::tokens::SourceLocation,
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

fn enforce_loop_iteration_limit(
    emitted_iterations: usize,
    iteration_limit: usize,
    location: &crate::compiler_frontend::tokenizer::tokens::SourceLocation,
) -> Result<(), TemplateError> {
    if emitted_iterations <= iteration_limit {
        return Ok(());
    }

    Err(CompilerDiagnostic::invalid_template_structure(
        InvalidTemplateStructureReason::TemplateConstLoopExpansionLimitExceeded {
            limit: iteration_limit,
        },
        location.clone(),
    )
    .into())
}

fn apply_loop_shared_head(
    template: &Template,
    aggregate_id: StringId,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<TemplateEmission, TemplateError> {
    let aggregate_expression = Expression::string_slice(
        aggregate_id,
        template.location.clone(),
        ValueMode::ImmutableOwned,
    );
    let mut content = template.content.to_owned();
    content.add(aggregate_expression);

    let mut can_fold = true;
    let composed = compose_template_head_chain(
        &content,
        &mut can_fold,
        fold_context.string_table,
        SlotResolutionMode::ComposeOnly,
    )
    .map_err(TemplateError::from)?;
    let plan = TemplateRenderPlan::from_content(&composed);
    let output = fold_plan(&plan, fold_context)?;

    Ok(TemplateEmission::Output(output))
}

fn fold_bool_condition(
    condition: &Expression,
    fallback_location: &crate::compiler_frontend::tokenizer::tokens::SourceLocation,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<bool, TemplateError> {
    let condition = resolve_fold_bindings_in_expression(condition.to_owned(), fold_context)?;

    fold_resolved_bool_condition(&condition, fallback_location)
}

fn fold_resolved_bool_condition(
    condition: &Expression,
    fallback_location: &crate::compiler_frontend::tokenizer::tokens::SourceLocation,
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

#[derive(Clone, Copy)]
enum ConstRangeValues {
    Int {
        start: i64,
        end: i64,
        end_kind: RangeEndKind,
        step_magnitude: i64,
    },
    Float {
        start: f64,
        end: f64,
        end_kind: RangeEndKind,
        step_magnitude: f64,
    },
}

#[derive(Clone, Copy)]
enum ConstRangeCounter {
    Int(i64),
    Float(f64),
}

impl ConstRangeValues {
    fn iterations(
        self,
        iteration_limit: usize,
        location: &crate::compiler_frontend::tokenizer::tokens::SourceLocation,
    ) -> Result<Vec<ConstRangeCounter>, TemplateError> {
        match self {
            Self::Int {
                start,
                end,
                end_kind,
                step_magnitude,
            } => int_range_iterations(
                start,
                end,
                end_kind,
                step_magnitude,
                iteration_limit,
                location,
            ),

            Self::Float {
                start,
                end,
                end_kind,
                step_magnitude,
            } => float_range_iterations(
                start,
                end,
                end_kind,
                step_magnitude,
                iteration_limit,
                location,
            ),
        }
    }
}

fn const_range_values(
    range: &crate::compiler_frontend::ast::ast_nodes::RangeLoopSpec,
) -> Result<ConstRangeValues, TemplateError> {
    let start = const_numeric_expression(&range.start)?;
    let end = const_numeric_expression(&range.end)?;
    let step = range
        .step
        .as_ref()
        .map(const_numeric_expression)
        .transpose()?;

    match (start, end, step) {
        (ConstNumericValue::Int(start), ConstNumericValue::Int(end), None) => {
            Ok(ConstRangeValues::Int {
                start,
                end,
                end_kind: range.end_kind,
                step_magnitude: 1,
            })
        }

        (
            ConstNumericValue::Int(start),
            ConstNumericValue::Int(end),
            Some(ConstNumericValue::Int(step)),
        ) => Ok(ConstRangeValues::Int {
            start,
            end,
            end_kind: range.end_kind,
            step_magnitude: int_step_magnitude(
                step,
                range
                    .step
                    .as_ref()
                    .map(|step_expression| &step_expression.location)
                    .unwrap_or(&range.start.location),
            )?,
        }),

        (start, end, step) => {
            let start = start.as_float();
            let end = end.as_float();
            let step_magnitude = step.map(ConstNumericValue::as_float).unwrap_or(1.0).abs();
            Ok(ConstRangeValues::Float {
                start,
                end,
                end_kind: range.end_kind,
                step_magnitude,
            })
        }
    }
}

fn int_step_magnitude(
    step: i64,
    location: &crate::compiler_frontend::tokenizer::tokens::SourceLocation,
) -> Result<i64, TemplateError> {
    step.checked_abs().ok_or_else(|| {
        CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::TemplateLoopRangeBoundsNotConst,
            location.clone(),
        )
        .into()
    })
}

#[derive(Clone, Copy)]
enum ConstNumericValue {
    Int(i64),
    Float(f64),
}

impl ConstNumericValue {
    fn as_float(self) -> f64 {
        match self {
            Self::Int(value) => value as f64,
            Self::Float(value) => value,
        }
    }
}

fn const_numeric_expression(expression: &Expression) -> Result<ConstNumericValue, TemplateError> {
    match &expression.kind {
        ExpressionKind::Int(value) => Ok(ConstNumericValue::Int(*value)),
        ExpressionKind::Float(value) => Ok(ConstNumericValue::Float(*value)),
        ExpressionKind::Coerced { value, .. } => const_numeric_expression(value),
        _ => Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::TemplateLoopRangeBoundsNotConst,
            expression.location.clone(),
        )
        .into()),
    }
}

fn int_range_iterations(
    start: i64,
    end: i64,
    end_kind: RangeEndKind,
    step_magnitude: i64,
    iteration_limit: usize,
    location: &crate::compiler_frontend::tokenizer::tokens::SourceLocation,
) -> Result<Vec<ConstRangeCounter>, TemplateError> {
    if step_magnitude == 0 {
        return Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::TemplateLoopRangeBoundsNotConst,
            location.clone(),
        )
        .into());
    }

    let mut counters = Vec::new();
    let ascending = start <= end;
    let step = if ascending {
        step_magnitude
    } else {
        -step_magnitude
    };
    let mut current = start;

    while int_range_contains(current, end, end_kind, ascending) {
        counters.push(ConstRangeCounter::Int(current));
        enforce_loop_iteration_limit(counters.len(), iteration_limit, location)?;
        current = match current.checked_add(step) {
            Some(next) => next,
            None => {
                return Err(CompilerDiagnostic::invalid_template_structure(
                    InvalidTemplateStructureReason::TemplateLoopRangeBoundsNotConst,
                    location.clone(),
                )
                .into());
            }
        };
    }

    Ok(counters)
}

fn int_range_contains(current: i64, end: i64, end_kind: RangeEndKind, ascending: bool) -> bool {
    match (ascending, end_kind) {
        (true, RangeEndKind::Exclusive) => current < end,
        (true, RangeEndKind::Inclusive) => current <= end,
        (false, RangeEndKind::Exclusive) => current > end,
        (false, RangeEndKind::Inclusive) => current >= end,
    }
}

fn float_range_iterations(
    start: f64,
    end: f64,
    end_kind: RangeEndKind,
    step_magnitude: f64,
    iteration_limit: usize,
    location: &crate::compiler_frontend::tokenizer::tokens::SourceLocation,
) -> Result<Vec<ConstRangeCounter>, TemplateError> {
    if !start.is_finite()
        || !end.is_finite()
        || step_magnitude == 0.0
        || !step_magnitude.is_finite()
    {
        return Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::TemplateLoopRangeBoundsNotConst,
            location.clone(),
        )
        .into());
    }

    let mut counters = Vec::new();
    let ascending = start <= end;
    let step = if ascending {
        step_magnitude
    } else {
        -step_magnitude
    };
    let mut current = start;

    while float_range_contains(current, end, end_kind, ascending) {
        counters.push(ConstRangeCounter::Float(current));
        enforce_loop_iteration_limit(counters.len(), iteration_limit, location)?;
        current += step;

        if !current.is_finite() {
            return Err(CompilerDiagnostic::invalid_template_structure(
                InvalidTemplateStructureReason::TemplateLoopRangeBoundsNotConst,
                location.clone(),
            )
            .into());
        }
    }

    Ok(counters)
}

fn float_range_contains(current: f64, end: f64, end_kind: RangeEndKind, ascending: bool) -> bool {
    match (ascending, end_kind) {
        (true, RangeEndKind::Exclusive) => current < end,
        (true, RangeEndKind::Inclusive) => current <= end,
        (false, RangeEndKind::Exclusive) => current > end,
        (false, RangeEndKind::Inclusive) => current >= end,
    }
}

fn const_collection_items(iterable: &Expression) -> Result<Vec<Expression>, TemplateError> {
    match &iterable.kind {
        ExpressionKind::Collection(items) => Ok(items.to_owned()),
        ExpressionKind::Coerced { value, .. } => const_collection_items(value),
        _ => Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::TemplateLoopSourceNotConst,
            iterable.location.clone(),
        )
        .into()),
    }
}

fn build_range_iteration_bindings(
    bindings: &crate::compiler_frontend::ast::ast_nodes::LoopBindings,
    counter: ConstRangeCounter,
    zero_based_index: usize,
) -> Vec<TemplateFoldBinding> {
    let mut fold_bindings = Vec::new();

    if let Some(item) = &bindings.item {
        let value = match counter {
            ConstRangeCounter::Int(value) => Expression::int(
                value,
                item.value.location.clone(),
                ValueMode::ImmutableOwned,
            ),
            ConstRangeCounter::Float(value) => Expression::float(
                value,
                item.value.location.clone(),
                ValueMode::ImmutableOwned,
            ),
        };
        fold_bindings.push(TemplateFoldBinding {
            path: item.id.clone(),
            value,
        });
    }

    if let Some(index) = &bindings.index {
        fold_bindings.push(TemplateFoldBinding {
            path: index.id.clone(),
            value: Expression::int(
                zero_based_index as i64,
                index.value.location.clone(),
                ValueMode::ImmutableOwned,
            ),
        });
    }

    fold_bindings
}

fn build_collection_iteration_bindings(
    bindings: &crate::compiler_frontend::ast::ast_nodes::LoopBindings,
    item_value: &Expression,
    zero_based_index: usize,
) -> Vec<TemplateFoldBinding> {
    let mut fold_bindings = Vec::new();

    if let Some(item) = &bindings.item {
        let mut value = item_value.to_owned();
        value.location = item.value.location.clone();
        fold_bindings.push(TemplateFoldBinding {
            path: item.id.clone(),
            value,
        });
    }

    if let Some(index) = &bindings.index {
        fold_bindings.push(TemplateFoldBinding {
            path: index.id.clone(),
            value: Expression::int(
                zero_based_index as i64,
                index.value.location.clone(),
                ValueMode::ImmutableOwned,
            ),
        });
    }

    fold_bindings
}

fn apply_conditional_child_wrappers(
    template: &Template,
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

    if template.conditional_child_wrappers.is_empty() {
        return Ok(template_emission_from_output_and_signal(
            output,
            signal_kind,
        ));
    }

    let output_expression =
        crate::compiler_frontend::ast::expressions::expression::Expression::string_slice(
            output,
            template.location.clone(),
            ValueMode::ImmutableOwned,
        );
    let mut wrapped_atom = TemplateAtom::Content(TemplateSegment::new(
        output_expression,
        TemplateSegmentOrigin::Body,
    ));

    for wrapper in template.conditional_child_wrappers.iter().rev() {
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

fn resolve_fold_bindings_in_expression(
    expression: Expression,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<Expression, TemplateError> {
    match &expression.kind {
        ExpressionKind::Reference(path) => Ok(fold_context
            .lookup_binding(path)
            .cloned()
            .unwrap_or(expression)),

        ExpressionKind::Coerced { value, to_type } => {
            let resolved = resolve_fold_bindings_in_expression((**value).clone(), fold_context)?;
            if matches!(resolved.kind, ExpressionKind::Reference(_)) {
                return Ok(expression);
            }

            Ok(Expression {
                kind: ExpressionKind::Coerced {
                    value: Box::new(resolved),
                    to_type: *to_type,
                },
                ..expression
            })
        }

        ExpressionKind::Runtime(nodes) => {
            fold_runtime_expression_with_bindings(&expression, nodes, fold_context)
        }

        _ => Ok(expression),
    }
}

fn fold_runtime_expression_with_bindings(
    expression: &Expression,
    nodes: &[AstNode],
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<Expression, TemplateError> {
    let mut substituted = Vec::with_capacity(nodes.len());

    for node in nodes {
        let mut node = node.to_owned();
        if let crate::compiler_frontend::ast::ast_nodes::NodeKind::Rvalue(value) = &node.kind {
            node.kind = crate::compiler_frontend::ast::ast_nodes::NodeKind::Rvalue(
                resolve_fold_bindings_in_expression(value.to_owned(), fold_context)?,
            );
        }
        substituted.push(node);
    }

    match constant_fold(&substituted, fold_context.string_table) {
        Ok(ConstantFoldResult::Folded(stack)) => {
            if stack.len() == 1
                && let crate::compiler_frontend::ast::ast_nodes::NodeKind::Rvalue(folded) =
                    &stack[0].kind
            {
                return Ok(folded.to_owned());
            }
            Ok(expression.to_owned())
        }

        Ok(ConstantFoldResult::Unchanged) => Ok(expression.to_owned()),

        Err(_) => Ok(expression.to_owned()),
    }
}

/// Recursively folds a render plan into a single interned string ID.
fn fold_plan(
    plan: &TemplateRenderPlan,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<StringId, TemplateError> {
    match fold_plan_to_emission(plan, fold_context)? {
        TemplateEmission::NoOutput => Ok(fold_context.string_table.intern("")),
        TemplateEmission::Output(output) => Ok(output),
        TemplateEmission::Break(_) | TemplateEmission::Continue(_) => {
            Err(CompilerError::compiler_error(
                "Template loop-control signal escaped the nearest template loop during folding.",
            )
            .into())
        }
    }
}

/// Recursively folds a render plan while preserving structural loop-control signals.
fn fold_plan_to_emission(
    plan: &TemplateRenderPlan,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<TemplateEmission, TemplateError> {
    let mut output_buffer = String::new();
    let mut emitted_output = false;

    for piece in &plan.pieces {
        // Map each render piece to an optional expression to fold. Head and body text
        // are treated identically during folding — the distinction only matters for
        // formatter boundary detection, which already ran before this stage.
        let maybe_expression = match piece {
            RenderPiece::Text(p) => Some(Expression::string_slice(
                p.text,
                p.location.clone(),
                ValueMode::ImmutableOwned,
            )),
            RenderPiece::HeadContent(p) => Some(Expression::string_slice(
                p.text,
                p.location.clone(),
                ValueMode::ImmutableOwned,
            )),
            RenderPiece::ChildTemplate(p) => Some(p.expression.clone()),
            RenderPiece::DynamicExpression(p) => Some(p.expression.clone()),
            RenderPiece::LoopControl(signal) => {
                let output =
                    emitted_output.then(|| fold_context.string_table.intern(&output_buffer));
                return Ok(match signal.kind {
                    TemplateLoopControlKind::Break => TemplateEmission::Break(output),
                    TemplateLoopControlKind::Continue => TemplateEmission::Continue(output),
                });
            }
            RenderPiece::Slot(_) => {
                // Unfilled slots intentionally fold to empty; the surrounding authored
                // content still renders.
                None
            }
            RenderPiece::RuntimeSlotSite(_) => None,
        };

        let Some(expression) = maybe_expression else {
            continue;
        };
        let expression = resolve_fold_bindings_in_expression(expression, fold_context)?;

        // Delegate the "what can become string content" policy to the coercion module.
        // Template mechanics (slot resolution, formatting) live in the template subsystem;
        // the decision about which expression kinds are renderable lives in type_coercion::string.
        match fold_expression_kind_to_string(&expression.kind, fold_context.string_table) {
            Some(FoldedStringPiece::Text(text)) => {
                emitted_output = true;
                output_buffer.push_str(&text);
            }

            Some(FoldedStringPiece::Char(ch)) => {
                emitted_output = true;
                output_buffer.push(ch);
            }

            Some(FoldedStringPiece::Skip) => {
                continue;
            }

            Some(FoldedStringPiece::NestedTemplate) => {
                // The expression kind was a Template — retrieve the template from the
                // original piece to recursively fold it with full project context.
                let ExpressionKind::Template(template) = expression.kind else {
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
                        emitted_output = true;
                        output_buffer.push_str(fold_context.string_table.resolve(folded_nested));
                    }
                    TemplateEmission::Break(output) => {
                        if let Some(output) = output {
                            emitted_output = true;
                            output_buffer.push_str(fold_context.string_table.resolve(output));
                        }
                        let output = emitted_output
                            .then(|| fold_context.string_table.intern(&output_buffer));
                        return Ok(TemplateEmission::Break(output));
                    }
                    TemplateEmission::Continue(output) => {
                        if let Some(output) = output {
                            emitted_output = true;
                            output_buffer.push_str(fold_context.string_table.resolve(output));
                        }
                        let output = emitted_output
                            .then(|| fold_context.string_table.intern(&output_buffer));
                        return Ok(TemplateEmission::Continue(output));
                    }
                }
            }

            // Anything else can't be folded and should not get to this stage.
            None => {
                return Err(CompilerError::compiler_error(
                    "Invalid Expression Used Inside template when trying to fold into a string.\
                         The compiler_frontend should not be trying to fold this template.",
                )
                .into());
            }
        }
    }

    ast_log!("Folded template into: ", output_buffer);

    if !emitted_output {
        return Ok(TemplateEmission::NoOutput);
    }

    Ok(TemplateEmission::Output(
        fold_context.string_table.intern(&output_buffer),
    ))
}
