use super::*;
use crate::compiler_frontend::headers::parse_file_headers::{HeaderKind, Headers, parse_headers};
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
fn sorts_strict_import_dependencies_before_dependents() {
    let (headers, mut string_table) = parse_module_headers(
        &[
            ("src/a.bst", "import @b\nb()\nio(\"a\")\n"),
            ("src/b.bst", "import @c\nc()\nio(\"b\")\n"),
            ("src/c.bst", "io(\"c\")\n"),
        ],
        "src/a.bst",
    );

    let sorted = resolve_module_dependencies(headers, &mut string_table)
        .expect("dependency sort should pass");

    let start_order = sorted
        .headers
        .iter()
        .filter(|header| matches!(header.kind, HeaderKind::StartFunction))
        .map(|header| header.source_file.to_portable_string(&string_table))
        .collect::<Vec<_>>();

    assert_eq!(start_order, vec!["src/c.bst", "src/b.bst", "src/a.bst"]);
}


#[test]
fn reports_circular_dependencies() {
    let (headers, mut string_table) = parse_module_headers(
        &[
            ("src/a.bst", "import @b\nb()\nio(\"a\")\n"),
            ("src/b.bst", "import @a\na()\nio(\"b\")\n"),
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
fn reports_ambiguous_suffix_import_resolution() {
    let (headers, mut string_table) = parse_module_headers(
        &[
            ("src/app.bst", "import @shared/util\nutil()\nio(\"app\")\n"),
            ("src/features/shared/util.bst", "io(\"feature util\")\n"),
            ("src/lib/shared/util.bst", "io(\"lib util\")\n"),
        ],
        "src/app.bst",
    );

    let errors = resolve_module_dependencies(headers, &mut string_table)
        .expect_err("ambiguous suffix import should fail dependency sorting");

    assert!(
        errors
            .iter()
            .any(|error| error.msg.contains("Missing import target")
                && error.msg.contains("shared/util")),
        "expected an ambiguous-import diagnostic, got: {errors:?}"
    );
}
