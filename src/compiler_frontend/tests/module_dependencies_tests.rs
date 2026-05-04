//! Dependency sorting regression tests.
//!
//! WHAT: validates topological ordering, cycle detection, deterministic order, and start-function
//!       exclusion from the import dependency graph.
//! WHY: dependency sorting is the single producer of sorted declaration placeholders; any drift
//!      here breaks cross-file visibility and AST constant dependency ordering.

use super::*;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::headers::parse_file_headers::{
    HeaderKind, HeaderParseOptions, Headers, parse_headers,
};
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::newline_handling::NewlineMode;
use crate::compiler_frontend::tokenizer::tokens::TokenizeMode;
use std::path::PathBuf;

fn parse_module_headers(files: &[(&str, &str)], entry_path: &str) -> (Headers, StringTable) {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();

    let mut tokenized_files = Vec::with_capacity(files.len());
    for (path, source) in files {
        let path_buf = PathBuf::from(path);
        let interned_path = InternedPath::from_path_buf(&path_buf, &mut string_table);
        let tokens = tokenize(
            source,
            &interned_path,
            TokenizeMode::Normal,
            NewlineMode::NormalizeToLf,
            &style_directives,
            &mut string_table,
            None,
        )
        .expect("tokenization should succeed");
        tokenized_files.push(tokens);
    }

    let external_package_registry = ExternalPackageRegistry::new();
    let mut warnings = Vec::new();
    let headers = parse_headers(
        tokenized_files,
        &external_package_registry,
        &mut warnings,
        &PathBuf::from(entry_path),
        HeaderParseOptions::default(),
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
            ("src/a.bst", "import @b/Middle\n#Top Middle = Middle\n"),
            ("src/b.bst", "import @c/Thing\n#Middle Thing = Thing\n"),
            ("src/c.bst", "#Thing Int = 1\n"),
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
            ("src/a.bst", "import @b/Middle\n#Top Middle = Middle\n"),
            ("src/b.bst", "import @a/Top\n#Middle Top = Top\n"),
        ],
        "src/a.bst",
    );

    let errors = resolve_module_dependencies(headers, &mut string_table)
        .expect_err("cycle should fail dependency sorting");

    // resolve_module_dependencies now returns Vec<CompilerError>
    assert!(
        errors.iter().any(|error| error
            .msg
            .contains("Circular declaration dependency detected")),
        "expected a cycle diagnostic, got: {errors:?}"
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
            ("src/a.bst", "import @b/Value\n#Config = Value\n"),
            ("src/b.bst", "#Value Int = 42\n"),
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
            "#first || -> Int:\n    return second()\n;\n\n#second || -> Int:\n    return 1\n;\n",
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
fn reports_ambiguous_suffix_import_resolution() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();

    let mut tokenized_files = Vec::with_capacity(3);
    for (path, source) in &[
        (
            "src/app.bst",
            "import @shared/util/Thing\n#Top Thing = Thing\n",
        ),
        ("src/features/shared/util.bst", "#Thing Int = 1\n"),
        ("src/lib/shared/util.bst", "#Thing Int = 2\n"),
    ] {
        let path_buf = PathBuf::from(path);
        let interned_path = InternedPath::from_path_buf(&path_buf, &mut string_table);
        let tokens = tokenize(
            source,
            &interned_path,
            TokenizeMode::Normal,
            NewlineMode::NormalizeToLf,
            &style_directives,
            &mut string_table,
            None,
        )
        .expect("tokenization should succeed");
        tokenized_files.push(tokens);
    }

    let external_package_registry = ExternalPackageRegistry::new();
    let mut warnings = Vec::new();
    let errors = match parse_headers(
        tokenized_files,
        &external_package_registry,
        &mut warnings,
        &PathBuf::from("src/app.bst"),
        HeaderParseOptions::default(),
        &mut string_table,
    ) {
        Ok(_) => panic!("ambiguous suffix import should fail header parsing"),
        Err(e) => e,
    };

    assert!(
        errors
            .iter()
            .any(|error| error.msg.contains("Ambiguous import target")
                && error.msg.contains("shared/util/Thing")),
        "expected an ambiguous-import diagnostic, got: {errors:?}"
    );
}
