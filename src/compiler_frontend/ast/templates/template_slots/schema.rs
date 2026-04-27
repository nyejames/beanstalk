//! Slot schema discovery.
//!
//! WHAT: Scans a wrapper template to discover all declared `$slot` targets
//! (default, named, positional) so composition can validate `$insert(...)`
//! contributions and route loose atoms correctly.
//!
//! WHY: Separating schema collection from composition lets both phases stay
//! focused: schema answers "what slots exist?", composition answers "how do
//! contributions map to those slots?".

use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::templates::template::{SlotKey, TemplateAtom};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::return_rule_error;
use rustc_hash::FxHashSet;
use std::collections::BTreeSet;

/// Declared slot targets on a wrapper template.
///
/// Collected recursively so nested template expressions that declare slots are
/// still accounted for in the wrapper's schema.
#[derive(Clone, Debug, Default)]
pub(super) struct SlotSchema {
    pub has_default_slot: bool,
    pub named_slots: FxHashSet<StringId>,
    pub positional_slots: BTreeSet<usize>,
}

impl SlotSchema {
    pub fn has_any_slots(&self) -> bool {
        self.has_default_slot || !self.named_slots.is_empty() || !self.positional_slots.is_empty()
    }

    pub fn accepts_target(&self, target: &SlotKey) -> bool {
        match target {
            SlotKey::Default => self.has_default_slot,
            SlotKey::Named(name) => self.named_slots.contains(name),
            SlotKey::Positional(index) => self.positional_slots.contains(index),
        }
    }

    pub fn ordered_positional_slots(&self) -> impl Iterator<Item = &usize> {
        self.positional_slots.iter()
    }
}

/// Builds a `SlotSchema` by recursively walking the wrapper's content atoms.
pub(super) fn collect_slot_schema(
    wrapper: &Template,
    error_location: &SourceLocation,
) -> Result<SlotSchema, CompilerError> {
    let mut schema = SlotSchema::default();
    collect_slot_schema_atoms(&wrapper.content.atoms, &mut schema, error_location)?;
    Ok(schema)
}

fn collect_slot_schema_atoms(
    atoms: &[TemplateAtom],
    schema: &mut SlotSchema,
    error_location: &SourceLocation,
) -> Result<(), CompilerError> {
    // This recursive walk intentionally traverses nested template expressions so a
    // wrapper template can declare slots at any depth while still being resolved in
    // one deterministic pass.
    for atom in atoms {
        match atom {
            TemplateAtom::Slot(slot) => match &slot.key {
                SlotKey::Default => {
                    if schema.has_default_slot {
                        return_rule_error!(
                            "Templates can only define one default '$slot'.",
                            error_location.to_owned()
                        );
                    }
                    schema.has_default_slot = true;
                }
                SlotKey::Named(name) => {
                    schema.named_slots.insert(*name);
                }
                SlotKey::Positional(index) => {
                    schema.positional_slots.insert(*index);
                }
            },
            TemplateAtom::Content(segment) => {
                if let ExpressionKind::Template(template) = &segment.expression.kind {
                    collect_slot_schema_atoms(&template.content.atoms, schema, error_location)?;
                }
            }
        }
    }

    Ok(())
}
