//! # Error Reporting for Borrow Checker
//!
//! Provides detailed error reporting for borrow checking violations with actionable suggestions.
//! Integrates with the existing compiler error system to provide consistent error formatting.

use super::{
    borrow_state::{BorrowConflict, BorrowKind, ConflictType},
    place_registry::{Place, PlaceId, PlaceRegistry},
};
use crate::compiler::compiler_errors::ErrorType::BorrowChecker;
use crate::compiler::compiler_errors::{CompilerError, ErrorMetaDataKey, ErrorType};
use crate::compiler::string_interning::StringTable;
use std::collections::HashMap;

/// Error reporter for borrow checking violations
pub struct BorrowErrorReporter<'a> {
    place_registry: PlaceRegistry,
    string_table: &'a StringTable,
}

impl<'a> BorrowErrorReporter<'a> {
    /// Create a new error reporter
    pub fn new(place_registry: PlaceRegistry, string_table: &'a StringTable) -> Self {
        Self {
            place_registry,
            string_table,
        }
    }

    /// Create a detailed error for a borrow conflict
    pub fn create_borrow_conflict_error(&self, conflict: &BorrowConflict) -> CompilerError {
        match conflict.conflict_type {
            ConflictType::MultipleMutableBorrows => {
                self.create_multiple_mutable_borrows_error(conflict)
            }
            ConflictType::SharedMutableConflict => {
                self.create_shared_mutable_conflict_error(conflict)
            }
            ConflictType::UseAfterMove => self.create_use_after_move_error(conflict),
            ConflictType::MoveWhileBorrowed => self.create_move_while_borrowed_error(conflict),
            ConflictType::WholeObjectBorrow => self.create_whole_object_borrow_error(conflict),
        }
    }

    /// Create error for multiple mutable borrows
    fn create_multiple_mutable_borrows_error(&self, conflict: &BorrowConflict) -> CompilerError {
        let place_name = self.get_place_name(conflict.existing_borrow.place);
        let message = format!(
            "cannot mutably borrow `{}` because it is already mutably borrowed",
            place_name
        );

        let mut metadata = HashMap::new();
        metadata.insert(
            ErrorMetaDataKey::VariableName,
            self.leak_string(place_name.clone()),
        );
        metadata.insert(ErrorMetaDataKey::BorrowKind, "Mutable");
        metadata.insert(
            ErrorMetaDataKey::ConflictingVariable,
            self.leak_string(place_name),
        );
        metadata.insert(ErrorMetaDataKey::CompilationStage, "Borrow Checking");
        metadata.insert(
            ErrorMetaDataKey::PrimarySuggestion,
            "Ensure the first mutable borrow is no longer used before creating the second",
        );
        metadata.insert(
            ErrorMetaDataKey::LifetimeHint,
            "Only one mutable borrow can exist at a time",
        );

        CompilerError {
            msg: message,
            location: conflict
                .attempted_access
                .location
                .to_error_location_without_table(),
            error_type: ErrorType::BorrowChecker,
            metadata,
        }
    }

    /// Create error for shared/mutable borrow conflicts
    fn create_shared_mutable_conflict_error(&self, conflict: &BorrowConflict) -> CompilerError {
        let place_name = self.get_place_name(conflict.existing_borrow.place);

        let (message, suggestion, lifetime_hint) = match conflict.existing_borrow.kind {
            BorrowKind::Shared => (
                format!(
                    "cannot mutably borrow `{}` because it is already referenced",
                    place_name
                ),
                "Ensure all shared references are finished before creating mutable access",
                "Mutable access is exclusive - no other access can exist while active",
            ),
            BorrowKind::Mutable => (
                format!(
                    "cannot reference `{}` because it is already mutably borrowed",
                    place_name
                ),
                "Finish using the mutable borrow before creating shared references",
                "Mutable borrows are exclusive - no other borrows can exist while active",
            ),
            _ => (
                format!("conflicting borrows of `{}`", place_name),
                "Resolve the borrow conflict by restructuring your code",
                "Check the borrow rules for your specific case",
            ),
        };

        let mut metadata = HashMap::new();
        metadata.insert(ErrorMetaDataKey::VariableName, self.leak_string(place_name));
        metadata.insert(ErrorMetaDataKey::CompilationStage, "Borrow Checking");
        metadata.insert(ErrorMetaDataKey::PrimarySuggestion, suggestion);
        metadata.insert(ErrorMetaDataKey::LifetimeHint, lifetime_hint);

        CompilerError {
            msg: message,
            location: conflict
                .attempted_access
                .location
                .to_error_location_without_table(),
            error_type: ErrorType::BorrowChecker,
            metadata,
        }
    }

