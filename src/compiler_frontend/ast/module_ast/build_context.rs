//! Shared AST build inputs.
//!
//! WHAT: groups long-lived frontend services and per-build settings used by the AST phases.
//! WHY: environment building, node emission, and finalization all need the same build services,
//! but each phase owns its own mutable state.

use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;

/// Shared dependencies/configuration required to build one module AST.
pub struct AstBuildContext<'a> {
    pub external_package_registry: &'a ExternalPackageRegistry,
    pub style_directives: &'a StyleDirectiveRegistry,
    pub string_table: &'a mut StringTable,
    pub entry_dir: InternedPath,
    pub build_profile: FrontendBuildProfile,
    pub project_path_resolver: Option<ProjectPathResolver>,
    pub path_format_config: PathStringFormatConfig,
}

pub(crate) struct AstPhaseContext<'a> {
    pub(crate) external_package_registry: &'a ExternalPackageRegistry,
    pub(crate) style_directives: &'a StyleDirectiveRegistry,
    pub(crate) entry_dir: InternedPath,
    pub(crate) build_profile: FrontendBuildProfile,
    pub(crate) project_path_resolver: Option<ProjectPathResolver>,
    pub(crate) path_format_config: PathStringFormatConfig,
}

impl<'a> AstPhaseContext<'a> {
    pub(crate) fn from_build_context(context: AstBuildContext<'a>) -> (Self, &'a mut StringTable) {
        let AstBuildContext {
            external_package_registry,
            style_directives,
            string_table,
            entry_dir,
            build_profile,
            project_path_resolver,
            path_format_config,
        } = context;

        (
            Self {
                external_package_registry,
                style_directives,
                entry_dir,
                build_profile,
                project_path_resolver,
                path_format_config,
            },
            string_table,
        )
    }
}
