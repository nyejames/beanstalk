//! Self-tests for canonical manifest parsing and fixture ordering.
//!
//! WHAT: protects manifest validation and its authoritative case order.
//! WHY: manifest metadata controls which canonical fixtures the runner executes.

use super::super::fixture::load_test_suite_from_root;
use super::super::{EXPECT_FILE_NAME, INPUT_DIR_NAME, MANIFEST_FILE_NAME};
use crate::compiler_tests::test_support::temp_dir;
use std::fs;
use std::path::Path;

fn write_success_fixture(root: &Path, case_name: &str) {
    let case_root = root.join(case_name);
    let input_root = case_root.join(INPUT_DIR_NAME);
    fs::create_dir_all(&input_root).expect("should create fixture input directory");
    fs::write(input_root.join("#page.bst"), "#[:ok]\n").expect("should write fixture source");
    fs::write(
        case_root.join(EXPECT_FILE_NAME),
        "[backends.html]\nmode = \"success\"\nwarnings = \"forbid\"\n",
    )
    .expect("should write expect file");
}

#[test]
fn rejects_manifest_case_without_tags() {
    let root = temp_dir("manifest_missing_tags");
    fs::create_dir_all(&root).expect("should create root");
    write_success_fixture(&root, "case");

    fs::write(
        root.join(MANIFEST_FILE_NAME),
        "[[case]]\nid = \"case\"\npath = \"case\"\n",
    )
    .expect("should write manifest");

    let Err(error) = load_test_suite_from_root(&root) else {
        panic!("manifest missing tags should be rejected");
    };
    assert!(
        error.contains("missing required tags"),
        "unexpected: {error}"
    );

    fs::remove_dir_all(&root).expect("should clean up temp fixture root");
}

#[test]
fn manifest_order_is_preserved() {
    let root = temp_dir("manifest_order");
    fs::create_dir_all(&root).expect("should create root");

    write_success_fixture(&root, "case_a");
    write_success_fixture(&root, "case_b");
    write_success_fixture(&root, "case_c");

    fs::write(
        root.join(MANIFEST_FILE_NAME),
        "[[case]]\nid = \"case_b\"\npath = \"case_b\"\ntags = [\"ordered\"]\n\n[[case]]\nid = \"case_a\"\npath = \"case_a\"\ntags = [\"ordered\"]\n\n[[case]]\nid = \"case_c\"\npath = \"case_c\"\ntags = [\"ordered\"]\n",
    )
    .expect("should write manifest");

    let suite = load_test_suite_from_root(&root).expect("suite should load");
    let names = suite
        .cases
        .iter()
        .map(|case| case.display_name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec!["case_b [html]", "case_a [html]", "case_c [html]"]
    );

    fs::remove_dir_all(&root).expect("should clean up temp fixture root");
}

#[test]
fn manifest_must_declare_every_fixture_directory() {
    let root = temp_dir("manifest_authoritative");
    fs::create_dir_all(&root).expect("should create root");

    write_success_fixture(&root, "case_a");
    write_success_fixture(&root, "case_b");

    fs::write(
        root.join(MANIFEST_FILE_NAME),
        "[[case]]\nid = \"case_a\"\npath = \"case_a\"\ntags = [\"coverage\"]\n",
    )
    .expect("should write manifest");

    let Err(error) = load_test_suite_from_root(&root) else {
        panic!("manifest should reject undeclared fixtures");
    };
    assert!(
        error.contains("undeclared fixtures"),
        "unexpected error: {error}"
    );

    fs::remove_dir_all(&root).expect("should clean up temp fixture root");
}
