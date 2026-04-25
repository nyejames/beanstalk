use super::compile_project_frontend;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_tests::test_support::temp_dir;
use crate::projects::settings::Config;
use std::fs;

// ── Single-file flow ──────────────────────────────────────────────────────────

#[test]
fn single_file_compiles_minimal_bst() {
    let dir = temp_dir("single_file_ok");
    fs::create_dir_all(&dir).expect("should create temp dir");
    let bst_path = dir.join("test.bst");
    fs::write(&bst_path, "x ~= 10\n").expect("should write .bst");

    let mut config = Config::new(bst_path.clone());
    let style_directives = StyleDirectiveRegistry::built_ins();
    let mut string_table = StringTable::new();

    let result = compile_project_frontend(
        &mut config,
        &[],
        &style_directives,
        &ExternalPackageRegistry::new(),
        &mut string_table,
    );

    assert!(result.is_ok(), "expected Ok for minimal .bst file");
    assert_eq!(
        result.expect("checked above").len(),
        1,
        "expected exactly one module"
    );

    fs::remove_dir_all(&dir).expect("should remove temp dir");
}

#[test]
fn single_file_rejects_wrong_extension() {
    let dir = temp_dir("single_file_wrong_ext");
    fs::create_dir_all(&dir).expect("should create temp dir");
    let txt_path = dir.join("test.txt");
    fs::write(&txt_path, "x ~= 10\n").expect("should write .txt");

    let mut config = Config::new(txt_path);
    let style_directives = StyleDirectiveRegistry::built_ins();
    let mut string_table = StringTable::new();

    let result = compile_project_frontend(
        &mut config,
        &[],
        &style_directives,
        &ExternalPackageRegistry::new(),
        &mut string_table,
    );

    assert!(result.is_err(), "expected Err for wrong extension");
    let messages = result.err().expect("checked above");
    assert!(!messages.errors.is_empty(), "expected at least one error");
    let error_text = &messages.errors[0].msg;
    assert!(
        error_text.contains(".bst"),
        "expected error to mention .bst, got: {error_text}"
    );

    fs::remove_dir_all(&dir).expect("should remove temp dir");
}

#[test]
fn single_file_rejects_missing_file() {
    let dir = temp_dir("single_file_missing");
    fs::create_dir_all(&dir).expect("should create temp dir");
    let missing_path = dir.join("does_not_exist.bst");

    let mut config = Config::new(missing_path);
    let style_directives = StyleDirectiveRegistry::built_ins();
    let mut string_table = StringTable::new();

    let result = compile_project_frontend(
        &mut config,
        &[],
        &style_directives,
        &ExternalPackageRegistry::new(),
        &mut string_table,
    );

    assert!(result.is_err(), "expected Err for missing file");
    assert!(
        !result.err().expect("checked above").errors.is_empty(),
        "expected at least one error"
    );

    fs::remove_dir_all(&dir).expect("should remove temp dir");
}

// ── Directory-project flow ────────────────────────────────────────────────────

#[test]
fn directory_project_compiles_single_entry_module() {
    let dir = temp_dir("dir_single_module");
    fs::create_dir_all(&dir).expect("should create temp dir");
    fs::write(dir.join("#config.bst"), "").expect("should write config");
    fs::write(dir.join("#page.bst"), "x ~= 10\n").expect("should write entry");

    let mut config = Config::new(dir.clone());
    let style_directives = StyleDirectiveRegistry::built_ins();
    let mut string_table = StringTable::new();

    let result = compile_project_frontend(
        &mut config,
        &[],
        &style_directives,
        &ExternalPackageRegistry::new(),
        &mut string_table,
    );

    assert!(
        result.is_ok(),
        "expected Ok for single-module directory project"
    );
    assert_eq!(
        result.expect("checked above").len(),
        1,
        "expected exactly one module"
    );

    fs::remove_dir_all(&dir).expect("should remove temp dir");
}

#[test]
fn directory_project_discovers_multiple_entry_modules() {
    let dir = temp_dir("dir_multi_module");
    fs::create_dir_all(&dir).expect("should create temp dir");
    fs::write(dir.join("#config.bst"), "").expect("should write config");
    fs::write(dir.join("#page.bst"), "x ~= 10\n").expect("should write page");
    fs::write(dir.join("#layout.bst"), "y ~= 20\n").expect("should write layout");

    let mut config = Config::new(dir.clone());
    let style_directives = StyleDirectiveRegistry::built_ins();
    let mut string_table = StringTable::new();

    let result = compile_project_frontend(
        &mut config,
        &[],
        &style_directives,
        &ExternalPackageRegistry::new(),
        &mut string_table,
    );

    assert!(
        result.is_ok(),
        "expected Ok for multi-module directory project"
    );
    assert_eq!(
        result.expect("checked above").len(),
        2,
        "expected exactly two modules"
    );

    fs::remove_dir_all(&dir).expect("should remove temp dir");
}

#[test]
fn directory_project_rejects_missing_entry_root() {
    let dir = temp_dir("dir_missing_entry_root");
    fs::create_dir_all(&dir).expect("should create temp dir");
    // Config declares an entry_root that does not exist.
    fs::write(dir.join("#config.bst"), "#entry_root = \"nonexistent\"\n")
        .expect("should write config");

    let mut config = Config::new(dir.clone());
    let style_directives = StyleDirectiveRegistry::built_ins();
    let mut string_table = StringTable::new();

    // Parse config so entry_root is applied to Config.
    let config_path = dir.join("#config.bst");
    let parse_result = crate::build_system::project_config::parse_project_config_file(
        &mut config,
        &config_path,
        &style_directives,
        &mut string_table,
    );
    assert!(parse_result.is_ok(), "config parse should succeed");

    let result = compile_project_frontend(
        &mut config,
        &[],
        &style_directives,
        &ExternalPackageRegistry::new(),
        &mut string_table,
    );

    assert!(result.is_err(), "expected Err for missing entry root");
    assert!(
        !result.err().expect("checked above").errors.is_empty(),
        "expected at least one error"
    );

    fs::remove_dir_all(&dir).expect("should remove temp dir");
}
