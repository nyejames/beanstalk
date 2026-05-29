//! Validation entry points for structured template control flow.
//!
//! Runtime-capable templates are validated for escaped helper artifacts that
//! should have been composed or routed into AST-owned slot application plans.
//! Const-required templates are validated for full foldability while still
//! allowing slot/helper structure that a parent const template may compose
//! before folding.

use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::template::{TemplateAtom, TemplateContent};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidTemplateStructureReason,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

use super::const_eval::{
    collect_option_capture_binding_path, content_is_const_evaluable_with_bindings,
    expression_is_const_evaluable_with_bindings, option_capture_presence_is_const_decidable,
};
use super::types::{
    TemplateBranchChain, TemplateBranchSelector, TemplateControlFlow, TemplateLoopControlFlow,
    TemplateLoopHeader,
};

impl TemplateControlFlow {
    pub(crate) fn has_unresolved_slots(&self) -> bool {
        match self {
            Self::BranchChain(branch_chain) => branch_chain.has_unresolved_slots(),
            Self::Loop(template_loop) => template_loop.has_unresolved_slots(),
            Self::LoopControl(_) => false,
        }
    }

    pub(crate) fn contains_slot_insertions(&self) -> bool {
        match self {
            Self::BranchChain(branch_chain) => branch_chain.contains_slot_insertions(),
            Self::Loop(template_loop) => template_loop.contains_slot_insertions(),
            Self::LoopControl(_) => false,
        }
    }
}

impl TemplateBranchChain {
    fn has_unresolved_slots(&self) -> bool {
        self.branches
            .iter()
            .any(|branch| branch.content.has_unresolved_slots())
            || self
                .fallback
                .as_ref()
                .is_some_and(|fallback| fallback.content.has_unresolved_slots())
    }

    fn contains_slot_insertions(&self) -> bool {
        self.branches
            .iter()
            .any(|branch| branch.content.contains_slot_insertions())
            || self
                .fallback
                .as_ref()
                .is_some_and(|fallback| fallback.content.contains_slot_insertions())
    }
}

impl TemplateLoopControlFlow {
    fn has_unresolved_slots(&self) -> bool {
        self.body_content.has_unresolved_slots()
    }

    fn contains_slot_insertions(&self) -> bool {
        self.body_content.contains_slot_insertions()
    }
}

/// Validates structured template control flow in a const-required context.
///
/// Runtime-capable templates keep structured control flow for HIR lowering.
/// Const-required callers use this narrower entry point so supported forms can
/// fold now while unsupported const shapes produce source diagnostics instead of
/// leaking as infrastructure errors during finalization.
#[allow(clippy::result_large_err)]
pub(crate) fn validate_const_required_template_control_flow(
    template: &Template,
    location: &SourceLocation,
) -> Result<(), CompilerDiagnostic> {
    validate_const_required_template_control_flow_with_bindings(template, location, &[])
}

/// Rejects slot composition artifacts that would otherwise reach runtime
/// control-flow lowering.
///
/// Compile-time-required callers use the const validator above because slots can
/// still be resolved or folded before runtime. This runtime-only check runs
/// after composition/formatting, when any remaining slot or insertion inside a
/// control-flow body would otherwise become a HIR invariant failure.
#[allow(clippy::result_large_err)]
pub(crate) fn validate_runtime_template_control_flow_slot_artifacts(
    template: &Template,
) -> Result<(), CompilerDiagnostic> {
    if let Some(control_flow) = &template.control_flow {
        validate_runtime_control_flow_slot_artifacts(control_flow)?;
    }

    validate_runtime_content_control_flow_slot_artifacts(&template.content)
}

#[allow(clippy::result_large_err)]
fn validate_const_required_template_control_flow_with_bindings(
    template: &Template,
    location: &SourceLocation,
    loop_binding_paths: &[InternedPath],
) -> Result<(), CompilerDiagnostic> {
    if let Some(control_flow) = &template.control_flow {
        validate_const_required_control_flow(control_flow, location, loop_binding_paths)?;
    }

    validate_const_required_content_control_flow(&template.content, loop_binding_paths)
}

