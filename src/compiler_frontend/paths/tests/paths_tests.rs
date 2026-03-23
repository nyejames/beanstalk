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

    collect_paths_from_tokens(&file_tokens.tokens, &string_table)
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
fn parse_file_path_preserves_internal_spaces_for_non_grouped_path() {
    let paths = first_path_token_values("import @docs/my file.md\n");
    assert_eq!(paths, vec!["docs/my file.md".to_string()]);
}

#[test]
fn parse_file_path_grouped_paths_expand_leaf_entries() {
    let paths = first_path_token_values("import @styles/docs {footer, navbar}\n");
    assert_eq!(
        paths,
        vec![
            "styles/docs/footer".to_string(),
            "styles/docs/navbar".to_string(),
        ]
    );
}

#[test]
fn parse_file_path_wrapped_grouped_paths_expand_leaf_entries() {
    let paths = first_path_token_values("import @(styles/docs {footer, navbar})\n");
    assert_eq!(
        paths,
        vec![
            "styles/docs/footer".to_string(),
            "styles/docs/navbar".to_string(),
        ]
    );
}

#[test]
fn parse_file_path_grouped_leaf_paths_support_nested_directories() {
    let paths = first_path_token_values("import @docs {thing.md, subfolder/another_thing.md}\n");
    assert_eq!(
        paths,
        vec![
            "docs/thing.md".to_string(),
            "docs/subfolder/another_thing.md".to_string(),
        ]
    );
}

#[test]
fn parse_file_path_grouped_paths_expand_nested_shared_prefixes() {
    let paths = first_path_token_values(
        "import @docs { subfolder { another_thing.md, and_another.md } }\n",
    );
    assert_eq!(
        paths,
        vec![
            "docs/subfolder/another_thing.md".to_string(),
            "docs/subfolder/and_another.md".to_string(),
        ]
    );
}

#[test]
fn parse_file_path_grouped_paths_expand_deeper_nested_prefixes() {
    let paths = first_path_token_values(
        "import @docs { subfolder/another_folder { another_subfolder/thing.md, second.md } }\n",
    );
    assert_eq!(
        paths,
        vec![
            "docs/subfolder/another_folder/another_subfolder/thing.md".to_string(),
            "docs/subfolder/another_folder/second.md".to_string(),
        ]
    );
}

#[test]
fn parse_file_path_grouped_paths_support_mixed_leaf_and_branch_entries() {
    let paths = first_path_token_values(
        "import @docs { intro.md, subfolder { a.md, b.md }, subfolder/another_folder/c.md }\n",
    );
    assert_eq!(
        paths,
        vec![
            "docs/intro.md".to_string(),
            "docs/subfolder/a.md".to_string(),
            "docs/subfolder/b.md".to_string(),
            "docs/subfolder/another_folder/c.md".to_string(),
        ]
    );
}

#[test]
fn parse_file_path_grouped_paths_accept_whitespace_and_trailing_commas() {
    let paths =
        first_path_token_values("import @docs { thing.md , subfolder { a.md , b.md , } , }\n");
    assert_eq!(
        paths,
        vec![
            "docs/thing.md".to_string(),
            "docs/subfolder/a.md".to_string(),
            "docs/subfolder/b.md".to_string(),
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
fn parse_file_path_accepts_backslash_separator() {
    let paths = first_path_token_values("import @(styles\\docs\\footer)\n");
    assert_eq!(paths, vec!["styles/docs/footer".to_string()]);
}

#[test]
fn parse_file_path_accepts_hash_prefixed_file_names() {
    let paths = first_path_token_values("import @docs { #config.bst, subfolder/#page.bst }\n");
    assert_eq!(
        paths,
        vec![
            "docs/#config.bst".to_string(),
            "docs/subfolder/#page.bst".to_string(),
        ]
    );
}

#[test]
fn parse_file_path_accepts_dotted_and_dashed_file_names() {
    let paths = first_path_token_values("import @docs { thing.v1.md, my-file-name.md }\n");
    assert_eq!(
        paths,
        vec![
            "docs/thing.v1.md".to_string(),
            "docs/my-file-name.md".to_string(),
        ]
    );
}

#[test]
fn parse_file_path_rejects_grouped_path_with_empty_block() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @docs {}\n",
        &source_path,
        TokenizeMode::Normal,
        &style_directives,
        &mut string_table,
    );

    assert!(result.is_err(), "empty grouped path should fail");
    let error = result.expect_err("expected tokenizer error");
    assert!(error.msg.contains("requires at least one entry"));
}

#[test]
fn parse_file_path_rejects_grouped_path_with_multiple_commas() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @docs { a.md,, b.md }\n",
        &source_path,
        TokenizeMode::Normal,
        &style_directives,
        &mut string_table,
    );

    assert!(result.is_err(), "double commas should fail");
}

