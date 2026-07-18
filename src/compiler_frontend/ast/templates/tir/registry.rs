//! Module-local TIR registry.
//!
//! WHAT: `TemplateIrRegistry` owns every `TemplateIrStore` used during AST
//! template processing for one module. It allocates stores, tracks their freeze
//! state, and provides store-qualified lookups.
//!
//! WHY: the final TIR system allows multiple stores per module (parser stores,
//! imported-const stores, per-phase derived stores). A registry keeps store
//! identity explicit, validates cross-store references, and manages the string
//! domain boundaries that let stores share template/node refs safely.
//!
//! ## Ownership contract
//!
//! The registry is AST-local. It is created during AST template construction,
//! used through finalization, and dropped before the AST stage returns. It is
//! not exposed to HIR, backends, or the public API.
//!
//! Each store is held as `Rc<RefCell<TemplateIrStore>>` so AST contexts
//! (`AstPhaseContext`, `ScopeContext`) can keep a direct shared handle to the
//! primary store allocated by the registry. The registry remains the single
//! owner of store identity, freeze state, and cross-store validation.

use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;

use crate::compiler_frontend::arena::FrontendArenaCapacityEstimate;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::compiler_errors::CompilerError;

use super::ids::ExpressionSiteId;
use super::node::{TemplateIr, TemplateIrNode};
use super::overlays::{
    TemplateOverlaySet, TemplateOverlaySetId, TirExpressionOverlay, TirExpressionOverlayId,
    TirSlotResolutionOverlay, TirSlotResolutionOverlayId, TirWrapperContextOverlay,
    TirWrapperContextOverlayId,
};
use super::refs::{TemplateNodeRef, TemplateRef, TemplateStoreId};
use super::store::{TemplateIrStore, TemplateStoreState};

#[cfg(test)]
use super::refs::{TemplateStringDomainId, TemplateWrapperSetRef};
#[cfg(test)]
use super::store::TemplateWrapperSet;

// -------------------------
//  Registry
// -------------------------

/// Owns all `TemplateIrStore` values for one module's AST template phase.
///
/// WHAT: stores are created in a `Building` state, mutated by parser/builder
/// consumers, and then frozen into a module-local string domain. The registry
/// validates that cross-store references only cross between compatible frozen
/// stores.
///
/// WHY: a single module may need separate TIR stores for different template
/// sources or construction phases. Centralizing ownership here prevents ad hoc
/// store collections and makes cross-store invariants checkable.
pub(crate) struct TemplateIrRegistry {
    stores: Vec<TemplateIrStoreEntry>,
    // Overlay sets and per-dimension overlay entries are immutable once
    // allocated. They live on the registry (not on individual stores) because
    // overlays describe contextual changes layered over a structural root, and
    // the registry is the single owner of cross-store template identity.
    overlay_sets: Vec<TemplateOverlaySet>,
    expression_overlays: Vec<TirExpressionOverlay>,
    slot_resolution_overlays: Vec<TirSlotResolutionOverlay>,
    wrapper_context_overlays: Vec<TirWrapperContextOverlay>,
    // String-domain counter is exercised only by focused freeze tests. Gated
    // under #[cfg(test)] so the module-level dead-code allowance can be removed
    // without deleting the storage shape or freeze API.
    #[cfg(test)]
    next_string_domain: u32,
}

struct TemplateIrStoreEntry {
    /// Shared store handle. The registry owns the canonical `Rc`; AST contexts
    /// hold clones of the same `Rc` so they can keep a direct `Rc<RefCell<_>>`
    /// handle to the primary store without a separate ownership path.
    store: Rc<RefCell<TemplateIrStore>>,
    state: TemplateStoreState,
}

impl TemplateIrRegistry {
    /// Creates an empty registry.
    pub(crate) fn new() -> Self {
        Self {
            stores: Vec::new(),
            overlay_sets: Vec::new(),
            expression_overlays: Vec::new(),
            slot_resolution_overlays: Vec::new(),
            wrapper_context_overlays: Vec::new(),
            #[cfg(test)]
            next_string_domain: 0,
        }
    }

