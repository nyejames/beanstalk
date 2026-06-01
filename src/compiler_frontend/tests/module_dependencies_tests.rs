//! Dependency sorting regression tests.
//!
//! WHAT: validates topological ordering, cycle detection, deterministic order, and start-function
//!       exclusion from the import dependency graph.
//! WHY: dependency sorting is the single producer of sorted declaration placeholders; any drift
//!      here breaks cross-file visibility and AST constant dependency ordering.

use super::*;
use crate::compiler_frontend::compiler_messages::DiagnosticPayload;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::headers::parse_file_headers::{
    HeaderKind, HeaderParseOptions, Headers, parse_headers, prepare_file_from_tokens,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::TokenizeMode;
use crate::libraries::SourceLibraryRegistry;
use crate::libraries::external_import_providers::resolution_table::ExternalImportResolutionTable;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

fn parse_module_headers(files: &[(&str, &str)], entry_path: &str) -> (Headers, StringTable) {
    let mut string_table = StringTable::new();
    let external_package_registry = ExternalPackageRegistry::new();
    let options = HeaderParseOptions::default();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let entry_path_buf = PathBuf::from(entry_path);

    let mut prepared_outputs = Vec::with_capacity(files.len());
    let mut const_template_offset = 0usize;
    let mut runtime_fragment_offset = 0usize;

    for (path, source) in files {
        let path_buf = PathBuf::from(path);
        let interned_path = InternedPath::from_path_buf(&path_buf, &mut string_table);
        let file_tokens = tokenize(
            source,
            &interned_path,
            TokenizeMode::Normal,
            &style_directives,
            &mut string_table,
            None,
        )
        .expect("tokenization should succeed");

        let output = prepare_file_from_tokens(
            file_tokens,
            &entry_path_buf,
            &options,
            &external_package_registry,
            &mut string_table,
            const_template_offset,
            runtime_fragment_offset,
        )
        .expect("preparation should succeed");

        const_template_offset += output.const_template_count;
        runtime_fragment_offset += output.runtime_fragment_count;
        prepared_outputs.push(output);
    }

    let headers = parse_headers(
        prepared_outputs,
        &external_package_registry,
        &ExternalImportResolutionTable::default(),
        options.project_path_resolver.as_ref(),
        &mut string_table,
    )
    .expect("header parsing should succeed");

    (headers, string_table)
}

fn parse_module_headers_with_project_resolver(
    files: &[(PathBuf, &str)],
    entry_path: &Path,
    project_path_resolver: ProjectPathResolver,
) -> (Headers, StringTable) {
    let mut string_table = StringTable::new();
    let external_package_registry = ExternalPackageRegistry::new();
    let options = HeaderParseOptions {
        project_path_resolver: Some(project_path_resolver),
        ..HeaderParseOptions::default()
    };
    let style_directives = StyleDirectiveRegistry::built_ins();

    let mut prepared_outputs = Vec::with_capacity(files.len());
    let mut const_template_offset = 0usize;
    let mut runtime_fragment_offset = 0usize;

    for (path, source) in files {
        let canonical_path = fs::canonicalize(path).expect("test source should canonicalize");
        let logical_path = options
            .project_path_resolver
            .as_ref()
            .expect("test resolver should be present")
            .logical_path_for_canonical_file(&canonical_path, &mut string_table)
            .expect("test source should have a logical path");
        let interned_path = InternedPath::from_path_buf(&logical_path, &mut string_table);
        let mut file_tokens = tokenize(
            source,
            &interned_path,
            TokenizeMode::Normal,
            &style_directives,
            &mut string_table,
            None,
        )
        .expect("tokenization should succeed");
        file_tokens.canonical_os_path = Some(canonical_path);

        let output = prepare_file_from_tokens(
            file_tokens,
            entry_path,
            &options,
            &external_package_registry,
            &mut string_table,
            const_template_offset,
            runtime_fragment_offset,
        )
        .expect("preparation should succeed");

        const_template_offset += output.const_template_count;
        runtime_fragment_offset += output.runtime_fragment_count;
        prepared_outputs.push(output);
    }

    let headers = parse_headers(
        prepared_outputs,
        &external_package_registry,
        &ExternalImportResolutionTable::default(),
        options.project_path_resolver.as_ref(),
        &mut string_table,
    )
    .expect("header parsing should succeed");

    (headers, string_table)
}

fn header_name(
    header: &crate::compiler_frontend::headers::parse_file_headers::Header,
    string_table: &StringTable,
) -> String {
    header
        .tokens
        .src_path
        .name_str(string_table)
        .unwrap_or_default()
        .to_string()
}

#[test]
fn sorts_strict_top_level_dependencies_before_dependents_and_appends_start_last() {
    let (headers, mut string_table) = parse_module_headers(
        &[
            ("src/a.bst", "import @b { Middle }\nTop #Middle = Middle\n"),
            ("src/b.bst", "import @c { Thing }\nMiddle #Thing = Thing\n"),
            ("src/c.bst", "Thing #Int = 1\n"),
        ],
        "src/a.bst",
    );

    let sorted = resolve_module_dependencies(headers, &mut string_table)
        .expect("dependency sort should pass");

    let non_start_order = sorted
        .headers
        .iter()
        .filter(|header| !matches!(header.kind, HeaderKind::StartFunction))
        .map(|header| header_name(header, &string_table))
        .collect::<Vec<_>>();

    assert_eq!(non_start_order, vec!["Thing", "Middle", "Top"]);
    assert!(
        matches!(
            sorted.headers.last().map(|header| &header.kind),
            Some(HeaderKind::StartFunction)
        ),
        "entry start header must be appended after sorted top-level declarations"
    );

    let start_order = sorted
        .headers
        .iter()
        .filter(|header| matches!(header.kind, HeaderKind::StartFunction))
        .map(|header| header.source_file.to_portable_string(&string_table))
        .collect::<Vec<_>>();

    assert_eq!(start_order, vec!["src/a.bst"]);
}

#[test]
fn reports_circular_dependencies() {
    let (headers, mut string_table) = parse_module_headers(
        &[
            ("src/a.bst", "import @b { Middle }\nTop #Middle = Middle\n"),
            ("src/b.bst", "import @a { Top }\nMiddle #Top = Top\n"),
        ],
        "src/a.bst",
    );

    let bag = resolve_module_dependencies(headers, &mut string_table)
        .expect_err("cycle should fail dependency sorting");

    let cycle_diagnostic = bag
        .diagnostics()
        .iter()
        .find(|diagnostic| {
            let DiagnosticPayload::CircularDependency { path } = &diagnostic.payload else {
                return false;
            };

            let path = path.to_portable_string(&string_table);
            path.contains("Top") || path.contains("Middle")
        })
        .unwrap_or_else(|| panic!("expected a cycle diagnostic, got: {bag:?}"));

    assert!(
        cycle_diagnostic
            .primary_location
            .scope
            .to_portable_string(&string_table)
            .contains("src/"),
        "cycle diagnostics should point at a declaration location instead of the default location"
    );
}

#[test]
fn constant_initializer_creates_dependency_sort_edge() {
    // WHY: header-stage constant_dependencies.rs now extracts initializer reference edges.
    // Constant initializers that reference other constants create top-level dependency edges
    // that dependency sorting respects.
    let (headers, mut string_table) = parse_module_headers(
        &[
            // Config's initializer references Value.
            // That reference creates a dependency edge from Config to Value.
            ("src/a.bst", "import @b { Value }\nConfig #= Value\n"),
            ("src/b.bst", "Value #Int = 42\n"),
        ],
        "src/a.bst",
    );

    let sorted = resolve_module_dependencies(headers, &mut string_table)
        .expect("sort must succeed — constant initializer edges are resolved by headers");

    let non_start_names: Vec<_> = sorted
        .headers
        .iter()
        .filter(|h| !matches!(h.kind, HeaderKind::StartFunction))
        .map(|h| header_name(h, &string_table))
        .collect();

    // Both headers must be present and Value must precede Config.
    assert_eq!(
        non_start_names,
        vec!["Value", "Config"],
        "constant initializer dependency must order Value before Config"
    );
}

#[test]
fn function_body_references_do_not_influence_header_provided_sort_order() {
    // WHY: function body references are AST/body-phase concerns, not
    // header-provided top-level dependency edges. Sorting should preserve source order
    // for otherwise-independent declarations.
    let (headers, mut string_table) = parse_module_headers(
        &[(
            "src/a.bst",
            "first|| -> Int:\n    return second()\n;\n\nsecond|| -> Int:\n    return 1\n;\n",
        )],
        "src/a.bst",
    );

    let sorted = resolve_module_dependencies(headers, &mut string_table)
        .expect("dependency sort should ignore body-only references");

    let non_start_names: Vec<_> = sorted
        .headers
        .iter()
        .filter(|header| !matches!(header.kind, HeaderKind::StartFunction))
        .map(|header| header_name(header, &string_table))
        .collect();

    assert_eq!(
        non_start_names,
        vec!["first", "second"],
        "function body call graph must not perturb strict header sorting"
    );
}

#[test]
fn function_error_return_dependency_orders_error_type_before_function() {
    // WHY: `T!` is signature metadata, not a body reference. Header dependency sorting must
    // order imported error payload declarations before functions that expose them.
    let (headers, mut string_table) = parse_module_headers(
        &[
            (
                "src/app.bst",
                "import @errors { AppError }\nparse|| -> Int, AppError!:\n    return 1\n;\n",
            ),
            ("src/errors.bst", "AppError = |message String|\n"),
        ],
        "src/app.bst",
    );

    let sorted = resolve_module_dependencies(headers, &mut string_table)
        .expect("error return dependency should be sortable");

    let non_start_names: Vec<_> = sorted
        .headers
        .iter()
        .filter(|header| !matches!(header.kind, HeaderKind::StartFunction))
        .map(|header| header_name(header, &string_table))
        .collect();

    assert_eq!(
        non_start_names,
        vec!["AppError", "parse"],
        "error payload type must be sorted before the fallible function signature"
    );
}

#[test]
fn module_root_facade_export_dependencies_order_facade_before_consumers() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let project_root =
        fs::canonicalize(temp_dir.path()).expect("temp project root should canonicalize");
    let entry_root = project_root.join("src");

    fs::create_dir_all(entry_root.join("styles")).expect("should create styles dir");
    fs::create_dir_all(entry_root.join("docs")).expect("should create nested docs dir");

    let root_facade = entry_root.join("#mod.bst");
    let styles_file = entry_root.join("styles/docs.bst");
    let nested_page = entry_root.join("docs/#page.bst");

    let root_facade_source = "\
import @styles/docs { theme_head as internal_theme_head }\n\
theme_head #= internal_theme_head\n";
    let styles_source = "theme_head #String = \"head\"\n";
    let nested_page_source = "\
import @styles/docs { theme_head }\n\
page_head #= theme_head\n";

    fs::write(&root_facade, root_facade_source).expect("should write root facade");
    fs::write(&styles_file, styles_source).expect("should write style source");
    fs::write(&nested_page, nested_page_source).expect("should write nested page");

    let project_path_resolver =
        ProjectPathResolver::new(project_root, entry_root, &SourceLibraryRegistry::default())
            .expect("resolver creation should succeed");

    let (headers, mut string_table) = parse_module_headers_with_project_resolver(
        &[
            (root_facade, root_facade_source),
            (styles_file, styles_source),
            (nested_page, nested_page_source),
        ],
        Path::new("docs/#page.bst"),
        project_path_resolver,
    );

    let sorted = resolve_module_dependencies(headers, &mut string_table)
        .expect("module-root facade dependency edge should order the facade before consumers");

    let non_start_paths = sorted
        .headers
        .iter()
        .filter(|header| !matches!(header.kind, HeaderKind::StartFunction))
        .map(|header| header.tokens.src_path.to_portable_string(&string_table))
        .collect::<Vec<_>>();

    assert_eq!(
        non_start_paths,
        vec![
            "styles/docs.bst/theme_head",
            "#mod.bst/theme_head",
            "docs/#page.bst/page_head",
        ],
        "public facade constants must be sorted before external module constants that import them"
    );
}

