//! Header parsing stage modules.
//!
//! WHAT: extracts file-level declarations/imports and start-function boundaries before AST build.
//! Header parsing also owns top-level symbol collection (`module_symbols`), so dependency sorting
//! and AST construction receive a pre-built symbol package without a separate manifest stage.

mod const_fragments;
mod dependency_edges;
mod file_parser;
mod header_dispatch;
mod imports;
pub(crate) mod module_symbols;
pub(crate) mod parse_file_headers;
mod start_capture;
mod types;
