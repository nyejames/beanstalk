//! Header parsing stage modules.
//!
//! WHAT: extracts file-level declarations/imports and start-function boundaries before AST build.
//! Header parsing also owns top-level symbol collection (`module_symbols`), so dependency sorting
//! and AST construction receive a pre-built symbol package without a separate manifest stage.

pub(crate) mod module_symbols;
pub(crate) mod parse_file_headers;
pub(crate) mod visible_scope;
