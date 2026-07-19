//! Self-tests for canonical manifest parsing and fixture ordering.
//!
//! WHAT: protects manifest validation and its authoritative case order.
//! WHY: manifest metadata controls which canonical fixtures the runner executes.

use super::super::fixture::load_test_suite_from_root;
use super::super::{CaseRole, EXPECT_FILE_NAME, INPUT_DIR_NAME, MANIFEST_FILE_NAME};
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
fn rejects_manifest_case_with_unknown_role() {
    let root = temp_dir("manifest_unknown_role");
    fs::create_dir_all(&root).expect("should create root");
    write_success_fixture(&root, "case");

    fs::write(
        root.join(MANIFEST_FILE_NAME),
        "[[case]]\nid = \"case\"\npath = \"case\"\ntags = [\"coverage\"]\nrole = \"unknown\"\n",
    )
    .expect("should write manifest");

    let Err(error) = load_test_suite_from_root(&root) else {
        panic!("unknown manifest role should be rejected");
    };
    assert!(error.contains("case"), "unexpected: {error}");
    assert!(error.contains("unknown"), "unexpected: {error}");
    assert!(error.contains("role"), "unexpected: {error}");

    fs::remove_dir_all(&root).expect("should clean up temp fixture root");
}

#[test]
fn parses_every_supported_manifest_role() {
    let supported_roles = [
        ("primary", CaseRole::Primary),
        ("boundary", CaseRole::Boundary),
        ("backend", CaseRole::Backend),
        ("adversarial", CaseRole::Adversarial),
        ("smoke", CaseRole::Smoke),
    ];

    for (spelling, expected) in supported_roles {
        assert_eq!(CaseRole::parse(spelling), Ok(expected));
    }
}

#[test]
fn retains_primary_manifest_case_without_contract_for_policy_evaluation() {
    let root = temp_dir("manifest_primary_without_contract");
    fs::create_dir_all(&root).expect("should create root");
    write_success_fixture(&root, "case");

    fs::write(
        root.join(MANIFEST_FILE_NAME),
        "[[case]]\nid = \"case\"\npath = \"case\"\ntags = [\"coverage\"]\nrole = \"primary\"\n",
    )
    .expect("should write manifest");

    let suite = load_test_suite_from_root(&root)
        .expect("cross-case primary policy should be evaluated after loading");
    assert_eq!(suite.cases[0].role, Some(CaseRole::Primary));
    assert_eq!(suite.cases[0].contract, None);

    fs::remove_dir_all(&root).expect("should clean up temp fixture root");
}

#[test]
fn rejects_manifest_case_with_empty_contract() {
    let root = temp_dir("manifest_empty_contract");
    fs::create_dir_all(&root).expect("should create root");
    write_success_fixture(&root, "case");

    fs::write(
        root.join(MANIFEST_FILE_NAME),
        "[[case]]\nid = \"case\"\npath = \"case\"\ntags = [\"coverage\"]\ncontract = \" \"\n",
    )
    .expect("should write manifest");

    let Err(error) = load_test_suite_from_root(&root) else {
        panic!("empty manifest contract should be rejected");
    };
    assert!(error.contains("case"), "unexpected: {error}");
    assert!(error.contains("empty"), "unexpected: {error}");
    assert!(error.contains("contract"), "unexpected: {error}");

    fs::remove_dir_all(&root).expect("should clean up temp fixture root");
}

#[test]
fn retains_duplicate_primary_contracts_for_policy_evaluation() {
    let root = temp_dir("manifest_duplicate_primary_contract");
    fs::create_dir_all(&root).expect("should create root");
    write_success_fixture(&root, "case_a");
    write_success_fixture(&root, "case_b");

    fs::write(
        root.join(MANIFEST_FILE_NAME),
        "[[case]]\nid = \"case_a\"\npath = \"case_a\"\ntags = [\"coverage\"]\ncontract = \"language.example\"\nrole = \"primary\"\n\n[[case]]\nid = \"case_b\"\npath = \"case_b\"\ntags = [\"coverage\"]\ncontract = \"language.example\"\nrole = \"primary\"\n",
    )
    .expect("should write manifest");

    let suite = load_test_suite_from_root(&root)
        .expect("cross-case primary policy should be evaluated after loading");
    assert_eq!(suite.cases.len(), 2);
    assert_eq!(suite.cases[0].contract, Some("language.example".to_owned()));
    assert_eq!(suite.cases[1].contract, Some("language.example".to_owned()));

    fs::remove_dir_all(&root).expect("should clean up temp fixture root");
}

#[test]
fn permits_shared_contracts_for_distinct_non_primary_roles() {
    let root = temp_dir("manifest_shared_non_primary_contract");
    fs::create_dir_all(&root).expect("should create root");
    write_success_fixture(&root, "case_a");
    write_success_fixture(&root, "case_b");

    fs::write(
        root.join(MANIFEST_FILE_NAME),
        "[[case]]\nid = \"case_a\"\npath = \"case_a\"\ntags = [\"coverage\"]\ncontract = \"language.example\"\nrole = \"boundary\"\n\n[[case]]\nid = \"case_b\"\npath = \"case_b\"\ntags = [\"coverage\"]\ncontract = \"language.example\"\nrole = \"backend\"\n",
    )
    .expect("should write manifest");

    let suite =
        load_test_suite_from_root(&root).expect("non-primary cases may share a semantic contract");
    assert_eq!(suite.cases.len(), 2);
    assert_eq!(suite.cases[0].role, Some(CaseRole::Boundary));
    assert_eq!(suite.cases[1].role, Some(CaseRole::Backend));

    fs::remove_dir_all(&root).expect("should clean up temp fixture root");
}

#[test]
fn retains_unclassified_manifest_metadata_as_optional() {
    let root = temp_dir("manifest_unclassified_metadata");
    fs::create_dir_all(&root).expect("should create root");
    write_success_fixture(&root, "case");

    fs::write(
        root.join(MANIFEST_FILE_NAME),
        "[[case]]\nid = \"case\"\npath = \"case\"\ntags = [\"coverage\"]\n",
    )
    .expect("should write manifest");

    let suite = load_test_suite_from_root(&root).expect("unclassified case should load");
    let case = &suite.cases[0];
    assert_eq!(case.case_id, "case");
    assert_eq!(case.tags, vec!["coverage"]);
    assert_eq!(case.contract, None);
    assert_eq!(case.role, None);

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