#[allow(clippy::result_large_err)]
fn validate_runtime_control_flow_slot_artifacts(
    control_flow: &TemplateControlFlow,
) -> Result<(), CompilerDiagnostic> {
    match control_flow {
        TemplateControlFlow::BranchChain(branch_chain) => {
            for branch in &branch_chain.branches {
                validate_runtime_control_flow_content_slot_artifacts(
                    &branch.content,
                    &branch.location,
                )?;
            }

            if let Some(fallback) = &branch_chain.fallback {
                validate_runtime_control_flow_content_slot_artifacts(
                    &fallback.content,
                    &fallback.location,
                )?;
            }
        }

        TemplateControlFlow::Loop(template_loop) => {
            validate_runtime_control_flow_content_slot_artifacts(
                &template_loop.body_content,
                &template_loop.location,
            )?;
        }

        TemplateControlFlow::LoopControl(_) => {}
    }

    Ok(())
}

#[allow(clippy::result_large_err)]
fn validate_runtime_control_flow_content_slot_artifacts(
    content: &TemplateContent,
    location: &SourceLocation,
) -> Result<(), CompilerDiagnostic> {
    if content.contains_slot_insertions() {
        return Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::RuntimeControlFlowUnresolvedInsert,
            location.clone(),
        ));
    }

    if content.has_unresolved_slots() {
        return Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::RuntimeControlFlowUnresolvedSlot,
            location.clone(),
        ));
    }

    validate_runtime_content_control_flow_slot_artifacts(content)
}

#[allow(clippy::result_large_err)]
fn validate_runtime_content_control_flow_slot_artifacts(
    content: &TemplateContent,
) -> Result<(), CompilerDiagnostic> {
    for atom in &content.atoms {
        let TemplateAtom::Content(segment) = atom else {
            continue;
        };

        let ExpressionKind::Template(child_template) = &segment.expression.kind else {
            continue;
        };

        validate_runtime_template_control_flow_slot_artifacts(child_template)?;
    }

    Ok(())
}

#[allow(clippy::result_large_err)]
fn validate_const_required_control_flow(
    control_flow: &TemplateControlFlow,
    fallback_location: &SourceLocation,
    loop_binding_paths: &[InternedPath],
) -> Result<(), CompilerDiagnostic> {
    match control_flow {
        TemplateControlFlow::BranchChain(branch_chain) => validate_const_required_branch_chain(
            branch_chain,
            fallback_location,
            loop_binding_paths,
        ),

        TemplateControlFlow::Loop(template_loop) => {
            validate_const_required_loop(template_loop, loop_binding_paths)
        }

        TemplateControlFlow::LoopControl(_) => Ok(()),
    }
}

#[allow(clippy::result_large_err)]
fn validate_const_required_branch_chain(
    branch_chain: &TemplateBranchChain,
    fallback_location: &SourceLocation,
    loop_binding_paths: &[InternedPath],
) -> Result<(), CompilerDiagnostic> {
    for branch in &branch_chain.branches {
        let branch_binding_paths = validate_const_required_branch_selector(
            &branch.selector,
            branch_chain,
            loop_binding_paths,
        )?;

        validate_const_required_branch_content(
            &branch.content,
            fallback_location,
            &branch_binding_paths,
        )?;
    }

    if let Some(fallback) = &branch_chain.fallback {
        validate_const_required_branch_content(
            &fallback.content,
            fallback_location,
            loop_binding_paths,
        )?;
    }

    Ok(())
}

fn validate_const_required_branch_selector(
    selector: &TemplateBranchSelector,
    branch_chain: &TemplateBranchChain,
    loop_binding_paths: &[InternedPath],
) -> Result<Vec<InternedPath>, CompilerDiagnostic> {
    let mut branch_binding_paths = loop_binding_paths.to_vec();

    match selector {
        TemplateBranchSelector::Bool(condition) => {
            if !expression_is_const_evaluable_with_bindings(condition, loop_binding_paths) {
                return Err(CompilerDiagnostic::invalid_template_structure(
                    InvalidTemplateStructureReason::TemplateIfConditionNotConst,
                    condition.location.clone(),
                ));
            }
        }

        TemplateBranchSelector::OptionPresentCapture { scrutinee, pattern } => {
            let location = if scrutinee.location == SourceLocation::default() {
                branch_chain.location.clone()
            } else {
                scrutinee.location.clone()
            };

            if !option_capture_presence_is_const_decidable(scrutinee, loop_binding_paths) {
                return Err(CompilerDiagnostic::invalid_template_structure(
                    InvalidTemplateStructureReason::TemplateOptionCaptureConstDeferred,
                    location,
                ));
            }

            collect_option_capture_binding_path(pattern, &mut branch_binding_paths);
        }
    }

    Ok(branch_binding_paths)
}