    /// Create error for use after move
    fn create_use_after_move_error(&self, conflict: &BorrowConflict) -> CompilerError {
        let place_name = self.get_place_name(conflict.existing_borrow.place);
        let message = format!("borrow of moved value: `{}`", place_name);

        let mut metadata = HashMap::new();
        metadata.insert(
            ErrorMetaDataKey::VariableName,
            self.leak_string(place_name.clone()),
        );
        metadata.insert(
            ErrorMetaDataKey::MovedVariable,
            self.leak_string(place_name),
        );
        metadata.insert(ErrorMetaDataKey::CompilationStage, "Borrow Checking");
        metadata.insert(
            ErrorMetaDataKey::PrimarySuggestion,
            "Consider using a reference instead of moving the value",
        );
        metadata.insert(
            ErrorMetaDataKey::AlternativeSuggestion,
            "Clone the value before moving if you need to use it later",
        );
        metadata.insert(
            ErrorMetaDataKey::LifetimeHint,
            "Once a value is moved, ownership transfers and the original variable can no longer be used",
        );

        CompilerError {
            msg: message,
            location: conflict
                .attempted_access
                .location
                .to_error_location_without_table(),
            error_type: ErrorType::BorrowChecker,
            metadata,
        }
    }

    /// Create error for move while borrowed
    fn create_move_while_borrowed_error(&self, conflict: &BorrowConflict) -> CompilerError {
        let place_name = self.get_place_name(conflict.existing_borrow.place);
        let borrow_type = match conflict.existing_borrow.kind {
            BorrowKind::Shared => "referenced",
            BorrowKind::Mutable => "mutably borrowed",
            BorrowKind::CandidateMove => "mutably borrowed",
            BorrowKind::Move => "moved",
        };

        let message = format!(
            "cannot move out of `{}` because it is {}",
            place_name, borrow_type
        );

        let mut metadata = HashMap::new();
        metadata.insert(
            ErrorMetaDataKey::VariableName,
            self.leak_string(place_name.clone()),
        );
        metadata.insert(
            ErrorMetaDataKey::BorrowedVariable,
            self.leak_string(place_name),
        );
        metadata.insert(ErrorMetaDataKey::CompilationStage, "Borrow Checking");
        metadata.insert(
            ErrorMetaDataKey::PrimarySuggestion,
            "Ensure all borrows are finished before moving the value",
        );
        metadata.insert(
            ErrorMetaDataKey::AlternativeSuggestion,
            "Use references instead of moving the value",
        );
        metadata.insert(
            ErrorMetaDataKey::LifetimeHint,
            "Cannot move a value while it has active borrows - the borrows must end first",
        );

        CompilerError {
            msg: message,
            location: conflict
                .attempted_access
                .location
                .to_error_location_without_table(),
            error_type: BorrowChecker,
            metadata,
        }
    }

    /// Create error for whole object borrow violations
    fn create_whole_object_borrow_error(&self, conflict: &BorrowConflict) -> CompilerError {
        let place_name = self.get_place_name(conflict.existing_borrow.place);
        let message = format!(
            "Cannot borrow whole object '{}' while part is already borrowed",
            place_name
        );

        let mut metadata = HashMap::new();
        metadata.insert(
            ErrorMetaDataKey::ConflictingPlace,
            self.leak_string(place_name.clone()),
        );
        metadata.insert(
            ErrorMetaDataKey::ExistingBorrowPlace,
            self.leak_string(place_name),
        );
        metadata.insert(
            ErrorMetaDataKey::ConflictType,
            "WholeObjectBorrowingViolation",
        );
        metadata.insert(
            ErrorMetaDataKey::PrimarySuggestion,
            "Consider using the existing borrow of the part, or end the part borrow first",
        );

        CompilerError {
            msg: message,
            location: conflict
                .attempted_access
                .location
                .to_error_location_without_table(),
            error_type: ErrorType::BorrowChecker,
            metadata,
        }
    }

    /// Get a human-readable name for a place
    fn get_place_name(&self, place_id: PlaceId) -> String {
        if let Some(place) = self.place_registry.get_place(place_id) {
            self.place_to_string(place)
        } else {
            format!("<unknown place {:?}>", place_id)
        }
    }

    /// Convert a place to a human-readable string
    fn place_to_string(&self, place: &Place) -> String {
        match place {
            Place::Variable(name) => self.string_table.resolve(*name).to_string(),
            Place::Field { base, field } => {
                if let Some(base_place) = self.place_registry.get_place(*base) {
                    format!(
                        "{}.{}",
                        self.place_to_string(base_place),
                        self.string_table.resolve(*field)
                    )
                } else {
                    format!("<unknown>.{}", self.string_table.resolve(*field))
                }
            }
            Place::Index { base, index: _ } => {
                if let Some(base_place) = self.place_registry.get_place(*base) {
                    format!("{}[<index>]", self.place_to_string(base_place))
                } else {
                    "<unknown>[<index>]".to_string()
                }
            }
            Place::Unknown => "<unknown>".to_string(),
        }
    }

    /// Leak a string to get a 'static reference (for metadata)
    fn leak_string(&self, s: String) -> &'static str {
        Box::leak(s.into_boxed_str())
    }
}
