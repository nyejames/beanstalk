//! Runtime contribution source planning.
//!
//! WHAT: Detects whether routed contributions require runtime lowering and
//! converts routed atoms into deterministic source plans.
//!
//! WHY: Source plans describe authored contribution work that HIR should lower
//! exactly once. Wrapper-local `$children(..)` and `$fresh` behavior belongs to
//! site planning so repeated placeholders can replay the same source safely.

use super::types::{
    RuntimeContributionRenderPlan, RuntimeSlotContributionSource,
    RuntimeSlotContributionSourceDraft, RuntimeSlotContributionSourceId,
};
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::templates::template::{
    SlotKey, TemplateAtom, TemplateContent, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_render_plan::TemplateRenderPlan;
use crate::compiler_frontend::ast::templates::template_slots::composition::RoutedSlotContributions;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::SourceLocation;
use crate::compiler_frontend::symbols::string_interning::StringTable;

/// Decides whether a routed slot application should lower at runtime.
///
/// Runtime plans are selected only after normal AST routing has proven the
/// application valid. Const-required callers use `SlotResolutionMode::ComposeOnly`
/// so fold-time loop and branch bindings still resolve through the structural
/// composition path instead of becoming HIR work.
pub(super) fn should_lower_as_runtime(routed: &RoutedSlotContributions) -> bool {
    routed_slot_contributions_contain_runtime_content(routed)
}

pub(in crate::compiler_frontend::ast::templates::template_slots) fn routed_slot_contributions_contain_runtime_content(
    routed: &RoutedSlotContributions,
) -> bool {
    if routed.schema.has_default_slot
        && contribution_atoms_need_runtime(routed.contributions.atoms_for_slot(&SlotKey::Default))
    {
        return true;
    }

    for index in routed.schema.ordered_positional_slots() {
        let key = SlotKey::Positional(*index);
        if contribution_atoms_need_runtime(routed.contributions.atoms_for_slot(&key)) {
            return true;
        }
    }

    for name in &routed.schema.named_slots {
        let key = SlotKey::Named(*name);
        if contribution_atoms_need_runtime(routed.contributions.atoms_for_slot(&key)) {
            return true;
        }
    }

    false
}

fn contribution_atoms_need_runtime(atoms: &[TemplateAtom]) -> bool {
    if atoms.is_empty() {
        return false;
    }

    let content = TemplateContent {
        atoms: atoms.to_vec(),
    };

    !content.is_const_evaluable_value()
}

/// Converts routed contribution atoms into deterministic source plans.
///
/// WHAT:
/// - Iterates slot keys in schema order: default, then positional ascending,
///   then named slots by resolved source spelling.
/// - Creates one source per routed atom so repeated slot sites can replay the
///   accumulated source without re-lowering expressions.
///
/// WHY: Site plans apply placeholder-local wrappers later. Source plans only
/// describe the authored contribution work that should happen once.
pub(super) fn build_runtime_contribution_sources(
    routed: &RoutedSlotContributions,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Vec<RuntimeSlotContributionSourceDraft> {
    let mut sources = Vec::new();

    for target in routed.schema.ordered_slot_keys(string_table) {
        push_runtime_contribution_sources_for_target(
            &mut sources,
            routed.contributions.atoms_for_slot(&target),
            target,
            location,
        );
    }

    sources
}

fn push_runtime_contribution_sources_for_target(
    sources: &mut Vec<RuntimeSlotContributionSourceDraft>,
    atoms: &[TemplateAtom],
    target: SlotKey,
    location: &SourceLocation,
) {
    for atom in atoms {
        let id = RuntimeSlotContributionSourceId(sources.len());
        let plan = build_contribution_render_plan(atom.clone());

        sources.push(RuntimeSlotContributionSourceDraft {
            source: RuntimeSlotContributionSource {
                id,
                target: target.clone(),
                render_plan: plan.render_plan,
                renders_wrapper_unconditionally: plan.renders_wrapper_unconditionally,
                location: location.clone(),
            },
            atom: atom.clone(),
        });
    }
}

/// Builds the uniform HIR payload and records whether the wrapper is always structural output.
fn build_contribution_render_plan(atom: TemplateAtom) -> RuntimeContributionRenderPlan {
    let content = content_prepared_for_runtime_rendering(&TemplateContent { atoms: vec![atom] });
    let renders_wrapper_unconditionally = content.is_const_evaluable_value();

    RuntimeContributionRenderPlan {
        render_plan: TemplateRenderPlan::from_content(&content),
        renders_wrapper_unconditionally,
    }
}

pub(super) fn content_prepared_for_runtime_rendering(content: &TemplateContent) -> TemplateContent {
    let mut content = content.to_owned();

    for atom in &mut content.atoms {
        prepare_atom_for_runtime_rendering(atom);
    }

    content
}

fn prepare_atom_for_runtime_rendering(atom: &mut TemplateAtom) {
    let TemplateAtom::Content(segment) = atom else {
        return;
    };

    let ExpressionKind::Template(template) = &mut segment.expression.kind else {
        return;
    };

    prepare_template_for_runtime_rendering(template);
}

fn prepare_template_for_runtime_rendering(template: &mut Template) {
    for atom in &mut template.content.atoms {
        prepare_atom_for_runtime_rendering(atom);
    }

    if template.control_flow.is_none() && template.render_plan.is_none() {
        template.resync_runtime_metadata();
    }

    if template.control_flow.is_none() && matches!(template.kind, TemplateType::String) {
        // Runtime slot plans are HIR handoff objects. A nested template that was
        // compile-time renderable in isolation may still sit inside a runtime
        // slot contribution, so AST marks it as a runtime string chunk instead
        // of asking HIR to lower a const template value.
        template.kind = TemplateType::StringFunction;
        if template.render_plan.is_none() {
            template.render_plan = Some(TemplateRenderPlan::from_content(&template.content));
        }
    }
}
