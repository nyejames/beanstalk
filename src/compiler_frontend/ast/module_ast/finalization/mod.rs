//! AST finalization sub-modules for template normalization.
//!
//! WHAT: Groups all finalization logic that prepares AST for HIR consumption,
//! including AST node normalization, module constant normalization, and shared
//! template folding helpers.
//!
//! WHY: Separates finalization concerns from the entry point orchestration in
//! `pass_finalize.rs`, making both the high-level pass sequence and detailed
//! normalization logic easier to understand independently.

pub(super) mod normalize_ast;
pub(super) mod normalize_constants;
pub(super) mod template_helpers;
