//! Final store-qualified TIR handles.
//!
//! WHAT: `TemplateStoreId`, `TemplateStringDomainId`, `TemplateRef`, and
//! `TemplateNodeRef` identify active production data across the module-local TIR
//! registry. Focused tests also use `TemplateWrapperSetRef` to cover
//! store-qualified wrapper-set invariants. Each handle pairs a store identifier
//! with a store-local typed ID so cross-store references are explicit and the
//! registry can validate them.
//!
//! WHY: store-local IDs (`TemplateIrId`, `TemplateIrNodeId`, ...) are only valid
//! inside one `TemplateIrStore`. The registry owns multiple stores; these
//! qualified handles make store boundaries explicit without exposing registry
//! internals.
//!
//! ## Ownership contract
//!
//! These handles are AST-local. They are not exposed to HIR, backends, or the
//! public API. They remain valid only within the `TemplateIrRegistry` that
//! created them.

use std::fmt;

use super::ids::{TemplateIrId, TemplateIrNodeId, TemplateWrapperSetId};
use super::overlays::TemplateOverlaySetId;
use super::view::TemplateTirPhase;

// -------------------------
//  Template Store ID
// -------------------------

/// Stable index for a `TemplateIrStore` inside a `TemplateIrRegistry`.
///
/// WHAT: identifies one store entry by position in the registry's store vector.
/// WHY: the final TIR system supports multiple stores per module; this ID lets
/// the registry distinguish them without relying on store-owner pointer equality.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TemplateStoreId(u32);

impl TemplateStoreId {
    /// Creates a new ID from a raw index.
    ///
    /// Panics if the index exceeds `u32::MAX`. This is an internal invariant —
    /// no realistic store count will approach this bound.
    pub(crate) fn new(index: usize) -> Self {
        Self(
            u32::try_from(index)
                .expect("template store index exceeds u32::MAX; this is a compiler bug"),
        )
    }

    /// Returns the raw index for registry lookups.
    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}

impl fmt::Display for TemplateStoreId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TemplateStoreId({})", self.0)
    }
}

// -------------------------
//  Template String Domain ID
// -------------------------

/// Identifies a set of stores that share a compatible string table.
///
/// WHAT: groups frozen stores whose interned string identities are mutually
/// resolvable. Stores in the same domain may hold cross-references safely.
/// WHY: string-table merges can remap interned IDs; the domain records when a
/// group of stores has been reconciled to the same string domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TemplateStringDomainId(u32);

impl TemplateStringDomainId {
    /// Creates a new ID from a raw index for focused registry-domain tests.
    ///
    /// Panics if the index exceeds `u32::MAX`. This is an internal invariant.
    #[cfg(test)]
    pub(crate) fn new(index: usize) -> Self {
        Self(
            u32::try_from(index)
                .expect("template string domain index exceeds u32::MAX; this is a compiler bug"),
        )
    }

    /// Returns the raw index for focused registry-domain tests.
    #[cfg(test)]
    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}

impl fmt::Display for TemplateStringDomainId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TemplateStringDomainId({})", self.0)
    }
}

// -------------------------
//  Store-Qualified Template Ref
// -------------------------

/// A `TemplateIrId` qualified by the store that owns it.
///
/// WHAT: references a top-level template entry inside a specific store in the
/// module-local registry.
/// WHY: cross-store template references need both the store and the template
/// index so the registry can resolve and validate them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TemplateRef {
    pub(crate) store_id: TemplateStoreId,
    pub(crate) template_id: TemplateIrId,
}

impl TemplateRef {
    /// Creates a store-qualified template reference.
    pub(crate) fn new(store_id: TemplateStoreId, template_id: TemplateIrId) -> Self {
        Self {
            store_id,
            template_id,
        }
    }
}

impl fmt::Display for TemplateRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TemplateRef({}, {})", self.store_id, self.template_id)
    }
}

// -------------------------
//  Child-Template View Identity
// -------------------------

/// Store-qualified identity for a `ChildTemplate` TIR node.
///
/// WHAT: carries the information a [`TirView`] needs to resolve and fold a nested
///       child template: the store-qualified root, the pipeline phase of that
///       root, and the overlay set that applies to it.
///
/// WHY: a bare `TemplateIrId` is only valid inside one store and says nothing
///      about which phase or overlay context should be used when the child is
///      folded. Threading the full identity on the node makes cross-store
///      folding safe and keeps the eventual production `fold_tir_view` path
///      precise without guessing from the parent view.
///
/// This type is intentionally smaller than [`TemplateTirReference`]. The parser
/// proves same-store before recording a child, while foreign children resolve
/// through the owning registry. A durable `Template` retains the owner token
/// because direct-store consumers may inspect it outside that registry context.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TemplateTirChildReference {
    pub(crate) root: TemplateRef,
    pub(crate) phase: TemplateTirPhase,
    pub(crate) overlay_set_id: TemplateOverlaySetId,
}

impl TemplateTirChildReference {
    /// Creates a child reference from an explicit root, phase, and overlay set.
    pub(crate) fn new(
        root: TemplateRef,
        phase: TemplateTirPhase,
        overlay_set_id: TemplateOverlaySetId,
    ) -> Self {
        Self {
            root,
            phase,
            overlay_set_id,
        }
    }

