//! Frontend stage-by-stage pipeline regression tests.
//!
//! WHAT: exercises tokenizer, header parsing, dependency sorting, AST construction, HIR lowering,
//! and borrow checking through the public frontend entrypoints.
//! WHY: Phase 4 needs coverage across stage boundaries so refactors cannot silently break the
//! compiler pipeline while unit tests still pass in isolation.

use crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport;
use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::headers::parse_file_headers::{HeaderKind, Headers};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::style_directives::{
    StyleDirectiveEffects, StyleDirectiveHandlerSpec, StyleDirectiveRegistry, StyleDirectiveSpec,
};
use crate::compiler_frontend::symbols::identity::SourceFileTable;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::newline_handling::NewlineMode;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TemplateBodyMode, TokenizeMode};
use crate::compiler_frontend::{CompilerFrontend, FrontendBuildProfile};
use crate::compiler_frontend::{
    hir::hir_nodes::{HirPlace, HirStatementKind},
    host_functions::CallTarget,
};
use crate::projects::settings::{Config, IMPLICIT_START_FUNC_NAME};
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
    fn new(
        files: &[(&str, &str)],
        entry_relative_path: &str,
        style_directives: StyleDirectiveRegistry,
    ) -> Self {
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
            style_directives,
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

    fn sorted_headers(&mut self) -> crate::compiler_frontend::module_dependencies::SortedHeaders {
        let headers = self.headers();
        self.frontend
            .sort_headers(headers)
            .expect("header sorting should succeed")
    }

    fn ast(&mut self) -> crate::compiler_frontend::ast::Ast {
        let sorted = self.sorted_headers();
        self.frontend
            .headers_to_ast(sorted, &self.entry_file, FrontendBuildProfile::Dev)
            .expect("AST construction should succeed")
    }

    fn hir(&mut self) -> crate::compiler_frontend::hir::hir_nodes::HirModule {
        let ast = self.ast();
        self.frontend
            .generate_hir(ast)
            .expect("HIR lowering should succeed")
    }

    fn borrow_checked_hir(&mut self) -> BorrowCheckReport {
        let hir = self.hir();

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
fn start_function_is_excluded_from_dependency_graph_and_appended_last() {
    let mut project = FrontendProject::new(
        &[
            (
                "src/#page.bst",
                "import @helper\n#helper_const = \"page_helper\"\nio(\"page\")\n",
            ),
            (
                "src/helper.bst",
                "import @leaf\n#leaf_const = \"helper_leaf\"\n",
            ),
            ("src/leaf.bst", "#leaf_const = \"leaf_value\"\n"),
        ],
        "src/#page.bst",
        StyleDirectiveRegistry::built_ins(),
    );

    let headers = project.headers();
    let sorted = project
        .frontend
        .sort_headers(headers)
        .expect("dependency sorting should succeed");

    // `start` does not participate in graph sorting; only the entry file has one.
    let start_headers: Vec<_> = sorted
        .headers
        .iter()
        .filter(|header| matches!(header.kind, HeaderKind::StartFunction))
        .collect();

    assert_eq!(
        start_headers.len(),
        1,
        "only the entry file should produce a StartFunction header"
    );
    assert_eq!(
        start_headers[0].source_file,
        project.logical_path("src/#page.bst"),
        "entry start should be the only StartFunction header"
    );

    // `start` is appended last among all headers for the entry file.
    let entry_file_headers: Vec<_> = sorted
        .headers
        .iter()
        .filter(|header| header.source_file == project.logical_path("src/#page.bst"))
        .collect();

    assert!(
        matches!(
            entry_file_headers.last().unwrap().kind,
            HeaderKind::StartFunction
        ),
        "start should be appended last for the entry file"
    );
}

#[test]
fn ast_resolves_struct_constructor_field_types_and_emits_start_last() {
    let mut project = FrontendProject::new(
        &[(
            "src/#page.bst",
            "Inner = |\n    value Int,\n|\n\nOuter = |\n    inner Inner,\n|\n\nwrapper = Outer(Inner(1))\nio(wrapper.inner.value)\n",
        )],
        "src/#page.bst",
        StyleDirectiveRegistry::built_ins(),
    );

    let ast = project.ast();
    let mut inner_path = None;
    let mut outer_field_type = None;
    let mut start_function_index = None;

    for (index, node) in ast.nodes.iter().enumerate() {
        match &node.kind {
            NodeKind::StructDefinition(path, fields)
                if path.name_str(&project.frontend.string_table) == Some("Inner") =>
            {
                inner_path = Some(path.clone());
                assert_eq!(fields.len(), 1, "Inner should expose exactly one field");
            }
            NodeKind::StructDefinition(path, fields)
                if path.name_str(&project.frontend.string_table) == Some("Outer") =>
            {
                assert_eq!(fields.len(), 1, "Outer should expose exactly one field");
                outer_field_type = Some(fields[0].value.data_type.clone());
            }
            NodeKind::Function(path, _, _)
                if path.name_str(&project.frontend.string_table)
                    == Some(IMPLICIT_START_FUNC_NAME) =>
            {
                start_function_index = Some(index);
            }
            _ => {}
        }
    }

    let inner_path = inner_path.expect("Inner struct definition should be emitted");
    let outer_field_type = outer_field_type.expect("Outer struct definition should be emitted");

    match outer_field_type {
        DataType::Struct { nominal_path, .. } => {
            assert_eq!(
                nominal_path, inner_path,
                "Outer.inner should resolve to the concrete Inner struct type",
            );
        }
        DataType::NamedType(_) => {
            panic!("Outer.inner should not retain unresolved NamedType placeholders in AST");
        }
        other => {
            panic!("Outer.inner should resolve to struct type, got: {other:?}");
        }
    }

    let start_function_index =
        start_function_index.expect("entry start function should be emitted by AST");
    assert_eq!(
        start_function_index,
        ast.nodes.len() - 1,
        "entry start function should be emitted after top-level declarations",
    );
}

#[test]
fn reports_circular_imports_through_frontend_header_sorting() {
    let mut project = FrontendProject::new(
        &[
            (
                "src/a.bst",
                "import @b/BStruct\n#AStruct = | value String, link BStruct |\n",
            ),
            (
                "src/b.bst",
                "import @a/AStruct\n#BStruct = | value String, link AStruct |\n",
            ),
        ],
        "src/a.bst",
        StyleDirectiveRegistry::built_ins(),
    );

    let headers = project.headers();
    let errors = project
        .frontend
        .sort_headers(headers)
        .expect_err("cycle should fail dependency sorting");

    assert!(
        errors
            .iter()
            .any(|error| error.msg.contains("Circular dependency detected")),
        "expected circular dependency diagnostic, got: {errors:?}"
    );
}

#[test]
fn compiles_single_file_program_through_borrow_check() {
    let mut project = FrontendProject::new(
        &[(
            "src/#page.bst",
            "Point = |\n    value Int,\n|\npoint = Point(1)\nloop 0 to 2 |i|:\n    io(point.value)\n;\n",
        )],
        "src/#page.bst",
        StyleDirectiveRegistry::built_ins(),
    );

    let report = project.borrow_checked_hir();

    assert!(report.stats.functions_analyzed >= 1);
    assert!(!report.analysis.statement_facts.is_empty());
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
        StyleDirectiveRegistry::built_ins(),
    );

    let report = project.borrow_checked_hir();

    assert!(report.stats.functions_analyzed >= 2);
    assert!(!report.analysis.value_facts.is_empty());
}

#[test]
fn compiles_collection_builtins_and_error_propagation_through_borrow_check() {
    let mut project = FrontendProject::new(
        &[(
            "src/#page.bst",
            "first_or_error |values {Int}, idx Int| -> Int, Error!:\n    return values.get(idx)!\n;\n\nmutate_and_length || -> Int:\n    values ~= {1, 2, 3}\n    ~values.set(0, 9)\n    values.get(1) = 8\n    ~values.push(4)\n    ~values.remove(0)\n    return values.length()\n;\n\ntotal = mutate_and_length()\npicked = first_or_error({10, 20}, 1) ! 0\nio(total)\nio(picked)\n",
        )],
        "src/#page.bst",
        StyleDirectiveRegistry::built_ins(),
    );

    let hir = project.hir();

    assert!(
        hir.blocks
            .iter()
            .flat_map(|block| block.statements.iter())
            .any(|statement| matches!(
                statement.kind,
                HirStatementKind::Assign {
                    target: HirPlace::Index { .. },
                    ..
                }
            )),
        "expected at least one indexed assignment in lowered HIR"
    );

    assert!(
        hir.blocks
            .iter()
            .flat_map(|block| block.statements.iter())
            .any(|statement| match &statement.kind {
                HirStatementKind::Call {
                    target: CallTarget::HostFunction(path),
                    ..
                } => {
                    path.name_str(&project.frontend.string_table) == Some("__bs_collection_get")
                }
                _ => false,
            }),
        "expected collection get(...) to lower into a host call target"
    );

    let report = project
        .frontend
        .check_borrows(&hir)
        .expect("borrow checking should succeed");

    assert!(report.stats.functions_analyzed >= 2);
    assert!(!report.analysis.statement_facts.is_empty());
}

#[test]
fn ast_stage_errors_preserve_string_table_context() {
    let mut project = FrontendProject::new(
        &[("src/#page.bst", "#bad = io(\"runtime host call\")\n")],
        "src/#page.bst",
        StyleDirectiveRegistry::built_ins(),
    );

    let sorted = project.sorted_headers();
    let Err(messages) =
        project
            .frontend
            .headers_to_ast(sorted, &project.entry_file, FrontendBuildProfile::Dev)
    else {
        panic!("const host calls should fail during AST construction");
    };

    let resolved_scope = messages.errors[0]
        .location
        .scope
        .to_portable_string(&messages.string_table);
    let expected_scope = project
        .logical_path("src/#page.bst")
        .to_portable_string(&messages.string_table);
    assert!(
        resolved_scope == expected_scope,
        "AST errors should preserve the logical source path in the returned StringTable, expected '{expected_scope}', got '{resolved_scope}'",
    );
}

#[test]
fn borrow_checker_errors_preserve_string_table_context() {
    let mut project = FrontendProject::new(
        &[(
            "src/#page.bst",
            "data ~= [\"shared data\"]\nref1 ~= data\nref2 ~= data\nresult = [ref1, ref2]\n",
        )],
        "src/#page.bst",
        StyleDirectiveRegistry::built_ins(),
    );

    let hir = project.hir();
    let messages = project
        .frontend
        .check_borrows(&hir)
        .expect_err("multiple mutable borrows should fail borrow checking");

    let resolved_scope = messages.errors[0]
        .location
        .scope
        .to_portable_string(&messages.string_table);
    let expected_scope = project
        .logical_path("src/#page.bst")
        .to_portable_string(&messages.string_table);
    assert!(
        resolved_scope == expected_scope,
        "borrow checker errors should preserve the logical source path in the returned StringTable, expected '{expected_scope}', got '{resolved_scope}'",
    );
}

// -----------------------------------------------------------------------------
// Deferred constant-header resolution regression tests
// -----------------------------------------------------------------------------
#[test]
fn deferred_constant_resolution_same_file_constant() {
    // WHAT: a constant references another exported constant in the same file that appears
    // later in source order. Because constant initializer references are soft edges (not
    // strict structural dependencies), the topological sort may place the dependent constant
    // first. The fixed-point loop must defer it until the referenced constant is resolved.
    let mut project = FrontendProject::new(
        &[("src/#page.bst", "#page_head = theme\n#theme = \"dark\"\n")],
        "src/#page.bst",
        StyleDirectiveRegistry::built_ins(),
    );

    let ast = project.ast();

    let page_head = ast
        .module_constants
        .iter()
        .find(|c| c.id.name_str(&project.frontend.string_table) == Some("page_head"))
        .expect("page_head constant should exist");
    assert!(
        matches!(page_head.value.kind, ExpressionKind::StringSlice(_)),
        "page_head should be resolved to a string literal, got {:?}",
        page_head.value.kind
    );
}

#[test]
fn deferred_constant_resolution_imported_constant() {
    // WHAT: a constant references an imported constant from another file. The imported
    // constant may be processed later in the fixed-point loop. This must defer cleanly
    // and then resolve successfully.
    let mut project = FrontendProject::new(
        &[
            (
                "src/#page.bst",
                "import @helper/theme\n#page_head = theme\n",
            ),
            ("src/helper.bst", "#theme = \"dark\"\n"),
        ],
        "src/#page.bst",
        StyleDirectiveRegistry::built_ins(),
    );

    let ast = project.ast();

    let page_head = ast
        .module_constants
        .iter()
        .find(|c| c.id.name_str(&project.frontend.string_table) == Some("page_head"))
        .expect("page_head constant should exist");
    assert!(
        matches!(page_head.value.kind, ExpressionKind::StringSlice(_)),
        "page_head should be resolved to a string literal, got {:?}",
        page_head.value.kind
    );
}

#[test]
fn deferred_constant_resolution_nested_template_reference() {
    // WHAT: a template constant references another template constant inside its body.
    // The reference is nested inside a TemplateAtom::Content expression. The walker
    // must find the unresolved Reference and defer the outer constant.
    let mut project = FrontendProject::new(
        &[(
            "src/#page.bst",
            "#head = [$html: <style>[css]</style>]\n#css = [$css: body {}]\n",
        )],
        "src/#page.bst",
        StyleDirectiveRegistry::built_ins(),
    );

    let ast = project.ast();

    let head = ast
        .module_constants
        .iter()
        .find(|c| c.id.name_str(&project.frontend.string_table) == Some("head"))
        .expect("head constant should exist");
    assert!(
        matches!(head.value.kind, ExpressionKind::Template(_)),
        "head should be resolved to a template, got {:?}",
        head.value.kind
    );
}

#[test]
fn deferred_constant_resolution_collection_reference() {
    // WHAT: a collection constant contains a reference to an unresolved constant.
    // The walker must recurse into Collection items and detect the placeholder.
    let mut project = FrontendProject::new(
        &[(
            "src/#page.bst",
            "#all = {theme, \"extra\"}\n#theme = \"dark\"\n",
        )],
        "src/#page.bst",
        StyleDirectiveRegistry::built_ins(),
    );

    let ast = project.ast();

    let all = ast
        .module_constants
        .iter()
        .find(|c| c.id.name_str(&project.frontend.string_table) == Some("all"))
        .expect("all constant should exist");
    assert!(
        matches!(all.value.kind, ExpressionKind::Collection(_)),
        "all should be resolved to a collection, got {:?}",
        all.value.kind
    );
}

#[test]
fn deferred_constant_resolution_struct_literal_reference() {
    // WHAT: a struct-instance constant references an unresolved constant in a field
    // position. The walker must recurse into StructInstance fields.
    let mut project = FrontendProject::new(
        &[(
            "src/#page.bst",
            "#wrapper = Wrapper(theme)\n#theme = \"dark\"\n\nWrapper = | value String |\n",
        )],
        "src/#page.bst",
        StyleDirectiveRegistry::built_ins(),
    );

    let ast = project.ast();

    let wrapper = ast
        .module_constants
        .iter()
        .find(|c| c.id.name_str(&project.frontend.string_table) == Some("wrapper"))
        .expect("wrapper constant should exist");
    assert!(
        matches!(wrapper.value.kind, ExpressionKind::StructInstance(_)),
        "wrapper should be resolved to a struct instance, got {:?}",
        wrapper.value.kind
    );
}

// -----------------------------------------------------------------------------
// Struct field default value regression tests
// -----------------------------------------------------------------------------

#[test]
fn struct_default_references_same_file_constant() {
    // WHAT: a struct field default references a constant declared in the same file.
    // The constant is resolved before struct field types, so the default should inline.
    let mut project = FrontendProject::new(
        &[(
            "src/#page.bst",
            "#default_theme = \"dark\"\nConfig = | theme String = default_theme |\n",
        )],
        "src/#page.bst",
        StyleDirectiveRegistry::built_ins(),
    );

    let ast = project.ast();

    let config_fields = ast
        .nodes
        .iter()
        .find_map(|n| match &n.kind {
            NodeKind::StructDefinition(path, fields)
                if path.name_str(&project.frontend.string_table) == Some("Config") =>
            {
                Some(fields.clone())
            }
            _ => None,
        })
        .expect("Config struct should exist");

    let theme_default = config_fields
        .iter()
        .find(|f| f.id.name_str(&project.frontend.string_table) == Some("theme"))
        .expect("theme field should exist");
    assert!(
        matches!(theme_default.value.kind, ExpressionKind::StringSlice(_)),
        "theme default should be resolved to a string literal, got {:?}",
        theme_default.value.kind
    );
}

#[test]
fn struct_default_references_imported_constant() {
    // WHAT: a struct field default references an imported constant.
    let mut project = FrontendProject::new(
        &[
            (
                "src/#page.bst",
                "import @helper/default_theme\nConfig = | theme String = default_theme |\n",
            ),
            ("src/helper.bst", "#default_theme = \"dark\"\n"),
        ],
        "src/#page.bst",
        StyleDirectiveRegistry::built_ins(),
    );

    let ast = project.ast();

    let config_fields = ast
        .nodes
        .iter()
        .find_map(|n| match &n.kind {
            NodeKind::StructDefinition(path, fields)
                if path.name_str(&project.frontend.string_table) == Some("Config") =>
            {
                Some(fields.clone())
            }
            _ => None,
        })
        .expect("Config struct should exist");

    let theme_default = config_fields
        .iter()
        .find(|f| f.id.name_str(&project.frontend.string_table) == Some("theme"))
        .expect("theme field should exist");
    assert!(
        matches!(theme_default.value.kind, ExpressionKind::StringSlice(_)),
        "theme default should inline the imported constant, got {:?}",
        theme_default.value.kind
    );
}

#[test]
fn struct_default_errors_on_visible_non_constant() {
    // WHAT: a struct field default references a visible symbol that is a function,
    // not a constant. This must fail with a clear compile-time value error.
    let mut project = FrontendProject::new(
        &[(
            "src/#page.bst",
            "helper || -> String:\n    return \"value\"\n;\nConfig = | theme String = helper |\n",
        )],
        "src/#page.bst",
        StyleDirectiveRegistry::built_ins(),
    );

    let sorted = project.sorted_headers();
    let result =
        project
            .frontend
            .headers_to_ast(sorted, &project.entry_file, FrontendBuildProfile::Dev);
    let messages = match result {
        Err(messages) => messages,
        Ok(_) => panic!("AST should fail when struct default references a function"),
    };
    assert!(
        messages
            .errors
            .iter()
            .any(|e| e.msg.contains("compile-time value")),
        "expected compile-time error for function reference in struct default, got: {:?}",
        messages.errors
    );
}

// -----------------------------------------------------------------------------
// Build-system style directive regression test
// -----------------------------------------------------------------------------

#[test]
fn html_style_directive_available_during_header_parsing() {
    // WHAT: project-owned style directives (like $html) must be visible during header-owned
    // parsing paths — specifically constant header expression parsing and template parsing.
    // This covers the docs-build failure mode where [$html: ...] templates in exported
    // constants could not be parsed because the directive registry was incomplete.
    let html_directive = StyleDirectiveSpec::handler(
        "html",
        TemplateBodyMode::Normal,
        StyleDirectiveHandlerSpec::new(
            None,
            StyleDirectiveEffects {
                style_id: Some("html"),
                ..StyleDirectiveEffects::default()
            },
            None,
        ),
    );
    let directives = StyleDirectiveRegistry::merged(&[html_directive])
        .expect("merged directive registry should build");

    let mut project = FrontendProject::new(
        &[("src/#page.bst", "#head = [$html: <div>Hello</div>]\n")],
        "src/#page.bst",
        directives,
    );

    let ast = project.ast();

    let head = ast
        .module_constants
        .iter()
        .find(|c| c.id.name_str(&project.frontend.string_table) == Some("head"))
        .expect("head constant should exist");
    assert!(
        matches!(head.value.kind, ExpressionKind::Template(_)),
        "head should be resolved to a template when $html directive is available, got {:?}",
        head.value.kind
    );
}
