//! Template struct and core type definitions.
//!
//! WHAT: Houses the central `Template` struct and its associated identity and
//! classification queries.
//!
//! WHY: Separates the `Template` data type from the parsing/composition logic
//! so other modules can depend on the struct without pulling in the full parser.

use crate::compiler_frontend::ast::templates::template::TemplateType;
use crate::compiler_frontend::ast::templates::tir::{
    MaterializedTirTemplateClassification, TemplateIrId, TemplateIrStoreOwner, TemplateTirReference,
};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use std::sync::Arc;

// -------------------------
//  Template AST Node
// -------------------------

/// The central template representation in the AST.
///
/// A `Template` is a narrow durable handle carrying its TIR identity, kind
/// classification and source location. Effective style and wrapper context are
/// owned by the `TemplateIr` entry resolved through `tir_reference`. The
/// `Template` is the primary data structure passed between parsing, composition,
/// formatting, folding and HIR preparation.
///
/// ## Metadata lifecycle invariants
///
/// - **`kind`** is refreshed from finalized effective TIR views while preserving
///   semantic markers such as `SlotDefinition`, `SlotInsert`, and `Comment` that
///   must not be overwritten by generic cleanup.
/// - **`style`** is owned by `TemplateIr` and read through the registry-backed
///   TIR view after construction.
#[derive(Debug)]
pub struct Template {
    pub kind: TemplateType,

    /// Finalized TIR reference.
    ///
    /// WHAT: holds the store-qualified `TemplateRef` root and store-owner token
    /// WHY: this is the long-lived reference; the `TemplateRef` makes the owning
    ///      store explicit for registry/view consumers, while the store-owner
    ///      `Arc` keeps the same-store instance-identity proof that numeric store
    ///      IDs alone cannot provide.
    pub(crate) tir_reference: Option<TemplateTirReference>,

    pub id: String,
    pub location: SourceLocation,
}

impl Clone for Template {
    fn clone(&self) -> Self {
        Self {
            kind: self.kind.to_owned(),
            // `tir_reference` contains an `Arc`-backed store owner, so a normal
            // `clone()` is the explicit, cheap reference-count increment.
            tir_reference: self.tir_reference.clone(),

            id: self.id.to_owned(),
            location: self.location.to_owned(),
        }
    }
}

// -------------------------
//  Template Implementation
// -------------------------

impl Template {
    /// Creates an empty template handle with default style and no TIR identity.
    ///
    /// Production parser construction no longer uses this; the durable
    /// `Template` is constructed once after authoritative TIR identity exists.
    /// Direct test fixtures may still use it until Phase 2D completes the
    /// non-optional reference migration.
    #[cfg(test)]
    pub fn empty() -> Template {
        Self::build_empty()
    }

    #[cfg(test)]
    fn build_empty() -> Template {
        Template {
            kind: TemplateType::StringFunction,
            tir_reference: None,
            id: String::new(),
            location: SourceLocation::default(),
        }
    }

    /// Refreshes the ordinary durable kind from authoritative TIR classification.
    ///
    /// Helper, slot-definition, and comment markers remain semantic tags and
    /// are not replaced with the generic string/runtime classification.
    pub(crate) fn refresh_kind_from_tir_classification(
        &mut self,
        classification: &MaterializedTirTemplateClassification,
    ) {
        if matches!(
            self.kind,
            TemplateType::SlotInsert(_)
                | TemplateType::SlotDefinition(_)
                | TemplateType::Comment(_)
        ) {
            return;
        }

        self.kind = if classification.shape_const_evaluable && !classification.has_slot_insertions {
            TemplateType::String
        } else {
            TemplateType::StringFunction
        };
    }

    /// Returns the store-owner token for this template's finalized TIR
    /// reference, if any.
    ///
    /// WHAT: lets callers prove that a finalized `TemplateIrId` belongs to the
    ///       same `TemplateIrStore` they are writing into before recording it as
    ///       a `ChildTemplate` reference.
    /// WHY: ordinary `Template::clone()` preserves the finalized reference after
    ///      parsing so callers can prove same-store ownership of a finalized
    ///      `TemplateIrId` without carrying the full builder-state children/summary.
    pub(crate) fn tir_store_owner(&self) -> Option<Arc<TemplateIrStoreOwner>> {
        self.tir_reference
            .as_ref()
            .map(|reference| Arc::clone(&reference.store_owner))
    }

    /// Returns the finalized TIR template ID for this template, when present.
    pub(crate) fn tir_template_id(&self) -> Option<TemplateIrId> {
        self.tir_reference
            .as_ref()
            .map(|reference| reference.root.template_id)
    }
}
