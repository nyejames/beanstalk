//! Builtin package registration functions.
//!
//! WHAT: each submodule registers one standard-library or test package into a
//! mutable `ExternalPackageRegistry`. This keeps package definitions isolated
//! so adding a new standard package does not bloat the registry constructor.
//! WHY: package definitions are data, not logic. Separating them by package
//! makes the registry orchestration readable and prevents merge conflicts.

pub(crate) mod std_collections;
pub(crate) mod std_error;
pub(crate) mod std_io;
pub(crate) mod std_math;
pub(crate) mod test_packages;
