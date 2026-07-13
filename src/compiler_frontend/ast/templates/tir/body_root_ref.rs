//! Final TIR body/root reference.
//!
//! WHAT: `TemplateTirBodyReference` is the canonical store-qualified identity
//! for a control-flow body root or aggregate-wrapper root that can be consumed
//! through the TIR view system. It carries the store-qualified node root, the
//! pipeline phase, the overlay set, the source location, and the store-owner
//! proof needed for same-store mutation checks.
//!
//! WHY: branch/fallback/loop and aggregate-wrapper body roots previously
//! traveled as a raw `TemplateIrNodeId` paired with an owner token. That shape
//! lost registry identity, phase, and overlay context, forcing consumers to
//! either re-derive them or skip the view system. This reference makes body
//! roots first-class view inputs while preserving the same-store proof that
//! keeps store-local mutation safe.
//!
//! ## View consumption
//!
//! `TirView` is currently defined over a top-level `TemplateRef`, not a node
//! reference. Body roots that are themselves top-level templates (parser-emitted
//! body shells) can be viewed through the owning `TemplateRef`; body roots that
//! are nodes installed into an owning `BranchChain`/`Loop` are consumed via
//! `same_store_root` today. The identity fields (`TemplateStoreId`, phase,
//! overlay set, source location) are carried uniformly so later phases can
//! construct the appropriate view without re-deriving context.

use std::sync::Arc;

use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

use super::ids::TemplateIrNodeId;
use super::overlays::TemplateOverlaySetId;
use super::refs::{TemplateNodeRef, TemplateStoreId};
use super::store::{TemplateIrStore, TemplateIrStoreOwner};
use super::view::TemplateTirPhase;

// -------------------------
//  Body/root view reference
// -------------------------

/// Store-qualified identity for a TIR body/root node.
///
/// WHAT: references a `BranchChain`/`Loop` body node or a loop aggregate-wrapper
///       root with the registry context needed to consume it through the TIR
///       view system. The `node_ref` carries the `TemplateStoreId` and
///       `TemplateIrNodeId`; the remaining fields carry phase, overlay context,
///       source location, and the same-store owner proof.
///
/// WHY: a bare `TemplateIrNodeId` is only valid inside one store and carries no
///      pipeline context. Threading the full identity on the reference lets
///      finalization and later view consumers treat body roots as structured
///      inputs instead of ad hoc node IDs.
#[derive(Clone, Debug)]
pub(crate) struct TemplateTirBodyReference {
    /// Store-qualified body-root node.
    pub(crate) node_ref: TemplateNodeRef,

    /// Pipeline phase represented by this body root.
    pub(crate) phase: TemplateTirPhase,

    /// Overlay set that applies when the body is consumed as a view.
    pub(crate) overlay_set_id: TemplateOverlaySetId,

    /// Source location for diagnostics pointing at the body.
    pub(crate) location: SourceLocation,

    /// Owner token proving this reference belongs to a specific `TemplateIrStore`
    /// instance.
    ///
    /// WHY: `TemplateStoreId` is assigned when a store is adopted by the
    ///      registry. Construction sites that run before adoption (or that hold
    ///      a direct store handle) keep the owner token so same-store checks
    ///      remain valid even if the numeric store ID is reassigned.
    pub(crate) store_owner: Arc<TemplateIrStoreOwner>,
}

impl TemplateTirBodyReference {
    /// Creates a body/root reference with full view identity.
    pub(crate) fn new(
        store_owner: Arc<TemplateIrStoreOwner>,
        store_id: TemplateStoreId,
        root: TemplateIrNodeId,
        phase: TemplateTirPhase,
        overlay_set_id: TemplateOverlaySetId,
        location: SourceLocation,
    ) -> Self {
        Self {
            node_ref: TemplateNodeRef::new(store_id, root),
            phase,
            overlay_set_id,
            location,
            store_owner,
        }
    }

    /// Convenience for store-local construction that does not yet have a
    /// non-empty overlay set or a finalized location.
    ///
    /// WHAT: builds a reference using the store's current `store_id`, the empty
    ///       overlay set, and a default source location.
    /// WHY: internal construction sites that only need same-store identity
    ///      today can call this without threading empty placeholders through
    ///      every helper.
    #[cfg(test)]
    pub(crate) fn with_store_local_identity(
        store: &TemplateIrStore,
        root: TemplateIrNodeId,
        phase: TemplateTirPhase,
    ) -> Self {
        Self::new(
            store.owner(),
            store.store_id(),
            root,
            phase,
            TemplateOverlaySetId::empty(),
            SourceLocation::default(),
        )
    }

    /// Returns the store-local node ID if this reference points at the given
    /// store instance and registry identity.
    ///
    /// WHAT: lets store-local consumers (finalization, current-state
    ///       materialization, validation, etc.) recover the `TemplateIrNodeId`
    ///      they already work with after proving the reference belongs to the
    ///      current store.
    /// WHY: the transition to store-qualified body identity is staged; existing
    ///      passes are still store-local and should not silently treat a
    ///      cross-store or stale-registry reference as local.
    pub(crate) fn same_store_root(&self, store: &TemplateIrStore) -> Option<TemplateIrNodeId> {
        if Arc::ptr_eq(&self.store_owner, &store.owner())
            && self.node_ref.store_id == store.store_id()
        {
            Some(self.node_ref.node_id)
        } else {
            None
        }
    }

    /// Returns the store-qualified node reference.
    ///
    /// WHY: part of the view/root identity surface; exercised by focused tests
    ///      today and consumed by view construction once body-root views land.
    #[allow(
        dead_code,
        reason = "used by focused body-root identity tests today; will drive view construction in a later slice"
    )]
    pub(crate) fn node_ref(&self) -> TemplateNodeRef {
        self.node_ref
    }

    /// Replaces the pipeline phase on this body reference.
    #[allow(
        dead_code,
        reason = "used by test fixtures that need to advance a hand-built body reference through the pipeline"
    )]
    pub(crate) fn set_phase(&mut self, phase: TemplateTirPhase) {
        self.phase = phase;
    }

    /// Returns the source location for diagnostics pointing at this body root.
    #[allow(
        dead_code,
        reason = "used by focused body-root identity tests today; will drive diagnostics once view consumption lands"
    )]
    pub(crate) fn location(&self) -> &SourceLocation {
        &self.location
    }
}
