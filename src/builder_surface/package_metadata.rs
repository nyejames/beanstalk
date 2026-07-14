//! Shared package metadata: origin and backing axes.
//!
//! WHAT: defines `PackageOrigin` and `PackageBacking` as orthogonal metadata for
//! any package the compiler encounters, plus `PackageMetadata` which bundles them.
//! WHY: origin (who owns/distributes the package) and backing (how the compiler
//! obtains its implementation) are independent concerns. Keeping them separate
//! avoids conflating builder-owned source packages with binding-backed core packages.

/// Who owns or distributes a package.
///
/// WHAT: classifies packages by their provider relationship.
/// WHY: diagnostics and availability checks need to distinguish core, builder,
/// project-local and future dependency packages without inspecting import paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PackageOrigin {
    /// Compiler-owned core packages (e.g. `@core/io`, `@core/math`).
    Core,
    /// Reserved for future standard-library packages.
    Standard,
    /// Builder-owned packages (e.g. `@html`, `@web/canvas`).
    Builder,
    /// Packages local to the current project.
    ProjectLocal,
    /// Reserved for future external dependency packages.
    Dependency,
}

/// How the compiler obtains a package's implementation.
///
/// WHAT: distinguishes Beanstalk-source packages from binding-backed packages.
/// WHY: source-backed packages contain Beanstalk modules resolved through
/// `SourcePackageRegistry`; binding-backed packages expose typed external
/// symbols resolved through `ExternalPackageRegistry`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PackageBacking {
    /// Package backed by Beanstalk source files.
    BeanstalkSource,
    /// Package backed by external bindings (JS, Wasm, host operations).
    ExternalBinding,
}

/// Origin and backing metadata for one package.
///
/// WHAT: bundles the two orthogonal package axes so every registration site
/// carries both pieces of information.
/// WHY: callers should not be able to construct invalid combinations such as
/// a `BeanstalkSource` backing inside `ExternalPackageRegistry`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PackageMetadata {
    pub origin: PackageOrigin,
    pub backing: PackageBacking,
}

impl PackageMetadata {
    /// Metadata for a source-backed package.
    pub const fn source(origin: PackageOrigin) -> Self {
        Self {
            origin,
            backing: PackageBacking::BeanstalkSource,
        }
    }

    /// Metadata for a binding-backed package.
    pub const fn binding(origin: PackageOrigin) -> Self {
        Self {
            origin,
            backing: PackageBacking::ExternalBinding,
        }
    }
}
