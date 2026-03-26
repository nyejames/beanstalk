//! Tests for canonical HTML output planning.

use super::*;
use std::path::{Path, PathBuf};

#[test]
fn wasm_root_page_colocates_artifacts_at_root() {
    let plan = plan_wasm_output(
        Path::new("/project/src/#page.bst"),
        Some(Path::new("/project/src")),
    )
    .expect("should plan");
    assert_eq!(plan.html_path, PathBuf::from("index.html"));
    assert_eq!(plan.js_path, Some(PathBuf::from("page.js")));
    assert_eq!(plan.wasm_path, Some(PathBuf::from("page.wasm")));
}

#[test]
fn wasm_about_route_colocates_artifacts_in_route_folder() {
    let plan = plan_wasm_output(
        Path::new("/project/src/#about.bst"),
        Some(Path::new("/project/src")),
    )
    .expect("should plan");
    assert_eq!(plan.html_path, PathBuf::from("about/index.html"));
    assert_eq!(plan.js_path, Some(PathBuf::from("about/page.js")));
    assert_eq!(plan.wasm_path, Some(PathBuf::from("about/page.wasm")));
}

#[test]
fn legacy_alias_for_nested_route_points_to_flat_html_file() {
    assert_eq!(
        derive_legacy_route_alias(Path::new("docs/basics/index.html")),
        Some(PathBuf::from("docs/basics.html"))
    );
}

#[test]
fn legacy_alias_for_root_level_route_points_to_flat_html_file() {
    assert_eq!(
        derive_legacy_route_alias(Path::new("about/index.html")),
        Some(PathBuf::from("about.html"))
    );
}

#[test]
fn homepage_has_no_legacy_flat_alias() {
    assert_eq!(derive_legacy_route_alias(Path::new("index.html")), None);
}

#[test]
fn legacy_alias_derivation_rejects_non_normal_paths() {
    for path in [
        Path::new("../docs/index.html"),
        Path::new("./docs/index.html"),
        Path::new("/docs/index.html"),
        Path::new("docs/basics.html"),
    ] {
        assert_eq!(derive_legacy_route_alias(path), None);
    }
}
