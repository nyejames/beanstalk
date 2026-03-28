//! Frontend stage-by-stage pipeline regression tests.
//!
//! WHAT: exercises tokenizer, header parsing, dependency sorting, AST construction, HIR lowering,
//! and borrow checking through the public frontend entrypoints.
//! WHY: Phase 4 needs coverage across stage boundaries so refactors cannot silently break the
//! compiler pipeline while unit tests still pass in isolation.

use crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport;
use crate::compiler_frontend::headers::parse_file_headers::{HeaderKind, Headers};
use crate::compiler_frontend::identity::SourceFileTable;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::tokenizer::newline_handling::NewlineMode;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenizeMode};
use crate::compiler_frontend::{CompilerFrontend, FrontendBuildProfile};
use crate::projects::settings::Config;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

struct FrontendProject {
    _temp_dir: TempDir,
    project_root: PathBuf,
    entry_file: PathBuf,
    files: Vec<PathBuf>,
    logical_paths: Vec<(PathBuf, InternedPath)>,
    frontend: CompilerFrontend,
}

impl FrontendProject {
    fn new(files: &[(&str, &str)], entry_relative_path: &str) -> Self {
        let temp_dir = tempfile::tempdir().expect("should create temp dir");
        let project_root = temp_dir.path().join("project");
        let entry_root = project_root.join("src");
        fs::create_dir_all(&entry_root).expect("should create project entry root");

        let mut canonical_files = Vec::with_capacity(files.len());
        for (relative_path, source) in files {
            let full_path = project_root.join(relative_path);
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent).expect("should create parent directories");
            }
            fs::write(&full_path, source).expect("should write test source");
            canonical_files.push(
                fs::canonicalize(&full_path).expect("test source should canonicalize after write"),
            );
        }

        let canonical_project_root =
            fs::canonicalize(&project_root).expect("project root should canonicalize");
        let canonical_entry_root =
            fs::canonicalize(&entry_root).expect("entry root should canonicalize");
        let entry_file = fs::canonicalize(project_root.join(entry_relative_path))
            .expect("entry file should canonicalize");
        let resolver =
            ProjectPathResolver::new(canonical_project_root.clone(), canonical_entry_root, &[])
                .expect("project path resolver should build");

        let mut string_table = StringTable::new();
        let source_files = SourceFileTable::build(
            &canonical_files,
            &entry_file,
            Some(&resolver),
            &mut string_table,
        )
        .expect("source file table should build");
        let logical_paths = canonical_files
            .iter()
            .map(|canonical_file| {
                let logical_path = source_files
                    .get_by_canonical_path(canonical_file)
                    .expect("source file identity should exist")
                    .logical_path
                    .clone();
                (canonical_file.clone(), logical_path)
            })
            .collect::<Vec<_>>();

        let mut frontend = CompilerFrontend::new(
            &Config::new(canonical_project_root),
            string_table,
            StyleDirectiveRegistry::built_ins(),
            Some(resolver),
            NewlineMode::NormalizeToLf,
        );
        frontend.set_source_files(source_files);

        Self {
            _temp_dir: temp_dir,
            project_root,
            entry_file,
            files: canonical_files,
            logical_paths,
            frontend,
        }
    }

    fn tokenize_all(&mut self) -> Vec<FileTokens> {
        let mut tokenized_files = Vec::with_capacity(self.files.len());

        for file in &self.files {
            let source = fs::read_to_string(file).expect("should read source file");
            tokenized_files.push(
                self.frontend
                    .source_to_tokens(&source, file, TokenizeMode::Normal)
                    .expect("tokenization should succeed"),
            );
        }

        tokenized_files
    }

    fn logical_path(&self, relative_path: &str) -> InternedPath {
        let canonical = fs::canonicalize(self.project_root.join(relative_path))
            .expect("fixture file should canonicalize");
        self.logical_paths
            .iter()
            .find_map(|(file, logical_path)| {
                if file == &canonical {
                    Some(logical_path.clone())
                } else {
                    None
                }
            })
            .expect("logical path should exist for fixture file")
    }

    fn headers(&mut self) -> Headers {
        let tokenized_files = self.tokenize_all();
        let mut warnings = Vec::new();
        self.frontend
            .tokens_to_headers(tokenized_files, &mut warnings, &self.entry_file)
            .expect("header parsing should succeed")
    }

    fn borrow_checked_hir(&mut self) -> BorrowCheckReport {
        let headers = self.headers();
        let sorted_headers = self
            .frontend
            .sort_headers(headers.headers)
            .expect("header sorting should succeed");
        let ast = self
            .frontend
            .headers_to_ast(
                sorted_headers,
                headers.top_level_template_items,
                &self.entry_file,
                FrontendBuildProfile::Dev,
            )
            .expect("AST construction should succeed");
        let hir = self
            .frontend
            .generate_hir(ast)
            .expect("HIR lowering should succeed");

        assert!(
            !hir.functions.is_empty(),
            "pipeline smoke test should produce at least one HIR function"
        );

        self.frontend
            .check_borrows(&hir)
            .expect("borrow checking should succeed")
    }
}

