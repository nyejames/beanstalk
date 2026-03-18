//! Tests for the core build orchestration and output writer APIs.

use super::{
    FileKind, OutputFile, Project, ProjectBuilder, WriteOptions, build_project,
    resolve_project_output_root, write_project_outputs,
};
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages, ErrorLocation};
use crate::compiler_frontend::compiler_warnings::{CompilerWarning, WarningKind};
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

impl ProjectBuilder for WarningBuilder {
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
}

#[test]
fn build_project_returns_result_without_writing_files() {
    let root = temp_dir("build_only");
    fs::create_dir_all(&root).expect("should create temp root");
    let entry_file = root.join("main.bst");
    fs::write(&entry_file, "value = 1\n").expect("should write source file");

    let builder = HtmlProjectBuilder::new();
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
fn build_single_file_project_includes_reachable_import_files() {
    let root = temp_dir("single_file_reachable_imports");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::create_dir_all(root.join("utils")).expect("should create utils directory");
    fs::write(
        root.join("main.bst"),
        "import @(utils/helper/greet)\ngreet()\n",
    )
    .expect("should write main file");
    fs::write(
        root.join("utils/helper.bst"),
        "#greet||:\n    io(\"hello\")\n;\n",
    )
    .expect("should write helper file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = HtmlProjectBuilder::new();
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

    let result = build_project(&WarningBuilder, "main.bst", &[]).expect("build should succeed");

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
fn build_project_emits_runtime_fragment_with_captured_start_local() {
    let root = temp_dir("runtime_fragment_capture");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "get_name|| -> String:\n    return \"Beanstalk\"\n;\nname = get_name()\n[:Hello [name]]\n",
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = HtmlProjectBuilder::new();
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

    let builder = HtmlProjectBuilder::new();
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

    let builder = HtmlProjectBuilder::new();
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
        },
    )
    .expect("should write project outputs");

    assert!(output_root.join("index.html").exists());
    assert!(output_root.join("404.html").exists());
    assert!(output_root.join("about.html").exists());
    assert!(output_root.join("docs/basics.html").exists());

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

    let builder = HtmlProjectBuilder::new();
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
        },
    )
    .expect("should write project outputs");

    assert!(output_root.join("index.html").exists());
    assert!(output_root.join("docs.html").exists());

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

    let builder = HtmlProjectBuilder::new();
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
fn build_project_allows_const_record_coercion_with_all_defaults() {
    let root = temp_dir("const_record_all_defaults");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "Basic = |\n    body String = \"ok\",\n|\n#basic = Basic()\n",
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = HtmlProjectBuilder::new();
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

    let builder = HtmlProjectBuilder::new();
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

    let builder = HtmlProjectBuilder::new();
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
        html.contains("y: 99"),
        "runtime constructor should fill missing trailing struct fields from defaults"
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

    let builder = HtmlProjectBuilder::new();
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

    let builder = HtmlProjectBuilder::new();
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

    let builder = HtmlProjectBuilder::new();
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

    let builder = HtmlProjectBuilder::new();
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

    let builder = HtmlProjectBuilder::new();
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
fn build_project_rejects_slot_insertion_constant_without_active_wrapper() {
    let root = temp_dir("const_slot_insertion_without_wrapper");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "#slot_1 = [$insert(\"content\"): hello]\n#[slot_1]\n",
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = HtmlProjectBuilder::new();
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
        "import @libs/html/{table}\n[table:\n    [: [:Type] [:Description] ]\n    [: [:float] [:64 bit floating point number] ]\n]\n",
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = HtmlProjectBuilder::new();
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
        "import @libs/html/{format}\n#page = [:\n  <body>[$slot]</body>\n]\n#table = [format.table]\n",
    )
    .expect("should write docs style library");
    fs::write(
        root.join("main.bst"),
        "import @styles/docs/{page, table}\n[page, $markdown:\n[table:\n    [: [:Type] [:Description] ]\n    [: [:float] [:64 bit floating point number] ]\n]\n]\n",
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = HtmlProjectBuilder::new();
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
    assert!(html.contains("<td><p>Type</p></td>"));
    assert!(html.contains("<td><p>Description</p></td>"));
    assert_eq!(html.matches("<td>").count(), 4);

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_struct_default_uses_imported_constant() {
    let root = temp_dir("struct_default_imported_constant");
    fs::create_dir_all(root.join("styles")).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "import @(styles/theme/base)\nCard = |\n    color String = base,\n|\ncard = Card()\nio([: card.color])\n",
    )
    .expect("should write main source file");
    fs::write(root.join("styles/theme.bst"), "#base = \"green\"\n")
        .expect("should write imported constant source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = HtmlProjectBuilder::new();
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

    let builder = HtmlProjectBuilder::new();
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

    let builder = HtmlProjectBuilder::new();
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

    let builder = HtmlProjectBuilder::new();
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