    /// Creates a same-store child reference with an explicit phase and overlay set.
    ///
    /// WHAT: convenience for construction sites that already know the child
    ///       template lives in the store being built.
    /// WHY: most production paths emit child references into the current store;
    ///      this keeps the common case readable without re-assembling a
    ///      [`TemplateRef`] at every call site.
    pub(crate) fn same_store(
        template_id: TemplateIrId,
        store_id: TemplateStoreId,
        phase: TemplateTirPhase,
        overlay_set_id: TemplateOverlaySetId,
    ) -> Self {
        Self::new(
            TemplateRef::new(store_id, template_id),
            phase,
            overlay_set_id,
        )
    }

    /// Returns the store-local template ID if this reference points to the
    /// given store.
    ///
    /// WHAT: lets same-store consumers (folding, validation, slot composition,
    ///       etc.) recover the `TemplateIrId` they already work with after
    ///       proving the reference belongs to the current store.
    /// WHY: the transition to store-qualified child identity is staged; most
    ///      existing passes are still store-local and should not silently treat
    ///      a cross-store reference as local.
    pub(crate) fn template_id_in_store(&self, store_id: TemplateStoreId) -> Option<TemplateIrId> {
        if self.root.store_id == store_id {
            Some(self.root.template_id)
        } else {
            None
        }
    }
}

// -------------------------
//  Wrapper Reference
// -------------------------

/// Effective identity for a wrapper template in a wrapper set.
///
/// WHAT: carries the store-qualified root, pipeline phase, and overlay-set ID
///       that together identify a wrapper's effective structural context.
/// WHY: a wrapper's effective identity is not only its structural root. A
///      wrapper with the same root but a different phase or overlay context
///      produces different output. Storing all three fields prevents a subtle
///      bug where two wrappers with the same root but different overlay contexts
///      are treated as equivalent.
///
/// This type mirrors [`TemplateTirChildReference`] but is semantically distinct:
/// wrapper references identify inherited `$children(..)` wrappers in a wrapper
/// set, while child references identify nested child-template occurrences in TIR
/// nodes. Keeping them separate avoids confusing wrapper application with child
/// resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TemplateWrapperReference {
    /// Store-qualified root of the wrapper template's TIR tree.
    pub(crate) root: TemplateRef,
    /// Pipeline phase of the wrapper's root (Parsed, Composed, Formatted, ...).
    pub(crate) phase: TemplateTirPhase,
    /// Overlay set that applies to the wrapper's root.
    pub(crate) overlay_set_id: TemplateOverlaySetId,
}

impl TemplateWrapperReference {
    /// Creates a wrapper reference from an explicit root, phase, and overlay set.
    pub(crate) fn new(
        root: TemplateRef,
        phase: TemplateTirPhase,
        overlay_set_id: TemplateOverlaySetId,
    ) -> Self {
        Self {
            root,
            phase,
            overlay_set_id,
        }
    }
}

impl fmt::Display for TemplateWrapperReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TemplateWrapperReference({}, phase={:?}, overlay_set_id={:?})",
            self.root, self.phase, self.overlay_set_id
        )
    }
}

// -------------------------
//  Store-Qualified Node Ref
// -------------------------

/// A `TemplateIrNodeId` qualified by the store that owns it.
///
/// WHAT: references a body-tree node inside a specific store in the registry.
/// WHY: node IDs are store-local; this handle makes the owning store explicit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TemplateNodeRef {
    pub(crate) store_id: TemplateStoreId,
    pub(crate) node_id: TemplateIrNodeId,
}

impl TemplateNodeRef {
    /// Creates a store-qualified node reference.
    pub(crate) fn new(store_id: TemplateStoreId, node_id: TemplateIrNodeId) -> Self {
        Self { store_id, node_id }
    }
}

impl fmt::Display for TemplateNodeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TemplateNodeRef({}, {})", self.store_id, self.node_id)
    }
}

// -------------------------
//  Store-Qualified Wrapper Set Ref
// -------------------------

/// A `TemplateWrapperSetId` qualified by the store that owns it.
///
/// WHAT: references a wrapper-set side-table entry inside a specific store.
/// WHY: wrapper sets are store-local; qualified refs let the registry resolve
/// them in focused registry and overlay tests, and `TirWrapperContext` stores
/// inherited wrapper sets as store-qualified refs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TemplateWrapperSetRef {
    pub(crate) store_id: TemplateStoreId,
    pub(crate) wrapper_set_id: TemplateWrapperSetId,
}

impl TemplateWrapperSetRef {
    /// Creates a store-qualified wrapper-set reference.
    pub(crate) fn new(store_id: TemplateStoreId, wrapper_set_id: TemplateWrapperSetId) -> Self {
        Self {
            store_id,
            wrapper_set_id,
        }
    }
}

#[cfg(test)]
impl fmt::Display for TemplateWrapperSetRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TemplateWrapperSetRef({}, {})",
            self.store_id, self.wrapper_set_id
        )
    }
}