#[test]
fn parse_file_path_rejects_grouped_path_missing_closing_brace() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @docs { a.md, b.md\n",
        &source_path,
        TokenizeMode::Normal,
        &style_directives,
        &mut string_table,
    );

    assert!(result.is_err(), "missing grouped closing brace should fail");
    let error = result.expect_err("expected tokenizer error");
    assert!(error.msg.contains("missing a closing '}'"));
}

#[test]
fn parse_file_path_rejects_grouped_path_missing_closing_parenthesis() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @(docs { a.md, b.md }\n",
        &source_path,
        TokenizeMode::Normal,
        &style_directives,
        &mut string_table,
    );

    assert!(result.is_err(), "missing closing parenthesis should fail");
    let error = result.expect_err("expected tokenizer error");
    assert!(error.msg.contains("closing parenthesis"));
}

#[test]
fn parse_file_path_rejects_slash_before_group_top_level() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @docs/{a.md, b.md}\n",
        &source_path,
        TokenizeMode::Normal,
        &style_directives,
        &mut string_table,
    );

    assert!(result.is_err(), "legacy slash-before-group should fail");
    let error = result.expect_err("expected tokenizer error");
    assert!(
        error
            .msg
            .contains("Slash-before-group syntax is not supported")
    );
}

#[test]
fn parse_file_path_rejects_slash_before_group_with_whitespace() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @docs/   {a.md}\n",
        &source_path,
        TokenizeMode::Normal,
        &style_directives,
        &mut string_table,
    );

    assert!(
        result.is_err(),
        "slash-before-group with whitespace should fail"
    );
}

#[test]
fn parse_file_path_rejects_nested_slash_before_group() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @docs { subfolder/ { a.md } }\n",
        &source_path,
        TokenizeMode::Normal,
        &style_directives,
        &mut string_table,
    );

    assert!(result.is_err(), "nested slash-before-group should fail");
}

#[test]
fn parse_file_path_rejects_grouped_prefix_with_trailing_separator() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @docs { subfolder/ }\n",
        &source_path,
        TokenizeMode::Normal,
        &style_directives,
        &mut string_table,
    );

    assert!(result.is_err(), "trailing separator prefix should fail");
    let error = result.expect_err("expected tokenizer error");
    assert!(error.msg.contains("cannot end with a separator"));
}

#[test]
fn parse_file_path_rejects_empty_path_component_in_grouped_entry() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @docs { subfolder//a.md }\n",
        &source_path,
        TokenizeMode::Normal,
        &style_directives,
        &mut string_table,
    );

    assert!(result.is_err(), "empty grouped component should fail");
}

#[test]
fn parse_file_path_rejects_nested_group_with_empty_prefix() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @docs { { a.md } }\n",
        &source_path,
        TokenizeMode::Normal,
        &style_directives,
        &mut string_table,
    );

    assert!(result.is_err(), "nested group without prefix should fail");
    let error = result.expect_err("expected tokenizer error");
    assert!(error.msg.contains("require a non-empty prefix"));
}

#[test]
fn parse_file_path_rejects_reserved_device_name_component() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @docs/CON\n",
        &source_path,
        TokenizeMode::Normal,
        &style_directives,
        &mut string_table,
    );

    assert!(result.is_err(), "reserved device name should fail");
    let error = result.expect_err("expected tokenizer error");
    assert!(error.msg.contains("Invalid path component"));
}

#[test]
fn parse_file_path_rejects_non_leading_dot_segments() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @(docs/../content)\n",
        &source_path,
        TokenizeMode::Normal,
        &style_directives,
        &mut string_table,
    );

    assert!(result.is_err(), "non-leading '..' should fail");
}

#[test]
fn parse_file_path_accepts_leading_relative_dot_segments() {
    let paths = first_path_token_values("import @(../shared/content)\n");
    assert_eq!(paths, vec!["../shared/content".to_string()]);
}

#[test]
fn parse_file_path_in_template_head_supports_grouped_expansion() {
    let paths = first_path_token_values("[@subdir {#test_file.txt, end_of_path.txt}]");
    assert_eq!(
        paths,
        vec![
            "subdir/#test_file.txt".to_string(),
            "subdir/end_of_path.txt".to_string(),
        ]
    );
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

    let result = collect_paths_from_tokens(&file_tokens.tokens, &string_table);
    assert!(result.is_err(), "import without a path token should fail");
}
