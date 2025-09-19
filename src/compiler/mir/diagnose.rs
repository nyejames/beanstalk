use crate::compiler::compiler_errors::CompileError;
use crate::compiler::mir::mir_nodes::{
    BorrowError, BorrowErrorType, BorrowKind, InvalidationType, Loan, LoanId, MirFunction,
    ProgramPoint,
};
use crate::compiler::mir::place::Place;
use crate::compiler::mir::streamlined_diagnostics::{StreamlinedDiagnostics, generate_borrow_errors_batch};
use crate::compiler::parsers::tokens::TextLocation;
use crate::{return_compiler_error, return_rule_error};
use std::collections::HashMap;

/// Comprehensive error diagnostics for MIR borrow checking
///
/// This module provides user-friendly error messages with source spans,
/// WASM context, loan origin points, and actionable suggestions for fixing
/// borrow violations in the simplified MIR borrow checker.
#[derive(Debug)]
pub struct BorrowDiagnostics {
    /// Mapping from loan IDs to their origin information
    loan_origins: HashMap<LoanId, LoanOrigin>,
    /// Function being analyzed (for context)
    function_name: String,
    /// WASM-specific context information
    wasm_context: WasmDiagnosticContext,
}

/// Origin information for a loan (borrow)
#[derive(Debug, Clone)]
pub struct LoanOrigin {
    /// Program point where the loan was issued
    pub origin_point: ProgramPoint,
    /// Source location of the borrow
    pub location: TextLocation,
    /// The place being borrowed
    pub borrowed_place: Place,
    /// Kind of borrow (shared, mutable, unique)
    pub borrow_kind: BorrowKind,
    /// Human-readable description of the borrow
    pub description: String,
}

/// WASM-specific diagnostic context
#[derive(Debug, Clone)]
pub struct WasmDiagnosticContext {
    /// Function ID in WASM module
    pub function_id: u32,
    /// WASM local indices for places
    pub place_to_wasm_local: HashMap<Place, u32>,
    /// WASM memory layout information
    pub memory_layout_info: MemoryLayoutInfo,
}

/// Memory layout information for WASM diagnostics
#[derive(Debug, Clone)]
pub struct MemoryLayoutInfo {
    /// Linear memory regions and their purposes
    pub memory_regions: Vec<MemoryRegionInfo>,
    /// Stack frame information
    pub stack_frame_size: u32,
    /// Heap allocation information
    pub heap_allocations: Vec<HeapAllocationInfo>,
}

/// Information about a memory region for diagnostics
#[derive(Debug, Clone)]
pub struct MemoryRegionInfo {
    /// Start offset in linear memory
    pub start_offset: u32,
    /// Size in bytes
    pub size: u32,
    /// Human-readable description
    pub description: String,
}

/// Information about a heap allocation for diagnostics
#[derive(Debug, Clone)]
pub struct HeapAllocationInfo {
    /// Allocation ID
    pub alloc_id: u32,
    /// Size in bytes
    pub size: u32,
    /// Offset in linear memory
    pub offset: u32,
    /// Type description
    pub type_description: String,
}

/// Diagnostic result with user-friendly error message
#[derive(Debug)]
pub struct DiagnosticResult {
    /// Main error message
    pub message: String,
    /// Primary source location
    pub primary_location: TextLocation,
    /// Additional notes and context
    pub notes: Vec<DiagnosticNote>,
    /// Suggested fixes
    pub suggestions: Vec<DiagnosticSuggestion>,
    /// WASM-specific context
    pub wasm_context: Option<String>,
}

/// Additional diagnostic note
#[derive(Debug, Clone)]
pub struct DiagnosticNote {
    /// Note message
    pub message: String,
    /// Associated source location (optional)
    pub location: Option<TextLocation>,
    /// Note type for formatting
    pub note_type: DiagnosticNoteType,
}

/// Types of diagnostic notes
#[derive(Debug, Clone, PartialEq)]
pub enum DiagnosticNoteType {
    /// Information about where something was defined/borrowed
    Origin,
    /// Explanation of why something is invalid
    Explanation,
    /// WASM-specific technical details
    WasmTechnical,
    /// Memory layout information
    MemoryLayout,
}