#[test]
fn sorts_cross_module_start_function_dependencies_through_frontend_entrypoints() {
    let mut project = FrontendProject::new(
        &[
            ("src/#page.bst", "import @helper\nhelper()\nio(\"page\")\n"),
            ("src/helper.bst", "import @leaf\nleaf()\nio(\"helper\")\n"),
            ("src/leaf.bst", "io(\"leaf\")\n"),
        ],
        "src/#page.bst",
    );

    let headers = project.headers();
    let sorted_headers = project
        .frontend
        .sort_headers(headers.headers)
        .expect("dependency sorting should succeed");

    let start_order = sorted_headers
        .iter()
        .filter(|header| matches!(header.kind, HeaderKind::StartFunction))
        .map(|header| header.source_file.clone())
        .collect::<Vec<_>>();

    assert_eq!(
        start_order,
        vec![
            project.logical_path("src/leaf.bst"),
            project.logical_path("src/helper.bst"),
            project.logical_path("src/#page.bst"),
        ]
    );
}

#[test]
fn reports_circular_imports_through_frontend_header_sorting() {
    let mut project = FrontendProject::new(
        &[
            ("src/a.bst", "import @b\nb()\nio(\"a\")\n"),
            ("src/b.bst", "import @a\na()\nio(\"b\")\n"),
        ],
        "src/a.bst",
    );

    let headers = project.headers();
    let errors = project
        .frontend
        .sort_headers(headers.headers)
        .expect_err("cycle should fail dependency sorting");

    assert!(
        errors
            .iter()
            .any(|error| error.msg.contains("Circular dependency detected")),
        "expected circular dependency diagnostic, got: {errors:?}"
    );
}

#[test]
fn preserves_symbol_resolution_order_for_struct_defaults_and_constants() {
    let mut project = FrontendProject::new(
        &[(
            "src/#page.bst",
            "User = |\n    name String = base,\n|\n#base = \"Ada\"\n#derived User = User(base)\n",
        )],
        "src/#page.bst",
    );

    let headers = project.headers();
    let sorted_headers = project
        .frontend
        .sort_headers(headers.headers)
        .expect("dependency sorting should succeed");

    let base_pos = sorted_headers
        .iter()
        .position(|header| {
            matches!(
                &header.kind,
                HeaderKind::Constant { metadata } if metadata.file_constant_order == 0
            )
        })
        .expect("base constant should exist");
    let struct_pos = sorted_headers
        .iter()
        .position(|header| matches!(header.kind, HeaderKind::Struct { .. }))
        .expect("struct header should exist");
    let derived_pos = sorted_headers
        .iter()
        .position(|header| {
            matches!(
                &header.kind,
                HeaderKind::Constant { metadata } if metadata.file_constant_order == 1
            )
        })
        .expect("derived constant should exist");

    assert!(base_pos < struct_pos);
    assert!(struct_pos < derived_pos);
}

#[test]
fn compiles_single_file_program_through_borrow_check() {
    let mut project = FrontendProject::new(
        &[(
            "src/#page.bst",
            "Point = |\n    value Int,\n|\npoint = Point(1)\nloop i in 0 to 2:\n    io(point.value)\n;\n",
        )],
        "src/#page.bst",
    );

    let report = project.borrow_checked_hir();

    assert!(report.stats.functions_analyzed >= 1);
    assert!(report.analysis.statement_facts.len() >= 1);
}

#[test]
fn compiles_multi_file_import_program_through_borrow_check() {
    let mut project = FrontendProject::new(
        &[
            (
                "src/#page.bst",
                "import @helper/add\nresult = add(1, 2)\nio(result)\n",
            ),
            (
                "src/helper.bst",
                "#add |left Int, right Int| -> Int:\n    return left + right\n;\n",
            ),
        ],
        "src/#page.bst",
    );

    let report = project.borrow_checked_hir();

    assert!(report.stats.functions_analyzed >= 2);
    assert!(report.analysis.value_facts.len() >= 1);
}
