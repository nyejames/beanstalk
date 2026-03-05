use super::*;
use crate::compiler_frontend::interned_path::InternedPath;

fn tokenize_source(source: &str) -> (FileTokens, StringTable) {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);
    let file_tokens = tokenize(
        source,
        &source_path,
        TokenizeMode::Normal,
        &mut string_table,
    )
    .expect("tokenization should succeed");
    (file_tokens, string_table)
}

fn find_token_index(tokens: &[Token], predicate: impl Fn(&TokenKind) -> bool) -> usize {
    tokens
        .iter()
        .position(|token| predicate(&token.kind))
        .expect("expected token to be present")
}

#[test]
fn tokenizes_style_directives_inside_template_heads() {
    let (file_tokens, string_table) = tokenize_source("[$markdown, $ignore: body]");

    let outer_head = find_token_index(&file_tokens.tokens, |kind| {
        matches!(kind, TokenKind::TemplateHead)
    });
    let markdown = find_token_index(
        &file_tokens.tokens,
        |kind| matches!(kind, TokenKind::StyleDirective(id) if string_table.resolve(*id) == "markdown"),
    );
    let ignore = find_token_index(
        &file_tokens.tokens,
        |kind| matches!(kind, TokenKind::StyleDirective(id) if string_table.resolve(*id) == "ignore"),
    );

    assert!(outer_head < markdown);
    assert!(markdown < ignore);
    assert!(matches!(
        file_tokens.tokens[markdown].kind,
        TokenKind::StyleDirective(..)
    ));
    assert!(matches!(
        file_tokens.tokens[ignore].kind,
        TokenKind::StyleDirective(..)
    ));
}

#[test]
fn tokenizes_style_child_templates_without_leaving_the_outer_template_head() {
    let (file_tokens, string_table) = tokenize_source("[$[:prefix], $markdown:\nhello\n]");

    let outer_head = find_token_index(&file_tokens.tokens, |kind| {
        matches!(kind, TokenKind::TemplateHead)
    });
    let style_child = find_token_index(&file_tokens.tokens, |kind| {
        matches!(kind, TokenKind::StyleTemplateHead)
    });
    let close = file_tokens
        .tokens
        .iter()
        .enumerate()
        .skip(style_child + 1)
        .find_map(|(index, token)| matches!(token.kind, TokenKind::TemplateClose).then_some(index))
        .expect("expected the child template to close");
    let comma = file_tokens
        .tokens
        .iter()
        .enumerate()
        .skip(close + 1)
        .find_map(|(index, token)| matches!(token.kind, TokenKind::Comma).then_some(index))
        .expect("expected a comma after the child template");
    let markdown = file_tokens
        .tokens
        .iter()
        .enumerate()
        .skip(comma + 1)
        .find_map(|(index, token)| {
            matches!(token.kind, TokenKind::StyleDirective(id) if string_table.resolve(id) == "markdown")
                .then_some(index)
        })
        .expect("expected the outer head to continue with '$markdown'");

    assert!(outer_head < style_child);
    assert!(style_child < close);
    assert!(close < comma);
    assert!(comma < markdown);
}

#[test]
fn rejects_style_directives_outside_template_heads() {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "$markdown\n",
        &source_path,
        TokenizeMode::Normal,
        &mut string_table,
    );
    assert!(
        result.is_err(),
        "style directives outside template heads should fail"
    );
}

#[test]
fn tokenizes_template_slot_markers_in_template_bodies() {
    let (file_tokens, _) = tokenize_source("[: before [   ....   ] after]");

    assert!(
        file_tokens
            .tokens
            .iter()
            .any(|token| matches!(token.kind, TokenKind::TemplateSlotMarker))
    );
}

#[test]
fn tokenizes_labeled_slot_targets_inside_template_heads() {
    let (file_tokens, _) = tokenize_source("[wrapper: [$1: first][$2]]");

    assert!(
        file_tokens
            .tokens
            .iter()
            .any(|token| matches!(token.kind, TokenKind::SlotTarget(1)))
    );
    assert!(
        file_tokens
            .tokens
            .iter()
            .any(|token| matches!(token.kind, TokenKind::SlotTarget(2)))
    );
}

#[test]
fn rejects_malformed_template_slot_markers() {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "[: before [ .x ] after]",
        &source_path,
        TokenizeMode::Normal,
        &mut string_table,
    );

    assert!(result.is_err(), "malformed slot markers should fail");
}
