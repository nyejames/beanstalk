//! Library identity and shared package metadata.
//!
//! WHAT: defines library identity and shared package metadata for core,
//! builder, and source libraries.
//! WHY: separates library definition from frontend parsing and backend
//! lowering so each stage has one clear responsibility.

pub mod core;
pub mod library_set;
pub mod source_library_registry;

pub use library_set::LibrarySet;
pub use source_library_registry::{ProvidedSourceRoot, SourceLibraryRegistry};
