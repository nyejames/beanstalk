//! Template IR (TIR) — AST-local intermediate representation for parsed templates.
//!
//! WHAT: TIR is a tree-structured representation of template content that replaces
//! the current ping-pong between `TemplateContent`, `TemplateRenderPlan`, rebuilt
//! content, and re-parsed atoms. It stores all template data in a single
//! `TemplateIrStore` with typed IDs.
//!
//! WHY: the current template pipeline rebuilds content multiple times during
//! composition, formatting, and folding. TIR gives each pass a single stable
//! representation to read from and write to, reducing clone churn and making
//! the data flow explicit.
//!
//! ## Ownership contract
//!
//! TIR is owned by the AST template subsystem. It does not own HIR, backend,
//! or public API data. The store is module-scoped and dropped after AST
//! template processing for that module completes.
//!
//! ## Semantic parity constraint
//!
//! TIR must produce the same user-visible template semantics as the current
//! `Template` → `TemplateContent` path. The converter (Phase B1) translates
//! existing AST templates into TIR and validates that the shapes match.
//! Behaviour changes are out of scope unless they are bug fixes with
//! regression tests.
//!
//! ## Temporary converter
//!
//! The converter in `convert_from_template.rs` (Phase B1) translates current
//! `Template` values into TIR. That converter is temporary — once TIR is the
//! authoritative path, the converter and the old `Template`-based internals it
//! replaces will be deleted at a documented checkpoint.
//!
//! ## No feature flag
//!
//! TIR types and the eventual production route are implemented directly on
//! `main` without a feature flag. The old path is removed, not gated.
//!
//! ## Module layout
//!
//! ```text
//! tir/
//! ├── mod.rs              This file — module entry and narrow re-exports
//! ├── ids.rs              Typed IDs: TemplateIrId, TemplateIrNodeId, TemplateWrapperSetId
//! ├── store.rs            TemplateIrStore — central owned storage
//! ├── node.rs             TemplateIr, TemplateIrNode, TemplateIrNodeKind
//! ├── summary.rs          TemplateIrSummary — shape metadata for capacity planning
//! ├── validation.rs       Structural validation after conversion (Phase B1)
//! ├── convert_from_template.rs  Temporary converter (Phase B1)
//! ├── fold.rs             TIR-native folding (Phase B2)
//! ├── formatter_view.rs   TIR-native formatter feed (Phase B3)
//! ├── render_unit.rs      Render-unit preparation from TIR (Phase B4)
//! ├── slot_plan.rs        Slot plan preparation (Phase B5)
//! ├── wrapper_sets.rs     Wrapper set management (Phase B5)
//! ├── reactive_metadata.rs  Reactive subscription metadata (Phase B1)
//! └── tests/              TIR-focused tests (Phase B1+)
//! ```
//!
//! Only `mod.rs` controls what is re-exported. Submodules keep their internals
//! `pub(crate)` and `mod.rs` selects a narrow API surface.

// -------------------------
//  Submodules
// -------------------------

// Scaffolding: all TIR types are currently unused by production code.
// They will be consumed by Phase B1 (converter), Phase B2 (fold), and later phases.
// Remove this allow once TIR is wired into production paths.
#[allow(dead_code, reason = "TIR converter — consumed by tests and Phase B1+")]
mod convert_from_template;
#[allow(dead_code, reason = "TIR scaffolding — consumed by Phase B1+")]
mod ids;
#[allow(dead_code, reason = "TIR scaffolding — consumed by Phase B1+")]
mod node;
#[allow(dead_code, reason = "TIR scaffolding — consumed by Phase B1+")]
mod store;
#[allow(dead_code, reason = "TIR scaffolding — consumed by Phase B1+")]
mod summary;
#[allow(dead_code, reason = "TIR validation — consumed by tests and Phase B1+")]
mod validation;

#[allow(dead_code, reason = "TIR fold — consumed by Phase B2+")]
mod fold;

#[cfg(test)]
mod tests;

// -------------------------
//  Re-exports
// -------------------------

// IDs are the primary external interface — consumers use them to reference
// store entries without reaching into the store module directly.
#[allow(unused_imports, reason = "TIR scaffolding — consumed by Phase B1+")]
pub(crate) use ids::{TemplateIrId, TemplateIrNodeId, TemplateWrapperSetId};

// Store and node types are re-exported so the converter and later phases
// can construct TIR data without deep import paths.
#[allow(unused_imports, reason = "TIR scaffolding — consumed by Phase B1+")]
pub(crate) use node::{TemplateIr, TemplateIrBranch, TemplateIrNode, TemplateIrNodeKind};
#[allow(unused_imports, reason = "TIR scaffolding — consumed by Phase B1+")]
pub(crate) use store::TemplateIrStore;
#[allow(unused_imports, reason = "TIR scaffolding — consumed by Phase B1+")]
pub(crate) use summary::TemplateIrSummary;

// Converter: translates AST Template values into TIR store entries.
// This is temporary — it will be deleted once TIR is the authoritative path.
#[allow(unused_imports, reason = "Converter consumed by tests and Phase B1+")]
pub(crate) use convert_from_template::convert_template_to_tir;

// Validation: structural integrity checks for the TIR store.
#[allow(unused_imports, reason = "Validation consumed by tests and Phase B1+")]
pub(crate) use validation::validate_tir_store;

// Folding: compile-time TIR template folding.
#[allow(unused_imports, reason = "Fold consumed by Phase B2+")]
pub(crate) use fold::fold_tir_template;
