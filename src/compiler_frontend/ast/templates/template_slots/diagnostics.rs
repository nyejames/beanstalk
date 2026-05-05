//! Slot composition diagnostics.
//!
//! WHAT: Structured error construction for slot composition failures.
//!
//! WHY: Keeping error messages in one place makes them easier to review for
//! consistency and ensures every composition error path produces an actionable
//! diagnostic.

use crate::compiler_frontend::ast::templates::template::{SlotKey, TemplateContent};
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::return_rule_error;

pub(super) fn extra_loose_content_without_default_slot_error(
    location: &SourceLocation,
) -> Result<TemplateContent, CompilerError> {
    return_rule_error!(
        "This template defines positional '$slot(n)' targets but no default '$slot'. There is more loose content than positional slots available.",
        location.to_owned()
    );
}

pub(super) fn loose_content_without_default_slot_error(
    location: &SourceLocation,
) -> Result<TemplateContent, CompilerError> {
    return_rule_error!(
        "This template defines named '$slot(...)' targets without a default '$slot'. Loose content is not allowed here; use '$insert(\"name\")'.",
        location.to_owned()
    );
}

pub(super) fn unknown_slot_target_error(
    target: &SlotKey,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<TemplateContent, CompilerError> {
    match target {
        SlotKey::Default => {
            return_rule_error!(
                "'$insert' cannot target the default slot because the parent template does not define '$slot'.",
                location.to_owned()
            )
        }
        SlotKey::Named(name) => {
            let slot_name = string_table.resolve(*name);
            return_rule_error!(
                format!(
                    "'$insert(\"{slot_name}\")' targets a named slot that does not exist on the immediate parent template.",
                ),
                location.to_owned()
            )
        }
        SlotKey::Positional(index) => {
            return_rule_error!(
                format!(
                    "'$insert' targets positional slot '{index}' which does not exist on the immediate parent template.",
                ),
                location.to_owned()
            )
        }
    }
}
