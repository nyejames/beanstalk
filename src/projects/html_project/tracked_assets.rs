//! HTML tracked-asset planning and passthrough emission.
//!
//! WHAT: interprets frontend-rendered path usages as HTML builder tracked assets when they map to
//! emitted files, chooses output paths, emits warnings, and converts files into ordinary
//! `OutputFile` artifacts.
//!
//! WHY: tracked-asset placement and emission are builder policy. The frontend records semantic
//! path facts, then the HTML builder decides which rendered file paths become emitted artifacts
//! and which directory-like links remain plain rendered URLs.

use crate::build_system::build::{FileKind, Module, OutputFile};
use crate::build_system::output_cleanup::validate_relative_output_path;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_warnings::{CompilerWarning, WarningKind};
use crate::compiler_frontend::paths::path_resolution::{CompileTimePathBase, CompileTimePathKind};
use crate::compiler_frontend::paths::rendered_path_usage::RenderedPathUsage;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use rustc_hash::FxHashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const DEFAULT_LARGE_TRACKED_ASSET_WARNING_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HtmlTrackedAssetReferenceKind {
    /// WHAT: the rendered URL is site-rooted (`/assets/logo.png` plus optional `#origin`).
    /// WHY: rooted and entry-root paths keep one stable emitted location independent of page path.
    SiteRelative,
    /// WHAT: the rendered URL stays relative to the page that emitted it (`./img/logo.png`).
    /// WHY: relative asset placement is page-local policy in v1 and may duplicate outputs.
    RelativeToPage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AssetPipelinePlan {
    /// WHAT: emit the source bytes unchanged at the chosen output path.
    /// WHY: v1 proves tracked-asset graph ownership before adding transforms, hashing, or plugins.
    Passthrough,
    // Planned(html-assets): image transforms (for example WebP conversion).
    // Planned(html-assets): hashed output names and URL rewrite support.
    // Planned(html-assets): pluggable asset processors after graph contract stabilization.
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HtmlTrackedAsset {
    /// Canonical source file on disk used for dedupe, emission, and warnings.
    pub source_filesystem_path: PathBuf,
    /// Authored compile-time path as written in source.
    pub source_path: crate::compiler_frontend::interned_path::InternedPath,
    /// Builder-chosen output path under the project output root.
    pub emitted_output_path: PathBuf,
    /// Whether the HTML reference is rooted or page-relative.
    pub reference_kind: HtmlTrackedAssetReferenceKind,
    /// Source byte size used for large-asset warnings and future pipeline planning.
    pub byte_size: u64,
    /// First render location retained for diagnostics and warning anchoring.
    pub source_location: SourceLocation,
    /// Current/future pipeline behavior for this asset.
    pub pipeline_plan: AssetPipelinePlan,
}

#[derive(Debug)]
pub(crate) struct PlannedTrackedAssets {
    pub assets: Vec<HtmlTrackedAsset>,
    pub warnings: Vec<CompilerWarning>,
}

/// Plan tracked assets for one HTML module using the final emitted HTML path as the relative base.
///
/// WHAT: turns semantic rendered-path usages into deduplicated tracked assets plus warnings.
/// WHY: relative asset placement depends on the page output path, which only the HTML builder
/// knows after route planning.
pub(crate) fn plan_module_tracked_assets(
    module: &Module,
    html_output_path: &Path,
    string_table: &mut StringTable,
) -> Result<PlannedTrackedAssets, CompilerMessages> {
    let mut assets_by_output: FxHashMap<PathBuf, HtmlTrackedAsset> = FxHashMap::default();
    let mut warnings = Vec::new();
    let mut large_warning_locations_by_source: FxHashMap<PathBuf, SourceLocation> =
        FxHashMap::default();

    for usage in &module.hir.rendered_path_usages {
        let Some(asset) = plan_one_tracked_asset(module, usage, html_output_path, string_table)?
        else {
            continue;
        };

        if asset.byte_size >= DEFAULT_LARGE_TRACKED_ASSET_WARNING_BYTES {
            match large_warning_locations_by_source.entry(asset.source_filesystem_path.clone()) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(asset.source_location.clone());
                    warnings.push(build_large_tracked_asset_warning(
                        module,
                        &asset,
                        &asset.source_location,
                        string_table,
                    ));
                }
                std::collections::hash_map::Entry::Occupied(_) => {}
            }
        }

        match assets_by_output.entry(asset.emitted_output_path.clone()) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(asset);
            }
            std::collections::hash_map::Entry::Occupied(existing) => {
                if existing.get().source_filesystem_path != asset.source_filesystem_path {
                    return Err(CompilerMessages::from_error(
                        conflicting_asset_output_error(&asset, existing.get(), string_table),
                        string_table.clone(),
                    ));
                }
            }
        }
    }

    let mut assets = assets_by_output.into_values().collect::<Vec<_>>();
    assets.sort_by(|left, right| left.emitted_output_path.cmp(&right.emitted_output_path));

    Ok(PlannedTrackedAssets { assets, warnings })
}