    /// Allocates an empty registry store for migration-era cross-store fixtures.
    #[cfg(test)]
    pub(crate) fn allocate_store(&mut self) -> TemplateStoreId {
        self.adopt_store(Rc::new(RefCell::new(TemplateIrStore::new())))
    }

    /// Allocates a primary store pre-sized from a module-level capacity estimate.
    ///
    /// WHAT: creates a store with conservative vector capacities derived from the
    ///       module-level `FrontendArenaCapacityEstimate` and registers it as the
    ///       primary store for the AST phase.
    /// WHY: `AstPhaseContext::from_build_context` needs the registry to own the
    ///      capacity-sized primary store so all parser contexts share one owner.
    pub(crate) fn allocate_primary_store_with_capacity(
        &mut self,
        estimate: FrontendArenaCapacityEstimate,
    ) -> TemplateStoreId {
        self.adopt_store(Rc::new(RefCell::new(
            TemplateIrStore::with_capacity_estimate(estimate),
        )))
    }

    /// Adopts an existing shared store handle into the registry.
    ///
    /// WHAT: registers a caller-allocated `Rc<RefCell<TemplateIrStore>>` as a new
    ///       building store and returns its registry-level ID.
    /// WHY: tests and isolated contexts may construct a store directly and then
    ///      need a registry-backed identity so the context does not drift into two
    ///      unrelated ownership paths.
    pub(crate) fn adopt_store(&mut self, store: Rc<RefCell<TemplateIrStore>>) -> TemplateStoreId {
        let store_id = TemplateStoreId::new(self.stores.len());

        // Stamp the store with its registry-assigned ID so store-local APIs can
        // self-qualify `TemplateIrId`s into `TemplateRef`s without callers
        // threading the store ID through every call.
        store.borrow_mut().set_store_id(store_id);

        self.stores.push(TemplateIrStoreEntry {
            store,
            state: TemplateStoreState::Building,
        });

        store_id
    }

    /// Returns the number of stores in the registry.
    ///
    /// WHAT: focused registry tests assert store allocation and adoption counts.
    /// WHY: not used by production callers; kept under `#[cfg(test)]` so the
    ///      module-level dead-code allowance can be removed.
    #[cfg(test)]
    pub(crate) fn store_count(&self) -> usize {
        self.stores.len()
    }

    /// Returns a shared handle to a store.
    ///
    /// WHAT: clones the `Rc<RefCell<TemplateIrStore>>` so the caller can keep a
    ///       direct handle to the store independent of the registry's lifetime.
    /// WHY: AST contexts need a shared store handle they can clone into child
    ///      scopes while the registry retains canonical ownership.
    pub(crate) fn store_handle(
        &self,
        store_id: TemplateStoreId,
    ) -> Option<Rc<RefCell<TemplateIrStore>>> {
        self.stores
            .get(store_id.index())
            .map(|entry| Rc::clone(&entry.store))
    }

