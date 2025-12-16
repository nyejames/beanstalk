//! HIR Place model
//!
//! Represents precise memory locations for borrow checking and move analysis.
//! Places are the foundation of Beanstalk's ownership system, providing
//! structured access to memory that can be analyzed by the borrow checker.

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
    #[allow(dead_code)]
    pub fn overlaps_with(&self, other: &Place) -> bool {
        // Must have same root
        if self.root != other.root {
            return false;
        }

        // One projection list must be a prefix of the other
        let min_len = self.projections.len().min(other.projections.len());

        for i in 0..min_len {
            if !self.projections[i].overlaps_with(&other.projections[i]) {
                return false;
            }
        }

        true
    }
}

impl Projection {
    /// Check if this projection overlaps with another
    #[allow(dead_code)]
    fn overlaps_with(&self, other: &Projection) -> bool {
        match (self, other) {
            // Field accesses overlap only if same field
            (Projection::Field(a), Projection::Field(b)) => a == b,

            // Index accesses
            (Projection::Index(a), Projection::Index(b)) => a.overlaps_with(b),

            // Dereferences always overlap
            (Projection::Deref, Projection::Deref) => true,

            // Different projection types don't overlap
            _ => false,
        }
    }
}

impl IndexKind {
    /// Check if this index overlaps with another
    #[allow(dead_code)]
    fn overlaps_with(&self, other: &IndexKind) -> bool {
        match (self, other) {
            // Same constant indices overlap
            (IndexKind::Constant(a), IndexKind::Constant(b)) => a == b,

            // Dynamic indices conservatively overlap with everything
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

impl Projection {
    /// Display projection with resolved string IDs for debugging
    pub fn display_with_table(&self, string_table: &StringTable) -> String {
        match self {
            Projection::Field(field) => format!(".{}", string_table.resolve(*field)),
            Projection::Index(index) => format!("[{}]", index),
            Projection::Deref => "*".to_string(),
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
