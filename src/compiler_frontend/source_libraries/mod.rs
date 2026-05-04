//! Source-library frontend helpers.
//!
//! WHAT: shared helpers for module facade identity and import-surface rules.
//! WHY: source-library boundaries are enforced across project discovery, header parsing,
//! dependency sorting, and header import environment construction, so facade-file checks need one owner.

pub(crate) mod mod_file;