    /// Returns an immutable borrow of a store.
    ///
    /// WHAT: borrows the store through its `RefCell` for read-only access.
    /// WHY: callers that only need to inspect store contents (validation,
    ///      finalization reads) can borrow without cloning the `Rc`.
    pub(crate) fn store(&self, store_id: TemplateStoreId) -> Option<Ref<'_, TemplateIrStore>> {
        self.stores
            .get(store_id.index())
            .map(|entry| entry.store.borrow())
    }

    /// Returns a mutable borrow of a building store.
    ///
    /// WHAT: callers may continue to mutate a building store. Mutating a frozen
    /// store is rejected here so finalized registry stores cannot accidentally
    /// receive parser or transform writes after string-domain freeze.
    pub(crate) fn store_mut(
        &self,
        store_id: TemplateStoreId,
    ) -> Result<RefMut<'_, TemplateIrStore>, CompilerError> {
        let entry = self.stores.get(store_id.index()).ok_or_else(|| {
            CompilerError::compiler_error(format!("store_mut: {} does not exist", store_id))
        })?;

        if !matches!(entry.state, TemplateStoreState::Building) {
            return Err(CompilerError::compiler_error(format!(
                "store_mut: {} is not in Building state",
                store_id
            )));
        }

        Ok(entry.store.borrow_mut())
    }

    /// Returns the current lifecycle state of a store.
    ///
    /// WHAT: supports focused freeze/validation tests and the test-only building
    ///       /frozen validators below.
    /// WHY: no production consumer reads store state directly; gated under tests.
    #[cfg(test)]
    pub(crate) fn store_state(&self, store_id: TemplateStoreId) -> Option<TemplateStoreState> {
        self.stores.get(store_id.index()).map(|entry| entry.state)
    }

    /// Freezes a building store into a new module-local string domain.
    ///
    /// WHAT: transitions the store from `Building` to `FrozenModuleLocal` and
    ///       assigns it a fresh `TemplateStringDomainId`.
    /// WHY: focused registry tests keep the final cross-store invariant covered;
    ///      production code currently mutates only building stores.
    #[cfg(test)]
    pub(crate) fn freeze_store(
        &mut self,
        store_id: TemplateStoreId,
    ) -> Result<TemplateStringDomainId, CompilerError> {
        let domain = TemplateStringDomainId::new(self.next_string_domain as usize);

        self.freeze_store_with_domain(store_id, domain)?;

        self.next_string_domain = self.next_string_domain.checked_add(1).ok_or_else(|| {
            CompilerError::compiler_error("freeze_store: string domain counter overflow")
        })?;

        Ok(domain)
    }

    /// Freezes a building store into the provided string domain.
    ///
    /// WHAT: supports tests that need to place multiple stores in the same string
    ///       domain to prove cross-store reference validation.
    /// WHY: no production caller freezes stores with an explicit domain today.
    #[cfg(test)]
    pub(crate) fn freeze_store_with_domain(
        &mut self,
        store_id: TemplateStoreId,
        domain: TemplateStringDomainId,
    ) -> Result<(), CompilerError> {
        let entry = self.stores.get_mut(store_id.index()).ok_or_else(|| {
            CompilerError::compiler_error(format!("freeze_store: {} does not exist", store_id))
        })?;

        if !matches!(entry.state, TemplateStoreState::Building) {
            return Err(CompilerError::compiler_error(format!(
                "freeze_store: {} is already frozen",
                store_id
            )));
        }

        entry.state = TemplateStoreState::FrozenModuleLocal {
            string_domain: domain,
        };

        Ok(())
    }

    /// Validates that two stores may safely hold cross-references.
    ///
    /// WHAT: returns `Ok(())` only when both stores exist and are frozen into the
    ///       same string domain.
    /// WHY: exercised only by focused cross-store validation tests; gated under
    ///      tests while the cross-store invariant remains part of the design.
    #[cfg(test)]
    pub(crate) fn validate_same_domain(
        &self,
        a: TemplateStoreId,
        b: TemplateStoreId,
    ) -> Result<(), CompilerError> {
        if a == b {
            self.store(a).ok_or_else(|| {
                CompilerError::compiler_error(format!("validate_same_domain: {} does not exist", a))
            })?;

            return Ok(());
        }

        let state_a = self.stores.get(a.index()).map(|e| e.state).ok_or_else(|| {
            CompilerError::compiler_error(format!("validate_same_domain: {} does not exist", a))
        })?;
        let state_b = self.stores.get(b.index()).map(|e| e.state).ok_or_else(|| {
            CompilerError::compiler_error(format!("validate_same_domain: {} does not exist", b))
        })?;

        match (state_a, state_b) {
            (TemplateStoreState::Building, _) | (_, TemplateStoreState::Building) => {
                Err(CompilerError::compiler_error(format!(
                    "cross-store reference between {} and {} involves a building store",
                    a, b
                )))
            }
            (
                TemplateStoreState::FrozenModuleLocal {
                    string_domain: domain_a,
                },
                TemplateStoreState::FrozenModuleLocal {
                    string_domain: domain_b,
                },
            ) if domain_a == domain_b => Ok(()),
            _ => Err(CompilerError::compiler_error(format!(
                "cross-store reference between {} and {} has incompatible string domains",
                a, b
            ))),
        }
    }

    /// Validates that a store exists and is still building.
    ///
    /// WHAT: returns `Ok(())` when the store exists and is in `Building`.
    /// WHY: focused tests assert building-state invariants; gated under tests.
    #[cfg(test)]
    pub(crate) fn validate_store_is_building(
        &self,
        store_id: TemplateStoreId,
    ) -> Result<(), CompilerError> {
        let state = self.store_state(store_id).ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "validate_store_is_building: {} does not exist",
                store_id
            ))
        })?;

        if !matches!(state, TemplateStoreState::Building) {
            return Err(CompilerError::compiler_error(format!(
                "validate_store_is_building: {} is not in Building state",
                store_id
            )));
        }

        Ok(())
    }

    /// Validates that a store exists and is frozen.
    ///
    /// WHAT: returns `Ok(())` when the store exists and is in `FrozenModuleLocal`.
    /// WHY: focused tests assert frozen-state invariants; gated under tests.
    #[cfg(test)]
    pub(crate) fn validate_store_is_frozen(
        &self,
        store_id: TemplateStoreId,
    ) -> Result<(), CompilerError> {
        let state = self.store_state(store_id).ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "validate_store_is_frozen: {} does not exist",
                store_id
            ))
        })?;

        if !matches!(state, TemplateStoreState::FrozenModuleLocal { .. }) {
            return Err(CompilerError::compiler_error(format!(
                "validate_store_is_frozen: {} is not frozen",
                store_id
            )));
        }

        Ok(())
    }

    /// Looks up a store-qualified template reference.
    pub(crate) fn template(&self, reference: TemplateRef) -> Option<Ref<'_, TemplateIr>> {
        let store = self.stores.get(reference.store_id.index())?.store.borrow();
        Ref::filter_map(store, |store| store.get_template(reference.template_id)).ok()
    }

    /// Looks up a store-qualified node reference.
    pub(crate) fn node(&self, reference: TemplateNodeRef) -> Option<Ref<'_, TemplateIrNode>> {
        let store = self.stores.get(reference.store_id.index())?.store.borrow();
        Ref::filter_map(store, |store| store.get_node(reference.node_id)).ok()
    }

    /// Looks up a store-qualified wrapper-set reference.
    ///
    /// WHAT: borrows a wrapper set from the store referenced by `reference`.
    /// WHY: no production path resolves wrapper sets through the registry today;
    ///      tests verify store adoption and wrapper-set identity.
    #[cfg(test)]
    pub(crate) fn wrapper_set(
        &self,
        reference: TemplateWrapperSetRef,
    ) -> Option<Ref<'_, TemplateWrapperSet>> {
        let store = self.stores.get(reference.store_id.index())?.store.borrow();
        Ref::filter_map(store, |store| {
            store.get_wrapper_set(reference.wrapper_set_id)
        })
        .ok()
    }

    // -------------------------
    //  Overlay set storage
    // -------------------------

    /// Allocates an overlay entry and returns its registry-level ID.
    ///
    /// WHAT: pushes `overlay` onto the expression overlay table and returns a
    ///       stable `TirExpressionOverlayId`.
    /// WHY: the registry owns durable expression-overlay storage so finalization
    ///      can attach normalized expression payloads without mutating shared
    ///      structural roots.
    pub(crate) fn allocate_expression_overlay(
        &mut self,
        overlay: TirExpressionOverlay,
    ) -> TirExpressionOverlayId {
        let id = TirExpressionOverlayId::new(self.expression_overlays.len());
        self.expression_overlays.push(overlay);
        id
    }

    /// Allocates an overlay entry and returns its registry-level ID.
    ///
    /// WHAT: pushes `overlay` onto the slot resolution overlay table and returns
    ///       a stable `TirSlotResolutionOverlayId`.
    /// WHY: the registry owns durable slot-resolution storage so composition
    ///      can attach routed slot state without mutating shared structural roots.
    pub(crate) fn allocate_slot_resolution_overlay(
        &mut self,
        overlay: TirSlotResolutionOverlay,
    ) -> TirSlotResolutionOverlayId {
        let id = TirSlotResolutionOverlayId::new(self.slot_resolution_overlays.len());
        self.slot_resolution_overlays.push(overlay);
        id
    }

    /// Allocates a wrapper-context overlay entry and returns its registry-level ID.
    ///
    /// WHAT: pushes `overlay` onto the wrapper context overlay table and returns
    ///       a stable `TirWrapperContextOverlayId`.
    /// WHY: the registry owns durable wrapper-context storage so child-template
    ///      occurrence state stays layered over shared structural roots.
    pub(crate) fn allocate_wrapper_context_overlay(
        &mut self,
        overlay: TirWrapperContextOverlay,
    ) -> TirWrapperContextOverlayId {
        let id = TirWrapperContextOverlayId::new(self.wrapper_context_overlays.len());
        self.wrapper_context_overlays.push(overlay);
        id
    }

    /// Looks up an expression overlay entry, or `None` if the ID is unknown.
    pub(crate) fn expression_overlay(
        &self,
        id: TirExpressionOverlayId,
    ) -> Option<&TirExpressionOverlay> {
        self.expression_overlays.get(id.index())
    }

    /// Looks up a slot resolution overlay entry, or `None` if the ID is unknown.
    pub(crate) fn slot_resolution_overlay(
        &self,
        id: TirSlotResolutionOverlayId,
    ) -> Option<&TirSlotResolutionOverlay> {
        self.slot_resolution_overlays.get(id.index())
    }

    /// Looks up a wrapper context overlay entry, or `None` if the ID is unknown.
    ///
    /// WHAT: registry lookup for an allocated wrapper-context overlay.
    /// WHY: `TirView` resolves wrapper-context overlays through the registry so
    ///      view-native folding can apply inherited wrappers at child-template
    ///      occurrence boundaries.
    pub(crate) fn wrapper_context_overlay(
        &self,
        id: TirWrapperContextOverlayId,
    ) -> Option<&TirWrapperContextOverlay> {
        self.wrapper_context_overlays.get(id.index())
    }

    /// Allocates an overlay set, canonicalizing identical sets to one ID.
    ///
    /// WHAT: if an equivalent `TemplateOverlaySet` already exists in the
    ///       registry, returns its existing ID; otherwise stores the set and
    ///       returns a fresh ID.
    /// WHY: equivalent overlay contexts must never be duplicated. Canonicalizing
    ///      here keeps `TemplateTirReference` overlay-set IDs stable across
    ///      structural transforms and lets consumers compare contexts by ID.
    pub(crate) fn allocate_overlay_set(&mut self, set: TemplateOverlaySet) -> TemplateOverlaySetId {
        for (index, existing) in self.overlay_sets.iter().enumerate() {
            if *existing == set {
                return TemplateOverlaySetId::new(index);
            }
        }

        let id = TemplateOverlaySetId::new(self.overlay_sets.len());
        self.overlay_sets.push(set);
        id
    }

    /// Returns an immutable borrow of an overlay set, or `None` if missing.
    ///
    /// WHAT: the canonical borrowed overlay-set lookup used by view and
    ///       composition consumers.
    /// WHY: callers read overlay sets through the registry instead of holding
    ///      their own copies, so overlay identity stays centralized.
    pub(crate) fn overlay_set(&self, id: TemplateOverlaySetId) -> Option<&TemplateOverlaySet> {
        self.overlay_sets.get(id.index())
    }

    /// Resolves an expression site through a root-first overlay stack.
    ///
    /// WHAT: searches each active overlay set in order and returns the first
    ///       expression override that owns `site_id`. Missing overlay sets or
    ///       expression overlays are internal invariant errors.
    /// WHY: final root overlays cover every reachable same-store expression
    ///      site, while nested child references may still carry an earlier
    ///      overlay for sites the root did not replace. Keeping this resolution
    ///      rule on the registry prevents annotation, normalization and handoff
    ///      from implementing subtly different stack semantics.
    pub(crate) fn expression_for_overlay_stack(
        &self,
        overlay_set_ids: &[TemplateOverlaySetId],
        site_id: ExpressionSiteId,
    ) -> Result<Option<&Expression>, CompilerError> {
        for overlay_set_id in overlay_set_ids.iter().copied() {
            let overlay_set = self.overlay_set(overlay_set_id).ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "TIR expression resolution referenced missing overlay set {}",
                    overlay_set_id
                ))
            })?;
            let Some(expression_overlay_id) = overlay_set.expression_overrides else {
                continue;
            };
            let expression_overlay =
                self.expression_overlay(expression_overlay_id)
                    .ok_or_else(|| {
                        CompilerError::compiler_error(format!(
                            "TIR expression resolution referenced missing expression overlay {}",
                            expression_overlay_id
                        ))
                    })?;

            if let Some(expression) = expression_overlay.expression_for_site(site_id) {
                return Ok(Some(expression));
            }
        }

        Ok(None)
    }

    /// Returns the number of canonical overlay sets stored by the registry.
    ///
    /// WHAT: focused overlay tests assert canonicalization and composition counts.
    /// WHY: no production caller needs the count; gated under tests.
    #[cfg(test)]
    pub(crate) fn overlay_set_count(&self) -> usize {
        self.overlay_sets.len()
    }

    /// Composes overlay sets in canonical resolution order.
    ///
    /// WHAT: merges `sets` so that for each overlay dimension the last
    ///       non-`None` value in the given composition order wins. The resulting
    ///       set is allocated/canonicalized and its ID returned. Missing
    ///       overlay-set IDs are rejected as internal compiler errors.
    /// WHY: consumers must not combine overlay maps ad hoc; this helper is the
    ///      single composition path. The per-dimension "last non-`None` wins"
    ///      rule lets later, more contextual overlays replace an earlier entry
    ///      for the same dimension.
    pub(crate) fn compose_overlay_sets(
        &mut self,
        sets: &[TemplateOverlaySetId],
    ) -> Result<TemplateOverlaySetId, CompilerError> {
        let mut expression_overrides = None;
        let mut slot_resolution = None;
        let mut wrapper_context = None;

        for set_id in sets {
            let set = self.overlay_set(*set_id).ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "compose_overlay_sets: {} does not exist",
                    set_id
                ))
            })?;

            // Each dimension is resolved independently. Later non-empty entries
            // replace earlier ones for that dimension while leaving unrelated
            // dimensions intact.
            if set.wrapper_context.is_some() {
                wrapper_context = set.wrapper_context;
            }
            if set.slot_resolution.is_some() {
                slot_resolution = set.slot_resolution;
            }
            if set.expression_overrides.is_some() {
                expression_overrides = set.expression_overrides;
            }
        }

        Ok(self.allocate_overlay_set(TemplateOverlaySet {
            expression_overrides,
            slot_resolution,
            wrapper_context,
        }))
    }
}