/// Suggested fix for a borrow checking error
#[derive(Debug, Clone)]
pub struct DiagnosticSuggestion {
    /// Description of the suggested fix
    pub description: String,
    /// Code example (optional)
    pub code_example: Option<String>,
    /// Confidence level of the suggestion
    pub confidence: SuggestionConfidence,
}

/// Confidence level for diagnostic suggestions
#[derive(Debug, Clone, PartialEq)]
pub enum SuggestionConfidence {
    /// High confidence - this will likely fix the issue
    High,
    /// Medium confidence - this might fix the issue
    Medium,
    /// Low confidence - this is just a general suggestion
    Low,
}

impl BorrowDiagnostics {
    /// Create a new borrow diagnostics instance
    pub fn new(function_name: String) -> Self {
        Self {
            loan_origins: HashMap::new(),
            function_name,
            wasm_context: WasmDiagnosticContext {
                function_id: 0,
                place_to_wasm_local: HashMap::new(),
                memory_layout_info: MemoryLayoutInfo {
                    memory_regions: Vec::new(),
                    stack_frame_size: 0,
                    heap_allocations: Vec::new(),
                },
            },
        }
    }



    /// Add program point to source location mapping (deprecated - for tests only)
    pub fn add_program_point_location(&mut self, _point: ProgramPoint, _location: TextLocation) {
        // This method is now deprecated since source locations are stored directly in MirFunction
        // Use function.store_source_location(point, location) instead
    }

    /// Add loan origin information
    pub fn add_loan_origin(&mut self, loan: &Loan, location: TextLocation, description: String) {
        let origin = LoanOrigin {
            origin_point: loan.origin_stmt,
            location,
            borrowed_place: loan.owner.clone(),
            borrow_kind: loan.kind.clone(),
            description,
        };
        self.loan_origins.insert(loan.id, origin);
    }

    /// Set WASM diagnostic context
    pub fn set_wasm_context(&mut self, context: WasmDiagnosticContext) {
        self.wasm_context = context;
    }

    /// Generate comprehensive diagnostic for a borrow error
    pub fn diagnose_borrow_error(
        &self,
        function: &MirFunction,
        error: &BorrowError,
    ) -> Result<DiagnosticResult, CompileError> {
        match &error.error_type {
            BorrowErrorType::ConflictingBorrows {
                existing_borrow,
                new_borrow,
                place,
            } => self.diagnose_conflicting_borrows(function, error, existing_borrow, new_borrow, place),
            BorrowErrorType::UseAfterMove { place, move_point } => {
                self.diagnose_use_after_move(function, error, place, *move_point)
            }
            BorrowErrorType::BorrowAcrossOwnerInvalidation {
                borrowed_place,
                owner_place,
                invalidation_point,
                invalidation_type,
            } => self.diagnose_borrow_across_invalidation(
                function,
                error,
                borrowed_place,
                owner_place,
                *invalidation_point,
                invalidation_type,
            ),
        }
    }

