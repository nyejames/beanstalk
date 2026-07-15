# Agent implementation plan: package terminology, documentation corrections and monolith synchronisation

## Current state

ACTIVE_PLAN: `docs/roadmap/plans/package-terminology-docs-monolith-sync-plan.md`
STATUS: complete
CURRENT_SLICE: final audit accepted and complete
LAST_ACCEPTED_COMMIT: completion checkpoint is the commit containing this state
WORKTREE: `main` at `/Users/aneirinjames/projects/beanstalk/beanstalk`, package-plan diff only
REQUIRED_RELOADS: startup files, this plan and current source/diff
RELEVANT_CONTEXT_NOW:
- docs: package, project-structure, Beandown, Markdown, language and compiler-design sources plus generated routes
- code: binding/source package registries, build-system discovery, JS binding modules and focused tests
ACCEPTANCE_CRITERIA:
- all required audit findings are resolved
- `just validate`, `just bench-check` and `git diff --check` passed after final corrections
- completed roadmap entry removed after acceptance
VALIDATION_STATE:
- `cargo run --quiet -- build docs --release`: passed after repeat-audit corrections, 72 files
- `cargo fmt --all --check`: passed
- `git diff --check`: passed
- `just validate`: passed after all corrections, including cross-target Clippy, 3,334 unit tests, 1,758 integration cases, docs checks and 28/28 benchmark cases
- `just bench-check`: passed separately after all corrections, 28/28 cases
DOCS_IMPACT: authorized by this plan and the user, including generated documentation and roadmap removal after final review
BLOCKERS_OR_OPEN_DECISIONS: none
DELEGATION_DECISION: codex-cli audit - independent first and repeated final audits completed
NEXT_WORKER_ORDER: none
STOP_REASON: active work source complete
NEXT_RESUME_ACTION: none
## Objective

Implement the accepted package terminology model across the compiler, project configuration,
diagnostics, tests and documentation, then complete the outstanding Project Structure, Packages,
Beandown and Plain Markdown review corrections.

This plan supersedes:

```text
project-structure-libraries-content-corrections-plan.md
```

The hash-root module plan is complete. Do not reopen or redesign its semantics.

The completed module contract is:

- one directory-scoped module rooted by one non-config `#*.bst`
- the root filename after `#` is cosmetic
- `config.bst` is the only project config filename
- `export:` is the only public API marker
- imported roots do not replay runtime or page-fragment activity
- API-only roots are valid and do not produce HTML artifacts
- HTML routes come from directory position, not root filename

This patch must preserve that behaviour exactly.

The patch has four connected goals:

1. Standardise **package**, **module**, **binding** and **prelude** terminology
2. Separate package origin from package backing in compiler metadata
3. Correct the remaining focused documentation defects
4. Synchronise the language and compiler-design monoliths with verified current behaviour


## Progress log

### Completed

- Phases A-G replaced formal source-library and builder-library identities with package, module, binding and prelude terminology across registries, diagnostics, backend owners and active fixtures.
- Phases 14-18 corrected the package, project-structure, Beandown and Markdown pages, synchronized both maintained monoliths, updated migration policy and rebuilt generated documentation under package routes.
- Phase 19 removed active formal library terminology. Exact `libraries` and `library_folders` spellings remain only in explicit legacy-key rejection paths. Generic Rust/software uses of library and narrow architectural facade types are not package-system categories.
- Phase 20 preserved module roots, exports, imported-root suppression, artifact filtering, prefix resolution, prelude visibility, external IDs, runtime assets and content imports.
- Phase 21 passed full validation and the non-recording benchmark gate after all audit corrections.
- Final audit triage accepted the package metadata, registry ownership, diagnostics and focused coverage. Required raw-string teaching, locator, migration-ledger and plan-state drift was corrected. A direct compiler probe rejected the audit's proposed diagnostic change by confirming the existing `BST-SYNTAX-0007` contract.
- Historical `facade` fixture IDs and tags remain stable test identities for pre-package module-surface regressions. They are not compiler concepts or user-facing terminology. Narrow architectural facades such as `AstTypeInterner`, the TIR builder and the Canvas browser bridge remain accurate local descriptions.
- The previous TIR blocker is resolved and did not reproduce. Docs checking, release docs generation, unit tests, integration tests and benchmarks all pass.
### Phase A completed

- Created `src/builder_surface/package_metadata.rs` with `PackageOrigin`, `PackageBacking`, `PackageMetadata`.
- Replaced `ExternalPackageOrigin` on `ExternalPackage` with `PackageMetadata`.
- Updated `ExternalPackageRegistry::register_package` to accept `PackageOrigin` and construct binding metadata internally.
- Updated all binding registrations to pass their concrete Core, Builder or ProjectLocal origin.
- Added `PackageMetadata` to `SourcePackageRoot` with `BeanstalkSource` backing.
- Updated `register_filesystem_root` to accept `PackageOrigin`.
- Removed `ExternalPackageOrigin` enum from `ids.rs`.

