use super::*;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::newline_handling::NewlineMode;
use crate::compiler_frontend::tokenizer::tokens::{PathTokenItem, TokenizeMode};

fn first_path_token_values(source: &str) -> Vec<String> {
    let (items, string_table) = first_path_token(source);

    items
        .iter()
        .map(|item| item.path.to_portable_string(&string_table))
        .collect()
}

fn first_path_token(source: &str) -> (Vec<PathTokenItem>, StringTable) {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);
    let file_tokens = tokenize(
        source,
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    )
    .expect("tokenization should succeed");

    let items = file_tokens
        .tokens
        .iter()
        .find_map(|token| {
            let TokenKind::Path(items) = &token.kind else {
                return None;
            };
            Some(items.to_owned())
        })
        .expect("expected at least one path token");

    (items, string_table)
}

fn collect_import_path_values(source: &str) -> Vec<String> {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);
    let file_tokens = tokenize(
        source,
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    )
    .expect("tokenization should succeed");

    collect_paths_from_tokens(&file_tokens.tokens)
        .expect("import collection should succeed")
        .iter()
        .map(|path| path.to_portable_string(&string_table))
        .collect()
}

#[test]
fn parse_file_path_preserves_final_segment() {
    let paths = first_path_token_values("import @a/b/c\n");
    assert_eq!(paths, vec!["a/b/c".to_string()]);
}

#[test]
fn parse_file_path_accepts_exact_public_root_literal() {
    let (paths, string_table) = first_path_token("import @/\n");
    assert_eq!(paths.len(), 1);
    assert!(paths[0].path.as_components().is_empty());
    assert_eq!(paths[0].path.to_portable_string(&string_table), "");
}

#[test]
fn parse_file_path_rejects_bare_at_symbol() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @\n",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    );

    assert!(result.is_err(), "bare '@' should fail");
    let error = result.expect_err("expected tokenizer error");
    assert!(error.msg.contains("Path cannot be empty"));
}

#[test]
fn parse_file_path_rejects_public_root_with_suffix() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @/foo\n",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    );

    assert!(result.is_err(), "'@/foo' should fail");
    let error = result.expect_err("expected tokenizer error");
    assert!(error.msg.contains("Only exact \"@/\" is supported"));
}

#[test]
fn parse_file_path_rejects_public_root_grouped_expansion() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @/{a,b}\n",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    );

    assert!(result.is_err(), "'@/{{a,b}}' should fail");
    let error = result.expect_err("expected tokenizer error");
    assert!(error.msg.contains("Only exact \"@/\" is supported"));
}

#[test]
fn parse_file_path_rejects_public_root_with_double_slash() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @//\n",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    );

    assert!(result.is_err(), "'@//' should fail");
    let error = result.expect_err("expected tokenizer error");
    assert!(error.msg.contains("Only exact \"@/\" is supported"));
}

#[test]
fn parse_file_path_rejects_public_root_backslash_variant() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @\\\n",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    );

    assert!(result.is_err(), "'@\\' should fail");
}

#[test]
fn parse_file_path_rejects_parenthesis_wrapper_syntax() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @(a/b/c)\n",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    );

    assert!(result.is_err(), "parenthesis wrapper should fail");
}

#[test]
fn parse_file_path_rejects_unquoted_whitespace_for_non_grouped_path() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @docs/my file.md\n",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    );

    assert!(result.is_err(), "unquoted whitespace should fail");
    let error = result.expect_err("expected tokenizer error");
    assert!(error.msg.contains("must be quoted"));
}

#[test]
fn parse_file_path_accepts_quoted_non_grouped_component() {
    let paths = first_path_token_values("import @docs/\"my file.md\"\n");
    assert_eq!(paths, vec!["docs/my file.md".to_string()]);
}