    /// Diagnose conflicting borrows error
    fn diagnose_conflicting_borrows(
        &self,
        function: &MirFunction,
        error: &BorrowError,
        existing_borrow: &BorrowKind,
        new_borrow: &BorrowKind,
        place: &Place,
    ) -> Result<DiagnosticResult, CompileError> {
        let primary_location = self.get_location_for_program_point(function, &error.point)?;
        let place_name = self.format_place_name(place);

        let message = match (existing_borrow, new_borrow) {
            (BorrowKind::Shared, BorrowKind::Mut) => {
                format!(
                    "Cannot borrow `{}` as mutable because it is already borrowed as immutable",
                    place_name
                )
            }
            (BorrowKind::Mut, BorrowKind::Shared) => {
                format!(
                    "Cannot borrow `{}` as immutable because it is already borrowed as mutable",
                    place_name
                )
            }
            (BorrowKind::Mut, BorrowKind::Mut) => {
                format!(
                    "Cannot borrow `{}` as mutable more than once at a time",
                    place_name
                )
            }
            (BorrowKind::Unique, _) | (_, BorrowKind::Unique) => {
                format!(
                    "Cannot borrow `{}` because it has been moved or uniquely borrowed",
                    place_name
                )
            }
            _ => {
                format!("Conflicting borrows of `{}` detected", place_name)
            }
        };

        let mut notes = Vec::new();
        let mut suggestions = Vec::new();

        // Add note about existing borrow if we can find its origin
        if let Some(existing_loan) = self.find_loan_for_place_and_kind(place, existing_borrow) {
            if let Some(origin) = self.loan_origins.get(&existing_loan) {
                notes.push(DiagnosticNote {
                    message: format!(
                        "{} borrow of `{}` starts here",
                        borrow_kind_description(&origin.borrow_kind),
                        self.format_place_name(&origin.borrowed_place)
                    ),
                    location: Some(origin.location.clone()),
                    note_type: DiagnosticNoteType::Origin,
                });
            }
        }

        // Add WASM context explanation
        let wasm_context = self.generate_wasm_context_for_place(place);
        if !wasm_context.is_empty() {
            notes.push(DiagnosticNote {
                message: wasm_context,
                location: None,
                note_type: DiagnosticNoteType::WasmTechnical,
            });
        }

        // Add suggestions based on borrow types
        match (existing_borrow, new_borrow) {
            (BorrowKind::Shared, BorrowKind::Mut) => {
                suggestions.push(DiagnosticSuggestion {
                    description: "Consider using the existing immutable borrow instead of creating a mutable one".to_string(),
                    code_example: Some(format!("-- Use the existing borrow of `{}`", place_name)),
                    confidence: SuggestionConfidence::Medium,
                });

                suggestions.push(DiagnosticSuggestion {
                    description: "Or ensure the immutable borrow is no longer needed before creating the mutable borrow".to_string(),
                    code_example: Some("-- Make sure all uses of the immutable borrow happen before the mutable borrow".to_string()),
                    confidence: SuggestionConfidence::High,
                });
            }
            (BorrowKind::Mut, BorrowKind::Mut) => {
                suggestions.push(DiagnosticSuggestion {
                    description: "Only one mutable borrow is allowed at a time. Use the existing mutable borrow instead".to_string(),
                    code_example: Some(format!("-- Reuse the existing mutable borrow of `{}`", place_name)),
                    confidence: SuggestionConfidence::High,
                });
            }
            _ => {
                suggestions.push(DiagnosticSuggestion {
                    description: "Ensure borrows don't overlap in time by restructuring the code"
                        .to_string(),
                    code_example: None,
                    confidence: SuggestionConfidence::Medium,
                });
            }
        }

        Ok(DiagnosticResult {
            message,
            primary_location,
            notes,
            suggestions,
            wasm_context: Some(self.generate_wasm_memory_explanation(place)),
        })
    }

    /// Diagnose use-after-move error
    fn diagnose_use_after_move(
        &self,
        function: &MirFunction,
        error: &BorrowError,
        place: &Place,
        move_point: ProgramPoint,
    ) -> Result<DiagnosticResult, CompileError> {
        let primary_location = self.get_location_for_program_point(function, &error.point)?;
        let move_location = self.get_location_for_program_point(function, &move_point)?;
        let place_name = self.format_place_name(place);

        let message = format!(
            "Use of moved value `{}`. Value was moved and is no longer accessible",
            place_name
        );

        let mut notes = vec![
            DiagnosticNote {
                message: format!("`{}` was moved here", place_name),
                location: Some(move_location),
                note_type: DiagnosticNoteType::Origin,
            },
            DiagnosticNote {
                message: "In Beanstalk, values are moved when passed to functions or assigned to new variables unless explicitly copied".to_string(),
                location: None,
                note_type: DiagnosticNoteType::Explanation,
            },
        ];

        // Add WASM context
        let wasm_context = self.generate_wasm_context_for_place(place);
        if !wasm_context.is_empty() {
            notes.push(DiagnosticNote {
                message: wasm_context,
                location: None,
                note_type: DiagnosticNoteType::WasmTechnical,
            });
        }

        let suggestions = vec![
            DiagnosticSuggestion {
                description: "If you need to use the value after moving it, consider borrowing instead of moving".to_string(),
                code_example: Some(format!("-- Use &{} instead of moving {}", place_name, place_name)),
                confidence: SuggestionConfidence::High,
            },
            DiagnosticSuggestion {
                description: "Or clone the value before moving if you need multiple copies".to_string(),
                code_example: Some(format!("cloned_value = {}.clone()", place_name)),
                confidence: SuggestionConfidence::Medium,
            },
            DiagnosticSuggestion {
                description: "Consider restructuring your code to avoid using the value after it's moved".to_string(),
                code_example: None,
                confidence: SuggestionConfidence::Low,
            },
        ];

        Ok(DiagnosticResult {
            message,
            primary_location,
            notes,
            suggestions,
            wasm_context: Some(self.generate_wasm_memory_explanation(place)),
        })
    }

