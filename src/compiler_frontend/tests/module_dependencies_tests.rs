use super::*;
use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind, parse_headers};
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::tokenizer::tokenizer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::TokenizeMode;
use std::path::PathBuf;

fn parse_module_headers(files: &[(&str, &str)], entry_path: &str) -> (Vec<Header>, StringTable) {
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
            &style_directives,
            &mut string_table,
        )
        .expect("tokenization should succeed");
        tokenized_files.push(tokens);
    }

    let host_registry = HostRegistry::new(&mut string_table);
    let mut warnings = Vec::new();
    let parsed_headers = parse_headers(
        tokenized_files,
        &host_registry,
        &mut warnings,
        &PathBuf::from(entry_path),
        &mut string_table,
    )
    .expect("header parsing should succeed");

    (parsed_headers.headers, string_table)
}

fn header_name(header: &Header, string_table: &StringTable) -> String {
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
            ("src/a.bst", "import @(b)\nb()\nio(\"a\")\n"),
            ("src/b.bst", "import @(c)\nc()\nio(\"b\")\n"),
            ("src/c.bst", "io(\"c\")\n"),
        ],
        "src/a.bst",
    );

    let sorted = resolve_module_dependencies(headers, &mut string_table)
        .expect("dependency sort should pass");

    let start_order = sorted
        .iter()
        .filter(|header| matches!(header.kind, HeaderKind::StartFunction))
        .map(|header| header.source_file.to_portable_string(&string_table))
        .collect::<Vec<_>>();

    assert_eq!(start_order, vec!["src/c.bst", "src/b.bst", "src/a.bst"]);
}

#[test]
fn applies_soft_struct_and_constant_edges_when_resolvable() {
    let (headers, mut string_table) = parse_module_headers(
        &[(
            "src/constants.bst",
            "User = |\n    name String = base,\n|\n#base = \"Ada\"\n#derived User = User(base)\n",
        )],
        "src/constants.bst",
    );

    let sorted = resolve_module_dependencies(headers, &mut string_table)
        .expect("dependency sort should pass");

    let base_pos = sorted
        .iter()
        .position(|header| header_name(header, &string_table) == "base")
        .expect("base constant should exist");
    let user_pos = sorted
        .iter()
        .position(|header| header_name(header, &string_table) == "User")
        .expect("User struct should exist");
    let derived_pos = sorted
        .iter()
        .position(|header| header_name(header, &string_table) == "derived")
        .expect("derived constant should exist");

    assert!(
        base_pos < user_pos,
        "struct defaults should order required constants first"
    );
    assert!(
        base_pos < derived_pos && user_pos < derived_pos,
        "constant symbol dependencies should sort struct/constant prerequisites before dependent constant"
    );
}

#[test]
fn reports_circular_dependencies() {
    let (headers, mut string_table) = parse_module_headers(
        &[
            ("src/a.bst", "import @(b)\nb()\nio(\"a\")\n"),
            ("src/b.bst", "import @(a)\na()\nio(\"b\")\n"),
        ],
        "src/a.bst",
    );

    let errors = resolve_module_dependencies(headers, &mut string_table)
        .expect_err("cycle should fail dependency sorting");

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
            (
                "src/app.bst",
                "import @(shared/util)\nutil()\nio(\"app\")\n",
            ),
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
