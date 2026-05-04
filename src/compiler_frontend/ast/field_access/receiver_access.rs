//! Shared receiver-access validation for postfix calls.
//!
//! WHAT: validates whether a receiver call needs `~`, a mutable place, or no mutable marker.
//! WHY: collection builtins and user receiver methods share the same access policy but need
//! caller-specific diagnostic wording.

use super::ReceiverAccessMode;
use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::place_access::{
    ast_node_is_mutable_place, ast_node_is_place, receiver_access_hint,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_rule_error;

pub(super) enum ReceiverAccessDiagnostic<'a> {
    CollectionBuiltin {
        method_name: &'a str,
    },
    ReceiverMethod {
        receiver_type: &'a DataType,
        method_name: &'a str,
    },
}

pub(super) struct ReceiverAccessRequirement<'a> {
    pub requires_mutable: bool,
    pub diagnostic: ReceiverAccessDiagnostic<'a>,
}

// --------------------------
//  Validation entry point
// --------------------------

pub(super) fn validate_receiver_access(
    receiver_node: &AstNode,
    access_mode: ReceiverAccessMode,
    location: &SourceLocation,
    requirement: ReceiverAccessRequirement<'_>,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    if requirement.requires_mutable {
        if !ast_node_is_place(receiver_node) {
            return reject_non_place_receiver(&requirement.diagnostic, location, string_table);
        }

        if !ast_node_is_mutable_place(receiver_node) {
            return reject_immutable_receiver(&requirement.diagnostic, location, string_table);
        }

        if access_mode == ReceiverAccessMode::Shared {
            return reject_missing_mutable_access_marker(
                receiver_node,
                &requirement.diagnostic,
                location,
                string_table,
            );
        }

        return Ok(());
    }

    if access_mode == ReceiverAccessMode::Mutable {
        return reject_unneeded_mutable_access_marker(
            &requirement.diagnostic,
            location,
            string_table,
        );
    }

    Ok(())
}

// --------------------------
//  Rejection helpers
// --------------------------

fn reject_non_place_receiver(
    diagnostic: &ReceiverAccessDiagnostic<'_>,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    match diagnostic {
        ReceiverAccessDiagnostic::CollectionBuiltin { method_name } => {
            return_rule_error!(
                format!(
                    "Collection mutating method '{}(...)' requires a mutable place receiver.",
                    method_name
                ),
                location.to_owned(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Call this method on a mutable variable or mutable field path, not on a temporary expression",
                }
            );
        }
        ReceiverAccessDiagnostic::ReceiverMethod {
            receiver_type,
            method_name,
        } => {
            return_rule_error!(
                format!(
                    "Mutable receiver method '{}.{}(...)' requires a mutable place receiver.",
                    receiver_type.display_with_table(string_table),
                    method_name
                ),
                location.to_owned(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Call this mutable method on a mutable variable or mutable field path, not on a temporary expression",
                }
            );
        }
    }
}

fn reject_immutable_receiver(
    diagnostic: &ReceiverAccessDiagnostic<'_>,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    match diagnostic {
        ReceiverAccessDiagnostic::CollectionBuiltin { method_name } => {
            return_rule_error!(
                format!(
                    "Collection mutating method '{}(...)' requires a mutable collection receiver.",
                    method_name
                ),
                location.to_owned(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Use a mutable receiver place for this mutating collection method",
                }
            );
        }
        ReceiverAccessDiagnostic::ReceiverMethod {
            receiver_type,
            method_name,
        } => {
            return_rule_error!(
                format!(
                    "Mutable receiver method '{}.{}(...)' requires a mutable place receiver.",
                    receiver_type.display_with_table(string_table),
                    method_name
                ),
                location.to_owned(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Use a mutable receiver place for this mutable receiver call",
                }
            );
        }
    }
}

fn reject_missing_mutable_access_marker(
    receiver_node: &AstNode,
    diagnostic: &ReceiverAccessDiagnostic<'_>,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    match diagnostic {
        ReceiverAccessDiagnostic::CollectionBuiltin { method_name } => {
            return_rule_error!(
                format!(
                    "Collection mutating method '{}(...)' expects mutable access at the receiver call site. Call this with `~{}`.",
                    method_name,
                    receiver_access_hint(receiver_node, string_table)
                ),
                location.to_owned(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Prefix the receiver with '~' for this mutating collection call",
                }
            );
        }
        ReceiverAccessDiagnostic::ReceiverMethod {
            receiver_type,
            method_name,
        } => {
            return_rule_error!(
                format!(
                    "Mutable receiver method '{}.{}(...)' expects mutable access at the receiver call site. Call this with `~{}`.",
                    receiver_type.display_with_table(string_table),
                    method_name,
                    receiver_access_hint(receiver_node, string_table)
                ),
                location.to_owned(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Prefix the receiver with '~' when calling mutable receiver methods",
                }
            );
        }
    }
}

fn reject_unneeded_mutable_access_marker(
    diagnostic: &ReceiverAccessDiagnostic<'_>,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    match diagnostic {
        ReceiverAccessDiagnostic::CollectionBuiltin { method_name } => {
            return_rule_error!(
                format!(
                    "Collection method '{}(...)' does not accept explicit mutable access marker '~'.",
                    method_name
                ),
                location.to_owned(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Remove '~' from this receiver call",
                }
            );
        }
        ReceiverAccessDiagnostic::ReceiverMethod {
            receiver_type,
            method_name,
        } => {
            return_rule_error!(
                format!(
                    "Receiver method '{}.{}(...)' does not accept explicit mutable access marker '~'.",
                    receiver_type.display_with_table(string_table),
                    method_name
                ),
                location.to_owned(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Remove '~' from this receiver call",
                }
            );
        }
    }
}
