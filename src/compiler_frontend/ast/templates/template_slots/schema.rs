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
use crate::compiler_frontend::ast::templates::template_slots::error::TemplateSlotError;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::SourceLocation;
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, InvalidTemplateSlotReason};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};

use rustc_hash::FxHashSet;
use std::collections::BTreeSet;

// -------------------------
//  Slot Schema
// -------------------------

/// Declared slot targets on a wrapper template.
///
/// Collected recursively so nested template expressions that declare slots are
/// still accounted for in the wrapper's schema.
#[derive(Clone, Debug, Default)]
pub(crate) struct SlotSchema {
    pub(super) has_default_slot: bool,
    pub(super) named_slots: FxHashSet<StringId>,
    pub(super) positional_slots: BTreeSet<usize>,
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

    pub(crate) fn ordered_named_slots(&self, string_table: &StringTable) -> Vec<StringId> {
        let mut names = self.named_slots.iter().copied().collect::<Vec<_>>();

        names.sort_by(|left, right| {
            string_table
                .resolve(*left)
                .cmp(string_table.resolve(*right))
        });

        names
    }

    /// Returns the deterministic runtime allocation order for slot accumulators.
    ///
    /// WHAT: default first, positional slots in numeric order, then named slots
    /// by resolved source spelling.
    /// WHY: HIR lowering needs stable local allocation without revalidating the
    /// slot schema that AST already accepted.
    pub(crate) fn ordered_slot_keys(&self, string_table: &StringTable) -> Vec<SlotKey> {
        let mut keys = Vec::new();

        if self.has_default_slot {
            keys.push(SlotKey::Default);
        }

        for index in self.ordered_positional_slots() {
            keys.push(SlotKey::Positional(*index));
        }

        for name in self.ordered_named_slots(string_table) {
            keys.push(SlotKey::Named(name));
        }

        keys
    }
}

// -------------------------
//  Schema Discovery
// -------------------------

/// Builds a `SlotSchema` by recursively walking the wrapper's content atoms.
pub(super) fn collect_slot_schema(
    wrapper: &Template,
    error_location: &SourceLocation,
) -> Result<SlotSchema, TemplateSlotError> {
    let mut schema = SlotSchema::default();
    collect_slot_schema_atoms(&wrapper.content.atoms, &mut schema, error_location)?;
    Ok(schema)
}

/// Recursively traverses atoms to identify all `$slot` declarations.
fn collect_slot_schema_atoms(
    atoms: &[TemplateAtom],
    schema: &mut SlotSchema,
    error_location: &SourceLocation,
) -> Result<(), TemplateSlotError> {
    // This recursive walk intentionally traverses nested template expressions so a
    // wrapper template can declare slots at any depth while still being resolved in
    // one deterministic pass.
    for atom in atoms {
        match atom {
            TemplateAtom::Slot(slot) => match &slot.key {
                SlotKey::Default => {
                    if schema.has_default_slot {
                        return Err(CompilerDiagnostic::invalid_template_slot(
                            InvalidTemplateSlotReason::MultipleDefaultSlots,
                            None,
                            error_location.to_owned(),
                        )
                        .into());
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