    /// Diagnose use-after-drop error
    fn diagnose_use_after_drop(
        &self,
        function: &MirFunction,
        error: &BorrowError,
        place: &Place,
        drop_point: ProgramPoint,
    ) -> Result<DiagnosticResult, CompileError> {
        let primary_location = self.get_location_for_program_point(function, &error.point)?;
        let drop_location = self.get_location_for_program_point(function, &drop_point)?;
        let place_name = self.format_place_name(place);

        let message = format!(
            "Use of dropped value `{}`. Value was dropped and its memory has been freed",
            place_name
        );

        let notes = vec![
            DiagnosticNote {
                message: format!("`{}` was dropped here", place_name),
                location: Some(drop_location),
                note_type: DiagnosticNoteType::Origin,
            },
            DiagnosticNote {
                message:
                    "Dropped values cannot be used because their memory has been freed for safety"
                        .to_string(),
                location: None,
                note_type: DiagnosticNoteType::Explanation,
            },
        ];

        let suggestions = vec![
            DiagnosticSuggestion {
                description: "Ensure all uses of the value happen before it's dropped".to_string(),
                code_example: None,
                confidence: SuggestionConfidence::High,
            },
            DiagnosticSuggestion {
                description:
                    "Consider extending the lifetime of the value by restructuring your code"
                        .to_string(),
                code_example: None,
                confidence: SuggestionConfidence::Medium,
            },
        ];

        Ok(DiagnosticResult {
            message,
            primary_location,
            notes,
            suggestions,
            wasm_context: Some(self.generate_wasm_memory_explanation(place)),
        })
    }

    /// Diagnose borrow across owner invalidation error
    fn diagnose_borrow_across_invalidation(
        &self,
        function: &MirFunction,
        error: &BorrowError,
        borrowed_place: &Place,
        owner_place: &Place,
        invalidation_point: ProgramPoint,
        invalidation_type: &InvalidationType,
    ) -> Result<DiagnosticResult, CompileError> {
        let primary_location = self.get_location_for_program_point(function, &error.point)?;
        let invalidation_location = self.get_location_for_program_point(function, &invalidation_point)?;
        let borrowed_name = self.format_place_name(borrowed_place);
        let owner_name = self.format_place_name(owner_place);

        let (action, explanation) = match invalidation_type {
            InvalidationType::Move => (
                "moved",
                "When the owner is moved, all borrows of it become invalid",
            ),
        };

        let message = format!(
            "Cannot use borrow of `{}` because its owner `{}` was {}",
            borrowed_name, owner_name, action
        );

        let notes = vec![
            DiagnosticNote {
                message: format!("`{}` was {} here", owner_name, action),
                location: Some(invalidation_location),
                note_type: DiagnosticNoteType::Origin,
            },
            DiagnosticNote {
                message: explanation.to_string(),
                location: None,
                note_type: DiagnosticNoteType::Explanation,
            },
        ];

        let suggestions = match invalidation_type {
            InvalidationType::Move => vec![
                DiagnosticSuggestion {
                    description: "Use the borrow before moving the owner".to_string(),
                    code_example: Some(format!(
                        "-- Use `{}` before moving `{}`",
                        borrowed_name, owner_name
                    )),
                    confidence: SuggestionConfidence::High,
                },
                DiagnosticSuggestion {
                    description: "Consider borrowing after the move if possible".to_string(),
                    code_example: None,
                    confidence: SuggestionConfidence::Medium,
                },
            ],
        };

        Ok(DiagnosticResult {
            message,
            primary_location,
            notes,
            suggestions,
            wasm_context: Some(self.generate_wasm_memory_explanation(borrowed_place)),
        })
    }

