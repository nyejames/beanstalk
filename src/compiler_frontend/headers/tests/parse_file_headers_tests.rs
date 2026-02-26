use super::*;
use crate::backends::function_registry::HostRegistry;
use crate::compiler_frontend::tokenizer::tokenizer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::TokenizeMode;
use std::path::PathBuf;

fn parse_single_file_headers(source: &str) -> Headers {
    let mut string_table = StringTable::new();
    let file_path = PathBuf::from("src/#page.bst");
    let interned_path = InternedPath::from_path_buf(&file_path, &mut string_table);
    let file_tokens = tokenize(
        source,
        &interned_path,
        TokenizeMode::Normal,
        &mut string_table,
    )
    .expect("tokenization should succeed");

    let host_registry = HostRegistry::new(&mut string_table);
    let mut warnings = Vec::new();

    parse_headers(
        vec![file_tokens],
        &host_registry,
        &mut warnings,
        &file_path,
        &mut string_table,
    )
    .expect("headers should parse")
}

#[test]
fn import_paths_are_captured_for_start_function_dependencies() {
    let headers = parse_single_file_headers("import @(libs/html/basic)\n[basic]\n");
    let start_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::StartFunction))
        .expect("expected start function header");

    assert!(
        !start_header.dependencies.is_empty(),
        "start function should depend on imported symbol path"
    );
}

#[test]
fn exported_constant_headers_are_parsed() {
    let headers = parse_single_file_headers("#theme = \"dark\"\n");
    assert!(
        headers
            .headers
            .iter()
            .any(|header| matches!(header.kind, HeaderKind::Constant)),
        "expected exported constant header"
    );
}

#[test]
fn exported_constant_dependency_tracks_imported_symbol() {
    let headers = parse_single_file_headers("import @(styles/docs/navbar)\n#theme = navbar\n");
    let constant_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::Constant))
        .expect("expected constant header");

    assert_eq!(constant_header.dependencies.len(), 1);
}
