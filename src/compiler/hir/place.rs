//! HIR Place model
//!
//! Represents precise memory locations for borrow checking and move analysis.
//! Places are the foundation of Beanstalk's ownership system, providing
//! structured access to memory that can be analyzed by the borrow checker.

use crate::compiler::string_interning::InternedString;

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
    Param(InternedString),
    
    /// Global variable or constant
    Global(InternedString),
}

/// A single projection step in a place access chain
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Projection {
    /// Field access (.field)
    Field(InternedString),
    
    /// Index access ([index])
    Index(IndexKind),
    
    /// Dereference (*)
    Deref,
}

/// Index access patterns for borrow checking analysis
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IndexKind {
    /// Constant index (e.g., arr[3])
    Constant(u32),
    
    /// Dynamic index (e.g., arr[i]) - conservative analysis
    Dynamic,
}

impl Place {
    /// Create a new local place
    pub fn local(name: InternedString) -> Self {
        Self {
            root: PlaceRoot::Local(name),
            projections: Vec::new(),
        }
    }
    
    /// Create a new parameter place
    pub fn param(name: InternedString) -> Self {
        Self {
            root: PlaceRoot::Param(name),
            projections: Vec::new(),
        }
    }
    
    /// Create a new global place
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
    pub fn index(mut self, index: IndexKind) -> Self {
        self.projections.push(Projection::Index(index));
        self
    }
    
    /// Add a dereference projection to this place
    pub fn deref(mut self) -> Self {
        self.projections.push(Projection::Deref);
        self
    }
    
    /// Check if this place overlaps with another place
    /// 
    /// Two places overlap if they share the same root and one projection
    /// list is a prefix of the other. This is used by the borrow checker
    /// to detect conflicting accesses.
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
    fn overlaps_with(&self, other: &IndexKind) -> bool {
        match (self, other) {
            // Same constant indices overlap
            (IndexKind::Constant(a), IndexKind::Constant(b)) => a == b,
            
            // Dynamic indices conservatively overlap with everything
            (IndexKind::Dynamic, _) | (_, IndexKind::Dynamic) => true,
        }
    }
}