    /// Convert diagnostic result to CompileError for the error system
    pub fn to_compile_error(&self, diagnostic: &DiagnosticResult) -> CompileError {
        // Build the full error message with notes and suggestions
        let mut full_message = diagnostic.message.clone();

        // Add notes
        for note in &diagnostic.notes {
            full_message.push_str(&format!("\n  note: {}", note.message));
        }

        // Add suggestions
        if !diagnostic.suggestions.is_empty() {
            full_message.push_str("\n  help:");
            for suggestion in &diagnostic.suggestions {
                full_message.push_str(&format!("\n    {}", suggestion.description));
                if let Some(code) = &suggestion.code_example {
                    full_message.push_str(&format!("\n      {}", code));
                }
            }
        }

        // Add WASM context if available
        if let Some(wasm_context) = &diagnostic.wasm_context {
            full_message.push_str(&format!("\n  WASM context: {}", wasm_context));
        }

        CompileError::new_rule_error(full_message, diagnostic.primary_location.clone())
    }

    /// Generate user-friendly error using the error macros
    pub fn generate_user_error(&self, diagnostic: &DiagnosticResult) -> Result<(), CompileError> {
        return_rule_error!(
            diagnostic.primary_location.clone(),
            "{}",
            self.format_diagnostic_message(diagnostic)
        );
    }

    /// Generate compiler bug error for internal issues
    pub fn generate_compiler_error(&self, message: &str) -> Result<(), CompileError> {
        return_compiler_error!("MIR borrow checker internal error: {}", message);
    }

    /// Generate errors using streamlined diagnostics (performance optimized)
    pub fn generate_errors_streamlined(&mut self, errors: &[BorrowError]) -> Vec<CompileError> {
        generate_borrow_errors_batch(&self.function_name, errors)
    }

    // Helper methods

    /// Get source location for a program point
    fn get_location_for_program_point(
        &self,
        function: &MirFunction,
        point: &ProgramPoint,
    ) -> Result<TextLocation, CompileError> {
        function
            .get_source_location(point)
            .cloned()
            .ok_or_else(|| {
                CompileError::new_rule_error(
                    format!("No source location found for program point {}", point),
                    TextLocation::default(),
                )
            })
    }

    /// Format a place name for user display
    fn format_place_name(&self, place: &Place) -> String {
        match place {
            Place::Local { index, .. } => {
                // Try to find a more user-friendly name
                format!("local variable #{}", index)
            }
            Place::Global { index, .. } => {
                format!("global variable #{}", index)
            }
            Place::Memory { offset, .. } => {
                format!("memory location at offset {}", offset.0)
            }
            Place::Projection { base, elem } => {
                use crate::compiler::mir::place::ProjectionElem;
                let base_name = self.format_place_name(base);
                match elem {
                    ProjectionElem::Field { index, .. } => {
                        format!("{}.field_{}", base_name, index)
                    }
                    ProjectionElem::Index { .. } => {
                        format!("{}[index]", base_name)
                    }
                    ProjectionElem::Length => {
                        format!("{}.length", base_name)
                    }
                    ProjectionElem::Data => {
                        format!("{}.data", base_name)
                    }
                    ProjectionElem::Deref => {
                        format!("*{}", base_name)
                    }
                }
            }
        }
    }

