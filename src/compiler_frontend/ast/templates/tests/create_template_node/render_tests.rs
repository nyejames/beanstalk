use super::*;
use crate::compiler_frontend::symbols::string_interning::StringTable;

#[test]
fn markdown_formatter_output_text_uses_non_default_render_plan_locations() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[$markdown:\n# Hello\n]", &mut string_table);
    let context = new_constant_context(token_stream.src_path.to_owned());

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("markdown template should parse");
    let locations = collect_body_text_locations_from_render_plan(&template);

    assert!(
        !locations.is_empty(),
        "expected body text pieces in render plan"
    );
    assert!(
        locations
            .iter()
            .all(|location| !is_default_text_location(location)),
        "formatter-emitted body text should keep coarse source provenance"
    );
}

#[test]
fn unformatted_content_preserves_pre_format_composed_structure() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[$markdown:\n# Hello\n]", &mut string_table);
    let context = new_constant_context(token_stream.src_path.to_owned());

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("markdown template should parse");

    let mut unformatted_rendered = String::new();
    collect_static_template_fragments(
        &template.unformatted_content.atoms,
        &string_table,
        &mut unformatted_rendered,
    );
    let formatted_body = collect_body_text_from_render_plan(&template, &string_table);

    assert!(
        unformatted_rendered.contains("# Hello"),
        "unformatted_content should keep pre-format source text"
    );
    assert!(
        formatted_body
            .iter()
            .any(|text| text.contains("<h1>Hello</h1>")),
        "render plan should carry formatted markdown output"
    );
}
