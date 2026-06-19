//! Typed identifiers for TIR store entries.
//!
//! WHAT: Newtyped `u32` IDs that index into the `TemplateIrStore` side vectors.
//! Each ID type is bound to one store collection so mixing indices across
//! collections is a compile error.
//!
//! WHY: Raw `usize` indices are easy to confuse. Newtypes prevent accidental
//! cross-collection index misuse and keep the store API self-documenting.
//!
//! ## Ownership contract
//!
//! IDs are AST-local. They are not exposed to HIR, backends, or the public API.
//! IDs remain valid only within the `TemplateIrStore` that created them.

use std::fmt;

// -------------------------
//  Template IR ID
// -------------------------

/// Stable index for a top-level template in `TemplateIrStore::templates`.
///
/// WHAT: identifies one `TemplateIr` entry by position in the store's template vector.
/// WHY: TIR uses IDs instead of borrowed references so the store can own all data
/// contiguously and hand out cheap `Copy` handles to later phases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TemplateIrId(u32);

impl TemplateIrId {
    /// Creates a new ID from a raw index.
    ///
    /// Panics if the index exceeds `u32::MAX`. This is an internal invariant —
    /// no realistic template count will approach this bound.
    pub(crate) fn new(index: usize) -> Self {
        Self(
            u32::try_from(index)
                .expect("template IR index exceeds u32::MAX; this is a compiler bug"),
        )
    }

    /// Returns the raw index for store lookups.
    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}

impl fmt::Display for TemplateIrId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TemplateIrId({})", self.0)
    }
}

// -------------------------
//  Template IR Node ID
// -------------------------

/// Stable index for a node in `TemplateIrStore::nodes`.
///
/// WHAT: identifies one `TemplateIrNode` entry by position in the store's node vector.
/// WHY: nodes form a tree via child IDs; having a dedicated type keeps tree
/// navigation distinct from template-level references.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TemplateIrNodeId(u32);

impl TemplateIrNodeId {
    /// Creates a new ID from a raw index.
    pub(crate) fn new(index: usize) -> Self {
        Self(
            u32::try_from(index)
                .expect("template IR node index exceeds u32::MAX; this is a compiler bug"),
        )
    }

    /// Returns the raw index for store lookups.
    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}

impl fmt::Display for TemplateIrNodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TemplateIrNodeId({})", self.0)
    }
}

// -------------------------
//  Template Wrapper Set ID
// -------------------------

/// Stable index for a wrapper set in `TemplateIrStore::wrapper_sets`.
///
/// WHAT: identifies a reusable set of `$children(..)` wrapper templates.
/// WHY: many templates share the same wrapper combination; deduplicating
/// wrapper sets through an ID avoids redundant storage and clone pressure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TemplateWrapperSetId(u32);

impl TemplateWrapperSetId {
    /// Creates a new ID from a raw index.
    pub(crate) fn new(index: usize) -> Self {
        Self(
            u32::try_from(index)
                .expect("template wrapper set index exceeds u32::MAX; this is a compiler bug"),
        )
    }

    /// Returns the raw index for store lookups.
    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}

impl fmt::Display for TemplateWrapperSetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TemplateWrapperSetId({})", self.0)
    }
}
