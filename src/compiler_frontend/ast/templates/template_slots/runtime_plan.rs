//! AST runtime slot application plan.
//!
//! WHAT: Represents a valid slot application whose contributions contain runtime
//! content that cannot be fully expanded at AST time. HIR lowering consumes this
//! plan to allocate per-slot accumulators and append them at wrapper slot sites.
//!
//! WHY: Separating the runtime application model from compile-time expansion lets
//! both paths share one routing implementation while keeping HIR lowering free of
//! source-level slot validation.

use super::composition::{
    RoutedSlotContributions, compose_wrapper_atoms_recursive, expand_slot_placeholder,
    route_slot_contributions,
};
use super::contributions::SlotContributions;
use super::error::TemplateSlotError;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::templates::template::{
    SlotKey, SlotPlaceholder, TemplateAtom, TemplateContent, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_render_plan::TemplateRenderPlan;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_errors::SourceLocation;
use crate::compiler_frontend::symbols::string_interning::{StringIdRemap, StringTable};

// -------------------------
//  Slot Resolution Outcome
// -------------------------

/// Result of resolving a slot application against a wrapper template.
///
/// WHAT:
/// - `Composed`: fully static expansion where every placeholder was replaced.
/// - `Runtime`: the application is valid but contains runtime-producing content,
///   so HIR lowering must handle it via accumulator locals.
///
/// WHY: A single reusable routing path should feed both compile-time expansion
/// and runtime planning without duplicating target validation or loose routing.
#[derive(Debug)]
pub(crate) enum SlotResolutionOutcome {
    Composed(TemplateContent),
    Runtime(RuntimeSlotApplicationPlan),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::compiler_frontend::ast::templates) enum SlotResolutionMode {
    AllowRuntimePlans,
    ComposeOnly,
}

impl SlotResolutionMode {
    fn allows_runtime_plans(self) -> bool {
        matches!(self, Self::AllowRuntimePlans)
    }
}

// -------------------------
//  Runtime Slot Plan
// -------------------------

/// AST handoff object for a runtime slot application.
///
/// WHAT: Carries the wrapper's render plan (still containing slot placeholders)
/// plus the already-routed contributions for each slot key.
///
/// WHY: HIR lowering needs both pieces together so it can:
/// 1. allocate one accumulator per slot key,
/// 2. lower each contribution into its slot accumulator,
/// 3. lower the wrapper plan while appending slot accumulators at placeholders.
#[derive(Clone, Debug)]
pub(crate) struct RuntimeSlotApplicationPlan {
    /// The wrapper template's render plan, still containing unresolved slot placeholders.
    pub(crate) wrapper_plan: TemplateRenderPlan,

    /// Routed contributions matched to each slot key in the wrapper schema.
    pub(crate) contribution_plan: RuntimeSlotContributionPlan,

    /// Source location for diagnostics and invariant reporting.
    pub(crate) location: SourceLocation,
}

/// Routed contributions for a runtime slot application.
#[derive(Clone, Debug)]
pub(crate) struct RuntimeSlotContributionPlan {
    /// Schema of the wrapper template at application time.
    pub(crate) schema: super::schema::SlotSchema,

    /// One entry per distinct slot key that received contributions.
    pub(crate) contributions: Vec<RuntimeSlotContribution>,
}

/// A single routed contribution to one slot target.
#[derive(Clone, Debug)]
pub(crate) struct RuntimeSlotContribution {
    pub(crate) target: SlotKey,
    pub(crate) content: RuntimeSlotContributionContent,
    pub(crate) location: SourceLocation,
}

/// Classification of a routed contribution's lowerability.
#[derive(Clone, Debug)]
pub(crate) enum RuntimeSlotContributionContent {
    /// Static contribution that can be appended as a pre-built string.
    Static(TemplateContent),

    /// Runtime contribution that requires ordinary HIR template lowering.
    Runtime(TemplateRenderPlan),
}

// -------------------------
//  Resolution Entry Point
// -------------------------

/// Resolves a slot application, returning either a composed result or a runtime plan.
///
/// WHAT:
/// - Reuses `route_slot_contributions` for schema discovery, insert extraction,
///   loose grouping, and target validation.
/// - For fully static applications, expands slot placeholders recursively.
/// - For runtime applications, builds a `RuntimeSlotApplicationPlan` instead.
///
/// WHY: One routing path keeps diagnostics and ordering consistent regardless of
/// whether the final outcome is composed at AST time or lowered at runtime.
pub(in crate::compiler_frontend::ast::templates) fn resolve_slot_application(
    wrapper: &crate::compiler_frontend::ast::templates::template_types::Template,
    fill_content: TemplateContent,
    location: &SourceLocation,
    string_table: &StringTable,
    resolution_mode: SlotResolutionMode,
) -> Result<SlotResolutionOutcome, TemplateSlotError> {
    let routed = route_slot_contributions(wrapper, fill_content, location, string_table)?;

    if resolution_mode.allows_runtime_plans() && should_lower_as_runtime(&routed) {
        let wrapper_content = content_prepared_for_runtime_rendering(&wrapper.content);
        let wrapper_plan = TemplateRenderPlan::from_content(&wrapper_content);
        let contributions = build_runtime_contributions(wrapper, &routed, location, string_table)?;

        return Ok(SlotResolutionOutcome::Runtime(RuntimeSlotApplicationPlan {
            wrapper_plan,
            contribution_plan: RuntimeSlotContributionPlan {
                schema: routed.schema,
                contributions,
            },
            location: location.clone(),
        }));
    }

    let atoms = compose_wrapper_atoms_recursive(
        &wrapper.content.atoms,
        &routed.contributions,
        string_table,
        resolution_mode,
    )?;

    Ok(SlotResolutionOutcome::Composed(TemplateContent { atoms }))
}

// -------------------------
//  Runtime Detection
// -------------------------

/// Decides whether a routed slot application should lower at runtime.
///
/// Runtime plans are selected only after normal AST routing has proven the
/// application valid. Const-required callers use `SlotResolutionMode::ComposeOnly`
/// so fold-time loop and branch bindings still resolve through the structural
/// composition path instead of becoming HIR work.
fn should_lower_as_runtime(routed: &RoutedSlotContributions) -> bool {
    routed_slot_contributions_contain_runtime_content(routed)
}

pub(super) fn routed_slot_contributions_contain_runtime_content(
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

// -------------------------
//  Contribution Building
// -------------------------

/// Converts routed `SlotContributions` into deterministic `RuntimeSlotContribution`s.
///
/// WHAT:
/// - Iterates slot keys in schema order: default, then positional ascending,
///   then named slots by resolved source spelling.
/// - Classifies each contribution as `Static` or `Runtime`.
/// - Skips empty contributions so HIR allocates every slot accumulator once,
///   then appends only real contribution bodies into those accumulators.
fn build_runtime_contributions(
    wrapper: &Template,
    routed: &RoutedSlotContributions,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<Vec<RuntimeSlotContribution>, TemplateSlotError> {
    let mut contributions = Vec::new();

    if routed.schema.has_default_slot {
        let target = SlotKey::Default;
        let atoms = runtime_contribution_atoms_for_slot(wrapper, routed, &target, string_table)?;
        if !atoms.is_empty() {
            contributions.push(RuntimeSlotContribution {
                target,
                content: classify_contribution_content(atoms),
                location: location.clone(),
            });
        }
    }

    for index in routed.schema.ordered_positional_slots().cloned() {
        let target = SlotKey::Positional(index);
        let atoms = runtime_contribution_atoms_for_slot(wrapper, routed, &target, string_table)?;
        if !atoms.is_empty() {
            contributions.push(RuntimeSlotContribution {
                target,
                content: classify_contribution_content(atoms),
                location: location.clone(),
            });
        }
    }

    for name in routed.schema.ordered_named_slots(string_table) {
        let target = SlotKey::Named(name);
        let atoms = runtime_contribution_atoms_for_slot(wrapper, routed, &target, string_table)?;
        if !atoms.is_empty() {
            contributions.push(RuntimeSlotContribution {
                target,
                content: classify_contribution_content(atoms),
                location: location.clone(),
            });
        }
    }

    Ok(contributions)
}

/// Classifies contribution atoms as static or runtime based on const evaluability.
fn classify_contribution_content(atoms: Vec<TemplateAtom>) -> RuntimeSlotContributionContent {
    let content = content_prepared_for_runtime_rendering(&TemplateContent { atoms });

    if content.is_const_evaluable_value() {
        RuntimeSlotContributionContent::Static(content)
    } else {
        RuntimeSlotContributionContent::Runtime(TemplateRenderPlan::from_content(&content))
    }
}

fn runtime_contribution_atoms_for_slot(
    wrapper: &Template,
    routed: &RoutedSlotContributions,
    target: &SlotKey,
    string_table: &StringTable,
) -> Result<Vec<TemplateAtom>, TemplateSlotError> {
    let source_atoms = routed.contributions.atoms_for_slot(target);
    if source_atoms.is_empty() {
        return Ok(Vec::new());
    }

    let Some(placeholder) = slot_placeholder_for_key(&wrapper.content.atoms, target) else {
        return Err(CompilerError::compiler_error(
            "Runtime slot plan could not find a placeholder for a routed slot target.",
        )
        .into());
    };

    let contributions = single_slot_contributions(target, source_atoms);
    expand_slot_placeholder(
        placeholder,
        &contributions,
        string_table,
        SlotResolutionMode::AllowRuntimePlans,
    )
}

fn single_slot_contributions(target: &SlotKey, atoms: &[TemplateAtom]) -> SlotContributions {
    let mut contributions = SlotContributions::default();

    match target {
        SlotKey::Default => contributions.extend_default_atoms(atoms.to_vec()),
        SlotKey::Named(name) => contributions.extend_named_atoms(*name, atoms.to_vec()),
        SlotKey::Positional(index) => contributions.extend_positional_atoms(*index, atoms.to_vec()),
    }

    contributions
}

fn slot_placeholder_for_key<'a>(
    atoms: &'a [TemplateAtom],
    target: &SlotKey,
) -> Option<&'a SlotPlaceholder> {
    for atom in atoms {
        match atom {
            TemplateAtom::Slot(placeholder) if &placeholder.key == target => {
                return Some(placeholder);
            }

            TemplateAtom::Slot(_) => {}

            TemplateAtom::Content(segment) => {
                let ExpressionKind::Template(template) = &segment.expression.kind else {
                    continue;
                };

                if let Some(placeholder) = slot_placeholder_for_key(&template.content.atoms, target)
                {
                    return Some(placeholder);
                }
            }
        }
    }

    None
}

fn content_prepared_for_runtime_rendering(content: &TemplateContent) -> TemplateContent {
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

// -------------------------
//  Remap Support
// -------------------------

impl RuntimeSlotApplicationPlan {
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.wrapper_plan.remap_string_ids(remap);
        self.contribution_plan.schema.remap_string_ids(remap);

        for contribution in &mut self.contribution_plan.contributions {
            contribution.target.remap_string_ids(remap);
            contribution.location.remap_string_ids(remap);
            contribution.content.remap_string_ids(remap);
        }

        self.location.remap_string_ids(remap);
    }
}

impl RuntimeSlotContributionContent {
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            Self::Static(content) => content.remap_string_ids(remap),
            Self::Runtime(plan) => plan.remap_string_ids(remap),
        }
    }
}
