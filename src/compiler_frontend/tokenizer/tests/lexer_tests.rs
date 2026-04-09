use super::*;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::style_directives::{
    StyleDirectiveHandlerSpec, StyleDirectiveRegistry, StyleDirectiveSpec,
};
use crate::projects::html_project::style_directives::html_project_style_directives;

fn frontend_test_style_directives() -> StyleDirectiveRegistry {
    StyleDirectiveRegistry::built_ins()
}

fn html_project_test_style_directives() -> StyleDirectiveRegistry {
    StyleDirectiveRegistry::merged(&html_project_style_directives())
        .expect("html project style directives should merge with core directives")
}

fn tokenize_source(source: &str) -> (FileTokens, StringTable) {
    let style_directives = frontend_test_style_directives();
    tokenize_source_with_registry(source, &style_directives)
}

fn tokenize_html_source(source: &str) -> (FileTokens, StringTable) {
    let style_directives = html_project_test_style_directives();
    tokenize_source_with_registry(source, &style_directives)
}

fn tokenize_source_with_registry(
    source: &str,
    style_directives: &StyleDirectiveRegistry,
) -> (FileTokens, StringTable) {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);
    let file_tokens = tokenize(
        source,
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        style_directives,
        &mut string_table,
        None,
    )
    .expect("tokenization should succeed");
    (file_tokens, string_table)
}

