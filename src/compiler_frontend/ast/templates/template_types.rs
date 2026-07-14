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

    /// Authoritative TIR reference.
    ///
    /// WHAT: holds the store-qualified root, logical store-owner token, pipeline
    ///       phase, and overlay-set ID.
    /// WHY: this is the long-lived reference. The `TemplateRef` makes the owning
    ///      store explicit for registry/view consumers, while the owner token
    ///      distinguishes equal registry-local store IDs from different registries.
    pub(crate) tir_reference: TemplateTirReference,

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

    /// Returns the logical store-owner token for this template's TIR reference.
    ///
    /// WHAT: lets direct-store consumers prove a `TemplateIrId` belongs to the
    ///       same logical store before using it as a local ID.
    /// WHY: these consumers may already hold a store borrow, so they cannot
    ///      re-borrow through the registry. The token also rejects references
    ///      from another registry whose numeric store ID happens to match.
    pub(crate) fn tir_store_owner(&self) -> Arc<TemplateIrStoreOwner> {
        Arc::clone(&self.tir_reference.store_owner)
    }

    /// Returns the authoritative TIR template ID for this template.
    pub(crate) fn tir_template_id(&self) -> TemplateIrId {
        self.tir_reference.root.template_id
    }
}
