//! Library set definition.
//!
//! WHAT: bundles the external packages and source libraries that a builder exposes.
//! WHY: builders provide both virtual package metadata and source library roots;
//!      the compiler needs both during different stages.

use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::libraries::source_library_registry::SourceLibraryRegistry;

/// The complete set of libraries a builder exposes to a project.
///
/// WHAT: collects every library kind the frontend and backends need.
/// WHY: one unified builder return type instead of separate APIs for packages
///      and source libraries.
#[derive(Clone, Debug)]
pub struct LibrarySet {
    pub external_packages: ExternalPackageRegistry,
    pub source_libraries: SourceLibraryRegistry,
}

impl LibrarySet {
    /// Builds a library set with only the core external packages and no source libraries.
    ///
    /// WHAT: the minimal default every builder starts from.
    /// WHY: core prelude is mandatory; source libraries are optional builder additions.
    pub fn with_core_packages() -> Self {
        Self {
            external_packages: ExternalPackageRegistry::new(),
            source_libraries: SourceLibraryRegistry::default(),
        }
    }
}
