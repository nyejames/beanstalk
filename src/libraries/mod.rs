//! Library identity and shared package metadata.
//!
//! WHAT: defines library identity and shared package metadata for core,
//! builder, and source libraries.
//! WHY: separates library definition from frontend parsing and backend
//! lowering so each stage has one clear responsibility.

pub mod config_key_registry;
pub mod core;
pub mod external_import_providers;
pub mod library_set;
pub mod source_file_kind_registry;
pub mod source_library_registry;

pub use library_set::LibrarySet;
pub use source_file_kind_registry::{SourceFileKind, SourceFileKindRegistry};
pub use source_library_registry::{ProvidedSourceRoot, SourceLibraryRegistry};

#[cfg(test)]
mod tests;
