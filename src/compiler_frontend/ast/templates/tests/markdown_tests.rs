use super::to_markdown;
use crate::compiler_frontend::ast::templates::styles::TEMPLATE_FORMAT_GUARD_CHAR;

#[test]
fn parses_links_for_all_supported_target_prefixes() {
    let cases = [
        ("@https://example.com (Example)", "https://example.com"),
        (
            "@//cdn.example.com/lib.js (CDN)",
            "//cdn.example.com/lib.js",
        ),
        ("@/docs/getting-started (Docs)", "/docs/getting-started"),
        ("@./local/path (Local)", "./local/path"),
        ("@../parent/path (Parent)", "../parent/path"),
        ("@#overview (Overview)", "#overview"),
        ("@?q=beanstalk (Search)", "?q=beanstalk"),
    ];

    for (input, target) in cases {
        let rendered = to_markdown(input, "p");
        let expected = format!(
            "<p><a href=\"{target}\">{}</a></p>",
            input
                .split_once('(')
                .expect("label start should exist")
                .1
                .trim_end_matches(')')
        );
        assert_eq!(rendered, expected);
    }
}

#[test]
fn requires_non_whitespace_or_start_before_at_sign() {
    let rendered = to_markdown("email@https://example.com (Example)", "p");
    assert_eq!(rendered, "<p>email@https://example.com (Example)</p>");
}

#[test]
fn invalid_scheme_like_targets_do_not_parse_as_links() {
    let rendered = to_markdown("@example.com (Example)", "p");
    assert_eq!(rendered, "<p>@example.com (Example)</p>");
}

#[test]
fn requires_horizontal_whitespace_before_label() {
    let rendered = to_markdown("@https://example.com(Example)", "p");
    assert_eq!(rendered, "<p>@https://example.com(Example)</p>");
}

#[test]
fn newline_between_target_and_label_breaks_link_recognition() {
    let rendered = to_markdown("@https://example.com\n(Example)", "p");
    assert_eq!(rendered, "<p>@https://example.com (Example)</p>");
}

#[test]
fn rejects_empty_or_whitespace_only_labels() {
    let empty_label = to_markdown("@/docs ()", "p");
    assert_eq!(empty_label, "<p>@/docs ()</p>");

    let whitespace_label = to_markdown("@/docs (   )", "p");
    assert_eq!(whitespace_label, "<p>@/docs (   )</p>");
}

#[test]
fn missing_closing_paren_falls_back_to_literal_text() {
    let rendered = to_markdown("@/docs (Docs", "p");
    assert_eq!(rendered, "<p>@/docs (Docs</p>");
}

#[test]
fn malformed_candidate_keeps_literal_at_symbol() {
    let rendered = to_markdown("Visit @docs (Docs) today", "p");
    assert_eq!(rendered, "<p>Visit @docs (Docs) today</p>");
}

#[test]
fn link_parsing_works_inside_heading_and_emphasis() {
    let heading = to_markdown("\n# @/docs (Docs)\n", "p");
    assert_eq!(heading, "<h1><a href=\"/docs\">Docs</a></h1>");

    let emphasis = to_markdown("\n*@/docs (Docs)*\n", "p");
    assert_eq!(emphasis, "<p><em>@/docs (Docs)</em> </p>");
}

#[test]
fn escapes_html_characters_in_plain_markdown_text() {
    let rendered = to_markdown("<tag> & \"quote\" 'apostrophe'", "p");
    assert_eq!(
        rendered,
        "<p>&lt;tag&gt; &amp; &quot;quote&quot; &#39;apostrophe&#39;</p>"
    );
}

#[test]
fn escapes_html_characters_inside_heading_and_emphasis_content() {
    let heading = to_markdown("\n# <h1> & \"q\" 'x'\n", "p");
    assert_eq!(
        heading,
        "<h1>&lt;h1&gt; &amp; &quot;q&quot; &#39;x&#39;</h1>"
    );

    let emphasis = to_markdown("\n*<tag>&\"'*\n", "p");
    assert!(emphasis.contains("<em>&lt;tag&gt;&amp;&quot;&#39;</em>"));
}

#[test]
fn escapes_link_target_and_label_html_characters() {
    let rendered = to_markdown(
        "@https://example.com?a=1&b=2\"x\"<tag> (\"<Click>\" & 'Go')",
        "p",
    );
    assert_eq!(
        rendered,
        "<p><a href=\"https://example.com?a=1&amp;b=2&quot;x&quot;&lt;tag&gt;\">&quot;&lt;Click&gt;&quot; &amp; &#39;Go&#39;</a></p>"
    );
}