#[test]
fn reports_ambiguous_suffix_import_resolution() {
    let mut string_table = StringTable::new();
    let external_package_registry = ExternalPackageRegistry::new();
    let options = HeaderParseOptions::default();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let entry_path = PathBuf::from("src/app.bst");

    let mut prepared_outputs = Vec::with_capacity(3);
    let mut const_template_offset = 0usize;
    let mut runtime_fragment_offset = 0usize;

    for (path, source) in &[
        (
            "src/app.bst",
            "import @shared/util/Thing\nTop #Thing = Thing\n",
        ),
        ("src/features/shared/util.bst", "Thing #Int = 1\n"),
        ("src/lib/shared/util.bst", "Thing #Int = 2\n"),
    ] {
        let path_buf = PathBuf::from(path);
        let interned_path = InternedPath::from_path_buf(&path_buf, &mut string_table);
        let file_tokens = tokenize(
            source,
            &interned_path,
            TokenizeMode::Normal,
            &style_directives,
            &mut string_table,
            None,
        )
        .expect("tokenization should succeed");

        let output = prepare_file_from_tokens(
            file_tokens,
            &entry_path,
            &options,
            &external_package_registry,
            &mut string_table,
            const_template_offset,
            runtime_fragment_offset,
        )
        .expect("preparation should succeed");

        const_template_offset += output.const_template_count;
        runtime_fragment_offset += output.runtime_fragment_count;
        prepared_outputs.push(output);
    }

    let bag = match parse_headers(
        prepared_outputs,
        &external_package_registry,
        &ExternalImportResolutionTable::default(),
        options.project_path_resolver.as_ref(),
        &mut string_table,
    ) {
        Ok(_) => panic!("ambiguous suffix import should fail header parsing"),
        Err(bag) => bag,
    };

    assert!(
        bag.diagnostics().iter().any(|diagnostic| {
            let DiagnosticPayload::AmbiguousImportTarget { path } = &diagnostic.payload else {
                return false;
            };

            path.to_portable_string(&string_table)
                .contains("shared/util/Thing")
        }),
        "expected an ambiguous-import diagnostic, got: {bag:?}"
    );
}