### Phase D completed

- All `@core/*` packages registered with `PackageOrigin::Core` + `PackageBacking::ExternalBinding`.
- `@html` registered with `PackageOrigin::Builder` + `PackageBacking::BeanstalkSource`.
- `@web/canvas` registered with `PackageOrigin::Builder` + `PackageBacking::ExternalBinding`.
- Project-local source-backed packages registered with `PackageOrigin::ProjectLocal` + `PackageBacking::BeanstalkSource`.
- Project-local JS imports registered with `PackageOrigin::ProjectLocal` + `PackageBacking::ExternalBinding`.

### Phase G completed

- Test case directories and manifest IDs renamed from `source_lib_*` to `source_package_*` terminology.
- Manifest entries updated.
- Test config fixtures updated from `library_folders` to `package_folders`.
- Benchmark configs updated.
- Added direct origin/backing mapping assertions, binding-registry invariants and distinct integration coverage for both legacy package-folder keys.

---

# 1. Fixed terminology decisions

These definitions are final for this plan.

## 1.1 Package

A **package** is a named reusable `@...` import root and the future unit of dependency and
distribution.

Examples:

```text
@html
@core/math
@web/canvas
@blog
```

## 1.2 Module

A **module** is a directory-scoped Beanstalk compilation and visibility unit rooted by one
non-config `#*.bst`.

Do not rename modules to packages.

A source-backed package contains one or more modules. Its import prefix exposes the package root
module's public `export:` surface.

## 1.3 Library

**Library** is informal prose only. It is not a distinct compiler concept, registry type, config
concept, diagnostic category or documentation route category.

Do not introduce a `LibraryKind`, library registry or library-specific visibility rule.

## 1.4 Binding

A **binding** is a typed bridge to an implementation outside Beanstalk.

Examples:

- compiler-provided host operations
- builder runtime APIs
- provider-backed JavaScript imports
- opaque external types
- external constants and functions

`ExternalPackageRegistry` may remain the specialised compiler registry for binding-backed packages.
Do not call the package itself an "external package" in user-facing origin classification when
"Core package" or "Builder package" is the relevant fact.

## 1.5 Prelude

The **prelude** is implicit-import policy.

It is not a package origin or a package kind.

The current prelude exposes the bare `io` namespace as an alias to `@core/io`. Package metadata
belongs to `@core/io`; implicit visibility belongs to the prelude policy.

## 1.6 Orthogonal package axes

Implement:

```rust
pub enum PackageOrigin {
    Core,
    Standard,
    Builder,
    ProjectLocal,
    Dependency,
}

pub enum PackageBacking {
    BeanstalkSource,
    ExternalBinding,
}

pub struct PackageMetadata {
    pub origin: PackageOrigin,
    pub backing: PackageBacking,
}
```

Use the exact visibility and derive set appropriate to current builder APIs and tests.

Current mapping:

| Package | Origin | Backing |
|---|---|---|
| `@html` | `Builder` | `BeanstalkSource` |
| `@core/io` | `Core` | `ExternalBinding` |
| `@core/math` | `Core` | `ExternalBinding` |
| `@core/text` | `Core` | `ExternalBinding` |
| `@core/random` | `Core` | `ExternalBinding` |
| `@core/time` | `Core` | `ExternalBinding` |
| `@web/canvas` | `Builder` | `ExternalBinding` |
| configured project folder child | `ProjectLocal` | `BeanstalkSource` |
| annotated project-local `.js` import | `ProjectLocal` | `ExternalBinding` |

`Standard` and `Dependency` remain reserved metadata values. They must have focused unit coverage but
do not need active user-facing package instances yet.

---

# 2. Evidence and authority

Use evidence in this order:

1. Decisions in this plan
2. Focused compiler probes and tests
3. Current implementation owners
4. Canonical split compiler-design documentation
5. Focused user-facing Advanced references
6. `docs/language-overview.md`
7. Legacy consolidated compiler-design overview
8. Old website prose

The monoliths are no longer immutable in this dedicated synchronisation plan.

They must remain useful and must not knowingly contradict verified compiler behaviour.

Ordinary route migration workers remain prohibited from editing them. This plan is an explicitly
authorised terminology and parity synchronisation pass.

---

# 3. Read first

Read:

```text
AGENTS.md
docs/codebase-style-guide.md
docs/src/docs/codebase/style-guide/style-guide.bd
docs/src/docs/codebase/style-guide/validation.bd

docs/language-overview.md
docs/compiler-design-overview.md
docs/roadmap/plans/docs-language-migration.md
docs/roadmap/plans/hash-root-export-block-module-system-plan.md

docs/src/docs/project-structure/**
docs/src/docs/packages/**
docs/src/docs/packages/core/**
docs/src/docs/beandown/**
docs/src/docs/markdown/**

docs/src/docs/codebase/compiler-design/overview.bd
docs/src/docs/codebase/compiler-design/build-system-and-frontend-boundary/**
docs/src/docs/codebase/compiler-design/imports-packages-and-bindings/**
docs/src/docs/codebase/compiler-design/stages/project-structure/**
docs/src/docs/codebase/compiler-design/stages/header-parsing/**
docs/src/docs/codebase/compiler-design/stages/dependency-sorting/**
docs/src/docs/codebase/compiler-design/backend/external-js-and-runtime-assets/**

README.md
docs/src/docs/#page.bst
docs/src/docs/codebase/language/overview.bd
docs/src/docs/codebase/language/#page.bst
docs/src/docs/progress/#page.bst
```

