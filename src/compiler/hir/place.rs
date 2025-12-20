//! HIR Place model
//!
//! Represents precise memory locations for borrow checking and move analysis.
//! Places are the foundation of Beanstalk's ownership system, providing
//! structured access to memory that can be analyzed by the borrow checker.

use crate::compiler::borrow_checker::types::BorrowKind;
use crate::compiler::string_interning::{InternedString, StringTable};
use std::fmt::{Display, Formatter, Result as FmtResult};

/// A Place represents a precise logical memory location
///
/// Places are used by the borrow checker to track ownership, borrowing,
/// and lifetimes. They provide a structured way to represent memory
/// access patterns that can be analyzed for conflicts.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Place {
    pub root: PlaceRoot,
    pub projections: Vec<Projection>,
}

/// Root of a place (without projections)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PlaceRoot {
    /// Local variable (stack-allocated)
    Local(InternedString),

    /// Function parameter
    #[allow(dead_code)]
    Param(InternedString),

    /// Global variable or constant
    #[allow(dead_code)]
    Global(InternedString),
}

/// A single projection step in a place access chain
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Projection {
    /// Field access (.field)
    Field(InternedString),

    /// Index access ([index])
    #[allow(dead_code)]
    Index(IndexKind),

    /// Dereference (*)
    #[allow(dead_code)]
    Deref,
}

/// Index access patterns for borrow checking analysis
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IndexKind {
    /// Constant index (e.g., arr[3])
    #[allow(dead_code)]
    Constant(u32),

    /// Dynamic index (e.g., arr[i]) - conservative analysis
    #[allow(dead_code)]
    Dynamic,
}

impl Place {
    /// Display place with resolved string IDs for debugging
    pub fn display_with_table(&self, string_table: &StringTable) -> String {
        let mut result = self.root.display_with_table(string_table);
        for projection in &self.projections {
            result.push_str(&projection.display_with_table(string_table));
        }
        result
    }

    /// Create a new local place
    pub fn local(name: InternedString) -> Self {
        Self {
            root: PlaceRoot::Local(name),
            projections: Vec::new(),
        }
    }

    /// Create a new parameter place
    #[allow(dead_code)]
    pub fn param(name: InternedString) -> Self {
        Self {
            root: PlaceRoot::Param(name),
            projections: Vec::new(),
        }
    }

    /// Create a new global place
    #[allow(dead_code)]
    pub fn global(name: InternedString) -> Self {
        Self {
            root: PlaceRoot::Global(name),
            projections: Vec::new(),
        }
    }

    /// Add a field projection to this place
    pub fn field(mut self, field: InternedString) -> Self {
        self.projections.push(Projection::Field(field));
        self
    }

    /// Add an index projection to this place
    #[allow(dead_code)]
    pub fn index(mut self, index: IndexKind) -> Self {
        self.projections.push(Projection::Index(index));
        self
    }

    /// Add a dereference projection to this place
    #[allow(dead_code)]
    pub fn deref(mut self) -> Self {
        self.projections.push(Projection::Deref);
        self
    }

    /// Check if this place overlaps with another place
    ///
    /// Two places overlap if they share the same root and one projection
    /// list is a prefix of the other. This is used by the borrow checker
    /// to detect conflicting accesses.
    ///
    /// Examples:
    /// - `x` overlaps with `x.field` (whole-object vs field)
    /// - `x.field` overlaps with `x.field.subfield` (field vs subfield)
    /// - `x.field1` does NOT overlap with `x.field2` (different fields)
    /// - `arr[1]` overlaps with `arr[1].field` (element vs element field)
    /// - `arr[i]` overlaps with `arr[j]` (conservative for dynamic indices)
    pub fn overlaps_with(&self, other: &Place) -> bool {
        // Must have same root
        if self.root != other.root {
            return false;
        }

        // Check prefix relationship: one projection list must be a prefix of the other
        self.is_prefix_of(other) || other.is_prefix_of(self)
    }

    /// Check if this place is a prefix of another place
    ///
    /// A place is a prefix of another if all its projections match the beginning
    /// of the other place's projections. This is used for overlap analysis.
    ///
    /// Examples:
    /// - `x` is a prefix of `x.field`
    /// - `x.field` is a prefix of `x.field.subfield`
    /// - `x.field1` is NOT a prefix of `x.field2`
    pub fn is_prefix_of(&self, other: &Place) -> bool {
        // Must have same root
        if self.root != other.root {
            return false;
        }

        // This place's projections must be a prefix of the other's projections
        if self.projections.len() > other.projections.len() {
            return false;
        }

        // Check that all projections match
        for (i, projection) in self.projections.iter().enumerate() {
            if !projection.overlaps_with(&other.projections[i]) {
                return false;
            }
        }

        true
    }

