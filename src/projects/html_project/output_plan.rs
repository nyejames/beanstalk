//! Canonical HTML route and artifact output planning for the HTML builder.
//!
//! WHAT: derives filesystem artifact locations from entry-file paths and route conventions.
//! WHY: both the JS-only and HTML+Wasm builder paths need to agree on where outputs land.
//!      Centralising this here means there is one place to change layout conventions later.
//!
//! This module owns path derivation only. Artifact emission (lowering JS, Wasm, generating HTML)
//! lives in the respective `js_path` and `wasm/artifacts` modules.

use crate::compiler_frontend::compiler_errors::CompilerError;
use std::path::{Component, Path, PathBuf};

/// A resolved output plan for one HTML route.
///
/// `html_path` is the physical HTML file location on disk. For JS-only mode this equals
/// `logical_html_path`; for Wasm mode both can differ only when legacy non-folder paths
/// are normalised into `<route>/index.html` form.
///
/// `js_path` and `wasm_path` are `None` for JS-only builds and `Some` for Wasm builds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HtmlRouteOutputPlan {
    /// Logical route path derived from the entry file (e.g. `about/index.html`).
    pub logical_html_path: PathBuf,
    /// Physical HTML file destination (may differ from `logical_html_path` in Wasm mode).
    pub html_path: PathBuf,
    /// Bootstrap JS path colocated with the HTML file (Wasm mode only).
    pub js_path: Option<PathBuf>,
    /// Wasm binary path colocated with the HTML file (Wasm mode only).
    pub wasm_path: Option<PathBuf>,
}

/// Build an output plan for one entry file in HTML+Wasm mode.
///
/// WHY: Wasm builds colocate JS and Wasm alongside `index.html` under a per-route folder.
pub(crate) fn plan_wasm_output(
    entry_point: &Path,
    entry_root: Option<&Path>,
) -> Result<HtmlRouteOutputPlan, CompilerError> {
    let logical_html_path = derive_logical_html_path(entry_point, entry_root)?;
    let route_base = derive_wasm_route_base(&logical_html_path)?;

    let (html_path, js_path, wasm_path) = if route_base.as_os_str().is_empty() {
        (
            PathBuf::from("index.html"),
            PathBuf::from("page.js"),
            PathBuf::from("page.wasm"),
        )
    } else {
        (
            route_base.join("index.html"),
            route_base.join("page.js"),
            route_base.join("page.wasm"),
        )
    };

    Ok(HtmlRouteOutputPlan {
        logical_html_path,
        html_path,
        js_path: Some(js_path),
        wasm_path: Some(wasm_path),
    })
}

/// Derive the logical HTML output path from an entry file.
///
/// WHAT: maps Beanstalk entry conventions to HTML paths:
/// - `#page.bst` (root) → `index.html`
/// - `#page.bst` (subdir) → `<subdir>/index.html`
/// - `#about.bst` (root) → `about/index.html` (folder-backed)
/// - Single-file builds strip `#` prefix and use legacy `.html` extension.
pub(crate) fn derive_logical_html_path(
    entry_point: &Path,
    entry_root: Option<&Path>,
) -> Result<PathBuf, CompilerError> {
    if let Some(entry_root) = entry_root {
        return derive_logical_html_path_from_entry_root(entry_point, entry_root);
    }

    // Single-file build: legacy flat naming.
    let file_stem = entry_point
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("main");

    if file_stem == "#page" {
        Ok(PathBuf::from("index.html"))
    } else {
        let route_name = file_stem.strip_prefix('#').unwrap_or(file_stem);
        Ok(PathBuf::from(format!("{route_name}.html")))
    }
}

fn derive_logical_html_path_from_entry_root(
    entry_point: &Path,
    entry_root: &Path,
) -> Result<PathBuf, CompilerError> {
    // Route derivation is deterministic: discovery order never affects output paths.
    let relative_entry = entry_point.strip_prefix(entry_root).map_err(|_| {
        CompilerError::file_error(
            entry_point,
            format!(
                "HTML entry '{}' is not inside the configured entry root '{}'.",
                entry_point.display(),
                entry_root.display(),
            ),
        )
    })?;
    let file_stem = relative_entry
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .ok_or_else(|| {
            CompilerError::file_error(
                entry_point,
                format!(
                    "HTML entry '{}' is missing a valid file stem.",
                    entry_point.display(),
                ),
            )
        })?;
    let parent = relative_entry.parent().unwrap_or_else(|| Path::new(""));

    if file_stem == "#page" {
        if parent.as_os_str().is_empty() {
            return Ok(PathBuf::from("index.html"));
        }
        return Ok(parent.join("index.html"));
    }

    let route_name = file_stem.strip_prefix('#').unwrap_or(file_stem);
    // Directory builds emit folder-backed routes so dev/prod routing semantics match
    // and every page has one canonical `.../index.html` backing file.
    let route_base = if parent.as_os_str().is_empty() {
        PathBuf::from(route_name)
    } else {
        parent.join(route_name)
    };
    Ok(route_base.join("index.html"))
}

/// Derive the legacy flat `.html` alias for a canonical folder-backed HTML route.
///
/// WHAT: maps `about/index.html` to `about.html` and `docs/basics/index.html` to
/// `docs/basics.html`.
/// WHY: route-shape cleanup should delete only aliases that are deterministic equivalents of the
/// current canonical route.
pub(crate) fn derive_legacy_route_alias(canonical_html_path: &Path) -> Option<PathBuf> {
    if canonical_html_path.is_absolute() || canonical_html_path.as_os_str().is_empty() {
        return None;
    }

    for component in canonical_html_path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                return None;
            }
        }
    }

    if canonical_html_path
        .file_name()
        .and_then(|name| name.to_str())
        != Some("index.html")
    {
        return None;
    }

    let parent = canonical_html_path.parent()?;
    if parent.as_os_str().is_empty() {
        return None;
    }

    Some(parent.with_extension("html"))
}

/// Derive the route folder base from a logical HTML path for Wasm artifact co-location.
///
/// - `index.html` → `` (root)
/// - `about/index.html` → `about`
/// - `about.html` (legacy) → `about`
fn derive_wasm_route_base(logical_html_path: &Path) -> Result<PathBuf, CompilerError> {
    if logical_html_path == Path::new("index.html") {
        return Ok(PathBuf::new());
    }

    if logical_html_path.file_name().and_then(|name| name.to_str()) == Some("index.html") {
        return Ok(logical_html_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default());
    }

    // Legacy flat path: normalise to route folder.
    if logical_html_path.extension().and_then(|ext| ext.to_str()) != Some("html") {
        return Err(CompilerError::file_error(
            logical_html_path,
            format!(
                "HTML Wasm output conversion expected an '.html' path, got '{}'",
                logical_html_path.display()
            ),
        ));
    }
    Ok(logical_html_path.with_extension(""))
}

#[cfg(test)]
#[path = "tests/output_plan_tests.rs"]
mod tests;