#[test]
fn malformed_links_still_escape_literal_html_characters() {
    let rendered = to_markdown("Visit @docs (<b>)", "p");
    assert_eq!(rendered, "<p>Visit @docs (&lt;b&gt;)</p>");
}

#[test]
fn hidden_skip_runs_remain_unescaped() {
    let source = format!(
        "prefix{marker}<strong>&\"'</strong>{marker}suffix",
        marker = TEMPLATE_FORMAT_GUARD_CHAR
    );

    let rendered = to_markdown(&source, "p");
    assert_eq!(
        rendered,
        format!(
            "<p>prefix{marker}<strong>&\"'</strong>{marker}suffix</p>",
            marker = TEMPLATE_FORMAT_GUARD_CHAR
        )
    );
}

#[test]
fn parses_unordered_list_markers_into_list_items() {
    let rendered = to_markdown("- first\n* second\n+ third", "p");
    assert_eq!(
        rendered,
        "<ul><li>first</li><li>second</li><li>third</li></ul>"
    );
}

#[test]
fn parses_ordered_list_markers_with_dot_or_paren() {
    let rendered = to_markdown("1. first\n2) second\n3. third", "p");
    assert_eq!(
        rendered,
        "<ol><li>first</li><li>second</li><li>third</li></ol>"
    );
}

#[test]
fn parses_nested_mixed_lists_by_indentation() {
    let rendered = to_markdown(
        "- parent\n  - child bullet\n  1. child ordered\n- sibling",
        "p",
    );

    assert!(rendered.starts_with("<ul><li>parent"));
    assert!(rendered.contains("<ul><li>child bullet</li></ul>"));
    assert!(rendered.contains("<ol><li>child ordered</li></ol>"));
    assert!(rendered.ends_with("<li>sibling</li></ul>"));
}

#[test]
fn newline_text_continues_previous_list_item() {
    let rendered = to_markdown(
        "- Square brackets are NOT used for arrays, curly braces are used instead.\nSquare brackets are only used for string templates. Items in collections are accessed via methods.\n- Equality and other logical operators use keywords like \"is\" and \"not\"\n(you can't use == or ! for example)",
        "p",
    );

    assert_eq!(
        rendered,
        "<ul><li>Square brackets are NOT used for arrays, curly braces are used instead. Square brackets are only used for string templates. Items in collections are accessed via methods.</li><li>Equality and other logical operators use keywords like &quot;is&quot; and &quot;not&quot; (you can&#39;t use == or ! for example)</li></ul>"
    );
}

#[test]
fn blank_line_breaks_list_continuation() {
    let rendered = to_markdown("- first line\ncontinuation line\n\nplain paragraph", "p");

    assert_eq!(
        rendered,
        "<ul><li>first line continuation line</li></ul><p>plain paragraph</p>"
    );
}

#[test]
fn heading_line_breaks_out_of_list_without_blank_line() {
    let rendered = to_markdown("- first line\n## Heading\nplain paragraph", "p");

    assert_eq!(
        rendered,
        "<ul><li>first line</li></ul><h2>Heading</h2><p>plain paragraph</p>"
    );
}

#[test]
fn continuation_lines_preserve_hidden_skip_segments() {
    let source = format!(
        "- prefix\n{marker}<strong>&\"'</strong>{marker}\n- next",
        marker = TEMPLATE_FORMAT_GUARD_CHAR
    );

    let rendered = to_markdown(&source, "p");
    assert_eq!(
        rendered,
        format!(
            "<ul><li>prefix {marker}<strong>&\"'</strong>{marker}</li><li>next</li></ul>",
            marker = TEMPLATE_FORMAT_GUARD_CHAR
        )
    );
}

#[test]
fn list_items_keep_inline_markdown_links_and_escaping() {
    let rendered = to_markdown("- item *bold* @/docs (Docs) <tag>", "p");

    assert!(rendered.contains("<li>item <em>bold</em>"));
    assert!(rendered.contains("<a href=\"/docs\">Docs</a>"));
    assert!(rendered.contains("&lt;tag&gt;"));
}

#[test]
fn non_list_lines_remain_plain_markdown_blocks() {
    let rendered = to_markdown("-not a list\nstill plain text", "p");
    assert_eq!(rendered, "<p>-not a list still plain text</p>");
}
