//! Source-library root-file identity.
//!
//! WHAT: exposes the shared root/config filename owner and its focused tests.
//! WHY: Stage 0, path resolution and header import validation must classify root files through one
//! policy while the remaining `#mod.bst` role is removed in later roadmap phases.

pub(crate) mod root_file;

#[cfg(test)]
#[path = "tests/root_file_tests.rs"]
mod tests;
