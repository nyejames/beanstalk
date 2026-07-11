//! Template struct and core type definitions.
//!
//! WHAT: Houses the central `Template` struct and its associated methods —
//! queries (constness, slots), construction helpers, and style application.
//!
//! WHY: Separates the `Template` data type from the parsing/composition logic
//! so other modules can depend on the struct without pulling in the full parser.

#[cfg(test)]
use crate::compiler_frontend::ast::templates::template::TemplateContent;
use crate::compiler_frontend::ast::templates::template::{Style, TemplateType};
use crate::compiler_frontend::ast::templates::template_control_flow::TemplateControlFlow;
use crate::compiler_frontend::ast::templates::tir::{
    MaterializedTirTemplateClassification, TemplateIrId, TemplateIrNodeId, TemplateIrStore,
    TemplateIrStoreOwner, TemplateTirReference, TemplateWrapperReference,
};
use crate::compiler_frontend::symbols::string_interning::StringIdRemap;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use std::sync::Arc;

// -------------------------
//  Template AST Node
// -------------------------

/// The central template representation in the AST.
///
/// A `Template` carries its TIR identity, style configuration, constness
/// classification and source location. It is the primary data structure passed
/// between parsing, composition, formatting, folding and HIR preparation.
///
/// ## Metadata lifecycle invariants
///
/// - **`kind`** is refreshed from finalized effective TIR views while preserving
///   semantic markers such as `SlotDefinition`, `SlotInsert`, and `Comment` that
///   must not be overwritten by generic cleanup.
#[derive(Debug)]
pub struct Template {
    /// Detached compatibility payload used only by legacy fixture builders.
    #[cfg(test)]
    pub content: TemplateContent,
    pub(crate) control_flow: Option<TemplateControlFlow>,
    pub kind: TemplateType,
    pub style: Style,

    /// `$children(..)` wrapper references that apply to this template's direct
    /// child-template outputs.
    ///
    /// WHAT: stores exact store-qualified roots with their phase and overlay
    ///       identity instead of recursively owning wrapper templates.
    /// WHY: `$children(..)` wrappers already have durable TIR authority when
    ///      accepted by the directive, so later parsing and composition can
    ///      propagate that identity without normalizing templates again.
    pub(crate) child_wrappers: Vec<TemplateWrapperReference>,

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
            #[cfg(test)]
            content: self.content.to_owned(),
            control_flow: self.control_flow.to_owned(),
            kind: self.kind.to_owned(),
            style: self.style.to_owned(),
            child_wrappers: self.child_wrappers.to_owned(),
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
    pub fn empty() -> Template {
        Self::build_empty()
    }

    fn build_empty() -> Template {
        Template {
            #[cfg(test)]
            content: TemplateContent::default(),
            control_flow: None,
            kind: TemplateType::StringFunction,
            style: Style::default(),
            child_wrappers: vec![],
            tir_reference: None,
            id: String::new(),
            location: SourceLocation::default(),
        }
    }

    /// Replace the template's effective style.
    ///
    /// WHAT: replaces the style field without touching TIR identity or children.
    /// WHY: composition and formatting passes may need to override the parsed
    ///      style after construction while preserving the structural TIR root.
    pub(crate) fn apply_style(&mut self, style: Style) {
        self.style = style;
    }

    /// Mutate the effective style in place.
    ///
    /// WHAT: passes a mutable reference to the style into the caller's closure.
    /// WHY: lets composition passes adjust individual style fields without
    ///      taking ownership of the current style value.
    pub(crate) fn apply_style_updates(&mut self, mut update: impl FnMut(&mut Style)) {
        update(&mut self.style);
    }

    /// Applies generic string/string-function classification from an already
    /// materialized current-state TIR classification.
    ///
    /// WHAT: updates only ordinary template kinds; helper, slot definition, and
    /// comment templates keep their semantic marker kinds.
    /// WHY: template construction and later mutation sites often need several
    /// facts from the same current-state TIR tree. Reusing one classification
    /// avoids repeated materialization while keeping kind ownership on
    /// `Template`.
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

    /// Returns the root node ID of the finalized TIR template.
    pub(crate) fn tir_root_node_id(&self, store: &TemplateIrStore) -> Option<TemplateIrNodeId> {
        self.tir_template_id()
            .and_then(|template_id| store.get_template(template_id))
            .map(|template_ir| template_ir.root)
    }

    /// Recursively remap interned string IDs in this template's live AST state
    /// and owned children.
    ///
    /// WHAT: rewrites source locations, control flow, kind markers and all
    ///       recursive child templates with the caller's string-id remap.
    /// WHY: the per-file frontend remaps string IDs before module-wide
    ///      dependency sorting. Template body expressions are owned and
    ///      remapped by the module-scoped TIR store.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.location.remap_string_ids(remap);
        if let Some(control_flow) = &mut self.control_flow {
            control_flow.remap_string_ids(remap);
        }
        self.kind.remap_string_ids(remap);

        // The finalized TIR reference only carries a store-local ID and an
        // owner token. The TIR store is always consumed before the module-wide
        // StringId remap boundary, so no per-template TIR remap is needed here.
    }
}