/// Read planned tracked assets from disk as ordinary `OutputFile::Bytes` artifacts.
///
/// WHAT: keeps tracked assets inside the central output writer rather than writing them directly.
/// WHY: manifest tracking and stale cleanup already operate on `OutputFile`.
pub(crate) fn emit_tracked_assets(
    assets: &[HtmlTrackedAsset],
    string_table: &StringTable,
) -> Result<Vec<OutputFile>, CompilerMessages> {
    let mut output_files = Vec::with_capacity(assets.len());

    for asset in assets {
        let bytes = fs::read(&asset.source_filesystem_path).map_err(|error| {
            file_error_messages(
                &asset.source_filesystem_path,
                format!(
                    "Failed to read tracked asset '{}': {error}",
                    asset.source_filesystem_path.display()
                ),
                string_table,
            )
        })?;

        output_files.push(OutputFile::new(
            asset.emitted_output_path.clone(),
            FileKind::Bytes(bytes),
        ));
    }

    Ok(output_files)
}

fn plan_one_tracked_asset(
    module: &Module,
    usage: &RenderedPathUsage,
    html_output_path: &Path,
    string_table: &mut StringTable,
) -> Result<Option<HtmlTrackedAsset>, CompilerMessages> {
    if usage.kind == CompileTimePathKind::Directory {
        if directory_usage_requires_tracked_asset_error(module, usage, string_table) {
            return Err(CompilerMessages::from_error(
                directory_asset_error(module, usage, string_table),
                string_table.clone(),
            ));
        }

        return Ok(None);
    }

    let canonical_source = fs::canonicalize(&usage.filesystem_path).map_err(|error| {
        let error = CompilerError::file_error(
            &usage.filesystem_path,
            format!(
                "Failed to canonicalize tracked asset source '{}': {error}",
                usage.filesystem_path.display()
            ),
            string_table,
        );
        CompilerMessages::from_error(error, string_table.clone())
    })?;
    let byte_size = fs::metadata(&canonical_source)
        .map_err(|error| {
            let error = CompilerError::file_error(
                &canonical_source,
                format!(
                    "Failed to read tracked asset metadata '{}': {error}",
                    canonical_source.display()
                ),
                string_table,
            );
            CompilerMessages::from_error(error, string_table.clone())
        })?
        .len();

    let (emitted_output_path, reference_kind) =
        derive_emitted_output_path(module, usage, html_output_path, string_table)?;

    Ok(Some(HtmlTrackedAsset {
        source_filesystem_path: canonical_source,
        source_path: usage.source_path.clone(),
        emitted_output_path,
        reference_kind,
        byte_size,
        source_location: usage.render_location.clone(),
        pipeline_plan: AssetPipelinePlan::Passthrough,
    }))
}

