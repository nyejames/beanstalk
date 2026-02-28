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

fn parse_single_file_headers_with_entry(
    source: &str,
    file_path: &str,
    entry_file_path: &str,
) -> Result<Headers, Vec<crate::compiler_frontend::compiler_errors::CompilerError>> {
    let mut string_table = StringTable::new();
    let file_path = PathBuf::from(file_path);
    let entry_file_path = PathBuf::from(entry_file_path);
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
        &entry_file_path,
        &mut string_table,
    )
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
            .any(|header| matches!(header.kind, HeaderKind::Constant { .. })),
        "expected exported constant header"
    );
}

#[test]
fn exported_constant_dependency_tracks_imported_symbol() {
    let headers = parse_single_file_headers("import @(styles/docs/navbar)\n#theme = navbar\n");
    let constant_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::Constant { .. }))
        .expect("expected constant header");

    assert_eq!(constant_header.dependencies.len(), 1);
}

#[test]
fn top_level_const_template_outside_entry_file_errors() {
    let result = parse_single_file_headers_with_entry(
        "#[html.head: [\"x\"]]\n",
        "src/lib.bst",
        "src/#page.bst",
    );

    assert!(
        result.is_err(),
        "const templates outside the entry file should error"
    );
}

#[test]
fn top_level_const_template_tokens_keep_close_and_eof_for_ast_parser() {
    let headers = parse_single_file_headers("#[3]\n");

    let const_template_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::ConstTemplate { .. }))
        .expect("expected top-level const template header");

    assert!(
        matches!(
            const_template_header
                .tokens
                .tokens
                .first()
                .map(|token| &token.kind),
            Some(TokenKind::TemplateHead)
        ),
        "const template token stream should start with template opener"
    );

    assert!(
        const_template_header
            .tokens
            .tokens
            .iter()
            .any(|token| matches!(token.kind, TokenKind::TemplateClose)),
        "const template token stream should preserve template close token"
    );

    assert!(
        matches!(
            const_template_header
                .tokens
                .tokens
                .last()
                .map(|token| &token.kind),
            Some(TokenKind::Eof)
        ),
        "const template token stream should end with EOF sentinel"
    );
}

#[test]
fn start_function_local_references_do_not_create_module_dependencies() {
    let headers = parse_single_file_headers(
        "value = 1\n\
         another = value + 1\n\
         io(another)\n",
    );

    let start_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::StartFunction))
        .expect("expected start function header");

    assert!(
        start_header.dependencies.is_empty(),
        "local start-function symbols must not be tracked as inter-header/module dependencies"
    );
}
