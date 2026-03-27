//! Shared frontend test utilities.
//!
//! WHAT: provides low-churn helpers reused across frontend subsystem tests.
//! WHY: path-resolution setup is identical in several suites and should stay consistent.

use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;

pub(crate) fn test_project_path_resolver() -> ProjectPathResolver {
    let cwd = std::env::temp_dir();
    ProjectPathResolver::new(cwd.clone(), cwd, &[]).expect("test path resolver should be valid")
}
