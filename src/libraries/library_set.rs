//! Library set definition.
//!
//! WHAT: bundles the external packages and source libraries that a builder exposes.
//! WHY: builders provide both virtual package metadata and source library roots;
//!      the compiler needs both during different stages.

use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::libraries::config_key_registry::ProjectConfigKeyRegistry;
use crate::libraries::external_import_providers::cache::ExternalImportProviderCache;
use crate::libraries::external_import_providers::provider::BuilderRuntimePackageMetadata;
use crate::libraries::external_import_providers::registry::ExternalImportProviderRegistry;
use crate::libraries::external_import_providers::resolution_table::ExternalImportResolutionTable;
use crate::libraries::source_file_kind_registry::SourceFileKindRegistry;
use crate::libraries::source_library_registry::SourceLibraryRegistry;
use std::path::PathBuf;

/// The complete set of libraries a builder exposes to a project.
///
/// WHAT: collects every library kind the frontend and backends need.
/// WHY: one unified builder return type instead of separate APIs for packages
///      and source libraries.
#[derive(Clone, Debug)]
pub struct LibrarySet {
    pub external_packages: ExternalPackageRegistry,
    pub source_libraries: SourceLibraryRegistry,
    pub config_keys: ProjectConfigKeyRegistry,
    pub external_import_providers: ExternalImportProviderRegistry,
    pub external_import_cache: ExternalImportProviderCache,
    pub external_import_resolution_table: ExternalImportResolutionTable,
    pub builder_runtime_packages: Vec<BuilderRuntimePackageMetadata>,
    pub source_file_kinds: SourceFileKindRegistry,
}

const BUILTIN_SOURCE_LIBRARIES_DIR: &str = "libraries";
const SUPPORTED_PROJECT_CONFIG_VALUES: &[&str] = &["html"];

impl LibrarySet {
    /// Builds a library set with mandatory compiler core packages and no source libraries.
    ///
    /// WHAT: the minimal default every builder starts from: prelude, IO, compiler-owned
    /// collection helpers, and error helpers.
    /// WHY: user-facing optional core libraries such as `@core/math` and `@core/text`
    /// must be explicit builder opt-ins.
    pub fn with_mandatory_core() -> Self {
        let mut config_keys = ProjectConfigKeyRegistry::new();

        // Core config keys are always accepted regardless of backend.
        config_keys.register_core_closed_string_set("project", SUPPORTED_PROJECT_CONFIG_VALUES);
        config_keys.register_core_string("entry_root");
        config_keys.register_core_string("dev_folder");
        config_keys.register_core_string("output_folder");
        config_keys.register_core_int("template_const_loop_iteration_limit");
        config_keys.register_core_string_collection("library_folders");
        config_keys.register_core_string("name");
        config_keys.register_core_string("project_name");
        config_keys.register_core_string("version");
        config_keys.register_core_string("author");
        config_keys.register_core_string("license");

        Self {
            external_packages: ExternalPackageRegistry::new(),
            source_libraries: SourceLibraryRegistry::default(),
            config_keys,
            external_import_providers: ExternalImportProviderRegistry::empty(),
            external_import_cache: ExternalImportProviderCache::new(),
            external_import_resolution_table: ExternalImportResolutionTable::new(),
            builder_runtime_packages: Vec::new(),
            source_file_kinds: SourceFileKindRegistry::new(),
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

    pub fn builtin_source_library_root(prefix: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(BUILTIN_SOURCE_LIBRARIES_DIR)
            .join(prefix)
    }
}