impl Default for TemplateIrRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// -------------------------
//  Registered store handle
// -------------------------

/// Couples the module-local TIR registry, a registry-level store ID, and the exact
/// `Rc<RefCell<TemplateIrStore>>` registered at that ID.
///
/// WHAT: wraps the three values that production carriers
///       (`AstPhaseContext`, `ScopeContext`, `ConstantHeaderParseContext`,
///       `TemplateConstructionContext`) previously stored as independent
///       fields. The coupling is established once during construction and the
///       fields are private, so callers cannot assign a store handle that does
///       not match the registry entry at the given ID.
///
/// WHY: the final TIR system lets multiple stores coexist in one module
///      registry. Store-qualified identity, cross-store validation, and parser
///      writes all depend on the direct store handle matching the registry
///      entry. Storing the three values independently allowed callers to
///      reassign one without the others, breaking the invariant silently.
#[derive(Clone)]
pub(crate) struct RegisteredTemplateIrStore {
    registry: Rc<RefCell<TemplateIrRegistry>>,
    store_id: TemplateStoreId,
    store: Rc<RefCell<TemplateIrStore>>,
}

impl RegisteredTemplateIrStore {
    /// Allocates a coupled registry store for migration-era cross-store fixtures.
    #[cfg(test)]
    pub(crate) fn allocate_in(registry: Rc<RefCell<TemplateIrRegistry>>) -> Self {
        let store_id = registry.borrow_mut().allocate_store();
        Self::from_registry_and_store_id(registry, store_id)
            .expect("newly allocated TIR store should remain registered")
    }