    /// Check if this place conflicts with another place given borrow kinds
    ///
    /// This method combines overlap analysis with borrow kind rules to determine
    /// if two borrows conflict according to Beanstalk's memory safety rules.
    ///
    /// Conflict rules:
    /// - Shared + Shared: No conflict (multiple readers allowed)
    /// - Shared + Mutable: Conflict (reader/writer conflict)
    /// - Shared + CandidateMove: Conflict (reader/potential writer conflict)
    /// - Mutable + Mutable: Conflict (multiple writers not allowed)
    /// - Mutable + CandidateMove: Conflict (writer/potential writer conflict)
    /// - CandidateMove + CandidateMove: Conflict (multiple potential writers)
    /// - Move + Any: Conflict (moved value cannot be accessed)
    /// - Any + Move: Conflict (cannot access moved value)
    pub fn conflicts_with(
        &self,
        other: &Place,
        self_kind: BorrowKind,
        other_kind: BorrowKind,
    ) -> bool {
        use crate::compiler::borrow_checker::types::BorrowKind;

        // Places must overlap to conflict
        if !self.overlaps_with(other) {
            return false;
        }

        match (self_kind, other_kind) {
            // Shared borrows don't conflict with each other
            (BorrowKind::Shared, BorrowKind::Shared) => false,

            // CandidateMove is treated conservatively as mutable for conflict detection
            // This ensures safety during the refinement phase
            (BorrowKind::CandidateMove, _) | (_, BorrowKind::CandidateMove) => {
                // CandidateMove conflicts with everything except shared-shared
                // (which is already handled above)
                true
            }

            // Move conflicts with everything (moved values cannot be accessed)
            (BorrowKind::Move, _) | (_, BorrowKind::Move) => true,

            // Any other combination conflicts if places overlap
            _ => true,
        }
    }
}

impl Projection {
    /// Check if this projection overlaps with another
    ///
    /// Projections overlap based on their type and content:
    /// - Field accesses overlap only if they access the same field
    /// - Index accesses overlap based on index overlap rules
    /// - Dereferences always overlap (same reference target)
    /// - Different projection types never overlap
    fn overlaps_with(&self, other: &Projection) -> bool {
        match (self, other) {
            // Field accesses overlap only if same field
            (Projection::Field(a), Projection::Field(b)) => a == b,

            // Index accesses use conservative overlap analysis
            (Projection::Index(a), Projection::Index(b)) => a.overlaps_with(b),

            // Dereferences always overlap (same reference target)
            (Projection::Deref, Projection::Deref) => true,

            // Different projection types don't overlap
            _ => false,
        }
    }

    /// Display projection with resolved string IDs for debugging
    pub fn display_with_table(&self, string_table: &StringTable) -> String {
        match self {
            Projection::Field(field) => format!(".{}", string_table.resolve(*field)),
            Projection::Index(index) => format!("[{}]", index),
            Projection::Deref => "*".to_string(),
        }
    }
}

impl IndexKind {
    /// Check if this index overlaps with another
    ///
    /// Index overlap rules for conservative borrow checking:
    /// - Same constant indices always overlap (arr[3] vs arr[3])
    /// - Different constant indices never overlap (arr[1] vs arr[2])
    /// - Dynamic indices conservatively overlap with everything (arr[i] vs arr[j])
    ///   This is conservative because we can't statically determine if i == j
    fn overlaps_with(&self, other: &IndexKind) -> bool {
        match (self, other) {
            // Same constant indices overlap
            (IndexKind::Constant(a), IndexKind::Constant(b)) => a == b,

            // Dynamic indices conservatively overlap with everything
            // This is conservative analysis - we assume they might be equal
            (IndexKind::Dynamic, _) | (_, IndexKind::Dynamic) => true,
        }
    }
}

// === Display Implementations for HIR Debugging ===

impl Display for Place {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        // Note: This Display implementation shows StringID placeholders.
        // Use display_with_table() for debugging with resolved strings.
        write!(f, "{}", self.root)?;
        for projection in &self.projections {
            write!(f, "{}", projection)?;
        }
        Ok(())
    }
}

impl PlaceRoot {
    /// Display place root with resolved string IDs for debugging
    pub fn display_with_table(&self, string_table: &StringTable) -> String {
        match self {
            PlaceRoot::Local(name) => string_table.resolve(*name).to_string(),
            PlaceRoot::Param(name) => format!("param:{}", string_table.resolve(*name)),
            PlaceRoot::Global(name) => format!("global:{}", string_table.resolve(*name)),
        }
    }
}

impl Display for PlaceRoot {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        // Note: This Display implementation shows StringID placeholders.
        // Use display_with_table() for debugging with resolved strings.
        match self {
            PlaceRoot::Local(name) => write!(f, "{}", name),
            PlaceRoot::Param(name) => write!(f, "param:{}", name),
            PlaceRoot::Global(name) => write!(f, "global:{}", name),
        }
    }
}

impl Display for Projection {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        // Note: This Display implementation shows StringID placeholders.
        // Use display_with_table() for debugging with resolved strings.
        match self {
            Projection::Field(field) => write!(f, ".{}", field),
            Projection::Index(index) => write!(f, "[{}]", index),
            Projection::Deref => write!(f, "*"),
        }
    }
}

impl Display for IndexKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            IndexKind::Constant(index) => write!(f, "{}", index),
            IndexKind::Dynamic => write!(f, "?"),
        }
    }
}
