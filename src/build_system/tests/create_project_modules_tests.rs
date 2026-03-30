use super::*;
use crate::build_system::project_config::parse_project_config_file;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages, ErrorType};
use crate::compiler_frontend::paths::path_resolution::{
    ProjectPathResolver, resolve_project_entry_root,
};
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
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

fn configured_resolver(config: &Config) -> ProjectPathResolver {
    // WHAT: rebuilds the same canonical resolver the real project build uses.
    // WHY: module-discovery tests should exercise the exact path rules used in production.
    let project_root = fs::canonicalize(&config.entry_dir).expect("project root should resolve");
    let entry_root =
        fs::canonicalize(resolve_project_entry_root(config)).expect("entry root should resolve");

    ProjectPathResolver::new(project_root, entry_root, &config.root_folders)
        .expect("project path resolver should build")
}

fn test_style_directives() -> StyleDirectiveRegistry {
    StyleDirectiveRegistry::built_ins()
}

fn parse_project_config_for_test(
    config: &mut Config,
    config_path: &std::path::Path,
    style_directives: &StyleDirectiveRegistry,
) -> Result<(), CompilerMessages> {
    let mut string_table = StringTable::new();
    parse_project_config_file(config, config_path, style_directives, &mut string_table)
}

fn discover_modules_for_test(
    config: &Config,
    resolver: &ProjectPathResolver,
    style_directives: &StyleDirectiveRegistry,
) -> Result<Vec<DiscoveredModule>, CompilerError> {
    let mut string_table = StringTable::new();
    discover_all_modules_in_project(config, resolver, style_directives, &mut string_table)
}

fn discover_modules_for_test_messages(
    config: &Config,
    resolver: &ProjectPathResolver,
    style_directives: &StyleDirectiveRegistry,
) -> Result<Vec<DiscoveredModule>, CompilerMessages> {
    let mut string_table = StringTable::new();
    discover_all_modules_in_project(config, resolver, style_directives, &mut string_table)
        .map_err(|error| CompilerMessages::from_error(error, string_table))
}