    /// Allocate a capacity-sized primary store in the registry and couple it.
    ///
    /// WHAT: calls `allocate_primary_store_with_capacity` on the registry and
    ///       retrieves the matching store handle through a checked lookup.
    /// WHY: `AstPhaseContext::from_build_context` needs a capacity-sized
    ///      primary store; coupling it here prevents the caller from
    ///      assembling the components independently.
    pub(crate) fn allocate_primary_with_capacity(
        registry: Rc<RefCell<TemplateIrRegistry>>,
        estimate: FrontendArenaCapacityEstimate,
    ) -> Self {
        let store_id = registry
            .borrow_mut()
            .allocate_primary_store_with_capacity(estimate);
        Self::from_registry_and_store_id(registry, store_id)
            .expect("newly allocated primary TIR store should remain registered")
    }

    /// Adopt a caller-allocated store into the registry and couple the result.
    ///
    /// WHAT: registers the given store handle through `adopt_store` and couples
    ///       the returned store ID with the same handle.
    /// WHY: tests and isolated contexts construct a store directly and then
    ///      need a registry-backed identity; adoption establishes the
    ///      relationship without a separate lookup.
    #[cfg(test)]
    pub(crate) fn adopt_into(
        registry: Rc<RefCell<TemplateIrRegistry>>,
        store: Rc<RefCell<TemplateIrStore>>,
    ) -> Self {
        let store_id = registry.borrow_mut().adopt_store(Rc::clone(&store));
        Self {
            registry,
            store_id,
            store,
        }
    }