fn tokenize_source_with_directives(
    source: &str,
    directives: &[StyleDirectiveSpec],
) -> (FileTokens, StringTable) {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);
    let registry = StyleDirectiveRegistry::merged(directives)
        .expect("test style directives should merge with core directives");
    let file_tokens = tokenize(
        source,
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &registry,
        &mut string_table,
        None,
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
fn tokenizes_reserved_trait_keywords_as_reserved_tokens() {
    let (file_tokens, _string_table) = tokenize_source("must This\n");

    assert!(
        matches!(file_tokens.tokens[0].kind, TokenKind::ModuleStart),
        "token streams always begin with the module sentinel"
    );
    assert!(
        matches!(file_tokens.tokens[1].kind, TokenKind::Must),
        "expected 'must' to lex as a reserved trait token"
    );
    assert!(
        matches!(file_tokens.tokens[2].kind, TokenKind::TraitThis),
        "expected 'This' to lex as a reserved trait token"
    );
    assert!(
        !matches!(file_tokens.tokens[1].kind, TokenKind::Symbol(_)),
        "'must' should not remain a user symbol"
    );
    assert!(
        !matches!(file_tokens.tokens[2].kind, TokenKind::Symbol(_)),
        "'This' should not remain a user symbol"
    );
}

#[test]
fn tokenizes_standalone_underscore_as_wildcard_but_prefixed_names_as_symbols() {
    let (file_tokens, string_table) = tokenize_source("_ _true __value\n");

    assert!(
        matches!(file_tokens.tokens[1].kind, TokenKind::Wildcard),
        "expected standalone '_' to remain wildcard"
    );
    assert!(
        matches!(file_tokens.tokens[2].kind, TokenKind::Symbol(id) if string_table.resolve(id) == "_true"),
        "expected '_true' to tokenize as a symbol identifier"
    );
    assert!(
        matches!(file_tokens.tokens[3].kind, TokenKind::Symbol(id) if string_table.resolve(id) == "__value"),
        "expected '__value' to tokenize as a symbol identifier"
    );
}

#[test]
fn tokenizes_in_as_symbol_after_loop_syntax_removal() {
    let (file_tokens, string_table) = tokenize_source("in\n");

    assert!(
        matches!(file_tokens.tokens[1].kind, TokenKind::Symbol(id) if string_table.resolve(id) == "in"),
        "expected 'in' to tokenize as a normal symbol after loop-syntax removal"
    );
}

#[test]
fn tokenizes_pipe_bindings_in_loop_headers() {
    let (file_tokens, string_table) = tokenize_source("loop items |item, index|:\n;\n");

    let loop_index = find_token_index(&file_tokens.tokens, |kind| matches!(kind, TokenKind::Loop));
    let items_index = find_token_index(
        &file_tokens.tokens,
        |kind| matches!(kind, TokenKind::Symbol(id) if string_table.resolve(*id) == "items"),
    );
    let item_index = find_token_index(
        &file_tokens.tokens,
        |kind| matches!(kind, TokenKind::Symbol(id) if string_table.resolve(*id) == "item"),
    );
    let index_index = find_token_index(
        &file_tokens.tokens,
        |kind| matches!(kind, TokenKind::Symbol(id) if string_table.resolve(*id) == "index"),
    );
    let first_pipe = find_token_index(&file_tokens.tokens, |kind| {
        matches!(kind, TokenKind::TypeParameterBracket)
    });
    let second_pipe = file_tokens
        .tokens
        .iter()
        .enumerate()
        .skip(first_pipe + 1)
        .find_map(|(idx, token)| {
            matches!(token.kind, TokenKind::TypeParameterBracket).then_some(idx)
        })
        .expect("expected closing pipe token");

    assert!(loop_index < items_index);
    assert!(items_index < first_pipe);
    assert!(first_pipe < item_index);
    assert!(item_index < index_index);
    assert!(index_index < second_pipe);
}

#[test]
fn tokenizes_bare_loop_bindings_without_special_keyword_support() {
    let (file_tokens, string_table) = tokenize_source("loop items item, index:\n;\n");

    let items_index = find_token_index(
        &file_tokens.tokens,
        |kind| matches!(kind, TokenKind::Symbol(id) if string_table.resolve(*id) == "items"),
    );
    let item_index = find_token_index(
        &file_tokens.tokens,
        |kind| matches!(kind, TokenKind::Symbol(id) if string_table.resolve(*id) == "item"),
    );
    let comma_index =
        find_token_index(&file_tokens.tokens, |kind| matches!(kind, TokenKind::Comma));
    let index_index = find_token_index(
        &file_tokens.tokens,
        |kind| matches!(kind, TokenKind::Symbol(id) if string_table.resolve(*id) == "index"),
    );

    assert!(items_index < item_index);
    assert!(item_index < comma_index);
    assert!(comma_index < index_index);
}

#[test]
fn tokenizes_none_question_mark_and_bang_markers() {
    let (file_tokens, _string_table) =
        tokenize_source("value String? = none\npersist()!\nrecover = may_fail() ! \"\"\n");

    assert!(
        file_tokens
            .tokens
            .iter()
            .any(|token| matches!(token.kind, TokenKind::QuestionMark)),
        "expected '?' optional-type marker token"
    );
    assert!(
        file_tokens
            .tokens
            .iter()
            .any(|token| matches!(token.kind, TokenKind::NoneLiteral)),
        "expected lowercase 'none' literal token"
    );
    assert!(
        file_tokens
            .tokens
            .iter()
            .filter(|token| matches!(token.kind, TokenKind::Bang))
            .count()
            >= 2,
        "expected bang tokens for both propagate and fallback call handling"
    );
}

#[test]
fn tokenizes_style_directives_inside_template_heads() {
    let (file_tokens, string_table) = tokenize_source("[$markdown, $fresh: body]");

    let outer_head = find_token_index(&file_tokens.tokens, |kind| {
        matches!(kind, TokenKind::TemplateHead)
    });
    let markdown = find_token_index(
        &file_tokens.tokens,
        |kind| matches!(kind, TokenKind::StyleDirective(id) if string_table.resolve(*id) == "markdown"),
    );
    let fresh = find_token_index(
        &file_tokens.tokens,
        |kind| matches!(kind, TokenKind::StyleDirective(id) if string_table.resolve(*id) == "fresh"),
    );

    assert!(outer_head < markdown);
    assert!(markdown < fresh);
    assert!(matches!(
        file_tokens.tokens[markdown].kind,
        TokenKind::StyleDirective(..)
    ));
    assert!(matches!(
        file_tokens.tokens[fresh].kind,
        TokenKind::StyleDirective(..)
    ));
}

#[test]
fn rejects_legacy_reset_style_directive_name() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);
    let error = tokenize(
        "[$reset: body]",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    )
    .expect_err("legacy reset directive should be rejected");

    assert!(
        error.msg.contains("Unsupported style directive '$reset'"),
        "unexpected error message: {}",
        error.msg
    );
}

