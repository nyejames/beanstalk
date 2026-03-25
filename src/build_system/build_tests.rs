//! Tests for the core build orchestration and output writer APIs.

use super::{
    BackendBuilder, FileKind, OutputFile, Project, ProjectBuilder, WriteOptions, build_project,
    resolve_project_output_root, write_project_outputs,
};
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::compiler_errors::{
    CompilerError, CompilerMessages, ErrorLocation, ErrorType,
};
use crate::compiler_frontend::compiler_warnings::{CompilerWarning, WarningKind};
use crate::compiler_frontend::style_directives::StyleDirectiveSpec;
use crate::projects::html_project::html_project_builder::HtmlProjectBuilder;
use crate::projects::settings::Config;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::SystemTime;

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("beanstalk_build_core_{prefix}_{unique}"))
}

struct CurrentDirGuard {
    _lock: MutexGuard<'static, ()>,
    previous: PathBuf,
}

impl CurrentDirGuard {
    fn set_to(path: &PathBuf) -> Self {
        let lock = current_dir_test_lock()
            .lock()
            .expect("current-dir test lock should not be poisoned");
        let previous = std::env::current_dir().expect("current dir should resolve");
        std::env::set_current_dir(path).expect("should change current directory for test");
        Self {
            _lock: lock,
            previous,
        }
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.previous);
    }
}

fn current_dir_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct WarningBuilder;

impl BackendBuilder for WarningBuilder {
    fn build_backend(
        &self,
        _modules: Vec<super::Module>,
        _config: &Config,
        _flags: &[Flag],
    ) -> Result<Project, CompilerMessages> {
        Ok(Project {
            output_files: vec![OutputFile::new(
                PathBuf::from("generated.js"),
                FileKind::Js(String::from("console.log('ok');")),
            )],
            entry_page_rel: None,
            warnings: vec![CompilerWarning::new(
                "builder warning",
                ErrorLocation::default(),
                WarningKind::UnusedVariable,
                PathBuf::from("builder"),
            )],
        })
    }

    fn validate_project_config(&self, _config: &Config) -> Result<(), CompilerError> {
        Ok(())
    }

    fn frontend_style_directives(&self) -> Vec<StyleDirectiveSpec> {
        Vec::new()
    }
}