    /// Construct from an existing registry and store ID via a checked lookup.
    ///
    /// WHAT: retrieves the store handle from the registry at `store_id`,
    ///       proving the handle matches the registry entry.
    /// WHY: callers that already hold a registry and store ID (for example,
    ///      test fixtures that allocate a store before wrapping the registry in
    ///      `Rc<RefCell>`) must establish the coupling through a checked lookup
    ///      rather than assembling the components independently.
    pub(crate) fn from_registry_and_store_id(
        registry: Rc<RefCell<TemplateIrRegistry>>,
        store_id: TemplateStoreId,
    ) -> Result<Self, CompilerError> {
        let store = registry.borrow().store_handle(store_id).ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "Registered TIR store context referenced missing store {}",
                store_id
            ))
        })?;

        Ok(Self {
            registry,
            store_id,
            store,
        })
    }

    /// Couple an existing registry and direct store handle after proving the
    /// handle is the registry entry at its stamped store ID.
    ///
    /// WHAT: reads the handle's store ID, resolves that ID through the registry,
    ///       and rejects a different handle even when its numeric ID collides.
    /// WHY: callers that already carry both values must preserve the exact
    ///      association instead of silently substituting a same-ID store from a
    ///      different registry.
    pub(crate) fn from_registry_and_store(
        registry: Rc<RefCell<TemplateIrRegistry>>,
        store: Rc<RefCell<TemplateIrStore>>,
    ) -> Result<Self, CompilerError> {
        let store_id = store.borrow().store_id();
        let registered_store = registry.borrow().store_handle(store_id).ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "Registered TIR store context referenced missing store {}",
                store_id
            ))
        })?;

        if !Rc::ptr_eq(&registered_store, &store) {
            return Err(CompilerError::compiler_error(format!(
                "Registered TIR store context received a foreign handle for store {}",
                store_id
            )));
        }

        Ok(Self {
            registry,
            store_id,
            store,
        })
    }

    /// Returns a reference to the shared module-local registry.
    ///
    /// WHAT: callers that need registry-level operations (overlay allocation,
    ///       cross-store validation, freeze state) borrow the registry through
    ///       this handle.
    pub(crate) fn registry(&self) -> &Rc<RefCell<TemplateIrRegistry>> {
        &self.registry
    }

    /// Returns the registry-level store ID.
    ///
    /// WHAT: store-qualified identity for `TemplateTirReference` construction
    ///       and cross-store reference validation.
    pub(crate) fn store_id(&self) -> TemplateStoreId {
        self.store_id
    }

    /// Returns a reference to the direct shared store handle.
    ///
    /// WHAT: parser recording methods borrow the store through this handle for
    ///       direct mutable access without routing through the registry on
    ///       every write.
    pub(crate) fn store(&self) -> &Rc<RefCell<TemplateIrStore>> {
        &self.store
    }
}