/// Decide whether a rendered directory usage should stay a plain link or fail as a tracked asset.
///
/// WHAT: keeps general directory links such as `@/` and `@./subdir` renderable without emission.
/// WHY: tracked assets are file-only in v1, but the legacy `@assets/...` directory lane would
/// imply recursive copying behavior and must still fail instead of silently doing nothing.
fn directory_usage_requires_tracked_asset_error(
    _module: &Module,
    usage: &RenderedPathUsage,
    string_table: &StringTable,
) -> bool {
    usage.base == CompileTimePathBase::ProjectRootFolder
        && usage
            .public_path
            .as_components()
            .first()
            .map(|segment| string_table.resolve(*segment) == "assets")
            .unwrap_or(false)
}

fn derive_emitted_output_path(
    _module: &Module,
    usage: &RenderedPathUsage,
    html_output_path: &Path,
    string_table: &StringTable,
) -> Result<(PathBuf, HtmlTrackedAssetReferenceKind), CompilerMessages> {
    let emitted_output_path = match usage.base {
        CompileTimePathBase::ProjectRootFolder | CompileTimePathBase::EntryRoot => {
            usage.public_path.to_path_buf(string_table)
        }
        CompileTimePathBase::RelativeToFile => {
            let mut emitted_output_path = html_output_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default();

            for component in usage.source_path.as_components() {
                match string_table.resolve(*component) {
                    "." => {}
                    ".." => {
                        emitted_output_path.pop();
                    }
                    segment => emitted_output_path.push(segment),
                }
            }

            emitted_output_path
        }
    };

    validate_relative_output_path(&emitted_output_path, string_table).map_err(|messages| {
        CompilerMessages::from_error(
            messages.errors.into_iter().next().unwrap_or_else(|| {
                CompilerError::compiler_error(
                    "Tracked asset output validation failed without an error.",
                )
            }),
            messages.string_table,
        )
    })?;

    let reference_kind = match usage.base {
        CompileTimePathBase::RelativeToFile => HtmlTrackedAssetReferenceKind::RelativeToPage,
        CompileTimePathBase::ProjectRootFolder | CompileTimePathBase::EntryRoot => {
            HtmlTrackedAssetReferenceKind::SiteRelative
        }
    };

    Ok((emitted_output_path, reference_kind))
}

fn directory_asset_error(
    _module: &Module,
    usage: &RenderedPathUsage,
    string_table: &StringTable,
) -> CompilerError {
    CompilerError::new_rule_error(
        format!(
            "Rendered directory path '{}' cannot be emitted as an HTML tracked asset. Tracked assets are file-only in this pass.",
            usage.source_path.to_portable_string(string_table)
        ),
        usage.render_location.clone(),
    )
}

fn conflicting_asset_output_error(
    new_asset: &HtmlTrackedAsset,
    existing_asset: &HtmlTrackedAsset,
    string_table: &mut StringTable,
) -> CompilerError {
    CompilerError::file_error(
        &new_asset.source_filesystem_path,
        format!(
            "Tracked asset '{}' would emit to '{}', but that output path is already claimed by '{}'.",
            new_asset.source_filesystem_path.display(),
            new_asset.emitted_output_path.display(),
            existing_asset.source_filesystem_path.display(),
        ),
        string_table,
    )
}

fn build_large_tracked_asset_warning(
    _module: &Module,
    asset: &HtmlTrackedAsset,
    first_location: &SourceLocation,
    string_table: &StringTable,
) -> CompilerWarning {
    let authored_path = asset.source_path.to_portable_string(string_table);
    CompilerWarning::new(
        &format!(
            "Tracked asset '{authored_path}' is {} and will be emitted unchanged in the current HTML tracked-asset pipeline. Consider external hosting or a lighter asset when appropriate; future optimization support will not make large media automatically suitable for bundling.",
            format_byte_size(asset.byte_size)
        ),
        first_location.clone(),
        WarningKind::LargeTrackedAsset,
    )
}

fn file_error_messages(
    path: &Path,
    msg: impl Into<String>,
    string_table: &StringTable,
) -> CompilerMessages {
    CompilerMessages::file_error(path, msg, string_table)
}

fn format_byte_size(byte_size: u64) -> String {
    let mib = byte_size as f64 / (1024.0 * 1024.0);
    format!("{mib:.1} MiB")
}

#[cfg(test)]
#[path = "tests/tracked_assets_tests.rs"]
mod tests;
