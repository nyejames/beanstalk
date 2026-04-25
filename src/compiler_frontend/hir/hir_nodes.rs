//! HIR data-model re-export surface.
//!
//! WHAT: keeps the existing `hir::hir_nodes::*` import path stable while the HIR data model is
//! split into focused files.
//! WHY: Phase 1 is structural only. Existing backends/tests should not need semantic rewrites.

pub use super::blocks::*;
pub use super::constants::*;
pub use super::expressions::*;
pub use super::functions::*;
pub use super::ids::*;
pub use super::module::*;
pub use super::operators::*;
pub use super::patterns::*;
pub use super::places::*;
pub use super::regions::*;
pub use super::statements::*;
pub use super::structs::*;
pub use super::terminators::*;
