use super::*;
use crate::compiler_frontend::headers::parse_file_headers::{
    HeaderKind, HeaderParseOptions, Headers, parse_headers,
};
use crate::compiler_frontend::host_functions::HostRegistry;
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

    let host_registry = HostRegistry::new();
    let mut warnings = Vec::new();
    let headers = parse_headers(
        tokenized_files,
        &host_registry,
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
        errors
            .iter()
            .any(|error| error.msg.contains("Circular dependency detected")),
        "expected a cycle diagnostic, got: {errors:?}"
    );
}

#[test]
fn constant_initializer_does_not_create_strict_sort_dependency() {
    // WHY: only declared-type annotations create strict graph edges for constants.
    // Initializer-expression symbol references are excluded so that the sort only
    // constrains ordering by structural type dependencies, not by runtime value flow.
    let (headers, mut string_table) = parse_module_headers(
        &[
            // Config's initializer references Value, but Config has no declared type.
            // That reference must NOT create a strict sort edge from Config to Value.
            ("src/a.bst", "import @b/Value\n#Config = Value\n"),
            ("src/b.bst", "#Value Int = 42\n"),
        ],
        "src/a.bst",
    );

    let sorted = resolve_module_dependencies(headers, &mut string_table)
        .expect("sort must succeed — initializer refs do not produce unresolvable strict edges");

    let non_start_names: Vec<_> = sorted
        .headers
        .iter()
        .filter(|h| !matches!(h.kind, HeaderKind::StartFunction))
        .map(|h| header_name(h, &string_table))
        .collect();

    // Both headers must be present in the sorted output.
    assert!(
        non_start_names.contains(&"Config".to_string()),
        "Config header must appear in sorted output"
    );
    assert!(
        non_start_names.contains(&"Value".to_string()),
        "Value header must appear in sorted output"
    );
}

#[test]
fn reports_ambiguous_suffix_import_resolution() {
    let (headers, mut string_table) = parse_module_headers(
        &[
            (
                "src/app.bst",
                "import @shared/util/Thing\n#Top Thing = Thing\n",
            ),
            ("src/features/shared/util.bst", "#Thing Int = 1\n"),
            ("src/lib/shared/util.bst", "#Thing Int = 2\n"),
        ],
        "src/app.bst",
    );

    let errors = resolve_module_dependencies(headers, &mut string_table)
        .expect_err("ambiguous suffix import should fail dependency sorting");

    assert!(
        errors
            .iter()
            .any(|error| error.msg.contains("Missing import target")
                && error.msg.contains("shared/util/Thing")),
        "expected an ambiguous-import diagnostic, got: {errors:?}"
    );
}
