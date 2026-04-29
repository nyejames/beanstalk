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
    /// Builds a library set with mandatory compiler core packages and no source libraries.
    ///
    /// WHAT: the minimal default every builder starts from: prelude, IO, compiler-owned
    /// collection helpers, and error helpers.
    /// WHY: user-facing optional core libraries such as `@core/math` and `@core/text`
    /// must be explicit builder opt-ins.
    pub fn with_mandatory_core() -> Self {
        Self {
            external_packages: ExternalPackageRegistry::new(),
            source_libraries: SourceLibraryRegistry::default(),
        }
    }

    /// Exposes the currently supported optional core packages for the HTML builder.
    ///
    /// WHAT: registers JS-backed core packages selected by the HTML builder.
    /// WHY: optional core libraries are builder surface; they should not be assumed by
    /// the compiler's mandatory registry.
    pub fn expose_html_core_libraries(&mut self) {
        crate::libraries::core::register_core_math_package(&mut self.external_packages);
        crate::libraries::core::register_core_text_package(&mut self.external_packages);
        crate::libraries::core::register_core_random_package(&mut self.external_packages);
        crate::libraries::core::register_core_time_package(&mut self.external_packages);
    }
}