Read implementation owners:

```text
src/builder_surface/
src/compiler_frontend/source_packages/
src/compiler_frontend/external_packages/
src/compiler_frontend/headers/public_exports.rs
src/compiler_frontend/headers/import_environment/
src/compiler_frontend/paths/
src/build_system/
src/build_system/create_project_modules/
src/build_system/project_config/
src/projects/settings.rs
src/projects/html_project/
packages/html/
```

Read focused tests and integration fixtures for:

```text
source-backed package
external package
builder package
project-local JavaScript import
prelude
package_folders and legacy package-folder keys
source/public prefix collision
grouped re-export alias
module public surface
Beandown public re-export
plain Markdown public re-export
```

---

# 4. Protected and editable files

## 4.1 Protected

Do not edit:

```text
AGENTS.md
CONTRIBUTING.md
docs/memory-management-design.md
docs/src/docs/codebase/memory-management/**
```

Do not alter source-language import syntax or module semantics.

## 4.2 Explicitly editable in this plan

This plan is authorised to edit:

```text
README.md
docs/language-overview.md
docs/compiler-design-overview.md
docs/roadmap/plans/docs-language-migration.md
docs/src/docs/progress/#page.bst
```

Update the canonical split compiler-design pages before mirroring relevant contracts into
`docs/compiler-design-overview.md`.

## 4.3 Generated output

Regenerate:

```text
docs/release/**
```

Do not edit generated HTML manually.

---

# 5. Concurrent work and baseline

Other TIR/compiler work may continue while this plan runs.

Record:

```sh
git branch --show-current
git log -12 --oneline
git status --short
git diff --name-only
```

Identify:

- pre-existing source edits
- pre-existing documentation edits
- generated output already modified
- files owned by active TIR work
- commits after the package-plan starting point

Do not reset unrelated work.

The package refactor must not touch template/TIR internals except for import path changes caused by
moving a shared module.

Capture baseline validation:

```sh
just validate
just bench-check
```

If baseline validation is already failing because of concurrent work, record the exact failure and
continue only when ownership is clear.

---

# 6. Refactor shape and naming policy

This is a pre-release compiler. Do not retain parallel terminology.

Prohibited compatibility shapes:

- `type SourceLibraryRegistry = SourcePackageRegistry`
- forwarding `libraries()` to `frontend_surface()`
- accepting both `library_folders` and `package_folders`
- duplicate diagnostic variants for old and new names
- `mod libraries { pub use crate::builder_surface::*; }`
- comments saying "library/package" throughout active code

Use one current API.

Use `git mv` for file and directory moves. Update references deliberately rather than running a
blind repository-wide replacement.

---

# 7. Phase A: shared package metadata

## 7.1 Add one shared owner

Create a focused package metadata owner under the renamed builder-surface package, for example:

```text
src/builder_surface/package_metadata.rs
```