#[test]
fn parse_file_path_accepts_quoted_root_component() {
    let paths = first_path_token_values("import @\"root folder\"/docs/file.md\n");
    assert_eq!(paths, vec!["root folder/docs/file.md".to_string()]);
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
fn parse_file_path_grouped_paths_accept_quoted_leaf_entry() {
    let paths = first_path_token_values("import @docs { \"my file.md\", intro.md }\n");
    assert_eq!(
        paths,
        vec!["docs/my file.md".to_string(), "docs/intro.md".to_string()]
    );
}

#[test]
fn parse_file_path_grouped_paths_accept_quoted_nested_prefix_entry() {
    let paths = first_path_token_values("import @docs { \"my folder\" { a.md, b.md } }\n");
    assert_eq!(
        paths,
        vec![
            "docs/my folder/a.md".to_string(),
            "docs/my folder/b.md".to_string(),
        ]
    );
}

#[test]
fn parse_file_path_grouped_paths_accept_mixed_quoted_and_unquoted_components() {
    let paths = first_path_token_values(
        "import @docs { \"my folder\"/\"another folder\"/c.md, intro.md }\n",
    );
    assert_eq!(
        paths,
        vec![
            "docs/my folder/another folder/c.md".to_string(),
            "docs/intro.md".to_string(),
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
    let paths = first_path_token_values("import @styles\\docs\\footer\n");
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
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
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
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
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
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    );

    assert!(result.is_err(), "missing grouped closing brace should fail");
    let error = result.expect_err("expected tokenizer error");
    assert!(error.msg.contains("missing a closing '}'"));
}

#[test]
fn parse_file_path_rejects_unterminated_quoted_component_non_grouped() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @docs/\"my file.md\n",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    );

    assert!(result.is_err(), "unterminated quote should fail");
    let error = result.expect_err("expected tokenizer error");
    assert!(error.msg.contains("Unclosed quoted path component"));
}

#[test]
fn parse_file_path_rejects_unterminated_quoted_component_grouped() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @docs { \"my file.md }\n",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    );

    assert!(result.is_err(), "unterminated grouped quote should fail");
    let error = result.expect_err("expected tokenizer error");
    assert!(error.msg.contains("Unclosed quoted path component"));
}

#[test]
fn parse_file_path_rejects_unknown_escape_in_quoted_component() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @docs/\"my\\nfile.md\"\n",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    );

    assert!(result.is_err(), "unknown escape should fail");
    let error = result.expect_err("expected tokenizer error");
    assert!(
        error
            .msg
            .contains("Invalid escape in quoted path component")
    );
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
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
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
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
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
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
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
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
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
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
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
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
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
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
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
        "import @docs/../content\n",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    );

    assert!(result.is_err(), "non-leading '..' should fail");
}

#[test]
fn parse_file_path_accepts_leading_relative_dot_segments() {
    let paths = first_path_token_values("import @../shared/content\n");
    assert_eq!(paths, vec!["../shared/content".to_string()]);
}

#[test]
fn parse_file_path_rejects_unquoted_whitespace_for_grouped_entry() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @docs { my file.md }\n",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    );

    assert!(result.is_err(), "unquoted grouped whitespace should fail");
    let error = result.expect_err("expected tokenizer error");
    assert!(error.msg.contains("must be quoted"));
}

#[test]
fn parse_file_path_rejects_unquoted_whitespace_for_grouped_nested_prefix() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @docs { my folder { a.md } }\n",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    );

    assert!(
        result.is_err(),
        "unquoted nested prefix whitespace should fail"
    );
    let error = result.expect_err("expected tokenizer error");
    assert!(error.msg.contains("must be quoted"));
}

#[test]
fn parse_file_path_rejects_quoted_component_with_structural_separator_character() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @docs/\"a/b.md\"\n",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    );

    assert!(
        result.is_err(),
        "quoted separator should still be invalid content"
    );
    let error = result.expect_err("expected tokenizer error");
    assert!(error.msg.contains("Invalid path component"));
}

#[test]
fn parse_file_path_rejects_quoted_component_with_grouped_delimiter_character() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @docs { \"a,b.md\" }\n",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    );

    assert!(
        result.is_err(),
        "quoted grouped delimiter should still be invalid content"
    );
    let error = result.expect_err("expected tokenizer error");
    assert!(error.msg.contains("Invalid path component"));
}

#[test]
fn parse_file_path_rejects_missing_comma_between_grouped_entries() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @docs { subfolder { a.md } other { b.md } }\n",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    );

    assert!(
        result.is_err(),
        "missing comma between siblings should fail"
    );
    let error = result.expect_err("expected tokenizer error");
    assert!(error.msg.contains("separated by commas"));
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
    let paths = collect_import_path_values("import\n@styles/docs/footer\n");
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
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    )
    .expect("tokenization should succeed");

    let result = collect_paths_from_tokens(&file_tokens.tokens);
    assert!(result.is_err(), "import without a path token should fail");
}

#[test]
fn parse_file_path_stops_at_as_keyword() {
    let values = first_path_token_values("@core/io/io as print");
    // Note: the returned path omits the leading `@` because the tokenizer
    // consumes `@` before calling `parse_file_path`.
    assert_eq!(values, vec!["core/io/io"]);
}

