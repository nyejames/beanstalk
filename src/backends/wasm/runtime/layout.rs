//! Runtime layout planning metadata.

#[allow(dead_code)] // todo
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeLayoutMode {
    /// Default phase-1 behavior: GC-first semantics.
    GcFirst,
    /// Ownership-aware runtime hooks present but still conservative.
    OwnershipScaffolded,
}
