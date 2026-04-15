use super::*;
use crate::compiler_frontend::headers::parse_file_headers::{Headers, parse_headers};
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
    // Non-entry files use named functions only; entry a.bst can have top-level code.
    // Function names match import path names so the dependency edges are created correctly.
    let (headers, mut string_table) = parse_module_headers(
        &[
            ("src/a.bst", "import @b\nb()\n"),
            ("src/b.bst", "import @c\n#b ||:\n    c()\n;\n"),
            ("src/c.bst", "#c ||:\n    io(\"c\")\n;\n"),
        ],
        "src/a.bst",
    );

    let sorted = resolve_module_dependencies(headers, &mut string_table)
        .expect("dependency sort should pass");

    // Verify dependency ordering via source_file positions: c must precede b, b must precede a.
    let file_positions: Vec<String> = sorted
        .headers
        .iter()
        .map(|header| header.source_file.to_portable_string(&string_table))
        .collect();

    let pos_c = file_positions
        .iter()
        .position(|f| f.contains("c.bst"))
        .expect("c.bst should have at least one header");
    let pos_b = file_positions
        .iter()
        .position(|f| f.contains("b.bst"))
        .expect("b.bst should have at least one header");
    let pos_a = file_positions
        .iter()
        .position(|f| f.contains("a.bst"))
        .expect("a.bst should have at least one header");

    assert!(
        pos_c < pos_b,
        "c.bst (dependency) must sort before b.bst (dependent), got order: {file_positions:?}"
    );
    assert!(
        pos_b < pos_a,
        "b.bst (dependency) must sort before a.bst (entry), got order: {file_positions:?}"
    );
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
        .headers
        .iter()
        .position(|header| header_name(header, &string_table) == "base")
        .expect("base constant should exist");
    let user_pos = sorted
        .headers
        .iter()
        .position(|header| header_name(header, &string_table) == "User")
        .expect("User struct should exist");
    let derived_pos = sorted
        .headers
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
    // Function names match import names to wire dependency edges.
    // b.bst needs a declaration so it appears in the graph and the cycle is detectable.
    let (headers, mut string_table) = parse_module_headers(
        &[
            ("src/a.bst", "import @b\nb()\n"),
            ("src/b.bst", "import @a\n#b ||:\n    a()\n;\n"),
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
    // Each util.bst has a declaration so it appears in the graph; ambiguity is detectable.
    let (headers, mut string_table) = parse_module_headers(
        &[
            ("src/app.bst", "import @shared/util\nutil()\n"),
            (
                "src/features/shared/util.bst",
                "#util ||:\n    io(\"feature util\")\n;\n",
            ),
            (
                "src/lib/shared/util.bst",
                "#util ||:\n    io(\"lib util\")\n;\n",
            ),
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
