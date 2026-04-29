//! Unit tests for HTML tracked-asset planning and passthrough emission.

use super::*;
use crate::compiler_frontend::compiler_warnings::WarningKind;
use crate::compiler_frontend::paths::path_resolution::{CompileTimePathBase, CompileTimePathKind};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_tests::test_support::temp_dir;
use crate::projects::html_project::tests::test_support::{
    RenderedPathUsageInput, create_test_module, expect_bytes_output, rendered_path_usage,
};
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn entry_root_asset_emits_site_relative_output_path() {
    let root = temp_dir("tracked_assets_entry_root");
    fs::create_dir_all(root.join("assets")).expect("should create assets dir");
    fs::write(root.join("assets/logo.png"), [1_u8, 2, 3]).expect("should write asset");

    let mut string_table = StringTable::new();
    let mut module = create_test_module(root.join("#page.bst"), &mut string_table);
    module.hir.rendered_path_usages.push(rendered_path_usage(
        &mut string_table,
        RenderedPathUsageInput {
            source_path_components: &["assets", "logo.png"],
            public_path_components: &["assets", "logo.png"],
            filesystem_path: root.join("assets/logo.png"),
            base: CompileTimePathBase::EntryRoot,
            kind: CompileTimePathKind::File,
            source_file_scope_components: &["#page.bst"],
            line_number: 1,
        },
    ));

    let planned = plan_module_tracked_assets(&module, Path::new("index.html"), &mut string_table)
        .expect("planning succeeds");

    assert_eq!(planned.assets.len(), 1);
    assert!(planned.warnings.is_empty());
    assert_eq!(
        planned.assets[0].emitted_output_path,
        PathBuf::from("assets/logo.png")
    );
    assert_eq!(
        planned.assets[0].reference_kind,
        HtmlTrackedAssetReferenceKind::SiteRelative
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn entry_root_asset_emits_visible_entry_root_path() {
    let root = temp_dir("tracked_assets_entry_root");
    fs::create_dir_all(root.join("src/images")).expect("should create entry-root dir");
    fs::write(root.join("src/images/logo.png"), [4_u8, 5, 6]).expect("should write asset");

    let mut string_table = StringTable::new();
    let mut module = create_test_module(root.join("src/#page.bst"), &mut string_table);
    module.hir.rendered_path_usages.push(rendered_path_usage(
        &mut string_table,
        RenderedPathUsageInput {
            source_path_components: &["images", "logo.png"],
            public_path_components: &["images", "logo.png"],
            filesystem_path: root.join("src/images/logo.png"),
            base: CompileTimePathBase::EntryRoot,
            kind: CompileTimePathKind::File,
            source_file_scope_components: &["src", "#page.bst"],
            line_number: 1,
        },
    ));

    let planned = plan_module_tracked_assets(&module, Path::new("index.html"), &mut string_table)
        .expect("planning succeeds");

    assert_eq!(
        planned.assets[0].emitted_output_path,
        PathBuf::from("images/logo.png")
    );
    assert_eq!(
        planned.assets[0].reference_kind,
        HtmlTrackedAssetReferenceKind::SiteRelative
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn relative_asset_emits_relative_to_final_page_directory() {
    let root = temp_dir("tracked_assets_relative");
    fs::create_dir_all(root.join("src/docs/guide/img")).expect("should create nested dir");
    fs::write(root.join("src/docs/guide/img/logo.png"), [7_u8, 8, 9]).expect("should write asset");

    let mut string_table = StringTable::new();
    let mut module = create_test_module(root.join("src/docs/guide/#page.bst"), &mut string_table);
    module.hir.rendered_path_usages.push(rendered_path_usage(
        &mut string_table,
        RenderedPathUsageInput {
            source_path_components: &[".", "img", "logo.png"],
            public_path_components: &[".", "img", "logo.png"],
            filesystem_path: root.join("src/docs/guide/img/logo.png"),
            base: CompileTimePathBase::RelativeToFile,
            kind: CompileTimePathKind::File,
            source_file_scope_components: &["src", "docs", "guide", "#page.bst"],
            line_number: 3,
        },
    ));

    let planned = plan_module_tracked_assets(
        &module,
        Path::new("docs/guide/index.html"),
        &mut string_table,
    )
    .expect("planning succeeds");

    assert_eq!(
        planned.assets[0].emitted_output_path,
        PathBuf::from("docs/guide/img/logo.png")
    );
    assert_eq!(
        planned.assets[0].reference_kind,
        HtmlTrackedAssetReferenceKind::RelativeToPage
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn duplicate_same_source_and_output_dedupes_within_module() {
    let root = temp_dir("tracked_assets_dedupe");
    fs::create_dir_all(root.join("assets")).expect("should create assets dir");
    fs::write(root.join("assets/logo.png"), [1_u8, 2, 3]).expect("should write asset");

    let mut string_table = StringTable::new();
    let mut module = create_test_module(root.join("#page.bst"), &mut string_table);
    let usage = rendered_path_usage(
        &mut string_table,
        RenderedPathUsageInput {
            source_path_components: &["assets", "logo.png"],
            public_path_components: &["assets", "logo.png"],
            filesystem_path: root.join("assets/logo.png"),
            base: CompileTimePathBase::EntryRoot,
            kind: CompileTimePathKind::File,
            source_file_scope_components: &["#page.bst"],
            line_number: 1,
        },
    );
    module.hir.rendered_path_usages.push(usage.clone());
    module.hir.rendered_path_usages.push(usage);

    let planned = plan_module_tracked_assets(&module, Path::new("index.html"), &mut string_table)
        .expect("planning succeeds");

    assert_eq!(planned.assets.len(), 1);

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn public_root_directory_usage_is_ignored() {
    let root = temp_dir("tracked_assets_public_root_directory");
    fs::create_dir_all(root.join("src")).expect("should create entry root");

    let mut string_table = StringTable::new();
    let mut module = create_test_module(root.join("src/#page.bst"), &mut string_table);
    module.hir.rendered_path_usages.push(rendered_path_usage(
        &mut string_table,
        RenderedPathUsageInput {
            source_path_components: &[],
            public_path_components: &[],
            filesystem_path: root.join("src"),
            base: CompileTimePathBase::EntryRoot,
            kind: CompileTimePathKind::Directory,
            source_file_scope_components: &["src", "#page.bst"],
            line_number: 2,
        },
    ));

    let planned = plan_module_tracked_assets(&module, Path::new("index.html"), &mut string_table)
        .expect("planning succeeds");

    assert!(planned.assets.is_empty());
    assert!(planned.warnings.is_empty());

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn non_asset_directory_link_is_ignored() {
    let root = temp_dir("tracked_assets_directory_link");
    fs::create_dir_all(root.join("src/docs/guide/subdir")).expect("should create nested dir");

    let mut string_table = StringTable::new();
    let mut module = create_test_module(root.join("src/docs/guide/#page.bst"), &mut string_table);
    module.hir.rendered_path_usages.push(rendered_path_usage(
        &mut string_table,
        RenderedPathUsageInput {
            source_path_components: &[".", "subdir"],
            public_path_components: &[".", "subdir"],
            filesystem_path: root.join("src/docs/guide/subdir"),
            base: CompileTimePathBase::RelativeToFile,
            kind: CompileTimePathKind::Directory,
            source_file_scope_components: &["src", "docs", "guide", "#page.bst"],
            line_number: 5,
        },
    ));

    let planned = plan_module_tracked_assets(
        &module,
        Path::new("docs/guide/index.html"),
        &mut string_table,
    )
    .expect("planning succeeds");

    assert!(planned.assets.is_empty());
    assert!(planned.warnings.is_empty());

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn large_asset_warning_dedupes_to_first_render_location() {
    let root = temp_dir("tracked_assets_large_warning");
    fs::create_dir_all(root.join("assets")).expect("should create assets dir");
    fs::write(
        root.join("assets/video.mp4"),
        vec![0_u8; (DEFAULT_LARGE_TRACKED_ASSET_WARNING_BYTES as usize) + 1],
    )
    .expect("should write large asset");

    let mut string_table = StringTable::new();
    let mut module = create_test_module(root.join("#page.bst"), &mut string_table);
    let first_usage = rendered_path_usage(
        &mut string_table,
        RenderedPathUsageInput {
            source_path_components: &["assets", "video.mp4"],
            public_path_components: &["assets", "video.mp4"],
            filesystem_path: root.join("assets/video.mp4"),
            base: CompileTimePathBase::EntryRoot,
            kind: CompileTimePathKind::File,
            source_file_scope_components: &["#page.bst"],
            line_number: 2,
        },
    );
    let second_usage = rendered_path_usage(
        &mut string_table,
        RenderedPathUsageInput {
            source_path_components: &["assets", "video.mp4"],
            public_path_components: &["assets", "video.mp4"],
            filesystem_path: root.join("assets/video.mp4"),
            base: CompileTimePathBase::EntryRoot,
            kind: CompileTimePathKind::File,
            source_file_scope_components: &["#page.bst"],
            line_number: 8,
        },
    );
    module.hir.rendered_path_usages.push(first_usage);
    module.hir.rendered_path_usages.push(second_usage);

    let planned = plan_module_tracked_assets(&module, Path::new("index.html"), &mut string_table)
        .expect("planning succeeds");

    assert_eq!(planned.warnings.len(), 1);
    assert!(matches!(
        planned.warnings[0].warning_kind,
        WarningKind::LargeTrackedAsset
    ));
    assert_eq!(planned.warnings[0].location.start_pos.line_number, 2);

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn emit_tracked_assets_reads_source_bytes_into_binary_outputs() {
    let root = temp_dir("tracked_assets_emit");
    fs::create_dir_all(root.join("assets")).expect("should create assets dir");
    fs::write(root.join("assets/logo.png"), [9_u8, 8, 7, 6]).expect("should write asset");

    let mut string_table = StringTable::new();
    let mut module = create_test_module(root.join("#page.bst"), &mut string_table);
    module.hir.rendered_path_usages.push(rendered_path_usage(
        &mut string_table,
        RenderedPathUsageInput {
            source_path_components: &["assets", "logo.png"],
            public_path_components: &["assets", "logo.png"],
            filesystem_path: root.join("assets/logo.png"),
            base: CompileTimePathBase::EntryRoot,
            kind: CompileTimePathKind::File,
            source_file_scope_components: &["#page.bst"],
            line_number: 1,
        },
    ));
    let planned = plan_module_tracked_assets(&module, Path::new("index.html"), &mut string_table)
        .expect("planning succeeds");

    let output_files =
        emit_tracked_assets(&planned.assets, &string_table).expect("emission succeeds");
    assert_eq!(
        expect_bytes_output(&output_files, "assets/logo.png"),
        [9_u8, 8, 7, 6]
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}