    /// Find a loan for a place and borrow kind (helper for diagnostics)
    fn find_loan_for_place_and_kind(&self, place: &Place, kind: &BorrowKind) -> Option<LoanId> {
        // This is a simplified implementation - in practice, we'd need more sophisticated
        // loan tracking to find the exact loan that matches the place and kind
        for (loan_id, origin) in &self.loan_origins {
            if &origin.borrowed_place == place && &origin.borrow_kind == kind {
                return Some(*loan_id);
            }
        }
        None
    }

    /// Generate WASM context information for a place
    fn generate_wasm_context_for_place(&self, place: &Place) -> String {
        match place {
            Place::Local { index, wasm_type } => {
                format!(
                    "This corresponds to WASM local {} of type {:?}",
                    index, wasm_type
                )
            }
            Place::Global { index, wasm_type } => {
                format!(
                    "This corresponds to WASM global {} of type {:?}",
                    index, wasm_type
                )
            }
            Place::Memory { offset, size, .. } => {
                format!(
                    "This corresponds to WASM linear memory at offset {} with size {:?}",
                    offset.0, size
                )
            }
            Place::Projection { base, .. } => {
                format!(
                    "This is a projection from {}",
                    self.generate_wasm_context_for_place(base)
                )
            }
        }
    }

    /// Generate WASM memory model explanation
    fn generate_wasm_memory_explanation(&self, place: &Place) -> String {
        match place {
            Place::Local { .. } => {
                "WASM locals are stored on the execution stack and are automatically managed".to_string()
            }
            Place::Global { .. } => {
                "WASM globals are stored in the global section and persist for the module lifetime".to_string()
            }
            Place::Memory { .. } => {
                "WASM linear memory is a contiguous array of bytes that can be accessed with load/store instructions".to_string()
            }
            Place::Projection { .. } => {
                "Complex data structures in WASM are typically stored in linear memory with computed offsets".to_string()
            }
        }
    }

    /// Format the complete diagnostic message
    fn format_diagnostic_message(&self, diagnostic: &DiagnosticResult) -> String {
        let mut message = diagnostic.message.clone();

        // Add notes
        for note in &diagnostic.notes {
            match note.note_type {
                DiagnosticNoteType::Origin => {
                    message.push_str(&format!("\n  --> {}", note.message));
                }
                DiagnosticNoteType::Explanation => {
                    message.push_str(&format!("\n  = note: {}", note.message));
                }
                DiagnosticNoteType::WasmTechnical => {
                    message.push_str(&format!("\n  = WASM: {}", note.message));
                }
                DiagnosticNoteType::MemoryLayout => {
                    message.push_str(&format!("\n  = memory: {}", note.message));
                }
            }
        }

        // Add suggestions
        if !diagnostic.suggestions.is_empty() {
            message.push_str("\n  = help:");
            for (i, suggestion) in diagnostic.suggestions.iter().enumerate() {
                let confidence_marker = match suggestion.confidence {
                    SuggestionConfidence::High => "💡",
                    SuggestionConfidence::Medium => "💭",
                    SuggestionConfidence::Low => "🤔",
                };

                message.push_str(&format!(
                    "\n    {} {}",
                    confidence_marker, suggestion.description
                ));

                if let Some(code) = &suggestion.code_example {
                    message.push_str(&format!("\n      Example: {}", code));
                }

                // Add separator between suggestions
                if i < diagnostic.suggestions.len() - 1 {
                    message.push_str("\n");
                }
            }
        }

        message
    }
}

/// Helper function to get human-readable borrow kind description
fn borrow_kind_description(kind: &BorrowKind) -> &'static str {
    match kind {
        BorrowKind::Shared => "immutable",
        BorrowKind::Mut => "mutable",
        BorrowKind::Unique => "unique",
    }
}