#[test]
fn parses_config_constant_declarations() {
    let root = temp_dir("config_constants");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(
        &config_path,
        "#entry_root = \"src\"\n#dev_folder = \"dev\"\n#output_folder = \"release\"\n#name = \"docs\"\n#version = \"1.2.3\"\n#project = \"html\"\n#page_url_style = \"trailing_slash\"\n#redirect_index_html = true\n#root_folders = { @lib, \"vendor\" }\n#custom_key = \"custom_value\"\n",
    )
    .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect("config should parse");

    assert_eq!(config.entry_root, PathBuf::from("src"));
    assert_eq!(config.dev_folder, PathBuf::from("dev"));
    assert_eq!(config.release_folder, PathBuf::from("release"));
    assert_eq!(config.project_name, "docs");
    assert_eq!(config.version, "1.2.3");
    assert_eq!(config.settings.get("project"), Some(&"html".to_string()));
    assert_eq!(
        config.settings.get("page_url_style"),
        Some(&"trailing_slash".to_string())
    );
    assert_eq!(
        config.settings.get("redirect_index_html"),
        Some(&"true".to_string())
    );
    assert_eq!(
        config.settings.get("custom_key"),
        Some(&"custom_value".to_string())
    );
    assert_eq!(
        config.root_folders,
        vec![PathBuf::from("lib"), PathBuf::from("vendor")]
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
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    assert!(
        !messages.errors.is_empty(),
        "should have at least one error"
    );
    let error = &messages.errors[0];
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
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    assert!(
        !messages.errors.is_empty(),
        "should have at least one error"
    );
    let error = &messages.errors[0];
    assert!(
        error.msg.contains("#entry_root"),
        "unexpected error message: {}",
        error.msg
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_legacy_libraries_config_key() {
    let root = temp_dir("config_libraries_rename");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "#libraries = { @lib }\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    assert!(
        !messages.errors.is_empty(),
        "should have at least one error"
    );
    let error = &messages.errors[0];
    assert!(
        error.msg.contains("#root_folders"),
        "unexpected error message: {}",
        error.msg
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_invalid_root_folder_entries() {
    let root = temp_dir("invalid_root_folders");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "#root_folders = { @core/html }\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    assert!(
        !messages.errors.is_empty(),
        "should have at least one error"
    );
    let error = &messages.errors[0];
    assert!(
        error.msg.contains("single top-level folder name"),
        "unexpected error message: {}",
        error.msg
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn malformed_import_syntax_keeps_precise_location_during_module_discovery() {
    let root = temp_dir("malformed_import_location");
    let src = root.join("src");
    fs::create_dir_all(&src).expect("should create src dir");
    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "#entry_root = \"src\"\n",
    )
    .expect("should write config");
    fs::write(src.join("#page.bst"), "import\n#[:ok]\n").expect("should write malformed entry");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");
    let resolver = configured_resolver(&config);
    let messages = match discover_modules_for_test_messages(&config, &resolver, &style_directives) {
        Ok(_) => panic!("malformed import should fail discovery"),
        Err(messages) => messages,
    };

    assert_eq!(messages.errors.len(), 1);
    let error = &messages.errors[0];
    assert_eq!(
        error.location.scope.to_path_buf(&messages.string_table),
        src.join("#page.bst")
            .canonicalize()
            .expect("entry file path should canonicalize")
    );
    assert_eq!(error.location.start_pos.line_number, 1);
    assert_eq!(error.location.start_pos.char_column, 1);
    assert!(
        error
            .msg
            .contains("Expected a path after the 'import' keyword")
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn config_import_parse_failure_keeps_precise_location_in_compiler_messages() {
    let root = temp_dir("config_import_location");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);
    fs::write(&config_path, "import\n").expect("should write malformed config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    assert_eq!(messages.errors.len(), 1);
    let error = &messages.errors[0];
    assert_eq!(
        error.location.scope.to_path_buf(&messages.string_table),
        config_path
    );
    assert_eq!(error.location.start_pos.line_number, 1);
    assert_eq!(error.location.start_pos.char_column, 0);
    assert!(
        error
            .msg
            .contains("Expected a path after the 'import' keyword")
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
    fs::write(src.join("#page.bst"), "import @libs/html/basic\n#[:ok]\n")
        .expect("should write entry");
    fs::write(src.join("#404.bst"), "#[:404]\n").expect("should write 404");
    fs::write(src.join("libs/html.bst"), "#basic = #[:basic]\n").expect("should write lib");
    fs::write(src.join("styles/docs.bst"), "#navbar = #[:nav]\n").expect("should write style");
    fs::write(src.join("docs/outdated.bst"), "this is invalid syntax")
        .expect("should write outdated file");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config parse");
    let resolver = configured_resolver(&config);

    let modules = discover_modules_for_test(&config, &resolver, &style_directives)
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
fn parses_root_folders_variants_and_dedupes_entries() {
    let root = temp_dir("root_folders_dedup");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(
        &config_path,
        "#root_folders = { @lib, \"vendor\", vendor, @lib, \"vendor\" }\n",
    )
    .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect("config should parse");

    assert_eq!(
        config.root_folders,
        vec![PathBuf::from("lib"), PathBuf::from("vendor")],
        "root folders should dedupe while preserving first-seen order"
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
        "import @./components/widget\nio(\"page\")\n",
    )
    .expect("should write page");
    fs::write(
        src.join("components/widget.bst"),
        "import @../shared/common\nio(\"widget\")\n",
    )
    .expect("should write widget file");
    fs::write(src.join("shared/common.bst"), "io(\"common\")\n").expect("should write common");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config parse");
    let resolver = configured_resolver(&config);

    let modules = discover_modules_for_test(&config, &resolver, &style_directives)
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
fn entry_root_fallback_wins_for_unmatched_non_relative_imports() {
    let root = temp_dir("entry_root_fallback");
    let src = root.join("src");
    let lib = root.join("lib");
    fs::create_dir_all(src.join("helpers")).expect("should create source helpers");
    fs::create_dir_all(lib.join("helpers")).expect("should create root-folder helpers");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "#entry_root = \"src\"\n#root_folders = { @lib }\n",
    )
    .expect("should write config");
    fs::write(
        src.join("#page.bst"),
        "import @helpers/theme\nio(\"page\")\n",
    )
    .expect("should write page");
    fs::write(src.join("helpers/theme.bst"), "io(\"source\")\n").expect("should write source");
    fs::write(lib.join("helpers/theme.bst"), "io(\"library\")\n")
        .expect("should write root-folder helper");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config parse");
    let resolver = configured_resolver(&config);

    let modules = discover_modules_for_test(&config, &resolver, &style_directives)
        .expect("module discovery should pass");
    assert_eq!(modules.len(), 1, "expected exactly one entry module");

    let source_theme = fs::canonicalize(src.join("helpers/theme.bst")).expect("canonical source");
    let library_theme = fs::canonicalize(lib.join("helpers/theme.bst")).expect("canonical library");
    let discovered_paths = modules[0]
        .input_files
        .iter()
        .map(|file| file.source_path.clone())
        .collect::<HashSet<_>>();

    assert!(
        discovered_paths.contains(&source_theme),
        "unmatched non-relative imports should fall back to the entry root"
    );
    assert!(
        !discovered_paths.contains(&library_theme),
        "configured root folders should not be searched unless the first import segment matches them"
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
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config parse");
    let resolver = configured_resolver(&config);

    let modules = discover_modules_for_test(&config, &resolver, &style_directives)
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

#[test]
fn explicit_root_folder_imports_resolve_from_project_root() {
    let root = temp_dir("explicit_root_folder");
    let src = root.join("src");
    let lib = root.join("lib");
    fs::create_dir_all(&src).expect("should create src root");
    fs::create_dir_all(&lib).expect("should create lib root");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "#entry_root = \"src\"\n#root_folders = { @lib }\n",
    )
    .expect("should write config");
    fs::write(
        src.join("#page.bst"),
        "import @lib/html {center}\n#[center: ok]\n",
    )
    .expect("should write page");
    fs::write(
        lib.join("html.bst"),
        "#center String = [$insert(\"style\"):text-align: center;]\n",
    )
    .expect("should write root-folder helper");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config parse");
    let resolver = configured_resolver(&config);

    let modules = discover_modules_for_test(&config, &resolver, &style_directives)
        .expect("module discovery should pass");
    let discovered_paths = modules[0]
        .input_files
        .iter()
        .map(|file| file.source_path.clone())
        .collect::<HashSet<_>>();

    assert!(
        discovered_paths.contains(&fs::canonicalize(lib.join("html.bst")).expect("canonical lib")),
        "explicit root-folder imports should resolve from the project root"
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_entry_root_folder_that_conflicts_with_root_folder_name() {
    let root = temp_dir("root_folder_collision");
    let src = root.join("src");
    fs::create_dir_all(src.join("lib")).expect("should create conflicting src/lib");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "#entry_root = \"src\"\n#root_folders = { @lib }\n",
    )
    .expect("should write config");
    fs::write(src.join("#page.bst"), "import @lib/html {center}\n").expect("should write page");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config parse");
    let resolver = configured_resolver(&config);

    let error = match discover_modules_for_test(&config, &resolver, &style_directives) {
        Ok(_) => panic!("conflicting src/lib folder should fail"),
        Err(error) => error,
    };

    assert!(
        error.msg.contains("unreachable"),
        "unexpected error message: {}",
        error.msg
    );
    assert!(
        error
            .metadata
            .get(&crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::PrimarySuggestion)
            .is_some_and(|suggestion| suggestion.contains("Rename")),
        "expected rename suggestion metadata: {error:?}"
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn detects_duplicate_config_keys() {
    // Duplicate constants are caught by the header parser during parsing.
    // This test verifies that config parsing properly reports the duplicate key error.
    let root = temp_dir("config_duplicate_keys");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(
        &config_path,
        "#entry_root = \"src\"\n#dev_folder = \"dev\"\n#entry_root = \"other\"\n",
    )
    .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    assert!(
        !messages.errors.is_empty(),
        "should have at least one error"
    );

    // Find the duplicate key error
    let duplicate_error = messages
        .errors
        .iter()
        .find(|e| e.msg.contains("Duplicate config key"));
    assert!(
        duplicate_error.is_some(),
        "should have a duplicate config key error"
    );

    let error = duplicate_error.unwrap();
    assert_eq!(
        error.error_type,
        ErrorType::Config,
        "duplicate config key error should use ErrorType::Config"
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}