#[allow(clippy::result_large_err)]
fn validate_const_required_loop(
    template_loop: &TemplateLoopControlFlow,
    inherited_loop_binding_paths: &[InternedPath],
) -> Result<(), CompilerDiagnostic> {
    match &template_loop.header {
        TemplateLoopHeader::Conditional { condition } => {
            validate_const_required_conditional_loop_condition(condition, template_loop)?;
        }

        TemplateLoopHeader::Range { range, .. } => {
            if !range.start.is_compile_time_constant()
                || !range.end.is_compile_time_constant()
                || range
                    .step
                    .as_ref()
                    .is_some_and(|step| !step.is_compile_time_constant())
            {
                return Err(CompilerDiagnostic::invalid_template_structure(
                    InvalidTemplateStructureReason::TemplateLoopRangeBoundsNotConst,
                    template_loop.location.clone(),
                ));
            }
        }

        TemplateLoopHeader::Collection { iterable, .. } => {
            if !iterable.is_compile_time_constant() {
                return Err(CompilerDiagnostic::invalid_template_structure(
                    InvalidTemplateStructureReason::TemplateLoopSourceNotConst,
                    iterable.location.clone(),
                ));
            }
        }
    }

    let loop_body_binding_paths =
        template_loop.body_const_evaluation_bindings(inherited_loop_binding_paths);

    if !content_is_const_evaluable_with_bindings(
        &template_loop.body_content,
        &loop_body_binding_paths,
    ) {
        return Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::TemplateLoopBodyNotConst,
            template_loop.location.clone(),
        ));
    }

    validate_const_required_content_control_flow(
        &template_loop.body_content,
        &loop_body_binding_paths,
    )
}

fn validate_const_required_conditional_loop_condition(
    condition: &Expression,
    template_loop: &TemplateLoopControlFlow,
) -> Result<(), CompilerDiagnostic> {
    match &condition.kind {
        ExpressionKind::Bool(false) => Ok(()),

        ExpressionKind::Bool(true) => Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::TemplateConditionalLoopConstTrue,
            condition_location_or_loop_location(condition, template_loop),
        )),

        ExpressionKind::Coerced { value, .. } => {
            validate_const_required_conditional_loop_condition(value, template_loop)
        }

        _ => Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::TemplateLoopConditionNotConst,
            condition_location_or_loop_location(condition, template_loop),
        )),
    }
}

fn condition_location_or_loop_location(
    condition: &Expression,
    template_loop: &TemplateLoopControlFlow,
) -> SourceLocation {
    if condition.location == SourceLocation::default() {
        template_loop.location.clone()
    } else {
        condition.location.clone()
    }
}

#[allow(clippy::result_large_err)]
fn validate_const_required_branch_content(
    content: &TemplateContent,
    fallback_location: &SourceLocation,
    loop_binding_paths: &[InternedPath],
) -> Result<(), CompilerDiagnostic> {
    if !content_is_const_evaluable_with_bindings(content, loop_binding_paths) {
        return Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::TemplateIfBranchNotConst,
            fallback_location.clone(),
        ));
    }

    validate_const_required_content_control_flow(content, loop_binding_paths)
}

#[allow(clippy::result_large_err)]
fn validate_const_required_content_control_flow(
    content: &TemplateContent,
    loop_binding_paths: &[InternedPath],
) -> Result<(), CompilerDiagnostic> {
    for atom in &content.atoms {
        let TemplateAtom::Content(segment) = atom else {
            continue;
        };

        let ExpressionKind::Template(child_template) = &segment.expression.kind else {
            continue;
        };

        validate_const_required_template_control_flow_with_bindings(
            child_template,
            &child_template.location,
            loop_binding_paths,
        )?;
    }

    Ok(())
}