#[test]
fn tokenizes_children_directive_with_template_argument() {
    let (file_tokens, string_table) =
        tokenize_source("[$children([:prefix]), $markdown:\nhello\n]");

    let outer_head = find_token_index(&file_tokens.tokens, |kind| {
        matches!(kind, TokenKind::TemplateHead)
    });
    let children = find_token_index(
        &file_tokens.tokens,
        |kind| matches!(kind, TokenKind::StyleDirective(id) if string_table.resolve(*id) == "children"),
    );
    let open_paren = find_token_index(&file_tokens.tokens, |kind| {
        matches!(kind, TokenKind::OpenParenthesis)
    });
    let child_template = file_tokens
        .tokens
        .iter()
        .enumerate()
        .skip(open_paren + 1)
        .find_map(|(index, token)| matches!(token.kind, TokenKind::TemplateHead).then_some(index))
        .expect("expected child template opener");
    let close = file_tokens
        .tokens
        .iter()
        .enumerate()
        .skip(child_template + 1)
        .find_map(|(index, token)| matches!(token.kind, TokenKind::TemplateClose).then_some(index))
        .expect("expected the child template to close");
    let close_paren = file_tokens
        .tokens
        .iter()
        .enumerate()
        .skip(close + 1)
        .find_map(|(index, token)| {
            matches!(token.kind, TokenKind::CloseParenthesis).then_some(index)
        })
        .expect("expected ')' after the child template");
    let comma = file_tokens
        .tokens
        .iter()
        .enumerate()
        .skip(close_paren + 1)
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

    assert!(outer_head < children);
    assert!(children < open_paren);
    assert!(open_paren < child_template);
    assert!(child_template < close);
    assert!(close < close_paren);
    assert!(close_paren < comma);
    assert!(comma < markdown);
}

#[test]
fn rejects_legacy_style_child_template_prefix_syntax() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "[$[:prefix], $markdown:\nhello\n]",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    );
    assert!(
        result.is_err(),
        "legacy '$[' child-template syntax should fail"
    );
}

#[test]
fn rejects_style_directives_outside_template_heads() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "$markdown\n",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    );
    assert!(
        result.is_err(),
        "style directives outside template heads should fail"
    );
}

#[test]
fn unknown_style_directives_fail_under_strict_registry() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "[$unknown: value]",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    );
    let error = result.expect_err("unknown directive should fail during tokenization");
    assert!(error.msg.contains("Unsupported style directive"));
    assert!(error.msg.contains("$unknown"));
}

#[test]
fn tokenizes_slot_and_insert_directives_inside_template_heads() {
    let (file_tokens, string_table) =
        tokenize_source("[wrapper: [$slot][$slot(\"style\")][$insert(\"style\"): blue]]");

    let slot_directive_count = file_tokens
        .tokens
        .iter()
        .filter(|token| {
            matches!(token.kind, TokenKind::StyleDirective(id) if string_table.resolve(id) == "slot")
        })
        .count();
    let has_insert_directive = file_tokens.tokens.iter().any(|token| {
        matches!(token.kind, TokenKind::StyleDirective(id) if string_table.resolve(id) == "insert")
    });

    assert_eq!(slot_directive_count, 2);
    assert!(has_insert_directive);
    assert!(
        file_tokens
            .tokens
            .iter()
            .any(|token| matches!(token.kind, TokenKind::StringSliceLiteral(_)))
    );
}

#[test]
fn rejects_numeric_slot_directive_prefixes() {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let source_path = InternedPath::from_single_str("test.bst", &mut string_table);

    let result = tokenize(
        "[wrapper: [$1: first]]",
        &source_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    );
    assert!(
        result.is_err(),
        "legacy numeric '$1' slot directives should fail"
    );
}

#[test]
fn code_template_body_keeps_nested_square_brackets_as_literal_text() {
    let (file_tokens, string_table) =
        tokenize_source("[$code(\"bst\"):\nconcatenated = [string_slice, a_mutable_string]\n]");

    let template_heads = file_tokens
        .tokens
        .iter()
        .filter(|token| matches!(token.kind, TokenKind::TemplateHead))
        .count();
    let template_closes = file_tokens
        .tokens
        .iter()
        .filter(|token| matches!(token.kind, TokenKind::TemplateClose))
        .count();

    assert_eq!(
        template_heads, 1,
        "code template bodies should not tokenize nested '[' as template opens"
    );
    assert_eq!(template_closes, 1);

    let body_literal = file_tokens
        .tokens
        .iter()
        .find_map(|token| match token.kind {
            TokenKind::StringSliceLiteral(id) => {
                let value = string_table.resolve(id);
                value
                    .contains("[string_slice, a_mutable_string]")
                    .then_some(value)
            }
            _ => None,
        })
        .expect("expected code template body text to include literal square brackets");

    assert!(body_literal.contains("concatenated"));
}

