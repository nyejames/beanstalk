//! Slot contribution bucketing and `$insert` extraction.
//!
//! WHAT:
//! - `SlotContributions` holds the partitioned fill atoms for each slot target.
//! - Loose-atom grouping coalesces whitespace/text around top-level template
//!   contributions so positional slots receive logical chunks, not raw atoms.
//! - `split_fill_atom_for_composition` separates explicit `$insert(...)` helpers
//!   from regular content, recursing into nested templates to collect direct
//!   child inserts.
//!
//! WHY:
//! - Separating bucketing from composition lets each phase own one clear
//!   responsibility: this module decides *which* atoms belong to *which* slot,
//!   while `composition.rs` decides *how* to expand them into the wrapper.

use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::templates::template::{
    SlotKey, TemplateAtom, TemplateContent, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::SourceLocation;
use crate::compiler_frontend::symbols::string_interning::StringId;
use rustc_hash::FxHashMap;

/// Partitioned fill atoms ready for slot expansion.
#[derive(Clone, Debug, Default)]
pub(super) struct SlotContributions {
    default_atoms: Vec<TemplateAtom>,
    named_atoms: FxHashMap<StringId, Vec<TemplateAtom>>,
    positional_atoms: FxHashMap<usize, Vec<TemplateAtom>>,
}

impl SlotContributions {
    pub fn extend_default_atoms(&mut self, atoms: Vec<TemplateAtom>) {
        self.default_atoms.extend(atoms);
    }

    pub fn extend_named_atoms(&mut self, name: StringId, atoms: Vec<TemplateAtom>) {
        self.named_atoms.entry(name).or_default().extend(atoms);
    }

    pub fn extend_positional_atoms(&mut self, index: usize, atoms: Vec<TemplateAtom>) {
        self.positional_atoms
            .entry(index)
            .or_default()
            .extend(atoms);
    }

    pub fn atoms_for_slot(&self, key: &SlotKey) -> &[TemplateAtom] {
        match key {
            SlotKey::Default => &self.default_atoms,
            SlotKey::Named(name) => self
                .named_atoms
                .get(name)
                .map(|v| v.as_slice())
                .unwrap_or(&[]),
            SlotKey::Positional(index) => self
                .positional_atoms
                .get(index)
                .map(|v| v.as_slice())
                .unwrap_or(&[]),
        }
    }
}

/// An explicit `$insert(...)` contribution extracted from fill content.
#[derive(Clone, Debug)]
pub(super) struct SlotInsertContribution {
    pub target: SlotKey,
    pub atoms: Vec<TemplateAtom>,
    pub location: SourceLocation,
}

// ----------------------------------------------------------------------------
// Loose contribution grouping
// ----------------------------------------------------------------------------

/// A group of loose atoms that should be treated as one positional contribution.
pub(super) struct LooseContribution {
    pub atoms: Vec<TemplateAtom>,
}

/// Groups loose atoms into logical contribution chunks.
///
/// Top-level template contributions (head-origin atoms, child template outputs,
/// explicit template expressions) each become their own chunk, with any
/// preceding whitespace merged in. This prevents body-level whitespace from
/// consuming positional slot positions.
pub(super) fn collect_loose_contributions(atoms: Vec<TemplateAtom>) -> Vec<LooseContribution> {
    let mut contributions = Vec::new();
    let mut pending_loose_atoms = Vec::new();

    for atom in atoms {
        if is_top_level_template_contribution(&atom) {
            let mut contribution_atoms = std::mem::take(&mut pending_loose_atoms);
            contribution_atoms.push(atom);
            contributions.push(LooseContribution {
                atoms: contribution_atoms,
            });
        } else {
            pending_loose_atoms.push(atom);
        }
    }

    flush_pending_loose_atoms(&mut pending_loose_atoms, &mut contributions);
    contributions
}

fn flush_pending_loose_atoms(
    pending: &mut Vec<TemplateAtom>,
    contributions: &mut Vec<LooseContribution>,
) {
    if pending.is_empty() {
        return;
    }
    contributions.push(LooseContribution {
        atoms: std::mem::take(pending),
    });
}

fn is_top_level_template_contribution(atom: &TemplateAtom) -> bool {
    let TemplateAtom::Content(segment) = atom else {
        return false;
    };

    if segment.origin == TemplateSegmentOrigin::Head {
        return true;
    }

    if segment.is_child_template_output {
        return true;
    }

    matches!(segment.expression.kind, ExpressionKind::Template(_))
}

// ----------------------------------------------------------------------------
// Slot insert extraction
// ----------------------------------------------------------------------------

/// Extracts a `SlotInsertContribution` from an atom if it is a `$insert(...)`
/// helper template.
pub(super) fn slot_insert_from_atom(atom: &TemplateAtom) -> Option<SlotInsertContribution> {
    let TemplateAtom::Content(segment) = atom else {
        return None;
    };
    let ExpressionKind::Template(template) = &segment.expression.kind else {
        return None;
    };
    let TemplateType::SlotInsert(target) = &template.kind else {
        return None;
    };
    Some(SlotInsertContribution {
        target: target.to_owned(),
        atoms: template.content.atoms.clone(),
        location: template.location.to_owned(),
    })
}

/// Splits a single fill atom into an optional loose atom and zero or more
/// explicit slot insert contributions.
///
/// For nested templates, this recurses to extract any *direct* child
/// `$insert(...)` atoms while leaving nested ones untouched.
pub(super) fn split_fill_atom_for_composition(
    atom: TemplateAtom,
) -> (Option<TemplateAtom>, Vec<SlotInsertContribution>) {
    let Some(slot_insert) = slot_insert_from_atom(&atom) else {
        let TemplateAtom::Content(mut segment) = atom else {
            return (Some(atom), Vec::new());
        };

        // Move the nested template out without cloning. The temporary `NoValue`
        // sentinel is always replaced before the segment returns.
        let template =
            match std::mem::replace(&mut segment.expression.kind, ExpressionKind::NoValue) {
                ExpressionKind::Template(template) => template,
                other_kind => {
                    segment.expression.kind = other_kind;
                    return (Some(TemplateAtom::Content(segment)), Vec::new());
                }
            };

        let (sanitized_template, extracted_inserts) =
            collect_direct_slot_insert_contributions(*template);
        if extracted_inserts.is_empty() {
            segment.expression.kind = ExpressionKind::Template(Box::new(sanitized_template));
            return (Some(TemplateAtom::Content(segment)), extracted_inserts);
        }

        if sanitized_template.content.is_empty() {
            return (None, extracted_inserts);
        }

        segment.expression.kind = ExpressionKind::Template(Box::new(sanitized_template));
        return (Some(TemplateAtom::Content(segment)), extracted_inserts);
    };

    (None, vec![slot_insert])
}

/// Extracts only direct-child `$insert(...)` atoms from a template, leaving
/// nested descendants untouched so they cannot bypass immediate-parent slot
/// scoping.
pub(super) fn collect_direct_slot_insert_contributions(
    mut template: Template,
) -> (Template, Vec<SlotInsertContribution>) {
    let mut sanitized_atoms = Vec::with_capacity(template.content.atoms.len());
    let mut extracted_inserts = Vec::new();

    for atom in template.content.atoms {
        if let Some(slot_insert) = slot_insert_from_atom(&atom) {
            extracted_inserts.push(slot_insert);
            continue;
        }

        sanitized_atoms.push(atom);
    }

    template.content = TemplateContent {
        atoms: sanitized_atoms,
    };
    if extracted_inserts.is_empty() {
        return (template, extracted_inserts);
    }

    template.resync_composition_metadata();

    (template, extracted_inserts)
}
