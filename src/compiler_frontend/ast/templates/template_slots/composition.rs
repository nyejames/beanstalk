//! Slot composition: expanding wrapper templates with filled contributions.
//!
//! WHAT:
//! - `route_slot_contributions` extracts reusable routing used by both composition
//!   and runtime planning.
//! - `compose_wrapper_atoms_recursive` expands slot placeholders for the compile-time
//!   `Composed` path.
//! - Recursively replaces `SlotPlaceholder` atoms with matched contributions.
//! - Applies `$children(..)` child wrappers to slot contributions where configured.
//!
//! WHY:
//! - Keeps the structural rewrite logic separate from contribution bucketing
//!   and schema discovery, so each submodule has one clear responsibility.
//! - Runtime planning in `runtime_plan/` consumes the same routed data without
//!   duplicating target validation or loose routing.

use super::contributions::{
    SlotContributions, SlotInsertContribution, collect_loose_contributions,
    split_fill_atom_for_composition,
};
use super::diagnostics::{
    extra_loose_content_without_default_slot_error, loose_content_without_default_slot_error,
    unknown_slot_target_error,
};
use super::error::TemplateSlotError;
use super::schema::collect_slot_schema;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::template::{
    SlotKey, SlotPlaceholder, TemplateAtom, TemplateContent, TemplateSegment, TemplateSegmentOrigin,
};
use crate::compiler_frontend::ast::templates::template_composition::apply_inherited_child_templates_to_content;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, InvalidTemplateSlotReason};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::value_mode::ValueMode;

// -------------------------
//  Routed Contributions
// -------------------------

/// Result of routing fill atoms against a wrapper's slot schema.
///
/// WHY: Both compile-time expansion and runtime planning need the same
/// partitioned contributions. Extracting this shape prevents duplication.
pub(super) struct RoutedSlotContributions {
    pub schema: super::schema::SlotSchema,
    pub contributions: SlotContributions,
}

// -------------------------
//  Routing
// -------------------------