#[test]
fn parse_import_clause_items_reads_alias() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);
    let file_tokens = tokenize(
        "import @core/io/io as print",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    )
    .expect("tokenization should succeed");

    let import_index = file_tokens
        .tokens
        .iter()
        .position(|token| matches!(token.kind, TokenKind::Import))
        .expect("expected an Import token");

    let (items, _next_index) =
        parse_import_clause_items(&file_tokens.tokens, import_index, &mut string_table)
            .expect("import clause parsing should succeed");

    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].path.to_portable_string(&string_table),
        "core/io/io"
    );
    assert_eq!(string_table.resolve(items[0].alias.unwrap()), "print");
}

// ---- Grouped import alias tests (Phase 3) ----

#[test]
fn parse_grouped_import_entries_with_aliases() {
    let (items, string_table) =
        first_path_token("import @components { render as render_component, Button as UiButton }");
    assert_eq!(items.len(), 2);

    assert_eq!(
        items[0].path.to_portable_string(&string_table),
        "components/render"
    );
    assert_eq!(
        string_table.resolve(items[0].alias.unwrap()),
        "render_component"
    );

    assert_eq!(
        items[1].path.to_portable_string(&string_table),
        "components/Button"
    );
    assert_eq!(string_table.resolve(items[1].alias.unwrap()), "UiButton");
}

#[test]
fn parse_nested_grouped_import_entries_with_aliases() {
    let (items, string_table) = first_path_token(
        "import @docs { pages { home/render as render_home, about/render as render_about } }",
    );
    assert_eq!(items.len(), 2);

    assert_eq!(
        items[0].path.to_portable_string(&string_table),
        "docs/pages/home/render"
    );
    assert_eq!(string_table.resolve(items[0].alias.unwrap()), "render_home");

    assert_eq!(
        items[1].path.to_portable_string(&string_table),
        "docs/pages/about/render"
    );
    assert_eq!(
        string_table.resolve(items[1].alias.unwrap()),
        "render_about"
    );
}

#[test]
fn reject_group_level_alias() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let file_tokens = tokenize(
        "import @components { render, Button } as ui",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    )
    .expect("tokenization should succeed");

    let import_index = file_tokens
        .tokens
        .iter()
        .position(|token| matches!(token.kind, TokenKind::Import))
        .expect("expected an Import token");

    let result = parse_import_clause_items(&file_tokens.tokens, import_index, &mut string_table);
    assert!(result.is_err(), "group-level alias should be rejected");
    let error = result.expect_err("expected import clause error");
    assert!(error.msg.contains("group-level alias"));
}

#[test]
fn reject_double_alias() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let file_tokens = tokenize(
        "import @x { foo as bar } as baz",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    )
    .expect("tokenization should succeed");

    let import_index = file_tokens
        .tokens
        .iter()
        .position(|token| matches!(token.kind, TokenKind::Import))
        .expect("expected an Import token");

    let result = parse_import_clause_items(&file_tokens.tokens, import_index, &mut string_table);
    assert!(result.is_err(), "double alias should be rejected");
    let error = result.expect_err("expected import clause error");
    assert!(
        error
            .msg
            .contains("Grouped imports cannot use a group-level alias")
    );
}

#[test]
fn reject_alias_on_non_leaf_group_prefix() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "import @docs { pages as p { home/render } }",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    );

    assert!(
        result.is_err(),
        "alias on non-leaf prefix should be rejected"
    );
    let error = result.expect_err("expected tokenizer error");
    assert!(error.msg.contains("only valid on leaf entries"));
}

#[test]
fn parse_grouped_import_mixed_aliased_and_plain() {
    let (items, string_table) =
        first_path_token("import @core/math { PI as pi, sin, cos as cosine }");
    assert_eq!(items.len(), 3);

    assert_eq!(
        items[0].path.to_portable_string(&string_table),
        "core/math/PI"
    );
    assert_eq!(string_table.resolve(items[0].alias.unwrap()), "pi");

    assert_eq!(
        items[1].path.to_portable_string(&string_table),
        "core/math/sin"
    );
    assert!(items[1].alias.is_none());

    assert_eq!(
        items[2].path.to_portable_string(&string_table),
        "core/math/cos"
    );
    assert_eq!(string_table.resolve(items[2].alias.unwrap()), "cosine");
}