/// Entry point for generating comprehensive diagnostics from borrow errors
pub fn diagnose_borrow_errors(
    function: &MirFunction,
    errors: &[BorrowError],
    loans: &[Loan],
) -> Result<Vec<DiagnosticResult>, CompileError> {
    let mut diagnostics = BorrowDiagnostics::new(function.name.clone());

    // Add loan origin information
    for loan in loans {
        // In a real implementation, we'd get the actual source location from the loan
        let location = TextLocation::default(); // Placeholder
        let description = format!(
            "{} borrow of {}",
            borrow_kind_description(&loan.kind),
            "variable" // Placeholder - would format the actual place
        );
        diagnostics.add_loan_origin(loan, location, description);
    }

    // Generate diagnostics for each error
    let mut results = Vec::new();
    for error in errors {
        match diagnostics.diagnose_borrow_error(function, error) {
            Ok(diagnostic) => results.push(diagnostic),
            Err(_e) => {
                // If we can't generate a diagnostic, create a basic one
                let basic_diagnostic = DiagnosticResult {
                    message: error.message.clone(),
                    primary_location: error.location.clone(),
                    notes: vec![],
                    suggestions: vec![],
                    wasm_context: None,
                };
                results.push(basic_diagnostic);
            }
        }
    }

    Ok(results)
}

/// Fast entry point for generating streamlined diagnostics (performance optimized)
/// 
/// This function provides a fast path for error generation that bypasses the complex
/// diagnostic generation and uses streamlined error formatting for better performance.
pub fn diagnose_borrow_errors_fast(
    function: &MirFunction,
    errors: &[BorrowError],
) -> Vec<CompileError> {
    generate_borrow_errors_batch(&function.name, errors)
}