/// Routes fill content against a wrapper's slot schema.
///
/// WHAT:
/// - Discovers the wrapper's declared slot targets.
/// - Partitions fill atoms into explicit `$insert(...)` contributions and loose atoms.
/// - Routes loose atoms to positional slots first, then the default slot.
/// - Produces actionable diagnostics for unknown targets or invalid loose content.
///
/// WHY: Shared routing keeps validation and ordering deterministic whether the
/// caller expands at AST time or builds a runtime lowering plan.
pub(super) fn route_slot_contributions(
    wrapper: &Template,
    fill_content: TemplateContent,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<RoutedSlotContributions, TemplateSlotError> {
    let slot_schema = collect_slot_schema(wrapper, location)?;

    if !slot_schema.has_any_slots() {
        return Err(CompilerError::compiler_error(
            "Internal template wrapper state error: expected at least one '$slot' while composing.",
        )
        .into());
    }

    let mut contributions = SlotContributions::default();
    let mut loose_atoms = Vec::new();

    // Walk authored fill content exactly once and bucket each atom either as:
    // 1) one or more explicit `$insert(...)` contributors, or
    // 2) loose content that should flow into positional or default slots.
    for atom in fill_content.atoms {
        let (loose_atom, slot_inserts) = split_fill_atom_for_composition(atom);

        for slot_insert in slot_inserts {
            let SlotInsertContribution {
                target,
                atoms,
                location,
            } = slot_insert;

            if !slot_schema.accepts_target(&target) {
                return Err(unknown_slot_target_error(&target, &location, string_table));
            }

            match target {
                SlotKey::Default => {
                    contributions.extend_default_atoms(atoms);
                }
                SlotKey::Named(name) => {
                    contributions.extend_named_atoms(name, atoms);
                }
                SlotKey::Positional(index) => {
                    contributions.extend_positional_atoms(index, atoms);
                }
            }
        }

        if let Some(loose_atom) = loose_atom {
            loose_atoms.push(loose_atom);
        }
    }

    // Route loose content to positional slots first, then to the default slot.
    let loose_contributions = collect_loose_contributions(loose_atoms);
    let ordered_positional_slots: Vec<usize> =
        slot_schema.ordered_positional_slots().cloned().collect();

    for (contribution_index, contribution) in loose_contributions.into_iter().enumerate() {
        if let Some(slot_index) = ordered_positional_slots.get(contribution_index) {
            contributions.extend_positional_atoms(*slot_index, contribution.atoms);
            continue;
        }

        if slot_schema.has_default_slot {
            contributions.extend_default_atoms(contribution.atoms);
            continue;
        }

        if !slot_schema.positional_slots.is_empty() {
            return Err(extra_loose_content_without_default_slot_error(location));
        }

        return Err(loose_content_without_default_slot_error(location));
    }

    Ok(RoutedSlotContributions {
        schema: slot_schema,
        contributions,
    })
}

// -------------------------
//  Recursive Expansion
// -------------------------

/// Recursively replaces slot placeholders in wrapper atoms with routed contributions.
///
/// WHY: Kept as a separate reusable helper so `runtime_plan/` can call it for
/// the `Composed` path without duplicating the recursive walk.
pub(super) fn compose_wrapper_atoms_recursive(
    wrapper_atoms: &[TemplateAtom],
    contributions: &SlotContributions,
    string_table: &StringTable,
    resolution_mode: super::runtime_plan::SlotResolutionMode,
) -> Result<Vec<TemplateAtom>, TemplateSlotError> {
    let mut composed = Vec::with_capacity(wrapper_atoms.len());

    for atom in wrapper_atoms {
        match atom {
            TemplateAtom::Slot(slot) => {
                // Slot replacement is intentionally non-consuming, so duplicate named
                // slot declarations replay the same aggregated contribution in each place.
                composed.extend(expand_slot_placeholder(
                    slot,
                    contributions,
                    string_table,
                    resolution_mode,
                )?);
            }
            TemplateAtom::Content(segment) => {
                // Nested templates can carry slot definitions too. Recursively resolve
                // them with the same contribution buckets so authored hierarchy is kept.
                if let ExpressionKind::Template(template) = &segment.expression.kind
                    && template.has_unresolved_slots()
                {
                    let mut nested_template = template.as_ref().clone_for_composition();
                    nested_template.content = TemplateContent {
                        atoms: compose_wrapper_atoms_recursive(
                            &nested_template.content.atoms,
                            contributions,
                            string_table,
                            resolution_mode,
                        )?,
                    };
                    nested_template.resync_composition_metadata();

                    let mut nested_expression = segment.expression.to_owned();
                    nested_expression.kind = ExpressionKind::Template(Box::new(nested_template));
                    composed.push(TemplateAtom::Content(TemplateSegment::new(
                        nested_expression,
                        segment.origin,
                    )));
                    continue;
                }

                composed.push(atom.to_owned());
            }
        }
    }

    Ok(composed)
}

pub(super) fn expand_slot_placeholder(
    placeholder: &SlotPlaceholder,
    contributions: &SlotContributions,
    string_table: &StringTable,
    resolution_mode: super::runtime_plan::SlotResolutionMode,
) -> Result<Vec<TemplateAtom>, TemplateSlotError> {
    let slot_atoms = contributions.atoms_for_slot(&placeholder.key);

    if placeholder.applied_child_wrappers.is_empty() && placeholder.child_wrappers.is_empty() {
        return Ok(slot_atoms.to_owned());
    }

    let mut expanded = Vec::with_capacity(slot_atoms.len());

    for source_atom in slot_atoms {
        let wrapped_atom = if placeholder.child_wrappers.is_empty()
            || contribution_skips_parent_child_wrappers(source_atom)
        {
            source_atom.clone()
        } else if contribution_is_child_template_output(source_atom)
            || contribution_template_ref(source_atom).is_some()
        {
            // `$children(..)` applies to this direct slot contribution as a whole.
            // It must not descend into the contribution and wrap grandchildren.
            wrap_child_slot_contribution(
                source_atom,
                &placeholder.child_wrappers,
                string_table,
                resolution_mode,
            )?
        } else {
            source_atom.clone()
        };

        if !placeholder.skip_parent_child_wrappers
            && !placeholder.applied_child_wrappers.is_empty()
            && is_child_slot_contribution(&wrapped_atom)
        {
            expanded.push(wrap_child_slot_contribution(
                &wrapped_atom,
                &placeholder.applied_child_wrappers,
                string_table,
                resolution_mode,
            )?);
        } else {
            expanded.push(wrapped_atom);
        }
    }

    Ok(expanded)
}

fn is_child_slot_contribution(atom: &TemplateAtom) -> bool {
    let TemplateAtom::Content(segment) = atom else {
        return false;
    };

    segment.is_child_template_output
        || matches!(segment.expression.kind, ExpressionKind::Template(_))
}

fn wrap_child_slot_contribution(
    atom: &TemplateAtom,
    child_wrappers: &[Template],
    string_table: &StringTable,
    resolution_mode: super::runtime_plan::SlotResolutionMode,
) -> Result<TemplateAtom, TemplateSlotError> {
    let wrapped_content = apply_inherited_child_templates_to_content(
        TemplateContent {
            atoms: vec![atom.to_owned()],
        },
        child_wrappers,
        string_table,
        resolution_mode,
    )?;

    if let Some(control_flow_atom) = single_control_flow_child_atom(&wrapped_content) {
        return Ok(control_flow_atom);
    }

    let origin = contribution_origin(atom);
    let mut wrapped_template = Template::empty();
    wrapped_template.content = wrapped_content;
    wrapped_template.location = contribution_location(atom);
    wrapped_template.resync_composition_metadata();

    Ok(TemplateAtom::Content(TemplateSegment::new(
        Expression::template(wrapped_template, ValueMode::ImmutableOwned),
        origin,
    )))
}

fn single_control_flow_child_atom(content: &TemplateContent) -> Option<TemplateAtom> {
    let [atom] = content.atoms.as_slice() else {
        return None;
    };

    let TemplateAtom::Content(segment) = atom else {
        return None;
    };

    let ExpressionKind::Template(template) = &segment.expression.kind else {
        return None;
    };

    template.is_control_flow_template().then(|| atom.to_owned())
}

fn contribution_template_ref(atom: &TemplateAtom) -> Option<&Template> {
    let TemplateAtom::Content(segment) = atom else {
        return None;
    };

    if let Some(source_child_template) = &segment.source_child_template {
        return Some(source_child_template.as_ref());
    }

    match &segment.expression.kind {
        ExpressionKind::Template(template) => Some(template.as_ref()),
        _ => None,
    }
}

fn contribution_skips_parent_child_wrappers(atom: &TemplateAtom) -> bool {
    contribution_template_ref(atom)
        .is_some_and(|template| template.style.skip_parent_child_wrappers)
}

/// Returns true when the atom is a child template output that was folded into a
/// string slice at parse time. These must be treated like template contributions
/// for the purpose of applying `$children(..)` wrappers in slot expansion.
fn contribution_is_child_template_output(atom: &TemplateAtom) -> bool {
    let TemplateAtom::Content(segment) = atom else {
        return false;
    };
    segment.is_child_template_output
}

fn contribution_origin(atom: &TemplateAtom) -> TemplateSegmentOrigin {
    match atom {
        TemplateAtom::Content(segment) => segment.origin,
        TemplateAtom::Slot(_) => TemplateSegmentOrigin::Body,
    }
}

fn contribution_location(atom: &TemplateAtom) -> SourceLocation {
    match atom {
        TemplateAtom::Content(segment) => segment.expression.location.to_owned(),
        TemplateAtom::Slot(_) => SourceLocation::default(),
    }
}

/// Validation pass that errors if any `$insert(...)` atoms are still present
/// in a template after composition.
///
/// `$insert(...)` helpers are only valid while filling an immediate parent
/// template. Once composition is complete, any remaining inserts are out of
/// scope and must produce a clear error.
pub(in crate::compiler_frontend::ast::templates) fn ensure_no_slot_insertions_remain(
    template: &Template,
    _string_table: &StringTable,
) -> Result<(), TemplateSlotError> {
    if template.contains_slot_insertions() {
        return Err(CompilerDiagnostic::invalid_template_slot(
            InvalidTemplateSlotReason::InsertOutsideParentSlot,
            None,
            template.location.clone(),
        )
        .into());
    }

    Ok(())
}
