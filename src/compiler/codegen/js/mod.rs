//! JavaScript codegen for Beanstalk HIR.
//!
//! This module exposes the entrypoints and keeps the JS codegen split across
//! focused files so each file stays under a single responsibility and the public
//! API surface remains small.

mod analysis;
mod context;
mod formatting;
mod identifiers;

pub use context::{JsModule, lower_hir_to_js};
