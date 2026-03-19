use super::*;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::time::SystemTime;

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("beanstalk_project_modules_{prefix}_{unique}"))
}

#[test]
fn parses_config_constant_declarations() {
    let root = temp_dir("config_constants");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(
        &config_path,
        "#entry_root = \"src\"\n#dev_folder = \"dev\"\n#output_folder = \"release\"\n#name = \"docs\"\n#version = \"1.2.3\"\n#project = \"html\"\n#libraries = { @(libs), \"vendor\" }\n#custom_key = \"custom_value\"\n",
    )
    .expect("should write config");

    let mut config = Config::new(root.clone());
    parse_project_config_file(&mut config, &config_path).expect("config should parse");

    assert_eq!(config.entry_root, PathBuf::from("src"));
    assert_eq!(config.dev_folder, PathBuf::from("dev"));
    assert_eq!(config.release_folder, PathBuf::from("release"));
    assert_eq!(config.project_name, "docs");
    assert_eq!(config.version, "1.2.3");
    assert_eq!(config.settings.get("project"), Some(&"html".to_string()));
    assert_eq!(
        config.settings.get("custom_key"),
        Some(&"custom_value".to_string())
    );
    assert_eq!(
        config.libraries,
        vec![PathBuf::from("libs"), PathBuf::from("vendor")]
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_legacy_config_assignment_syntax() {
    let root = temp_dir("config_invalid_assignment");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "#output_folder dist\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let error =
        parse_project_config_file(&mut config, &config_path).expect_err("config should fail");

    assert!(
        error
            .msg
            .contains("Use standard constant syntax: '#output_folder = value'."),
        "unexpected error message: {}",
        error.msg
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_deprecated_src_config_key() {
    let root = temp_dir("config_src_rename");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "#src = \"src\"\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let error =
        parse_project_config_file(&mut config, &config_path).expect_err("config should fail");

    assert!(
        error.msg.contains("#entry_root"),
        "unexpected error message: {}",
        error.msg
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn discover_modules_uses_reachable_files_only() {
    let root = temp_dir("reachable_only");
    let src = root.join("src");
    fs::create_dir_all(src.join("libs")).expect("should create libs folder");
    fs::create_dir_all(src.join("styles")).expect("should create styles folder");
    fs::create_dir_all(src.join("docs")).expect("should create docs folder");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "#entry_root = \"src\"\n",
    )
    .expect("should write config");
    fs::write(src.join("#page.bst"), "import @(libs/html/basic)\n#[:ok]\n")
        .expect("should write entry");
    fs::write(src.join("#404.bst"), "#[:404]\n").expect("should write 404");
    fs::write(src.join("libs/html.bst"), "#basic = #[:basic]\n").expect("should write lib");
    fs::write(src.join("styles/docs.bst"), "#navbar = #[:nav]\n").expect("should write style");
    fs::write(src.join("docs/outdated.bst"), "this is invalid syntax")
        .expect("should write outdated file");

    let mut config = Config::new(root.clone());
    parse_project_config_file(&mut config, &root.join(settings::CONFIG_FILE_NAME))
        .expect("config parse");
    let style_directives = StyleDirectiveRegistry::built_ins();

    let modules = discover_all_modules_in_project(&config, &style_directives)
        .expect("module discovery should pass");

    assert_eq!(modules.len(), 2);

    let page_module = modules
        .iter()
        .find(|module| module.entry_point.file_name() == Some(OsStr::new("#page.bst")))
        .expect("should include #page module");
    let page_paths = page_module
        .input_files
        .iter()
        .map(|file| {
            file.source_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
        .collect::<HashSet<_>>();

    assert!(page_paths.contains("#page.bst"));
    assert!(page_paths.contains("html.bst"));
    assert!(!page_paths.contains("outdated.bst"));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn parses_libraries_variants_and_dedupes_entries() {
    let root = temp_dir("libraries_dedup");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(
        &config_path,
        "#libraries = { @(libs), \"vendor\", vendor, @(libs), \"vendor\" }\n",
    )
    .expect("should write config");

    let mut config = Config::new(root.clone());
    parse_project_config_file(&mut config, &config_path).expect("config should parse");

    assert_eq!(
        config.libraries,
        vec![PathBuf::from("libs"), PathBuf::from("vendor")],
        "libraries should dedupe while preserving first-seen order"
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn discover_modules_resolves_relative_imports_with_dot_segments() {
    let root = temp_dir("relative_imports");
    let src = root.join("src");
    fs::create_dir_all(src.join("components")).expect("should create components folder");
    fs::create_dir_all(src.join("shared")).expect("should create shared folder");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "#entry_root = \"src\"\n",
    )
    .expect("should write config");
    fs::write(
        src.join("#page.bst"),
        "import @(./components/widget)\nio(\"page\")\n",
    )
    .expect("should write page");
    fs::write(
        src.join("components/widget.bst"),
        "import @(../shared/common)\nio(\"widget\")\n",
    )
    .expect("should write widget file");
    fs::write(src.join("shared/common.bst"), "io(\"common\")\n").expect("should write common");

    let mut config = Config::new(root.clone());
    parse_project_config_file(&mut config, &root.join(settings::CONFIG_FILE_NAME))
        .expect("config parse");
    let style_directives = StyleDirectiveRegistry::built_ins();

    let modules = discover_all_modules_in_project(&config, &style_directives)
        .expect("module discovery should pass");
    assert_eq!(modules.len(), 1, "expected exactly one entry module");

    let discovered = modules[0]
        .input_files
        .iter()
        .map(|file| {
            file.source_path
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or_default()
                .to_string()
        })
        .collect::<HashSet<_>>();

    assert!(discovered.contains("#page.bst"));
    assert!(discovered.contains("widget.bst"));
    assert!(discovered.contains("common.bst"));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn source_root_takes_precedence_over_library_root_for_imports() {
    let root = temp_dir("source_root_precedence");
    let src = root.join("src");
    let libs = root.join("libs");
    fs::create_dir_all(src.join("helpers")).expect("should create source helpers");
    fs::create_dir_all(libs.join("helpers")).expect("should create library helpers");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "#entry_root = \"src\"\n#libraries = { @(libs) }\n",
    )
    .expect("should write config");
    fs::write(
        src.join("#page.bst"),
        "import @(helpers/theme)\nio(\"page\")\n",
    )
    .expect("should write page");
    fs::write(src.join("helpers/theme.bst"), "io(\"source\")\n").expect("should write source");
    fs::write(libs.join("helpers/theme.bst"), "io(\"library\")\n").expect("should write library");

    let mut config = Config::new(root.clone());
    parse_project_config_file(&mut config, &root.join(settings::CONFIG_FILE_NAME))
        .expect("config parse");
    let style_directives = StyleDirectiveRegistry::built_ins();

    let modules = discover_all_modules_in_project(&config, &style_directives)
        .expect("module discovery should pass");
    assert_eq!(modules.len(), 1, "expected exactly one entry module");

    let source_theme = fs::canonicalize(src.join("helpers/theme.bst")).expect("canonical source");
    let library_theme =
        fs::canonicalize(libs.join("helpers/theme.bst")).expect("canonical library");
    let discovered_paths = modules[0]
        .input_files
        .iter()
        .map(|file| file.source_path.clone())
        .collect::<HashSet<_>>();

    assert!(
        discovered_paths.contains(&source_theme),
        "source-root import should resolve to source root first"
    );
    assert!(
        !discovered_paths.contains(&library_theme),
        "library root candidate should not win when source root contains the target"
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn discover_all_modules_finds_multiple_hash_entries_per_root() {
    let root = temp_dir("multi_hash_entries");
    let src = root.join("src");
    fs::create_dir_all(src.join("nested")).expect("should create nested folder");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "#entry_root = \"src\"\n",
    )
    .expect("should write config");
    fs::write(src.join("#page.bst"), "io(\"page\")\n").expect("should write #page");
    fs::write(src.join("#layout.bst"), "io(\"layout\")\n").expect("should write #layout");
    fs::write(src.join("nested/#lib.bst"), "io(\"lib\")\n").expect("should write nested #lib");
    fs::write(src.join("nested/file.bst"), "io(\"regular\")\n").expect("should write regular");

    let mut config = Config::new(root.clone());
    parse_project_config_file(&mut config, &root.join(settings::CONFIG_FILE_NAME))
        .expect("config parse");
    let style_directives = StyleDirectiveRegistry::built_ins();

    let modules = discover_all_modules_in_project(&config, &style_directives)
        .expect("module discovery should pass");
    assert_eq!(
        modules.len(),
        3,
        "expected one module per '#*.bst' root entry"
    );

    let entry_names = modules
        .iter()
        .map(|module| {
            module
                .entry_point
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or_default()
                .to_string()
        })
        .collect::<HashSet<_>>();

    assert!(entry_names.contains("#page.bst"));
    assert!(entry_names.contains("#layout.bst"));
    assert!(entry_names.contains("#lib.bst"));

    fs::remove_dir_all(&root).expect("should remove temp root");
}