Define:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PackageOrigin {
    Core,
    Standard,
    Builder,
    ProjectLocal,
    Dependency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PackageBacking {
    BeanstalkSource,
    ExternalBinding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PackageMetadata {
    pub origin: PackageOrigin,
    pub backing: PackageBacking,
}
```

Prefer constructors that prevent invalid backing assignment:

```rust
impl PackageMetadata {
    pub const fn source(origin: PackageOrigin) -> Self;
    pub const fn binding(origin: PackageOrigin) -> Self;
}
```

Do not encode prelude in `PackageOrigin`.

## 7.2 Source-backed packages

`SourcePackageRoot` must carry:

- import prefix
- root provider/path
- `PackageMetadata`

The source-package registry must only construct `BeanstalkSource` metadata.

## 7.3 Binding-backed packages

Replace `ExternalPackageOrigin` on `ExternalPackage` with shared `PackageMetadata`.

The external registry must only construct `ExternalBinding` metadata.

Do not let callers register:

```text
PackageBacking::BeanstalkSource
```

inside `ExternalPackageRegistry`.

Provider/runtime technology stays separate from package origin.

A project-local JavaScript package is:

```text
origin = ProjectLocal
backing = ExternalBinding
```

Its JavaScript/provider/runtime details remain in external import provider metadata.

## 7.4 Tests

Add focused tests for all active mappings and the two reserved origins.

Verify the backing invariant for both registries.

---

# 8. Phase B: rename compiler source-library concepts

Rename formal source-backed package concepts.

Required type/module changes include:

```text
SourceLibraryRegistry          -> SourcePackageRegistry
SourceLibraryRoot              -> SourcePackageRoot
PreparedSourceLibraryRoots     -> PreparedSourcePackageRoots
HashRootFileDiscovery          -> retain, unless a clearer generic root name already exists
source_libraries               -> source_packages
source_library                 -> source_package
file_library_membership        -> file_package_membership
source_library_public_exports  -> source_package_public_exports
source_library_root_files      -> source_package_root_files
SourceLibraryBoundaryCheckInput -> SourcePackageBoundaryCheckInput
ImportPublicSurfaceType::SourceLibrary -> SourcePackage
PublicExportSurfaceType::SourceLibrary -> SourcePackage
```

Apply the same rule to diagnostics, resolver APIs, counters, tests and comments.

## 8.1 File and directory moves

Move:

```text
src/compiler_frontend/source_libraries/
    -> src/compiler_frontend/source_packages/

src/build_system/create_project_modules/source_library_discovery.rs
    -> src/build_system/create_project_modules/source_package_discovery.rs
```

Rename focused tests accordingly.

## 8.2 Prepared roots

Keep the generic hash-root owner in the renamed source-package module.

Update file-level docs:

```text
WHAT: generic module-root and project-config identity shared by Stage 0,
source-backed package discovery and import validation.
```

Do not reintroduce facade terminology.

## 8.3 Public export construction

Rename source-library functions and maps in:

```text
src/compiler_frontend/headers/public_exports.rs
src/compiler_frontend/headers/module_symbols.rs
src/compiler_frontend/headers/import_environment/
src/compiler_frontend/module_dependencies.rs
```

Do not change export resolution behaviour.

The grouped public alias rule is already explicit in implementation:

```text
alias wins; otherwise use imported symbol name
```

Preserve it.

---

# 9. Phase C: rename the builder surface

## 9.1 Move the broad owner

Move:

```text
src/libraries/
    -> src/builder_surface/
```

The current owner contains more than packages:

- config keys
- source kinds
- external import providers
- caches and resolution tables
- runtime package metadata
- core package registration

`builder_surface` is the accurate formal owner.

Within it, use focused names:

```text
library_set.rs                 -> builder_surface.rs
source_library_registry.rs     -> source_package_registry.rs
core/                          -> core_packages/
```

Keep `external_import_providers/` and `source_file_kind_registry.rs` unless a narrower rename is
needed for clarity.

Update `crate::libraries` paths to `crate::builder_surface`.

Do not leave a compatibility `libraries` module.

## 9.2 Rename the API

Rename:

```text
LibrarySet                     -> BuilderSurface
BackendBuilder::libraries()    -> BackendBuilder::frontend_surface()
BuildBootstrap::libraries      -> BuildBootstrap::frontend_surface
```

Recommended shape:

```rust
pub struct BuilderSurface {
    pub binding_packages: ExternalPackageRegistry,
    pub source_packages: SourcePackageRegistry,
    pub config_keys: ProjectConfigKeyRegistry,
    pub external_import_providers: ExternalImportProviderRegistry,
    pub external_import_cache: ExternalImportProviderCache,
    pub external_import_resolution_table: ExternalImportResolutionTable,
    pub builder_runtime_packages: Vec<BuilderRuntimePackageMetadata>,
    pub source_file_kinds: SourceFileKindRegistry,
}
```

Use `binding_packages` for the field while retaining the specialised
`ExternalPackageRegistry` type.

Rename:

```text
with_mandatory_core()          -> with_mandatory_core()
expose_html_core_libraries()   -> expose_html_core_packages()
builtin_source_library_root()  -> builtin_source_package_root()
BUILTIN_SOURCE_LIBRARIES_DIR   -> BUILTIN_SOURCE_PACKAGES_DIR
```

`with_mandatory_core()` may keep its name because it already says core rather than library.

## 9.3 Move bundled source packages

Move:

```text
libraries/html/
    -> packages/html/
```

Update all path constants, docs, tests, build scripts and repository references.

The root filename may remain `#mod.bst` because it is cosmetic. Do not describe the suffix as
semantic.

---

# 10. Phase D: package origin and backing registration

Update every registration call.

## 10.1 Core packages

Register all `@core/*` packages with:

```text
PackageOrigin::Core
PackageBacking::ExternalBinding
```

This includes:

```text
@core/collections
@core/io
@core/math
@core/text
@core/random
@core/time
```

Do not register the prelude itself as a package or assign it a `PackageOrigin`.

## 10.2 Builder packages

Register:

```text
@html       -> Builder + BeanstalkSource
@web/canvas -> Builder + ExternalBinding
```

## 10.3 Project-local packages

Configured folder children:

```text
ProjectLocal + BeanstalkSource
```

Annotated project-local JavaScript imports:

```text
ProjectLocal + ExternalBinding
```

Replace:

```text
ExternalPackageOrigin::ProjectLocalJs
```

with generic origin metadata plus provider-specific JavaScript metadata.

## 10.4 Future origins

No active package should use:

```text
Standard
Dependency
```

Add tests showing they are representable without assigning current packages incorrectly.

---

# 11. Phase E: `package_folders`

Rename the project config key:

```text
library_folders -> package_folders
```

Rename:

```text
Config::library_folders
    -> package_folders

Config::has_explicit_library_folders
    -> has_explicit_package_folders
```

Update:

- config-key registration
- extraction and validation
- source-tree discovery
- dev-server watchers
- diagnostics
- benchmarks
- test fixtures
- documentation
- generated output

Keep the default directory name:

```text
lib
```

The folder convention does not need to be renamed to `packages`.

## 11.1 Compatibility decision

Do not accept `library_folders` as a temporary alias.

Beanstalk is pre-release and the compiler style guide rejects compatibility shims.

A config that uses `library_folders` must receive the ordinary unknown/deprecated-config diagnostic
chosen by the current config system.

Add one explicit rejection fixture.

## 11.2 Diagnostic types

Rename all config reasons and payload fields:

```text
UnsupportedLibraryFoldersValue       -> UnsupportedPackageFoldersValue
DuplicateLibraryFolder               -> DuplicatePackageFolder
InvalidLibraryFolder                 -> InvalidPackageFolder
ConfiguredLibraryFolderMissing       -> ConfiguredPackageFolderMissing
ConfiguredLibraryFolderNotDirectory  -> ConfiguredPackageFolderNotDirectory
SourceLibraryPrefixCollision         -> SourcePackagePrefixCollision
SourceLibraryBuilderPrefixCollision  -> SourcePackageBuilderPrefixCollision
EntryRootLibraryPrefixCollision      -> EntryRootPackagePrefixCollision
SourceLibraryMissingRoot             -> SourcePackageMissingRoot
SourceLibraryMultipleRoots           -> SourcePackageMultipleRoots
```

Preserve stable diagnostic codes when the behaviour is unchanged.

---

# 12. Phase F: diagnostics and import terminology

Standardise user-facing diagnostics on package terminology.

Examples:

```text
Unknown package prefix
Duplicate package name
Package name collides with a builder package
Package is unsupported by this builder
Source-backed package has no module root
Multiple roots found for project-local package
```

Use "source-backed package" when backing matters.

Use "binding-backed package" when the package is supplied through binding metadata.

Do not expose internal registry names in diagnostics.

Rename typed enums such as:

```text
ImportPublicSurfaceType::SourceLibrary
    -> ImportPublicSurfaceType::SourcePackage
```

Audit renderer strings and suggestions.

Keep module diagnostics as module diagnostics.

---

# 13. Phase G: tests and fixtures

Rename formal test terminology.

Examples:

```text
source_library_registry_tests
    -> source_package_registry_tests

source_lib_*
    -> source_package_*

config_library_folder_*
    -> config_package_folder_*
```

Historic fixture names may remain only when the fixture specifically tests migration from a legacy
spelling. Ordinary active tests must use package terminology.

Add regression coverage for:

## 13.1 Prefix resolution

- project-local source-backed package
- builder source-backed package
- core binding-backed package
- builder binding-backed package
- project-local JavaScript binding package
- longest package-prefix resolution

## 13.2 Origin and backing

Assert all current mappings from Section 1.6.

## 13.3 Builder availability

- unsupported package for selected builder
- builder source package available
- binding package target support rejection

## 13.4 Package folder discovery

- one string
- collection
- default `lib`
- duplicate folder
- missing folder
- absolute path
- `..`
- nested path
- source/binding prefix collision
- builder/project-local collision
- old `library_folders` rejected

## 13.5 Unchanged semantics

- prelude still exposes bare `io`
- module root rules unchanged
- public re-export aliases unchanged
- imported-root runtime suppression unchanged
- API-only artifact filtering unchanged

---

# 14. Focused documentation corrections

Keep the current concept split.

Rename the route:

```text
docs/src/docs/libraries/
    -> docs/src/docs/packages/

docs/src/docs/libraries/core/
    -> docs/src/docs/packages/core/
```

Rename public routes:

```text
/docs/libraries/
    -> /docs/packages/

/docs/libraries/core/
    -> /docs/packages/core/
```

Do not keep duplicate compatibility routes.

Update every link, pager, navbar entry, generated route and codebase index.

Rename the page:

```text
Libraries and imports
    -> Packages and imports
```

Rename concepts:

```text
source-libraries
    -> project-local-packages

core-builder-and-external-packages
    -> package-origins-and-backing
```

Other concepts may retain their names.

## 14.1 Public re-export aliases

Correct:

```text
docs/src/docs/project-structure/public-api.bd
docs/src/docs/packages/public-reexports.bd
```

The implementation rule is:

```text
A grouped alias inside `export:` becomes the public name.
Without an alias, the source name remains the public name.
The original source name is not also exported unless it is listed separately.
```

Remove the contradictory statement that `CardData as Card` exposes both names.

## 14.2 Cross-module example

Correct `module-visibility.bd`.

Use a real package/module public surface:

```text
lib/
└── components/
    ├── #api.bst
    └── utils.bst
```

Same-module code may import the implementation file.

Cross-module/package consumers import:

```beanstalk
import @components {
    internal_value,
}
```

They must not import the private implementation path.

Compile the example before documenting it.

## 14.3 Helper files

Correct:

```text
docs/src/docs/project-structure/entry-runtime-and-fragments.bd
```

Use:

```text
Normal helper files define reusable declarations. They do not own an export
block and cannot contain top-level runtime statements. The active module root
imports those declarations and decides which names become public.
```

Do not say helper files export declarations.

## 14.4 Imported roots

Correct observable wording in `module-roots.bd`:

```text
When another module imports a root, its top-level runtime and page-fragment
activity is inactive. Only the declarations selected by `export:` are visible
through the module boundary.
```

Remove "compiled only to validate" wording.

Rename:

```text
Migrating facade-era projects
    -> Migrating older module projects
```

## 14.5 Project-local package artifacts

Correct `project-local-packages.bd`.

A source-backed package root is imported, not an active HTML route.

Its top-level runtime body and fragments are inactive in the consuming project. It does not produce
an HTML artifact regardless of whether the source root contains authored runtime/template content.

Describe such content as inactive package-root activity, not an HTML page.

Do not limit the statement to "API-only" package roots.

## 14.6 Package axes

Replace the old core/builder/external page with a direct origin/backing explanation.

Use the mapping table from Section 1.6.

State:

- origin answers who owns/distributes the package
- backing answers how the compiler obtains its implementation
- the axes are independent
- source-backed packages contain Beanstalk modules
- binding-backed packages expose typed external symbols
- prelude is visibility policy
- package availability may still be builder/target-specific
- Library is informal prose only

## 14.7 README

Replace:

```text
@html is a core library
```

with:

```text
@html is the built-in, source-backed Builder package for HTML projects.
```

Keep the sentence concise.

## 14.8 Beandown `$md` links

Correct:

```text
docs/src/docs/beandown/implicit-markdown.bd
```

Beanstalk `$md` link syntax is:

```text
@/docs/path (Label)
@./relative (Label)
@../parent (Label)
@#anchor (Label)
@https://example.com (External)
```

Do not call `[Label](target)` standard syntax supported by `$md` unless a focused probe proves it.

Plain Markdown retains ordinary Markdown links.

## 14.9 Beandown root example

Use a neutral cosmetic filename such as:

```text
docs/#content.bst
```

State the suffix is cosmetic.

## 14.10 Plain Markdown

Update the rendering contract with focused probes for:

- images
- block quotes
- thematic breaks
- hard line breaks
- soft line breaks
- indented code blocks
- escapes
- entities
- autolinks

Document or explicitly reject each.

Add:

- images render literal `src`
- links render literal `href`
- neither is rewritten or tracked
- no compiler-recognised frontmatter
- no Beanstalk metadata surface
- YAML-like markers are ordinary Markdown input

Keep implementation option names in a short note after observable behaviour.

---

# 15. Synchronise `docs/language-overview.md`

This plan is explicitly authorised to update the language monolith.

Do not turn it into a tutorial. Keep it compact and compiler-facing.

## 15.1 Package terminology

Replace all formal library terminology:

```text
library_folders       -> package_folders
source library        -> source-backed package
project library       -> ProjectLocal package
core library          -> Core package
builder library       -> Builder package
external package category -> binding-backed package when backing is the point
library system        -> package system
```

Retain "library" only when used informally and clearly not as a compiler category.

Add the origin/backing table and current package mapping.

State that prelude is implicit-import policy.

Update the bundled `@html` path if the repository directory moves to `packages/html`.

Update route references to `/docs/packages/`.

## 15.2 Correct verified stale language facts

Apply these already verified corrections:

### Comments

The syntax summary must say:

```text
In ordinary code, `--` starts a single-line comment.
Template and Beandown bodies treat it as content.
```

### Raw string slices

Remove:

```beanstalk
raw_slice = `raw`
```

Remove claims that expression-position backticks create raw string slices.

State that raw backtick slices are not implemented in the current Alpha surface.

Remove the raw-backtick literal-delimiter example from Templates.

Keep backticks as `$md` inline-code delimiters inside Markdown content.

### Explicit copies

Add the place-only contract:

- visible binding
- field projection
- parenthesised place
- literals, templates, calls and computed expressions rejected

### Function parameter defaults

State that defaults fully fold in declaration-time compile-time context.

Reactive parameters do not support defaults.

### Fallible handling

Add supported inline bound catch:

```beanstalk
value = parse_number(text) catch |err| then err.code
```

State its binding is local to the inline fallback expression.

### Error-only functions

Keep `-> Error!` and normal fallthrough.

Do not show nested-block `return!` as current-valid in an error-only function.

Use a direct top-level `return!` example or label the current nested-block implementation gap.

### Result scope

Replace any "Result remains deferred" wording with:

```text
First-class public Result values are outside language design scope.
```

### Value-producing `if`

Add the supported inline choice predicate.

Do not present block `then` form as current-valid without the known implementation-gap note.

### Assertions

Use:

```beanstalk
assert(index < items.length())
assert(index < items.length(), "index must be in bounds")
```

State the second literal message is optional.

### Option equality

Add:

- option versus `none`
- option versus same option type with equality-capable inner
- option versus compatible inner value
- no ordering

### Option exhaustiveness

A full option match may omit `else =>` when it has:

- unguarded `none`
- unguarded present-value capture

Correct the broad "all non-choice scrutinees require else" statement.

### General capture discrepancy

Do not claim a general capture satisfies final exhaustiveness while the checker requires explicit
coverage/default.

Record the current compiler mismatch concisely.

### Stored named inserts

Remove the stored named-insert example from accepted current syntax.

Record it as an implementation gap.

### Map nesting

Add the current Alpha two-inline-level map type limit and named-alias workaround.

### Generics

Add the current unused generic parameter rejection.

### Type aliases

Add that aliases are transparent but alias spellings are not constructors. Construct through the
canonical nominal name.

### Import cycles

Replace:

```text
Circular imports are compilation errors.
```

with:

```text
Same-module file cycles are accepted when declarations resolve through the
dependency graph. Circular compile-time constant dependencies are rejected.
Cross-module/package visibility still applies.
```

### `$md` links

Add Beanstalk-aware link syntax and distinguish it from Plain Markdown links.

## 15.3 Existing module contracts

Keep the newly corrected generic hash-root, strict `export:`, active/imported-root and API-only
artifact sections.

Do not revert them.

---

# 16. Synchronise compiler-design documentation

## 16.1 Canonical split pages first

Update:

```text
docs/src/docs/codebase/compiler-design/overview.bd
docs/src/docs/codebase/compiler-design/build-system-and-frontend-boundary/**
docs/src/docs/codebase/compiler-design/imports-libraries-and-external-packages/**
docs/src/docs/codebase/compiler-design/stages/project-structure/**
docs/src/docs/codebase/compiler-design/stages/header-parsing/**
docs/src/docs/codebase/compiler-design/stages/dependency-sorting/**
docs/src/docs/codebase/compiler-design/backend/external-js-and-runtime-assets/**
```

Rename the route/concept:

```text
imports-libraries-and-external-packages
    -> imports-packages-and-bindings
```

Update links and generated routes.

## 16.2 Builder surface contract

Replace snippets and prose:

```rust
fn libraries(&self) -> LibrarySet
```

with:

```rust
fn frontend_surface(&self) -> BuilderSurface
```

Show the final real `BuilderSurface` struct.

Explain:

- source and binding registries remain separate
- shared `PackageMetadata` aligns origin/backing classification
- config keys/providers/source kinds are part of builder surface, not "libraries"
- package metadata is frontend-visible
- backend lowering metadata remains binding-specific

## 16.3 Import/package/binding contract

Replace formal source-library terminology with source-backed package.

State:

- project-local and builder source-backed packages resolve through `SourcePackageRegistry`
- binding-backed packages resolve through `ExternalPackageRegistry`
- package metadata does not change import syntax
- module public surfaces remain strict `export:` boundaries
- package origin/backing is orthogonal to module visibility
- prelude is a separate bare-name visibility policy

## 16.4 Stage 0

Update package folder discovery, collision ownership, source-tree indexing and root validation.

Use `package_folders` and `source_package_discovery.rs`.

## 16.5 Legacy monolith mirror

After canonical split pages are correct, update:

```text
docs/compiler-design-overview.md
```

Keep its legacy banner.

Synchronise:

- code-navigation paths
- `BuilderSurface`
- package metadata axes
- package folder key
- source package terminology
- binding package terminology
- package registration and provider boundaries
- prelude policy
- route links
- package/module distinction

Do not copy every implementation detail twice unnecessarily. Preserve the monolith's broad
architecture role.

---

# 17. Update the migration design plan

Edit:

```text
docs/roadmap/plans/docs-language-migration.md
```

## 17.1 Monolith maintenance policy

Replace the absolute prohibition with:

```text
Ordinary route migration workers must not edit the monoliths.

A dedicated, explicitly authorised parity or terminology synchronisation plan
may update `docs/language-overview.md` and `docs/compiler-design-overview.md`
after focused compiler verification. Canonical split compiler-design pages are
updated before the legacy compiler-design monolith.
```

Keep deletion/authority-switch protections.

## 17.2 Authority model

State that the language monolith remains authoritative during migration and must be maintained when
verified compiler behaviour proves it stale.

Do not knowingly preserve contradictions for the final audit.

## 17.3 Route terminology

Update:

```text
Libraries and Imports -> Packages and Imports
/docs/libraries/       -> /docs/packages/
```

Update concept maps and next-route planning.

## 17.4 Package terminology lesson

Add the origin/backing model and prelude policy.

## 17.5 Disparity ledger

Mark the monolith corrections from Section 15 as synchronised.

Keep unresolved implementation gaps visible.

---

# 18. Progress matrix and generated docs

Update the progress matrix terminology without changing statuses unless code changes genuinely
alter support.

Examples:

```text
source packages
package prefixes
package folders
binding-backed packages
builder surface
```

Do not use progress rows as normative package definitions.

Regenerate all docs.

Confirm stale routes are removed:

```text
docs/release/docs/libraries/
docs/release/docs/libraries/core/
```

Confirm new routes exist:

```text
docs/release/docs/packages/
docs/release/docs/packages/core/
```

---

# 19. Residual terminology audit

Run:

```sh
rg -n \
'SourceLibrary|source_library|source libraries|source library|LibrarySet|library_folders|libraries\(\)|ProjectLocalJs|ImportPublicSurfaceType::SourceLibrary|PublicExportSurfaceType::SourceLibrary' \
src tests benchmarks docs README.md
```

Every match must be:

- a deliberate historic migration note
- third-party terminology
- or a defect to remove

Also run:

```sh
rg -ni '\bfacade\b|#mod\.bst.*special|#page\.bst.*special' \
src docs tests
```

Active production/docs terminology should use:

```text
module root
public surface
export block
```

Historic fixture names may remain only with explicit justification.

Audit formal route links:

```sh
rg -n '/docs/libraries|@\.\./libraries|@libraries|docs/src/docs/libraries' \
README.md docs src tests
```

Expected: no active references after route rename.

---

# 20. Behaviour-preservation audit

Compare before/after behavior for:

- module root discovery
- public export aliases
- same-module visibility
- imported-root suppression
- API-only artifacts
- package prefix resolution
- prelude `io`
- external call IDs
- provider runtime assets
- Beandown imports
- plain Markdown imports
- HTML builder package availability

This patch changes terminology and metadata, not import syntax or module semantics.

No HIR shape should change except renamed metadata fields/types that do not alter values.

---

# 21. Validation

Run focused gates after each phase.

Recommended order:

```sh
cargo fmt --all --check

cargo test -p beanstalk source_package --quiet
cargo test -p beanstalk external_packages --quiet
cargo test -p beanstalk project_config --quiet
cargo test -p beanstalk create_project_modules --quiet

cargo run --quiet -- tests
cargo run --quiet -- check docs
cargo run --quiet -- build docs --release

just validate
just bench-check
git diff --check
```

If a test filter does not match after file/module renames, use the narrowest real owner filter and
report the exact command.

Do not claim validation that did not run.

---

# 22. Definition of done

Complete only when:

- package/module/binding/prelude definitions are consistent
- origin and backing are separate metadata axes
- every current package has correct metadata
- source and binding registries remain separate
- `SourceLibrary*` formal APIs are gone
- `LibrarySet` and `BackendBuilder::libraries()` are gone
- `ExternalPackageOrigin::ProjectLocalJs` is gone
- `library_folders` is rejected
- `package_folders` is fully supported
- diagnostics use package terminology
- no import/module semantics changed
- README calls `@html` a Builder package
- focused package docs use origin/backing terminology
- public re-export alias docs match implementation
- cross-module examples use the public package/module path
- Beandown links use `$md` syntax
- Plain Markdown renderer docs are complete
- language monolith contains no known verified stale claims listed in Section 15
- canonical compiler-design pages match final APIs
- legacy compiler-design monolith mirrors the final broad contracts
- migration plan allows controlled monolith synchronisation
- progress and generated docs are updated
- old generated Libraries routes are removed
- full validation passes
- benchmarks show no meaningful regression
- unrelated concurrent work remains intact

---

# 23. Required final report

## Starting state

Report branch, starting commit, concurrent work and pre-existing changes.

## Compiler refactor

List:

- file/directory moves
- type/function/field renames
- package metadata additions
- config key changes
- diagnostic renames

## Package mapping

Provide the final origin/backing table.

## Behaviour preservation

State whether module/import/prelude behavior changed. Expected:

```text
No source syntax or module semantics changed.
```

## Documentation corrections

List every focused-doc correction.

## Language monolith

List every updated stale rule and any remaining unresolved disparity.

## Compiler-design docs

List canonical split pages updated and the mirrored monolith sections.

## Migration policy

State the new controlled monolith-maintenance rule.

## Tests

List focused and integration coverage added or renamed.

## Validation

Report exact results of every command run.

## Generated output

List new package routes, removed stale routes and inspected pages.

## Residual terminology

Report remaining `library`/`facade` matches and justify each.

## Remaining uncertainty

Report only genuine unresolved semantic or implementation questions.
