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
        "#src = \"src\"\n#output_folder = \"dist\"\n#name = \"docs\"\n#version = \"1.2.3\"\n#project = \"html\"\n#libraries = { @(libs), \"vendor\" }\n#custom_key = \"custom_value\"\n",
    )
    .expect("should write config");

    let mut config = Config::new(root.clone());
    parse_project_config_file(&mut config, &config_path).expect("config should parse");

    assert_eq!(config.src, PathBuf::from("src"));
    assert_eq!(config.release_folder, PathBuf::from("dist"));
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
fn discover_modules_uses_reachable_files_only() {
    let root = temp_dir("reachable_only");
    let src = root.join("src");
    fs::create_dir_all(src.join("libs")).expect("should create libs folder");
    fs::create_dir_all(src.join("styles")).expect("should create styles folder");
    fs::create_dir_all(src.join("docs")).expect("should create docs folder");

    fs::write(root.join(settings::CONFIG_FILE_NAME), "#src = \"src\"\n")
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

    let modules = discover_all_modules_in_project(&config).expect("module discovery should pass");

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
