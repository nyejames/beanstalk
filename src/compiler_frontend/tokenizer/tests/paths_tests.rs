use super::*;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::tokenizer::tokenizer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::TokenizeMode;

fn first_path_token_values(source: &str) -> Vec<String> {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);
    let file_tokens = tokenize(
        source,
        &source_path,
        TokenizeMode::Normal,
        &style_directives,
        &mut string_table,
    )
    .expect("tokenization should succeed");

    file_tokens
        .tokens
        .iter()
        .find_map(|token| {
            let TokenKind::Path(paths) = &token.kind else {
                return None;
            };
            Some(
                paths
                    .iter()
                    .map(|path| path.to_portable_string(&string_table))
                    .collect::<Vec<_>>(),
            )
        })
        .expect("expected at least one path token")
}

fn collect_import_path_values(source: &str) -> Vec<String> {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);
    let file_tokens = tokenize(
        source,
        &source_path,
        TokenizeMode::Normal,
        &style_directives,
        &mut string_table,
    )
    .expect("tokenization should succeed");

    collect_import_paths_from_tokens(&file_tokens.tokens, &string_table)
        .expect("import collection should succeed")
        .iter()
        .map(|path| path.to_portable_string(&string_table))
        .collect()
}

#[test]
fn parse_file_path_preserves_final_segment() {
    let paths = first_path_token_values("import @(a/b/c)\n");
    assert_eq!(paths, vec!["a/b/c".to_string()]);
}

#[test]
fn parse_file_path_supports_bare_path_syntax() {
    let paths = first_path_token_values("import @a/b/c\n");
    assert_eq!(paths, vec!["a/b/c".to_string()]);
}

#[test]
fn parse_file_path_grouped_imports_expand_all_symbols() {
    let paths = first_path_token_values("import @(styles/docs/ {footer, navbar})\n");
    assert_eq!(
        paths,
        vec![
            "styles/docs/footer".to_string(),
            "styles/docs/navbar".to_string(),
        ]
    );
}

#[test]
fn parse_file_path_bare_grouped_imports_expand_all_symbols() {
    let paths = first_path_token_values("import @styles/docs/{footer, navbar}\n");
    assert_eq!(
        paths,
        vec![
            "styles/docs/footer".to_string(),
            "styles/docs/navbar".to_string(),
        ]
    );
}

#[test]
fn parse_file_path_bare_grouped_imports_accept_whitespace_before_group() {
    let paths = first_path_token_values("import @styles/docs/   {footer,\n  navbar}\n");
    assert_eq!(
        paths,
        vec![
            "styles/docs/footer".to_string(),
            "styles/docs/navbar".to_string(),
        ]
    );
}

#[test]
fn parse_file_path_stops_before_config_list_comma() {
    let paths = first_path_token_values("#root_folders = { @lib, @assets }\n");
    assert_eq!(paths, vec!["lib".to_string()]);
}

#[test]
fn parse_file_path_stops_before_config_list_closing_brace() {
    let paths = first_path_token_values("#root_folders = { @assets}\n");
    assert_eq!(paths, vec!["assets".to_string()]);
}

#[test]
fn parse_file_path_bare_syntax_stops_at_whitespace_without_group() {
    let paths = first_path_token_values("import @styles/docs/footer trailing_symbol\n");
    assert_eq!(paths, vec!["styles/docs/footer".to_string()]);
}

#[test]
fn parse_file_path_accepts_backslash_separator() {
    let paths = first_path_token_values("import @(styles\\docs\\footer)\n");
    assert_eq!(paths, vec!["styles/docs/footer".to_string()]);
}

#[test]
fn parse_file_path_rejects_malformed_grouped_imports() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let malformed_double_comma = tokenize(
        "import @(styles/docs/{footer,,navbar})\n",
        &source_path,
        TokenizeMode::Normal,
        &style_directives,
        &mut string_table,
    );
    assert!(
        malformed_double_comma.is_err(),
        "double commas in grouped imports should fail"
    );
}

#[test]
fn parse_file_path_rejects_grouped_import_missing_closing_brace() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @(styles/docs/{footer,navbar\n",
        &source_path,
        TokenizeMode::Normal,
        &style_directives,
        &mut string_table,
    );
    assert!(result.is_err(), "missing grouped-import '}}' should fail");
    let error = result.expect_err("expected tokenizer error");
    assert!(error.msg.contains("missing a closing '}'"));
}

#[test]
fn parse_file_path_rejects_grouped_import_missing_closing_parenthesis() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @(styles/docs/{footer,navbar}\n",
        &source_path,
        TokenizeMode::Normal,
        &style_directives,
        &mut string_table,
    );
    assert!(
        result.is_err(),
        "missing closing parenthesis after grouped import should fail"
    );
    let error = result.expect_err("expected tokenizer error");
    assert!(error.msg.contains("closing parenthesis"));
}

#[test]
fn collect_import_paths_from_tokens_supports_newline_after_import() {
    let paths = collect_import_path_values("import\n@(styles/docs/footer)\n");
    assert_eq!(paths, vec!["styles/docs/footer".to_string()]);
}

#[test]
fn collect_import_paths_from_tokens_rejects_missing_path() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);
    let file_tokens = tokenize(
        "import\nfooter\n",
        &source_path,
        TokenizeMode::Normal,
        &style_directives,
        &mut string_table,
    )
    .expect("tokenization should succeed");

    let result = collect_import_paths_from_tokens(&file_tokens.tokens, &string_table);
    assert!(result.is_err(), "import without a path token should fail");
}
