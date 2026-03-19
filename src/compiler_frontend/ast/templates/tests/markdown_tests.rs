use super::to_markdown;

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
        marker = crate::compiler_frontend::ast::templates::styles::markdown::HIDDEN_SKIP_CHAR
    );

    let rendered = to_markdown(&source, "p");
    assert_eq!(rendered, "<p>prefix<strong>&\"'</strong>suffix</p>");
}