#[test]
fn css_template_body_keeps_selector_brackets_as_literal_text() {
    let (file_tokens, string_table) =
        tokenize_html_source("[$css:\n.button[data-kind=\"cta\"] { color: red; }\n]");

    let template_heads = file_tokens
        .tokens
        .iter()
        .filter(|token| matches!(token.kind, TokenKind::TemplateHead))
        .count();
    let template_closes = file_tokens
        .tokens
        .iter()
        .filter(|token| matches!(token.kind, TokenKind::TemplateClose))
        .count();

    assert_eq!(
        template_heads, 1,
        "css template bodies should not tokenize selector brackets as nested templates"
    );
    assert_eq!(template_closes, 1);

    let body_literal = file_tokens
        .tokens
        .iter()
        .find_map(|token| match token.kind {
            TokenKind::StringSliceLiteral(id) => {
                let value = string_table.resolve(id);
                value.contains("[data-kind=\"cta\"]").then_some(value)
            }
            _ => None,
        })
        .expect("expected css template body text to include selector brackets");

    assert!(body_literal.contains(".button"));
}

#[test]
fn html_template_body_tokenizes_attribute_brackets_using_normal_rules() {
    let (file_tokens, string_table) =
        tokenize_html_source("[$html:\n<div data-tags=\"[one,two]\">Hello</div>\n]");

    let template_heads = file_tokens
        .tokens
        .iter()
        .filter(|token| matches!(token.kind, TokenKind::TemplateHead))
        .count();
    let template_closes = file_tokens
        .tokens
        .iter()
        .filter(|token| matches!(token.kind, TokenKind::TemplateClose))
        .count();

    assert_eq!(
        template_heads, 2,
        "normal template-body parsing should tokenize '[one,two]' as a nested template in $html"
    );
    assert_eq!(template_closes, 2);

    assert!(
        file_tokens.tokens.iter().any(
            |token| matches!(token.kind, TokenKind::Symbol(id) if string_table.resolve(id) == "one")
        ),
        "expected nested template symbol 'one' from bracket content"
    );
    assert!(
        file_tokens.tokens.iter().any(
            |token| matches!(token.kind, TokenKind::Symbol(id) if string_table.resolve(id) == "two")
        ),
        "expected nested template symbol 'two' from bracket content"
    );

    let preserves_literal_attribute_brackets =
        file_tokens.tokens.iter().any(|token| match token.kind {
            TokenKind::StringSliceLiteral(id) => {
                let value = string_table.resolve(id);
                value.contains("data-tags=\"[one,two]\"")
            }
            _ => false,
        });
    assert!(
        !preserves_literal_attribute_brackets,
        "normal $html tokenization should not preserve attribute bracket lists as one literal slice"
    );
}

#[test]
fn html_template_body_tokenizes_slot_templates_inside_quoted_attributes_with_normal_rules() {
    let (file_tokens, string_table) = tokenize_html_source(
        "[$html:\n<h1 style=\"font-size: 2em;[$slot(\"style\")]\">[$slot]</h1>\n]",
    );

    let template_heads = file_tokens
        .tokens
        .iter()
        .filter(|token| matches!(token.kind, TokenKind::TemplateHead))
        .count();
    let template_closes = file_tokens
        .tokens
        .iter()
        .filter(|token| matches!(token.kind, TokenKind::TemplateClose))
        .count();

    assert_eq!(
        template_heads, 3,
        "normal template-body parsing should still tokenize slot templates inside quoted attributes"
    );
    assert_eq!(template_closes, 3);

    let slot_directives = file_tokens
        .tokens
        .iter()
        .filter(|token| {
            matches!(token.kind, TokenKind::StyleDirective(id) if string_table.resolve(id) == "slot")
        })
        .count();
    assert_eq!(slot_directives, 2);
}