struct ValidationTrackingBuilder {
    validated: std::sync::Arc<std::sync::atomic::AtomicBool>,
    built: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl BackendBuilder for ValidationTrackingBuilder {
    fn build_backend(
        &self,
        _modules: Vec<super::Module>,
        _config: &Config,
        _flags: &[Flag],
    ) -> Result<Project, CompilerMessages> {
        self.built.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(Project {
            output_files: vec![],
            entry_page_rel: None,
            warnings: vec![],
        })
    }

    fn validate_project_config(&self, _config: &Config) -> Result<(), CompilerError> {
        self.validated
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    fn frontend_style_directives(&self) -> Vec<StyleDirectiveSpec> {
        Vec::new()
    }
}

struct FailingValidationBuilder;

impl BackendBuilder for FailingValidationBuilder {
    fn build_backend(
        &self,
        _modules: Vec<super::Module>,
        _config: &Config,
        _flags: &[Flag],
    ) -> Result<Project, CompilerMessages> {
        panic!("should not call build_backend if validation fails");
    }

    fn validate_project_config(&self, _config: &Config) -> Result<(), CompilerError> {
        Err(CompilerError::new(
            "Fake config error",
            ErrorLocation::default(),
            ErrorType::Config,
        ))
    }

    fn frontend_style_directives(&self) -> Vec<StyleDirectiveSpec> {
        Vec::new()
    }
}

struct NoDirectiveBuilder;

impl BackendBuilder for NoDirectiveBuilder {
    fn build_backend(
        &self,
        _modules: Vec<super::Module>,
        _config: &Config,
        _flags: &[Flag],
    ) -> Result<Project, CompilerMessages> {
        Ok(Project {
            output_files: vec![OutputFile::new(
                PathBuf::from("index.html"),
                FileKind::Html(String::new()),
            )],
            entry_page_rel: Some(PathBuf::from("index.html")),
            warnings: vec![],
        })
    }

    fn validate_project_config(&self, _config: &Config) -> Result<(), CompilerError> {
        Ok(())
    }

    fn frontend_style_directives(&self) -> Vec<StyleDirectiveSpec> {
        Vec::new()
    }
}

#[test]
fn build_project_returns_result_without_writing_files() {
    let root = temp_dir("build_only");
    fs::create_dir_all(&root).expect("should create temp root");
    let entry_file = root.join("main.bst");
    fs::write(&entry_file, "value = 1\n").expect("should write source file");

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let result = build_project(
        &builder,
        entry_file
            .to_str()
            .expect("temp file path should be valid UTF-8 for this test"),
        &[],
    )
    .expect("build should succeed");

    assert!(!result.project.output_files.is_empty());
    assert!(
        !root.join("index.html").exists(),
        "build_project should not write files to disk"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn html_project_directives_fail_when_builder_does_not_register_them() {
    let root = temp_dir("directive_boundary_missing");
    fs::create_dir_all(&root).expect("should create temp root");

    for (directive_name, source) in [
        ("html", "[$html:\n<div>Hello</div>\n]"),
        ("css", "[$css:\n.button { color: red; }\n]"),
        ("escape_html", "[$escape_html:\n<b>Hello</b>\n]"),
    ] {
        let entry_file = root.join(format!("{directive_name}.bst"));
        fs::write(&entry_file, source).expect("should write source file");

        let builder = ProjectBuilder::new(Box::new(NoDirectiveBuilder));
        let result = build_project(
            &builder,
            entry_file
                .to_str()
                .expect("temp file path should be valid UTF-8 for this test"),
            &[],
        );

        let messages = match result {
            Ok(_) => panic!("project-owned directives should fail when not registered"),
            Err(messages) => messages,
        };
        assert!(
            messages.errors.iter().any(|error| error
                .msg
                .contains(&format!("Unsupported style directive '${directive_name}'"))),
            "expected unsupported directive error for '${directive_name}', got: {:?}",
            messages
                .errors
                .iter()
                .map(|error| error.msg.as_str())
                .collect::<Vec<_>>()
        );
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn html_project_directives_are_available_under_html_builder() {
    let root = temp_dir("directive_boundary_registered");
    fs::create_dir_all(&root).expect("should create temp root");

    for (directive_name, source, expected_html_fragment) in [
        ("html", "[$html:\n<div>Hello</div>\n]", "<div>Hello</div>"),
        (
            "css",
            "[$css:\n.button { color: red; }\n]",
            ".button { color: red; }",
        ),
        (
            "escape_html",
            "[$escape_html:\n<b>Hello</b>\n]",
            "&lt;b&gt;Hello&lt;/b&gt;",
        ),
    ] {
        let entry_file = root.join(format!("{directive_name}.bst"));
        fs::write(&entry_file, source).expect("should write source file");

        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        let result = build_project(
            &builder,
            entry_file
                .to_str()
                .expect("temp file path should be valid UTF-8 for this test"),
            &[],
        )
        .expect("html builder should register HTML-project directives");

        let rendered_html = result
            .project
            .output_files
            .iter()
            .find_map(|file| match file.file_kind() {
                FileKind::Html(content) => Some(content.as_str()),
                _ => None,
            })
            .expect("expected an HTML output file");

        assert!(
            rendered_html.contains(expected_html_fragment),
            "expected rendered HTML for '${directive_name}' to contain '{expected_html_fragment}', got: {rendered_html}"
        );
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_single_file_project_includes_reachable_import_files() {
    let root = temp_dir("single_file_reachable_imports");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::create_dir_all(root.join("utils")).expect("should create utils directory");
    fs::write(
        root.join("main.bst"),
        "import @utils/helper/greet\ngreet()\n",
    )
    .expect("should write main file");
    fs::write(
        root.join("utils/helper.bst"),
        "#greet||:\n    io(\"hello\")\n;\n",
    )
    .expect("should write helper file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let result = build_project(&builder, "main.bst", &[]).expect("build should succeed");

    assert!(
        !result.project.output_files.is_empty(),
        "single-file build should compile reachable imported files"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn write_project_outputs_writes_all_supported_artifacts_and_skips_not_built() {
    let root = temp_dir("writer_success");
    fs::create_dir_all(&root).expect("should create temp root");

    let project = Project {
        output_files: vec![
            OutputFile::new(PathBuf::from("assets"), FileKind::Directory),
            OutputFile::new(
                PathBuf::from("scripts/app.js"),
                FileKind::Js(String::from("console.log('hi');")),
            ),
            OutputFile::new(
                PathBuf::from("index.html"),
                FileKind::Html(String::from("<html></html>")),
            ),
            OutputFile::new(PathBuf::from("bin/app.wasm"), FileKind::Wasm(vec![0, 1, 2])),
            OutputFile::new(PathBuf::new(), FileKind::NotBuilt),
        ],
        entry_page_rel: Some(PathBuf::from("index.html")),
        warnings: vec![],
    };

    write_project_outputs(
        &project,
        &WriteOptions {
            output_root: root.clone(),
            project_entry_dir: None,
        },
    )
    .expect("writer should succeed");

    assert!(root.join("assets").is_dir());
    assert_eq!(
        fs::read_to_string(root.join("scripts/app.js")).expect("should read JS file"),
        "console.log('hi');"
    );
    assert_eq!(
        fs::read_to_string(root.join("index.html")).expect("should read HTML file"),
        "<html></html>"
    );
    assert_eq!(
        fs::read(root.join("bin/app.wasm")).expect("should read wasm file"),
        vec![0, 1, 2]
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn write_project_outputs_rejects_invalid_paths() {
    let root = temp_dir("writer_invalid");
    fs::create_dir_all(&root).expect("should create temp root");

    let invalid_projects = vec![
        Project {
            output_files: vec![OutputFile::new(
                PathBuf::from("/tmp/absolute.js"),
                FileKind::Js(String::from("x")),
            )],
            entry_page_rel: None,
            warnings: vec![],
        },
        Project {
            output_files: vec![OutputFile::new(
                PathBuf::from("../escape.js"),
                FileKind::Js(String::from("x")),
            )],
            entry_page_rel: None,
            warnings: vec![],
        },
        Project {
            output_files: vec![OutputFile::new(
                PathBuf::new(),
                FileKind::Js(String::from("x")),
            )],
            entry_page_rel: None,
            warnings: vec![],
        },
    ];

    for project in invalid_projects {
        let result = write_project_outputs(
            &project,
            &WriteOptions {
                output_root: root.clone(),
                project_entry_dir: None,
            },
        );
        assert!(result.is_err(), "invalid output path should be rejected");
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_preserves_builder_warnings_in_build_result() {
    let root = temp_dir("warnings");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(root.join("main.bst"), "value = 1\n").expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let result = build_project(
        &ProjectBuilder::new(Box::new(WarningBuilder)),
        "main.bst",
        &[],
    )
    .expect("build should succeed");

    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.msg == "builder warning"),
        "build result should include backend warnings"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_calls_validate_project_config() {
    let root = temp_dir("validation_tracking");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(root.join("main.bst"), "value = 1\n").expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let validated = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let built = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    let builder = ProjectBuilder::new(Box::new(ValidationTrackingBuilder {
        validated: validated.clone(),
        built: built.clone(),
    }));

    build_project(&builder, "main.bst", &[]).expect("build should succeed");

    assert!(
        validated.load(std::sync::atomic::Ordering::SeqCst),
        "build_project should call validate_project_config"
    );
    assert!(
        built.load(std::sync::atomic::Ordering::SeqCst),
        "build_project should call build_backend"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn config_validation_failure_returns_config_error_before_compilation() {
    let root = temp_dir("failing_validation");
    fs::create_dir_all(&root).expect("should create temp root");
    // Invalid frontend syntax to prove it fails BEFORE frontend compilation
    fs::write(root.join("main.bst"), "invalid syntax;;;;;").expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = ProjectBuilder::new(Box::new(FailingValidationBuilder));
    let result = build_project(&builder, "main.bst", &[]);

    let messages = match result {
        Err(messages) => messages,
        Ok(_) => panic!("build_project should fail when config validation fails"),
    };
    assert_eq!(messages.errors[0].msg, "Fake config error");
    assert_eq!(messages.errors[0].error_type, ErrorType::Config);

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_emits_runtime_fragment_with_captured_start_local() {
    let root = temp_dir("runtime_fragment_capture");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "get_name|| -> String:\n    return \"Beanstalk\"\n;\nname = get_name()\n[:Hello [name]]\n",
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let build_result = build_project(&builder, "main.bst", &[]).expect("build should succeed");

    let html = match build_result.project.output_files[0].file_kind() {
        FileKind::Html(content) => content,
        other => panic!(
            "expected HTML output, got {:?}",
            std::mem::discriminant(other)
        ),
    };

    assert!(html.contains("<div id=\"bst-slot-0\"></div>"));
    assert!(
        html.contains("__bst_frag_0"),
        "runtime fragment function should be emitted and bootstrapped"
    );
    assert!(
        html.contains("Beanstalk"),
        "captured start-local value should be preserved in generated fragment code"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_preserves_const_and_runtime_fragment_order() {
    let root = temp_dir("fragment_order");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "#[:<meta charset=\"utf-8\">]\nname = \"Beanstalk\"\n[:<title>[name]</title>]\n",
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let build_result = build_project(&builder, "main.bst", &[]).expect("build should succeed");

    let html = match build_result.project.output_files[0].file_kind() {
        FileKind::Html(content) => content,
        other => panic!(
            "expected HTML output, got {:?}",
            std::mem::discriminant(other)
        ),
    };

    let const_index = html
        .find("<meta charset=\"utf-8\">")
        .expect("const fragment should be inlined");
    let slot_index = html
        .find("<div id=\"bst-slot-0\"></div>")
        .expect("runtime fragment slot should be emitted");

    assert!(
        const_index < slot_index,
        "const fragment should appear before runtime slot in source order"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn resolve_project_output_root_defaults_to_dev_and_release_for_directory_builds() {
    let root = temp_dir("output_defaults");
    let config = Config::new(root.clone());

    assert_eq!(resolve_project_output_root(&config, &[]), root.join("dev"));
    assert_eq!(
        resolve_project_output_root(&config, &[Flag::Release]),
        root.join("release")
    );
}

#[test]
fn resolve_project_output_root_respects_configured_dev_and_release_folders() {
    let root = temp_dir("output_overrides");
    let mut config = Config::new(root.clone());
    config.dev_folder = PathBuf::from("preview");
    config.release_folder = PathBuf::from("public");

    assert_eq!(
        resolve_project_output_root(&config, &[]),
        root.join("preview")
    );
    assert_eq!(
        resolve_project_output_root(&config, &[Flag::Release]),
        root.join("public")
    );
}

#[test]
fn resolve_project_output_root_uses_project_root_when_folder_is_explicitly_empty() {
    let root = temp_dir("output_root_fallback");
    let mut config = Config::new(root.clone());
    config.dev_folder = PathBuf::new();
    config.release_folder = PathBuf::new();

    assert_eq!(resolve_project_output_root(&config, &[]), root);
    assert_eq!(
        resolve_project_output_root(&config, &[Flag::Release]),
        config.entry_dir
    );
}

#[test]
fn build_directory_project_emits_index_and_404_and_ignores_unreachable_files() {
    let root = temp_dir("docs_like_project");
    let src = root.join("src");
    fs::create_dir_all(src.join("about")).expect("should create about folder");
    fs::create_dir_all(src.join("docs/basics")).expect("should create docs folder");
    fs::create_dir_all(&src).expect("should create source folder");

    fs::write(
        root.join("#config.bst"),
        "#entry_root = \"src\"\n#output_folder = \"release\"\n",
    )
    .expect("should write config");
    fs::write(src.join("#page.bst"), "#[:<h1>Home</h1>]\n").expect("should write #page");
    fs::write(src.join("#404.bst"), "#[:<h1>404</h1>]\n").expect("should write #404");
    fs::write(src.join("about").join("#page.bst"), "#[:<h1>About</h1>]\n")
        .expect("should write about");
    fs::write(
        src.join("docs").join("basics").join("#page.bst"),
        "#[:<h1>Docs Basics</h1>]\n",
    )
    .expect("should write docs basics");
    fs::write(
        src.join("docs/outdated.bst"),
        "this is invalid and should not compile",
    )
    .expect("should write unreachable invalid file");

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let build_result = build_project(
        &builder,
        root.to_str().expect("root path should be valid UTF-8"),
        &[],
    )
    .expect("docs-like directory build should succeed");
    assert_eq!(
        build_result.project.entry_page_rel,
        Some(PathBuf::from("index.html"))
    );

    let output_root = resolve_project_output_root(&build_result.config, &[]);

    write_project_outputs(
        &build_result.project,
        &WriteOptions {
            output_root: output_root.clone(),
            project_entry_dir: None,
        },
    )
    .expect("should write project outputs");

    assert!(output_root.join("index.html").exists());
    assert!(output_root.join("404/index.html").exists());
    assert!(output_root.join("about/index.html").exists());
    assert!(output_root.join("docs/basics/index.html").exists());

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_directory_project_respects_custom_entry_root() {
    let root = temp_dir("custom_entry_root");
    let pages = root.join("pages");
    fs::create_dir_all(pages.join("docs")).expect("should create docs folder");

    fs::write(
        root.join("#config.bst"),
        "#entry_root = \"pages\"\n#output_folder = \"release\"\n",
    )
    .expect("should write config");
    fs::write(pages.join("#page.bst"), "#[:<h1>Home</h1>]\n").expect("should write home");
    fs::write(pages.join("docs").join("#page.bst"), "#[:<h1>Docs</h1>]\n")
        .expect("should write docs");

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let build_result = build_project(
        &builder,
        root.to_str().expect("root path should be valid UTF-8"),
        &[],
    )
    .expect("directory build should succeed");

    let output_root = resolve_project_output_root(&build_result.config, &[]);
    write_project_outputs(
        &build_result.project,
        &WriteOptions {
            output_root: output_root.clone(),
            project_entry_dir: None,
        },
    )
    .expect("should write project outputs");

    assert!(output_root.join("index.html").exists());
    assert!(output_root.join("docs/index.html").exists());

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_directory_project_requires_root_page_in_configured_entry_root() {
    let root = temp_dir("missing_homepage");
    let src = root.join("src");
    fs::create_dir_all(src.join("about")).expect("should create about folder");

    fs::write(
        root.join("#config.bst"),
        "#entry_root = \"src\"\n#output_folder = \"release\"\n",
    )
    .expect("should write config");
    fs::write(src.join("about").join("#page.bst"), "#[:<h1>About</h1>]\n")
        .expect("should write about");

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let result = build_project(
        &builder,
        root.to_str().expect("root path should be valid UTF-8"),
        &[],
    );

    assert!(result.is_err(), "missing root homepage should fail");
    let messages = result.err().expect("expected missing homepage error");
    assert!(
        messages
            .errors
            .iter()
            .any(|error| error.msg.contains("require a '#page.bst' homepage")),
        "expected homepage error message"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_directory_project_rejects_invalid_page_url_style() {
    let root = temp_dir("invalid_page_url_style");
    let src = root.join("src");
    fs::create_dir_all(&src).expect("should create source folder");
    fs::write(
        root.join("#config.bst"),
        "#entry_root = \"src\"\n#output_folder = \"release\"\n#page_url_style = \"slashy\"\n",
    )
    .expect("should write config");
    fs::write(src.join("#page.bst"), "#[:<h1>Home</h1>]\n").expect("should write home page");

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let result = build_project(
        &builder,
        root.to_str().expect("root path should be valid UTF-8"),
        &[],
    );

    assert!(result.is_err(), "invalid page url style should fail build");
    let messages = result.err().expect("expected config error");
    assert!(
        messages
            .errors
            .iter()
            .any(|error| error.error_type == ErrorType::Config),
        "expected config-classified error"
    );
    assert!(
        messages
            .errors
            .iter()
            .any(|error| error.msg.contains("#page_url_style")),
        "expected page_url_style validation message"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_directory_project_rejects_invalid_redirect_index_html() {
    let root = temp_dir("invalid_redirect_index");
    let src = root.join("src");
    fs::create_dir_all(&src).expect("should create source folder");
    fs::write(
        root.join("#config.bst"),
        "#entry_root = \"src\"\n#output_folder = \"release\"\n#redirect_index_html = \"yes\"\n",
    )
    .expect("should write config");
    fs::write(src.join("#page.bst"), "#[:<h1>Home</h1>]\n").expect("should write home page");

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let result = build_project(
        &builder,
        root.to_str().expect("root path should be valid UTF-8"),
        &[],
    );

    assert!(
        result.is_err(),
        "invalid redirect_index_html should fail build"
    );
    let messages = result.err().expect("expected config error");
    assert!(
        messages
            .errors
            .iter()
            .any(|error| error.error_type == ErrorType::Config),
        "expected config-classified error"
    );
    assert!(
        messages
            .errors
            .iter()
            .any(|error| error.msg.contains("#redirect_index_html")),
        "expected redirect_index_html validation message"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_allows_const_record_coercion_with_all_defaults() {
    let root = temp_dir("const_record_all_defaults");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "Basic = |\n    body String = \"ok\",\n|\n#basic = Basic()\n",
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let result = build_project(&builder, "main.bst", &[]);
    assert!(
        result.is_ok(),
        "const struct coercion with defaults should compile"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_allows_const_record_coercion_with_constant_arguments() {
    let root = temp_dir("const_record_constant_args");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "Basic = |\n    body String = \"default\",\n    color String = \"red\",\n|\n#label = \"Docs\"\n#basic = Basic(label, \"green\")\n",
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let result = build_project(&builder, "main.bst", &[]);
    assert!(
        result.is_ok(),
        "const struct coercion with constant arguments should compile"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_runtime_struct_constructor_supports_partial_defaults() {
    let root = temp_dir("runtime_struct_partial_defaults");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "Point = |\n    x Int,\n    y Int = 99,\n|\npoint = Point(5)\nio([: point.y])\n",
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let build_result = build_project(&builder, "main.bst", &[])
        .expect("runtime struct constructor with defaults should compile");

    let html = match build_result.project.output_files[0].file_kind() {
        FileKind::Html(content) => content,
        other => panic!(
            "expected HTML output, got {:?}",
            std::mem::discriminant(other)
        ),
    };
    assert!(
        html.contains("99"),
        "runtime constructor should include the struct default value in emitted output"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_struct_default_uses_same_file_constant_declared_later() {
    let root = temp_dir("struct_default_forward_constant");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "Card = |\n    color String = base + \"!\",\n|\n#base = \"red\"\ncard = Card()\nio([: card.color])\n",
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let build_result = build_project(&builder, "main.bst", &[])
        .expect("struct default should resolve same-file constants declared later");

    let html = match build_result.project.output_files[0].file_kind() {
        FileKind::Html(content) => content,
        other => panic!(
            "expected HTML output, got {:?}",
            std::mem::discriminant(other)
        ),
    };
    assert!(
        html.contains("red!"),
        "forward constant dependency should be sorted before struct parsing and fold into one value",
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_constant_can_reference_same_file_struct_declared_later() {
    let root = temp_dir("const_depends_on_forward_struct");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "#basic = Basic()\nBasic = |\n    body String = \"ok\",\n|\nio([: basic.body])\n",
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let build_result = build_project(&builder, "main.bst", &[])
        .expect("constant should resolve same-file struct declared later");

    let html = match build_result.project.output_files[0].file_kind() {
        FileKind::Html(content) => content,
        other => panic!(
            "expected HTML output, got {:?}",
            std::mem::discriminant(other)
        ),
    };
    assert!(
        !html.is_empty(),
        "build output should still be produced when constant references forward-declared struct"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_typed_constant_template_head_can_reference_prior_constant() {
    let root = temp_dir("typed_constant_template_head_reference");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "# page String = [: world]\n# test = [page: Hello ]\nio(test)\n",
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let build_result = build_project(&builder, "main.bst", &[])
        .expect("typed constant should remain visible to later constants");

    let html = match build_result.project.output_files[0].file_kind() {
        FileKind::Html(content) => content,
        other => panic!(
            "expected HTML output, got {:?}",
            std::mem::discriminant(other)
        ),
    };
    assert!(
        html.contains("world Hello"),
        "typed constant reference in template head should compile and render expected output"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_const_struct_template_field_can_fill_template_slots() {
    let root = temp_dir("const_struct_template_field_slots");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "Basic = |\n    page String = [:<section>[$slot]</section>],\n|\n#basic = Basic()\n#[basic.page: Hello world]\n",
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let build_result = build_project(&builder, "main.bst", &[])
        .expect("const struct template field should remain foldable in const template heads");

    let html = match build_result.project.output_files[0].file_kind() {
        FileKind::Html(content) => content,
        other => panic!(
            "expected HTML output, got {:?}",
            std::mem::discriminant(other)
        ),
    };
    assert!(
        html.contains("<section>") && html.contains("Hello world") && html.contains("</section>"),
        "const struct wrapper field should compose slot content in place",
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_const_slot_insertion_constant_is_composed_at_use_site() {
    let root = temp_dir("const_slot_insertion_use_site");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "#wrapper = [:<section>[$slot(\"content\")]</section>]\n#slot_1 = [$insert(\"content\"): Hello world]\n#[wrapper, slot_1]\n",
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let build_result = build_project(&builder, "main.bst", &[])
        .expect("slot insertion constants should fold when consumed by wrapper templates");

    let html = match build_result.project.output_files[0].file_kind() {
        FileKind::Html(content) => content,
        other => panic!(
            "expected HTML output, got {:?}",
            std::mem::discriminant(other)
        ),
    };
    assert!(
        html.contains("<section>") && html.contains("Hello world") && html.contains("</section>"),
        "slot insertion constant should be resolved at the wrapper use-site",
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_const_top_level_header_with_unfilled_named_slots_folds_to_empty_strings() {
    let root = temp_dir("const_top_level_header_unfilled_named_slots");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        r#"Document = |
    prelude String = "<!DOCTYPE html>",
    en String = [$html:<html lang="en">],
    head String = [$html:
        <head>[$slot]</head>
    ],
    title String = [$html:<title>[$slot]</title>],
    style String = [$html:<style>[$slot]</style>],
|
#doc = Document()

# header = [:
    [doc.prelude, doc.en]
    [doc.head, $html:
        <meta charset="UTF-8">
        <link rel="icon" href="[$slot("favicon")]">
        [doc.title: Beanstalk Documentation]
        [doc.style:
            [$slot("css")]
        ]
    ]
]
#[header]
"#,
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let build_result = build_project(&builder, "main.bst", &[])
        .expect("top-level const wrappers should fold even when named slots are unfilled");

    let html = match build_result.project.output_files[0].file_kind() {
        FileKind::Html(content) => content,
        other => panic!(
            "expected HTML output, got {:?}",
            std::mem::discriminant(other)
        ),
    };
    assert!(
        html.contains("rel=\"icon\"") && html.contains("href=\"\""),
        "unfilled named slots should render as empty strings instead of failing compile-time folding",
    );
    assert!(
        html.contains("<meta charset=\"UTF-8\">"),
        "expected folded header content to remain present in output",
    );
    assert!(
        !html.contains("$slot(") && !html.contains("$insert("),
        "slot markers should not leak into folded output",
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_rejects_slot_insertion_constant_without_active_wrapper() {
    let root = temp_dir("const_slot_insertion_without_wrapper");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "#slot_1 = [$insert(\"content\"): hello]\n#[slot_1]\n",
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let result = build_project(&builder, "main.bst", &[]);
    assert!(
        result.is_err(),
        "slot insertion constants should fail when used outside wrapper composition",
    );
    let messages = match result {
        Err(messages) => messages,
        Ok(_) => unreachable!("assert above guarantees this is an error"),
    };

    assert!(
        messages.errors.iter().any(|error| error.msg.contains(
            "'$insert(...)' can only be used while filling an immediate parent template"
        )),
        "expected a targeted slot insertion usage diagnostic",
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_const_slot_children_wrap_table_rows_and_cells_without_cross_applying() {
    let root = temp_dir("const_slot_children_cells");
    fs::create_dir_all(root.join("libs")).expect("should create libs root");
    fs::write(
        root.join("libs").join("html.bst"),
        "#table = [$children([:<tr>[$slot]</tr>]):\n  <table>\n    [$children([:<td>[$slot]</td>]):[$slot]]\n  </table>\n]\n",
    )
    .expect("should write html library");
    fs::write(
        root.join("main.bst"),
        "import @libs/html {table}\n[table:\n    [: [:Type] [:Description] ]\n    [: [:float] [:64 bit floating point number] ]\n]\n",
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let build_result = build_project(&builder, "main.bst", &[])
        .expect("slot child wrapper tables should build successfully");

    let html = match build_result.project.output_files[0].file_kind() {
        FileKind::Html(content) => content,
        other => panic!(
            "expected HTML output, got {:?}",
            std::mem::discriminant(other)
        ),
    };

    assert_eq!(html.matches("<tr>").count(), 2);
    assert!(html.contains("<td>Type</td>"));
    assert!(html.contains("<td>Description</td>"));
    assert!(html.contains("<td>float</td>"));
    assert_eq!(html.matches("<td>").count(), 4);

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_markdown_page_reexported_table_keeps_rows_and_cells_inside_table() {
    let root = temp_dir("markdown_page_reexported_table");
    fs::create_dir_all(root.join("libs")).expect("should create libs root");
    fs::create_dir_all(root.join("styles")).expect("should create styles root");
    fs::write(
        root.join("libs").join("html.bst"),
        "Format = |\n  table String = [$children([:<tr>[$slot]</tr>]):\n    <table style=\"[$slot(\"style\")]\">\n      [$children([:<td>[$slot]</td>]):[$slot]]\n    </table>\n  ],\n|\n#format = Format()\n",
    )
    .expect("should write html library");
    fs::write(
        root.join("styles").join("docs.bst"),
        "import @libs/html {format}\n#page = [:\n  <body>[$slot]</body>\n]\n#table = [format.table]\n",
    )
    .expect("should write docs style library");
    fs::write(
        root.join("main.bst"),
        "import @styles/docs {page, table}\n[page, $markdown:\n[table:\n    [: [:Type] [:Description] ]\n    [: [:float] [:64 bit floating point number] ]\n]\n]\n",
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let build_result = build_project(&builder, "main.bst", &[])
        .expect("markdown page with re-exported table should build successfully");

    let html = match build_result.project.output_files[0].file_kind() {
        FileKind::Html(content) => content,
        other => panic!(
            "expected HTML output, got {:?}",
            std::mem::discriminant(other)
        ),
    };

    assert!(!html.contains('\u{FFFC}'));
    assert_eq!(html.matches("<tr>").count(), 2);
    assert!(html.contains("<td>Type</td>"));
    assert!(html.contains("<td>Description</td>"));
    assert_eq!(html.matches("<td>").count(), 4);
    assert!(!html.contains("<p>"));

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_docs_style_title_and_center_slot_chain_compiles() {
    let root = temp_dir("docs_title_center_slot_chain");
    fs::create_dir_all(root.join("lib")).expect("should create lib root");
    fs::create_dir_all(root.join("src/styles")).expect("should create styles root");

    fs::write(
        root.join("#config.bst"),
        "#project = \"html\"\n#entry_root = \"src\"\n#output_folder = \"release\"\n#root_folders = {\n    @lib,\n}\n",
    )
    .expect("should write config file");
    fs::write(
        root.join("lib").join("html.bst"),
        "#center String = [$insert(\"style\"):text-align: center;]\n",
    )
    .expect("should write html helper library");
    fs::write(
        root.join("src/styles").join("docs.bst"),
        "#title = [$html: <h1 style=\"font-size: 2em;[$slot(\"style\")]\">[$slot]</h1>]\n",
    )
    .expect("should write docs style library");
    fs::write(
        root.join("src").join("#page.bst"),
        "import @lib/html {center}\nimport @styles/docs {title}\n#[title, center: LANGUAGE BASICS]\n",
    )
    .expect("should write source file");

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let build_result = build_project(&builder, &root.to_string_lossy(), &[])
        .expect("docs-style title+center chain should compile successfully");

    // Find the generated route HTML so assertions stay stable even if file ordering changes.
    let html = build_result
        .project
        .output_files
        .iter()
        .find_map(|output| match output.file_kind() {
            FileKind::Html(content)
                if output.relative_output_path().to_string_lossy() == "index.html" =>
            {
                Some(content)
            }
            _ => None,
        })
        .expect("should emit index.html output");

    assert!(html.contains("text-align: center;"));
    assert!(html.contains("LANGUAGE BASICS"));
    assert!(!html.contains("$slot("));
    assert!(!html.contains("$insert("));

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_markdown_docs_row_wrappers_render_plain_cells_and_headers() {
    let root = temp_dir("markdown_docs_row_wrappers");
    fs::create_dir_all(root.join("libs")).expect("should create libs root");
    fs::create_dir_all(root.join("styles")).expect("should create styles root");
    fs::write(
        root.join("libs").join("html.bst"),
        "Format = |\n    table String = [:\n      <table style=\"[$slot(\"style\")]\">\n        [$slot]\n      </table>\n    ],\n|\n#format = Format()\n",
    )
    .expect("should write html library");
    fs::write(
        root.join("styles").join("docs.bst"),
        "import @libs/html {format}\n#page = [:\n  <body>[$slot]</body>\n]\n#table = [format.table:\n    [$insert(\"style\"):border-collapse: collapse; border: 1px solid; padding: 0.5em;]\n    [$slot]\n]\n#row = [:\n    <tr>[$fresh, $children([:<td>[$slot]</td>]):[$slot]]</tr>\n]\n#header_row = [:\n    <tr>\n        [$fresh, $children([:\n            <th style=\"border: 1px solid; padding: 0.5em; text-align: left;\">[$slot]</th>\n        ]):[$slot]]\n    </tr>\n]\n",
    )
    .expect("should write docs style library");
    fs::write(
        root.join("main.bst"),
        "import @styles/docs {page, table, row, header_row}\n[page, $markdown:\n[table:\n    [header_row: [: Type] [: Description] ]\n\n    [row: [: float ] [: 64 bit floating point number] ]\n\n    [row: [: int ] [:  64 bit signed integer ] ]\n]\n]\n",
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let build_result = build_project(&builder, "main.bst", &[])
        .expect("markdown docs-style row wrappers should build successfully");

    let html = match build_result.project.output_files[0].file_kind() {
        FileKind::Html(content) => content,
        other => panic!(
            "expected HTML output, got {:?}",
            std::mem::discriminant(other)
        ),
    };

    assert!(!html.contains('\u{FFFC}'));
    assert!(html.contains("border-collapse: collapse; border: 1px solid; padding: 0.5em;"));
    assert_eq!(
        html.matches("<th style=\"border: 1px solid; padding: 0.5em; text-align: left;\">")
            .count(),
        2
    );
    assert_eq!(html.matches("<td>").count(), 4);
    assert!(html.contains("Type</th>"));
    assert!(html.contains("Description</th>"));
    assert!(html.contains("float"));
    assert!(html.contains("64 bit floating point number"));
    assert!(!html.contains("<p>"));

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_struct_default_uses_imported_constant() {
    let root = temp_dir("struct_default_imported_constant");
    fs::create_dir_all(root.join("styles")).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "import @styles/theme/base\nCard = |\n    color String = base,\n|\ncard = Card()\nio([: card.color])\n",
    )
    .expect("should write main source file");
    fs::write(root.join("styles/theme.bst"), "#base = \"green\"\n")
        .expect("should write imported constant source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let build_result = build_project(&builder, "main.bst", &[])
        .expect("struct default should resolve imported constants");

    let html = match build_result.project.output_files[0].file_kind() {
        FileKind::Html(content) => content,
        other => panic!(
            "expected HTML output, got {:?}",
            std::mem::discriminant(other)
        ),
    };
    assert!(
        html.contains("green"),
        "imported constant should be available in struct default value resolution",
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_rejects_const_record_with_non_constant_argument() {
    let root = temp_dir("const_record_non_constant_arg");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "Basic = |\n    body String = \"ok\",\n|\nget_value || -> String:\n    return \"dynamic\"\n;\n#basic = Basic(get_value())\n",
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let result = build_project(&builder, "main.bst", &[]);
    assert!(
        result.is_err(),
        "non-constant struct constructor argument in '#'-constant should fail"
    );
    let messages = match result {
        Err(messages) => messages,
        Ok(_) => unreachable!("assert above guarantees this is an error"),
    };

    assert!(
        messages.errors.iter().any(|error| {
            error.msg.contains("get_value") && error.msg.contains("non-constant value")
        }),
        "expected a targeted error describing the non-constant argument"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_rejects_const_record_when_required_fields_are_missing() {
    let root = temp_dir("const_record_missing_required");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "Basic = |\n    body String,\n    color String = \"blue\",\n|\n#basic = Basic()\n",
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let result = build_project(&builder, "main.bst", &[]);
    assert!(
        result.is_err(),
        "missing required fields in const record constructor should fail"
    );
    let messages = match result {
        Err(messages) => messages,
        Ok(_) => unreachable!("assert above guarantees this is an error"),
    };

    assert!(
        messages
            .errors
            .iter()
            .any(|error| error.msg.contains("missing 1 required field argument")),
        "expected a missing-required-fields constructor diagnostic"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_rejects_struct_constructor_with_too_many_arguments() {
    let root = temp_dir("struct_constructor_too_many_args");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "Point = |\n    x Int,\n    y Int = 1,\n|\n#point = Point(1, 2, 3)\n",
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let result = build_project(&builder, "main.bst", &[]);
    assert!(
        result.is_err(),
        "too many struct constructor arguments should fail"
    );
    let messages = match result {
        Err(messages) => messages,
        Ok(_) => unreachable!("assert above guarantees this is an error"),
    };

    assert!(
        messages
            .errors
            .iter()
            .any(|error| error.msg.contains("received too many arguments")),
        "expected a too-many-arguments constructor diagnostic"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

// ---------------------------------------------------------------------------
//   Stale artifact cleanup tests
// ---------------------------------------------------------------------------

use super::{
    BUILD_MANIFEST_FILENAME, read_build_manifest, remove_stale_artifacts,
    validate_output_root_is_safe, write_build_manifest,
};
use std::collections::HashSet;

#[test]
fn cleanup_removes_stale_files_from_previous_build() {
    let root = temp_dir("cleanup_stale");
    fs::create_dir_all(&root).expect("should create temp root");
    let project_dir = root.join("project");
    fs::create_dir_all(&project_dir).expect("should create project dir");
    let output_root = project_dir.join("dev");

    // Build A: index.html + about/index.html
    let project_a = Project {
        output_files: vec![
            OutputFile::new(
                PathBuf::from("index.html"),
                FileKind::Html(String::from("<html>Home</html>")),
            ),
            OutputFile::new(
                PathBuf::from("about/index.html"),
                FileKind::Html(String::from("<html>About</html>")),
            ),
        ],
        entry_page_rel: Some(PathBuf::from("index.html")),
        warnings: vec![],
    };
    write_project_outputs(
        &project_a,
        &WriteOptions {
            output_root: output_root.clone(),
            project_entry_dir: Some(project_dir.clone()),
        },
    )
    .expect("build A should succeed");

    assert!(output_root.join("index.html").exists());
    assert!(output_root.join("about/index.html").exists());
    assert!(output_root.join(BUILD_MANIFEST_FILENAME).exists());

    // Build B: only index.html
    let project_b = Project {
        output_files: vec![OutputFile::new(
            PathBuf::from("index.html"),
            FileKind::Html(String::from("<html>Home v2</html>")),
        )],
        entry_page_rel: Some(PathBuf::from("index.html")),
        warnings: vec![],
    };
    write_project_outputs(
        &project_b,
        &WriteOptions {
            output_root: output_root.clone(),
            project_entry_dir: Some(project_dir.clone()),
        },
    )
    .expect("build B should succeed");

    assert!(output_root.join("index.html").exists());
    assert!(
        !output_root.join("about/index.html").exists(),
        "stale about/index.html should have been removed"
    );
    assert!(
        !output_root.join("about").exists(),
        "empty about/ directory should have been removed"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn cleanup_preserves_user_files_not_in_manifest() {
    let root = temp_dir("cleanup_preserves_user");
    fs::create_dir_all(&root).expect("should create temp root");
    let project_dir = root.join("project");
    fs::create_dir_all(&project_dir).expect("should create project dir");
    let output_root = project_dir.join("dev");

    // Build A
    let project_a = Project {
        output_files: vec![OutputFile::new(
            PathBuf::from("index.html"),
            FileKind::Html(String::from("<html>Home</html>")),
        )],
        entry_page_rel: Some(PathBuf::from("index.html")),
        warnings: vec![],
    };
    write_project_outputs(
        &project_a,
        &WriteOptions {
            output_root: output_root.clone(),
            project_entry_dir: Some(project_dir.clone()),
        },
    )
    .expect("build A should succeed");

    // User places a file manually in the output directory
    fs::write(output_root.join("notes.txt"), "user notes").expect("should write user file");

    // Build B (same outputs)
    write_project_outputs(
        &project_a,
        &WriteOptions {
            output_root: output_root.clone(),
            project_entry_dir: Some(project_dir.clone()),
        },
    )
    .expect("build B should succeed");

    assert!(
        output_root.join("notes.txt").exists(),
        "user file should not be removed by cleanup"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn cleanup_first_build_writes_manifest_without_removing() {
    let root = temp_dir("cleanup_first_build");
    fs::create_dir_all(&root).expect("should create temp root");
    let project_dir = root.join("project");
    fs::create_dir_all(&project_dir).expect("should create project dir");
    let output_root = project_dir.join("dev");

    assert!(!output_root.join(BUILD_MANIFEST_FILENAME).exists());

    let project = Project {
        output_files: vec![OutputFile::new(
            PathBuf::from("index.html"),
            FileKind::Html(String::from("<html>Home</html>")),
        )],
        entry_page_rel: Some(PathBuf::from("index.html")),
        warnings: vec![],
    };
    write_project_outputs(
        &project,
        &WriteOptions {
            output_root: output_root.clone(),
            project_entry_dir: Some(project_dir.clone()),
        },
    )
    .expect("first build should succeed");

    assert!(output_root.join("index.html").exists());
    assert!(
        output_root.join(BUILD_MANIFEST_FILENAME).exists(),
        "manifest should be written on first build"
    );

    let manifest = read_build_manifest(&output_root);
    assert_eq!(manifest, vec![PathBuf::from("index.html")]);

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn cleanup_removes_empty_parent_directories() {
    let root = temp_dir("cleanup_empty_parents");
    fs::create_dir_all(&root).expect("should create temp root");
    let project_dir = root.join("project");
    fs::create_dir_all(&project_dir).expect("should create project dir");
    let output_root = project_dir.join("dev");

    // Build A: deeply nested file
    let project_a = Project {
        output_files: vec![OutputFile::new(
            PathBuf::from("a/b/c/file.js"),
            FileKind::Js(String::from("console.log('deep');")),
        )],
        entry_page_rel: None,
        warnings: vec![],
    };
    write_project_outputs(
        &project_a,
        &WriteOptions {
            output_root: output_root.clone(),
            project_entry_dir: Some(project_dir.clone()),
        },
    )
    .expect("build A should succeed");
    assert!(output_root.join("a/b/c/file.js").exists());

    // Build B: no files in a/
    let project_b = Project {
        output_files: vec![OutputFile::new(
            PathBuf::from("index.html"),
            FileKind::Html(String::from("<html></html>")),
        )],
        entry_page_rel: Some(PathBuf::from("index.html")),
        warnings: vec![],
    };
    write_project_outputs(
        &project_b,
        &WriteOptions {
            output_root: output_root.clone(),
            project_entry_dir: Some(project_dir.clone()),
        },
    )
    .expect("build B should succeed");

    assert!(
        !output_root.join("a").exists(),
        "entire a/b/c/ chain should be removed when all contents are stale"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn validate_output_root_rejects_dangerous_paths() {
    let project_dir = PathBuf::from("/tmp/test_project");

    let dangerous_paths = vec![
        PathBuf::from("/"),
        PathBuf::from("/usr"),
        PathBuf::from("/etc"),
        PathBuf::from("/bin"),
        PathBuf::from("/var"),
    ];

    for dangerous in dangerous_paths {
        let result = validate_output_root_is_safe(&dangerous, &project_dir);
        assert!(
            result.is_err(),
            "should reject dangerous path: {}",
            dangerous.display()
        );
    }
}

#[test]
fn validate_output_root_accepts_project_subdirectory() {
    let root = temp_dir("validate_accept");
    fs::create_dir_all(root.join("dev")).expect("should create output dir");

    let result = validate_output_root_is_safe(&root.join("dev"), &root);
    assert!(
        result.is_ok(),
        "should accept output root inside project directory"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn cleanup_ignores_traversal_paths_in_corrupt_manifest() {
    let root = temp_dir("cleanup_corrupt_manifest");
    fs::create_dir_all(&root).expect("should create temp root");
    let project_dir = root.join("project");
    fs::create_dir_all(&project_dir).expect("should create project dir");
    let output_root = project_dir.join("dev");
    fs::create_dir_all(&output_root).expect("should create output dir");

    // Write a manifest containing path traversal attempts
    fs::write(
        output_root.join(BUILD_MANIFEST_FILENAME),
        "../escape.js\n/absolute/path.js\nvalid.html\n",
    )
    .expect("should write corrupt manifest");

    // Place a file that the traversal would target
    fs::write(project_dir.join("escape.js"), "should not be deleted")
        .expect("should write escape target");

    let current_paths: HashSet<PathBuf> = HashSet::new();
    let previous = read_build_manifest(&output_root);

    // Only "valid.html" should survive manifest validation
    assert_eq!(previous, vec![PathBuf::from("valid.html")]);

    remove_stale_artifacts(&output_root, &current_paths, &previous);

    assert!(
        project_dir.join("escape.js").exists(),
        "file outside output root should not be affected by cleanup"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn corrupt_manifest_treated_as_empty() {
    let root = temp_dir("cleanup_garbage_manifest");
    fs::create_dir_all(&root).expect("should create temp root");
    let project_dir = root.join("project");
    fs::create_dir_all(&project_dir).expect("should create project dir");
    let output_root = project_dir.join("dev");
    fs::create_dir_all(&output_root).expect("should create output dir");

    // Write garbage to the manifest
    fs::write(
        output_root.join(BUILD_MANIFEST_FILENAME),
        b"\0\0\x01\x02 binary garbage \xFF\xFE",
    )
    .expect("should write garbage manifest");

    let manifest = read_build_manifest(&output_root);
    assert!(
        manifest.is_empty(),
        "corrupt manifest should be treated as empty"
    );

    // A build should succeed and overwrite with a fresh manifest
    let project = Project {
        output_files: vec![OutputFile::new(
            PathBuf::from("index.html"),
            FileKind::Html(String::from("<html></html>")),
        )],
        entry_page_rel: Some(PathBuf::from("index.html")),
        warnings: vec![],
    };
    write_project_outputs(
        &project,
        &WriteOptions {
            output_root: output_root.clone(),
            project_entry_dir: Some(project_dir.clone()),
        },
    )
    .expect("build should succeed despite corrupt manifest");

    let fresh_manifest = read_build_manifest(&output_root);
    assert_eq!(fresh_manifest, vec![PathBuf::from("index.html")]);

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn no_cleanup_when_project_entry_dir_is_none() {
    let root = temp_dir("cleanup_disabled");
    fs::create_dir_all(&root).expect("should create temp root");

    let project = Project {
        output_files: vec![OutputFile::new(
            PathBuf::from("index.html"),
            FileKind::Html(String::from("<html></html>")),
        )],
        entry_page_rel: Some(PathBuf::from("index.html")),
        warnings: vec![],
    };
    write_project_outputs(
        &project,
        &WriteOptions {
            output_root: root.clone(),
            project_entry_dir: None,
        },
    )
    .expect("build should succeed");

    assert!(root.join("index.html").exists());
    assert!(
        !root.join(BUILD_MANIFEST_FILENAME).exists(),
        "manifest should not be written when cleanup is disabled"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn write_build_manifest_produces_sorted_output() {
    let root = temp_dir("manifest_sorted");
    fs::create_dir_all(&root).expect("should create temp root");

    let paths: HashSet<PathBuf> = [
        PathBuf::from("z/page.js"),
        PathBuf::from("index.html"),
        PathBuf::from("about/index.html"),
    ]
    .into_iter()
    .collect();

    write_build_manifest(&root, &paths).expect("should write manifest");

    let content =
        fs::read_to_string(root.join(BUILD_MANIFEST_FILENAME)).expect("should read manifest file");
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines, vec!["about/index.html", "index.html", "z/page.js"]);

    fs::remove_dir_all(&root).expect("should remove temp dir");
}
