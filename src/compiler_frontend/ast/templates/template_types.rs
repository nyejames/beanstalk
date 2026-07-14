//! Template struct and core type definitions.
//!
//! WHAT: Houses the central `Template` struct and its associated identity and
//! classification queries.
//!
//! WHY: Separates the `Template` data type from the parsing/composition logic
//! so other modules can depend on the struct without pulling in the full parser.

use crate::compiler_frontend::ast::templates::template::TemplateType;
use crate::compiler_frontend::ast::templates::tir::{
    TemplateIrRegistry, TemplateIrStore, TemplateTirReference, TirTemplateClassification,
    refresh_kind_from_classification,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use std::sync::Arc;

// -------------------------
//  Template AST Node
// -------------------------

/// The central template representation in the AST.
///
/// A `Template` is a narrow durable handle carrying its TIR identity, a cached
/// kind marker, and source location. Effective style and wrapper context are
/// owned by the `TemplateIr` entry resolved through `tir_reference`. The
/// `Template` is the primary data structure passed between parsing, composition,
/// formatting, folding and HIR preparation.
///
/// ## Metadata lifecycle invariants
///
/// - **`kind`** is a cached boundary marker. Construction initializes it once
///   alongside `TemplateIr.kind`. Post-construction refresh goes through
///   [`Template::synchronize_kind_from_classification`], the single owner that
///   writes both copies so they cannot drift. The cache is read at parser
///   boundaries where a template value may cross from a foreign TIR store whose
///   registry is not available to the receiving context. Callers that already
///   hold the owning store, registry, or `TirView` read the authoritative
///   `TemplateIr.kind` instead.
/// - **`style`** is owned by `TemplateIr` and read through the registry-backed
///   TIR view after construction.
#[derive(Debug)]
pub struct Template {
    /// Cached template-kind boundary marker.
    ///
    /// WHAT: a durable copy of `TemplateIr.kind` that survives crossing into a
    ///      parser context whose registry does not include the template's
    ///      originating TIR store.
    /// WHY: cross-store template-valued head expressions and children-directive
    ///      values reach parser routing before the foreign store can be
    ///      resolved. The cache lets those paths route correctly without
    ///      silently skipping validation. It is not structural authority:
    ///      classification, folding, finalization, and render-unit work read
    ///      `TemplateIr.kind` from the store, registry, or view they already
    ///      hold.
    pub(crate) kind: TemplateType,

    /// Authoritative TIR reference.
    ///
    /// WHAT: holds the store-qualified root, logical store-owner token, pipeline
    ///       phase, and overlay-set ID.
    /// WHY: this is the long-lived reference. The `TemplateRef` makes the owning
    ///      store explicit for registry/view consumers, while the owner token
    ///      distinguishes equal registry-local store IDs from different registries.
    pub(crate) tir_reference: TemplateTirReference,

    pub location: SourceLocation,
}

impl Clone for Template {
    fn clone(&self) -> Self {
        Self {
            kind: self.kind.to_owned(),
            // `tir_reference` contains an `Arc`-backed store owner, so a normal
            // `clone()` is the explicit, cheap reference-count increment.
            tir_reference: self.tir_reference.clone(),
            location: self.location.to_owned(),
        }
    }
}

// -------------------------
//  Template Implementation
// -------------------------

impl Template {
    /// Synchronizes the durable cache and the authoritative TIR kind from a
    /// classification result.
    ///
    /// WHAT: reads the current kind from the owning TIR store entry, applies
    ///       the generic `String` / `StringFunction` refresh rule while
    ///       preserving semantic markers (`SlotInsert`, `SlotDefinition`,
    ///       `Comment`), and writes the result to both `TemplateIr.kind` and
    ///       the durable `Template.kind`.
    /// WHY: this is the single post-construction synchronization owner.
    ///      Construction initializes both copies once; all later refresh goes
    ///      through here so the cache and the authoritative TIR entry cannot
    ///      drift through scattered writes.
    pub(crate) fn synchronize_kind_from_classification(
        &mut self,
        store: &mut TemplateIrStore,
        classification: &TirTemplateClassification,
    ) -> Result<(), CompilerError> {
        let mut kind = self.tir_kind_from_store(store).ok_or_else(|| {
            CompilerError::compiler_error(
                "Template kind synchronization requires the reference's owning TIR store.",
            )
        })?;

        refresh_kind_from_classification(&mut kind, classification);

        if !store.set_template_kind(self.tir_reference.root.template_id, kind.clone()) {
            return Err(CompilerError::compiler_error(
                "Template TIR entry was missing during kind synchronization write-back.",
            ));
        }

        self.kind = kind;

        Ok(())
    }

    /// Returns the authoritative template kind from the owning TIR store entry.
    ///
    /// WHAT: reads `TemplateIr.kind` from the store after verifying the store
    ///       owner matches this template's TIR reference.
    /// WHY: `TemplateIr.kind` is the authoritative post-construction owner.
    ///      Callers that hold the owning store read it here instead of the
    ///      durable cache.
    pub(crate) fn tir_kind_from_store(&self, store: &TemplateIrStore) -> Option<TemplateType> {
        if self.tir_reference.root.store_id != store.store_id()
            || !Arc::ptr_eq(&self.tir_reference.store_owner, &store.owner())
        {
            return None;
        }
        store
            .get_template(self.tir_reference.root.template_id)
            .map(|template_ir| template_ir.kind.clone())
    }

    /// Returns the authoritative template kind by resolving the TIR reference
    /// through the module registry.
    ///
    /// WHAT: borrows the registry, finds the owning store, and reads
    ///       `TemplateIr.kind` after verifying the store owner matches.
    /// WHY: callers that hold the module registry but not a direct store borrow
    ///      use this to read the authoritative kind without a second cache.
    pub(crate) fn tir_kind_via_registry(
        &self,
        registry: &TemplateIrRegistry,
    ) -> Option<TemplateType> {
        let store_handle = registry.store_handle(self.tir_reference.root.store_id)?;
        let store = store_handle.borrow();
        if !Arc::ptr_eq(&self.tir_reference.store_owner, &store.owner()) {
            return None;
        }
        store
            .get_template(self.tir_reference.root.template_id)
            .map(|template_ir| template_ir.kind.clone())
    }
}