/// Convert diagnostic results to compile errors for the error system
pub fn diagnostics_to_compile_errors(
    diagnostics: &BorrowDiagnostics,
    results: &[DiagnosticResult],
) -> Vec<CompileError> {
    results
        .iter()
        .map(|diagnostic| diagnostics.to_compile_error(diagnostic))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::mir::mir_nodes::*;
    use crate::compiler::mir::place::*;

    fn create_test_place() -> Place {
        Place::Local {
            index: 0,
            wasm_type: WasmType::I32,
        }
    }

    fn create_test_loan() -> Loan {
        Loan {
            id: LoanId::new(0),
            owner: create_test_place(),
            kind: BorrowKind::Shared,
            origin_stmt: ProgramPoint::new(0),
        }
    }

    #[test]
    fn test_diagnostics_creation() {
        let diagnostics = BorrowDiagnostics::new("test_function".to_string());
        assert_eq!(diagnostics.function_name, "test_function");
        assert!(diagnostics.loan_origins.is_empty());
    }

    #[test]
    fn test_add_program_point_location() {
        let mut function = MirFunction::new(0, "test".to_string(), vec![], vec![]);
        let point = ProgramPoint::new(0);
        let location = TextLocation::default();

        function.store_source_location(point, location.clone());

        assert_eq!(
            function.get_source_location(&point),
            Some(&location)
        );
    }

    #[test]
    fn test_add_loan_origin() {
        let mut diagnostics = BorrowDiagnostics::new("test".to_string());
        let loan = create_test_loan();
        let location = TextLocation::default();
        let description = "test borrow".to_string();

        diagnostics.add_loan_origin(&loan, location.clone(), description.clone());

        let origin = diagnostics.loan_origins.get(&loan.id).unwrap();
        assert_eq!(origin.location, location);
        assert_eq!(origin.description, description);
        assert_eq!(origin.borrow_kind, BorrowKind::Shared);
    }

    #[test]
    fn test_format_place_name() {
        let diagnostics = BorrowDiagnostics::new("test".to_string());

        let local = Place::Local {
            index: 5,
            wasm_type: WasmType::I32,
        };
        assert_eq!(diagnostics.format_place_name(&local), "local variable #5");

        let global = Place::Global {
            index: 3,
            wasm_type: WasmType::F64,
        };
        assert_eq!(diagnostics.format_place_name(&global), "global variable #3");
    }

    #[test]
    fn test_borrow_kind_description() {
        assert_eq!(borrow_kind_description(&BorrowKind::Shared), "immutable");
        assert_eq!(borrow_kind_description(&BorrowKind::Mut), "mutable");
        assert_eq!(borrow_kind_description(&BorrowKind::Unique), "unique");
    }

    #[test]
    fn test_wasm_context_generation() {
        let diagnostics = BorrowDiagnostics::new("test".to_string());
        let place = Place::Local {
            index: 2,
            wasm_type: WasmType::F32,
        };

        let context = diagnostics.generate_wasm_context_for_place(&place);
        assert!(context.contains("WASM local 2"));
        assert!(context.contains("F32"));
    }

    #[test]
    fn test_diagnostic_result_creation() {
        let result = DiagnosticResult {
            message: "Test error".to_string(),
            primary_location: TextLocation::default(),
            notes: vec![],
            suggestions: vec![],
            wasm_context: None,
        };

        assert_eq!(result.message, "Test error");
        assert!(result.notes.is_empty());
        assert!(result.suggestions.is_empty());
    }

    #[test]
    fn test_diagnostic_note_creation() {
        let note = DiagnosticNote {
            message: "Test note".to_string(),
            location: Some(TextLocation::default()),
            note_type: DiagnosticNoteType::Origin,
        };

        assert_eq!(note.message, "Test note");
        assert_eq!(note.note_type, DiagnosticNoteType::Origin);
        assert!(note.location.is_some());
    }

    #[test]
    fn test_diagnostic_suggestion_creation() {
        let suggestion = DiagnosticSuggestion {
            description: "Try this fix".to_string(),
            code_example: Some("example code".to_string()),
            confidence: SuggestionConfidence::High,
        };

        assert_eq!(suggestion.description, "Try this fix");
        assert_eq!(suggestion.confidence, SuggestionConfidence::High);
        assert!(suggestion.code_example.is_some());
    }

    #[test]
    fn test_conflicting_borrows_error_creation() {
        let mut diagnostics = BorrowDiagnostics::new("test".to_string());
        let point = ProgramPoint::new(0);
        let location = TextLocation::default();
        diagnostics.add_program_point_location(point, location.clone());

        let place = create_test_place();
        let error = BorrowError {
            point,
            error_type: BorrowErrorType::ConflictingBorrows {
                existing_borrow: BorrowKind::Shared,
                new_borrow: BorrowKind::Mut,
                place: place.clone(),
            },
            message: "Test conflict".to_string(),
            location: TextLocation::default(),
        };

        let mut function = MirFunction::new(0, "test".to_string(), vec![], vec![]);
        function.store_source_location(point, location.clone());
        let result = diagnostics.diagnose_borrow_error(&function, &error);
        assert!(result.is_ok());

        let diagnostic = result.unwrap();
        assert!(diagnostic.message.contains("Cannot borrow"));
        assert!(diagnostic.message.contains("mutable"));
        assert!(diagnostic.message.contains("immutable"));
    }

    #[test]
    fn test_use_after_move_error_creation() {
        let mut diagnostics = BorrowDiagnostics::new("test".to_string());
        let point = ProgramPoint::new(1);
        let move_point = ProgramPoint::new(0);
        let location = TextLocation::default();

        diagnostics.add_program_point_location(point, location.clone());
        diagnostics.add_program_point_location(move_point, location.clone());

        let place = create_test_place();
        let error = BorrowError {
            point,
            error_type: BorrowErrorType::UseAfterMove {
                place: place.clone(),
                move_point,
            },
            message: "Test use after move".to_string(),
            location: TextLocation::default(),
        };

        let mut function = MirFunction::new(0, "test".to_string(), vec![], vec![]);
        function.store_source_location(point, location.clone());
        function.store_source_location(move_point, location.clone());
        let result = diagnostics.diagnose_borrow_error(&function, &error);
        assert!(result.is_ok());

        let diagnostic = result.unwrap();
        assert!(diagnostic.message.contains("Use of moved value"));
        assert!(!diagnostic.suggestions.is_empty());
    }

    #[test]
    fn test_entry_point_function() {
        let function = MirFunction::new(0, "test".to_string(), vec![], vec![]);
        let errors = vec![];
        let loans = vec![];

        let result = diagnose_borrow_errors(&function, &errors, &loans);
        assert!(result.is_ok());

        let diagnostics = result.unwrap();
        assert!(diagnostics.is_empty()); // No errors to diagnose
    }
}