#[test]
fn html_template_body_tokenizes_symbol_wrappers_with_general_template_rules() {
    let (file_tokens, string_table) =
        tokenize_html_source("[$html:\n[title, center: LANGUAGE BASICS]\n]");

    let template_heads = file_tokens
        .tokens
        .iter()
        .filter(|token| matches!(token.kind, TokenKind::TemplateHead))
        .count();
    let template_closes = file_tokens
        .tokens
        .iter()
        .filter(|token| matches!(token.kind, TokenKind::TemplateClose))
        .count();

    assert_eq!(
        template_heads, 2,
        "normal template-body parsing should tokenize wrapper syntax in $html bodies"
    );
    assert_eq!(template_closes, 2);

    assert!(file_tokens.tokens.iter().any(
        |token| matches!(token.kind, TokenKind::Symbol(id) if string_table.resolve(id) == "title")
    ));
    assert!(file_tokens.tokens.iter().any(
        |token| matches!(token.kind, TokenKind::Symbol(id) if string_table.resolve(id) == "center")
    ));
}

#[test]
fn custom_balanced_directive_uses_general_balanced_mode() {
    let directives = vec![StyleDirectiveSpec::handler(
        "highlight",
        TemplateBodyMode::Balanced,
        StyleDirectiveHandlerSpec::no_op(),
    )];
    let (file_tokens, string_table) =
        tokenize_source_with_directives("[$highlight:\n[data-kind=\"cta\"]\n]", &directives);

    let template_heads = file_tokens
        .tokens
        .iter()
        .filter(|token| matches!(token.kind, TokenKind::TemplateHead))
        .count();
    let template_closes = file_tokens
        .tokens
        .iter()
        .filter(|token| matches!(token.kind, TokenKind::TemplateClose))
        .count();

    assert_eq!(template_heads, 1);
    assert_eq!(template_closes, 1);
    let body_literal = file_tokens
        .tokens
        .iter()
        .find_map(|token| match token.kind {
            TokenKind::StringSliceLiteral(id) => {
                let value = string_table.resolve(id);
                value.contains("[data-kind=\"cta\"]").then_some(value)
            }
            _ => None,
        })
        .expect("expected balanced directive body to keep brackets as literal text");
    assert!(body_literal.contains("data-kind"));
}

#[test]
fn note_and_todo_template_bodies_are_discarded_until_balanced_close() {
    for directive in ["note", "todo"] {
        let source = format!(
            "[${directive}:\n[this [body] has [nested [brackets]] and should be discarded]\n]"
        );
        let (file_tokens, string_table) = tokenize_source(&source);

        let template_heads = file_tokens
            .tokens
            .iter()
            .filter(|token| matches!(token.kind, TokenKind::TemplateHead))
            .count();
        let template_closes = file_tokens
            .tokens
            .iter()
            .filter(|token| matches!(token.kind, TokenKind::TemplateClose))
            .count();

        assert_eq!(template_heads, 1);
        assert_eq!(template_closes, 1);
        assert!(file_tokens.tokens.iter().any(|token| {
            matches!(token.kind, TokenKind::StyleDirective(id) if string_table.resolve(id) == directive)
        }));
        assert!(
            !file_tokens.tokens.iter().any(|token| {
                matches!(token.kind, TokenKind::StringSliceLiteral(id) if string_table.resolve(id).contains("discarded"))
            }),
            "expected ${directive} body text to be discarded during tokenization"
        );
    }
}

#[test]
fn doc_template_body_keeps_nested_templates_as_template_tokens() {
    let (file_tokens, string_table) = tokenize_source("[$doc:\n[: child]\n]");

    let template_heads = file_tokens
        .tokens
        .iter()
        .filter(|token| matches!(token.kind, TokenKind::TemplateHead))
        .count();
    let template_closes = file_tokens
        .tokens
        .iter()
        .filter(|token| matches!(token.kind, TokenKind::TemplateClose))
        .count();

    assert_eq!(
        template_heads, 2,
        "expected doc body nested template to tokenize as a child template"
    );
    assert_eq!(template_closes, 2);
    assert!(file_tokens.tokens.iter().any(|token| {
        matches!(token.kind, TokenKind::StyleDirective(id) if string_table.resolve(id) == "doc")
    }));
}
