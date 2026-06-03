//! Header parsing stage modules.
//!
//! WHAT: extracts file-level declarations/imports and start-function boundaries before AST build.
//! Header parsing also owns top-level symbol collection (`module_symbols`), so dependency sorting
//! and AST construction receive a pre-built symbol package without a separate manifest stage.

mod const_fragments;
mod constant_dependencies;
mod dependency_canonicalization;
mod dependency_edges;
mod facade_data;
mod file_imports;
mod file_parser;
mod file_state;
mod hash_items;
mod header_dispatch;
pub(crate) mod import_environment;
mod imports;
pub(crate) mod module_symbols;
pub(crate) mod parse_file_headers;
mod start_capture;
mod symbol_collection;
mod top_level_classifier;
mod trait_headers;
mod types;
