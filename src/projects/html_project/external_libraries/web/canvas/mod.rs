//! Built-in `@web/canvas` package registration.
//!
//! WHAT: parses the embedded `canvas.js` asset and registers `@web/canvas` as a builder-runtime
//!       virtual package with runtime asset metadata.
//! WHY: `@web/canvas` is a JS-only built-in library that shares the same parser, registry,
//!      and emission path as project-local `.js` imports.

use crate::compiler_frontend::external_packages::{ExternalPackageOrigin, ExternalPackageRegistry};
use crate::libraries::external_import_providers::provider::{
    BuilderRuntimePackageMetadata, RuntimeAssetIdentity,
};
use crate::projects::html_project::external_js::package_registration::{
    register_parsed_js_library, required_runtime_imports_from_parsed,
};
use crate::projects::html_project::external_js::parser::parse_js_library;
use crate::projects::html_project::external_js::runtime_module_registry::RuntimeModuleRegistry;
use std::path::PathBuf;

/// Registers the built-in `@web/canvas` package in the external package registry.
///
/// WHAT: parses the authored `canvas.js` file, registers opaque types and functions with
///       `ExternalPackageOrigin::BuilderRuntime`, and returns metadata so the build system
///       can emit the JS asset and generated glue through the existing `ModuleExternalImport`
///       path.
/// WHY: built-in JS-backed packages and project-local `.js` imports share the same runtime
///      asset/glue emission path.
pub fn register_web_canvas_package(
    registry: &mut ExternalPackageRegistry,
) -> BuilderRuntimePackageMetadata {
    let source = include_str!("canvas.js");
    let parsed = parse_js_library(source, &RuntimeModuleRegistry::v1());

    // Built-in packages should not have parser diagnostics. If they do, it is a compiler bug.
    assert!(
        parsed.diagnostics.is_empty(),
        "Built-in @web/canvas JS library has parser diagnostics: {:?}",
        parsed.diagnostics
    );

    let package_id = registry
        .register_package("@web/canvas", ExternalPackageOrigin::BuilderRuntime)
        .expect("builtin package registration should not collide");

    register_parsed_js_library(package_id, &parsed, registry)
        .expect("builtin package registration should not fail");

    let required_runtime_imports = required_runtime_imports_from_parsed(&parsed);

    let canonical_source_path = canvas_js_path();

    BuilderRuntimePackageMetadata {
        package_id,
        runtime_asset: Some(RuntimeAssetIdentity {
            canonical_source_path,
            asset_kind: "js".to_owned(),
        }),
        required_runtime_imports,
    }
}

fn canvas_js_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/projects/html_project/external_libraries/web/canvas/canvas.js")
}
