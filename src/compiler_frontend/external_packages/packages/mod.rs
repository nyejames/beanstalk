//! Test-only package registration functions.
//!
//! WHAT: each submodule registers one test package into a mutable
//! `ExternalPackageRegistry`. This keeps test package definitions isolated
//! so adding a new test package does not bloat the registry constructor.
//! WHY: test package definitions are data, not logic. Separating them by package
//! makes the registry orchestration readable and prevents merge conflicts.

pub(crate) mod test_packages;
