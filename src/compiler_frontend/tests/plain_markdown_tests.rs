use crate::compiler_frontend::plain_markdown::render_plain_markdown;

#[test]
fn renders_headings_and_paragraphs() {
    let html = render_plain_markdown("# Heading\n\nParagraph text.").html;

    assert!(
        html.contains("<h1>Heading</h1>"),
        "expected h1, got: {}",
        html
    );
    assert!(
        html.contains("<p>Paragraph text.</p>"),
        "expected paragraph, got: {}",
        html
    );
}

#[test]
fn renders_fenced_code_block() {
    let html = render_plain_markdown("```\ncode line\n```").html;

    assert!(
        html.contains("<pre><code>"),
        "expected pre/code open, got: {}",
        html
    );
    assert!(
        html.contains("code line"),
        "expected code content, got: {}",
        html
    );
    assert!(
        html.contains("</code></pre>"),
        "expected pre/code close, got: {}",
        html
    );
}

#[test]
fn renders_table() {
    let markdown = "| a | b |\n|---|---|\n| 1 | 2 |";
    let html = render_plain_markdown(markdown).html;

    assert!(html.contains("<table>"), "expected table, got: {}", html);
    assert!(
        html.contains("<th>a</th>"),
        "expected header cell, got: {}",
        html
    );
    assert!(
        html.contains("<td>1</td>"),
        "expected body cell, got: {}",
        html
    );
}

#[test]
fn renders_task_list() {
    let html = render_plain_markdown("- [x] done\n- [ ] todo").html;

    assert!(
        html.contains("<input"),
        "expected task list checkbox, got: {}",
        html
    );
    assert!(
        html.contains("disabled"),
        "expected disabled checkbox, got: {}",
        html
    );
    assert!(html.contains("done"), "expected done label, got: {}", html);
}

#[test]
fn renders_strikethrough() {
    let html = render_plain_markdown("~~deleted~~").html;

    assert!(
        html.contains("<del>deleted</del>"),
        "expected strikethrough, got: {}",
        html
    );
}

#[test]
fn renders_footnotes() {
    let markdown = "text[^1]\n\n[^1]: note";
    let html = render_plain_markdown(markdown).html;

    assert!(
        html.contains("#1"),
        "expected footnote anchor, got: {}",
        html
    );
    assert!(
        html.contains("note"),
        "expected footnote text, got: {}",
        html
    );
}

#[test]
fn preserves_raw_html() {
    let html = render_plain_markdown("<div>raw</div>").html;

    assert!(
        html.contains("<div>raw</div>"),
        "expected raw html preserved, got: {}",
        html
    );
}

#[test]
fn disables_smart_punctuation() {
    let html = render_plain_markdown("a -- b").html;

    assert!(
        html.contains("a -- b"),
        "expected dashes unchanged, got: {}",
        html
    );
}
