use super::*;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::tokenizer::tokenizer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::TokenizeMode;

fn first_path_token_values(source: &str) -> Vec<String> {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);
    let file_tokens = tokenize(
        source,
        &source_path,
        TokenizeMode::Normal,
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
                    .map(|path| path.to_string(&string_table))
                    .collect::<Vec<_>>(),
            )
        })
        .expect("expected at least one path token")
}

#[test]
fn parse_file_path_preserves_final_segment() {
    let paths = first_path_token_values("import @(a/b/c)\n");
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
fn parse_file_path_rejects_malformed_grouped_imports() {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let malformed_double_comma = tokenize(
        "import @(styles/docs/{footer,,navbar})\n",
        &source_path,
        TokenizeMode::Normal,
        &mut string_table,
    );
    assert!(
        malformed_double_comma.is_err(),
        "double commas in grouped imports should fail"
    );
}
