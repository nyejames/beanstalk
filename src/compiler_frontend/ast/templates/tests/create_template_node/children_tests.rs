use super::*;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::styles::markdown::markdown_formatter;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{CharPosition, SourceLocation};

#[test]
fn fresh_marks_template_to_skip_parent_child_wrappers() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[$fresh, $markdown:\n# Hello\n]", &mut string_table);
    let context = new_constant_context(token_stream.src_path.to_owned());

    let mut inherited = Template::empty();
    inherited.style.formatter = Some(markdown_formatter());
    inherited.style.child_templates.push(Template::empty());

    let template = Template::new(
        &mut token_stream,
        &context,
        vec![inherited],
        &mut string_table,
    )
    .expect("template should parse");

    assert!(template.style.formatter.is_some());
    assert!(template.style.skip_parent_child_wrappers);
    assert!(template.style.child_templates.is_empty());
}

#[test]
fn stores_style_child_templates_from_children_directive() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[$children([:prefix]), : body]", &mut string_table);
    let context = new_constant_context(token_stream.src_path.to_owned());

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("template should parse");

    assert_eq!(template.style.child_templates.len(), 1);
}

#[test]
fn children_directive_accepts_const_string_reference() {
    let mut string_table = StringTable::new();
    let scope = InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let prefix_name = string_table.intern("prefix");
    let declarations = vec![Declaration {
        id: scope.append(prefix_name),
        value: Expression::string_slice(
            string_table.intern("prefix: "),
            SourceLocation {
                scope: InternedPath::new(),
                start_pos: CharPosition {
                    line_number: 1,
                    char_column: 0,
                },
                end_pos: CharPosition {
                    line_number: 1,
                    char_column: 120,
                },
            },
            ValueMode::ImmutableOwned,
        ),
    }];

    let mut token_stream =
        template_tokens_from_source("[$children(prefix): [: child]]", &mut string_table);
    let context = constant_template_context(&token_stream.src_path, &declarations);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("children directive should accept const-folded references");
    let folded = fold_template_in_context(&template, &context, &mut string_table);

    assert!(string_table.resolve(folded).contains("prefix:"));
}

#[test]
fn children_directive_rejects_runtime_values() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[$children(value): [: child]]", &mut string_table);
    let context = runtime_template_context(&token_stream.src_path, &mut string_table);

    let error = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("children directive should reject runtime values");

    assert!(error.msg.contains("$children(..)"));
    assert!(error.msg.contains("compile-time"));
}

#[test]
fn children_wrappers_are_applied_to_direct_children() {
    let rendered = folded_template_output("[$children([: pref[$slot]suf]): [: body]]");
    assert!(rendered.contains("pref"));
    assert!(rendered.contains("body"));
    assert!(rendered.contains("suf"));
}

#[test]
fn children_wrappers_do_not_apply_to_grandchildren() {
    let rendered = folded_template_output("[$children([:<wrap>[$slot]</wrap>]): [:[ : body]]]");

    assert!(rendered.contains("body"));
    assert_eq!(rendered.matches("<wrap>").count(), 1);
}

#[test]
fn markdown_is_not_inherited_by_grandchildren() {
    let rendered = folded_template_output("[$markdown:\n[: [: <b>grandchild-body</b> ]]\n]");

    assert!(rendered.contains("<b>grandchild-body</b>"));
    assert!(!rendered.contains("&lt;b&gt;grandchild-body&lt;/b&gt;"));
}

#[test]
fn markdown_must_be_redeclared_at_each_nested_template_level() {
    let rendered = folded_template_output(
        "[$markdown:\n[$markdown:\n[$markdown: <b>grandchild-body</b>]\n]\n]",
    );

    assert!(rendered.contains("&lt;b&gt;grandchild-body&lt;/b&gt;"));
    assert!(!rendered.contains("<b>grandchild-body</b>"));
}

#[test]
fn nested_inline_templates_inside_table_cells_do_not_become_extra_cells() {
    let rendered = folded_template_output(
        "[
            $children([:<tr>[$slot]</tr>]):
            [$children([:<td>[$slot]</td>]):
                [: Cell with [:inline] content]
                [: Plain cell]
            ]
        ]",
    );

    assert_eq!(rendered.matches("<tr>").count(), 1, "expected one row");

    assert_eq!(
        rendered.matches("<td>").count(),
        2,
        "expected two cells; nested inline templates must not become extra cells"
    );

    assert!(
        rendered.contains("Cell with"),
        "expected text before nested inline template"
    );
    assert!(
        rendered.contains("inline"),
        "expected nested inline template content"
    );
    assert!(
        rendered.contains("content"),
        "expected text after nested inline template"
    );
}
