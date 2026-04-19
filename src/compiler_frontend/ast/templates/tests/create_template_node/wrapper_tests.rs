use super::*;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::datatypes::Ownership;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;

#[test]
fn slot_children_wrappers_apply_table_rows_and_cells_without_cross_applying() {
    let mut string_table = StringTable::new();
    let wrapper_scope =
        InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let mut wrapper_tokens = template_tokens_from_source(
        "[$children([:<tr>[$slot]</tr>]): <table style=\"[$slot(\"style\")]\">[$children([:<td>[$slot]</td>]):[$slot]]</table>]",
        &mut string_table,
    );
    let wrapper_context = new_constant_context(wrapper_tokens.src_path.to_owned());
    let wrapper = Template::new(
        &mut wrapper_tokens,
        &wrapper_context,
        vec![],
        &mut string_table,
    )
    .expect("table wrapper should parse");

    let declaration = Declaration {
        id: wrapper_scope.append(string_table.intern("table_wrapper")),
        value: Expression::template(wrapper, Ownership::ImmutableOwned),
    };

    let mut token_stream = template_tokens_from_source(
        "[table_wrapper:\n    [: [:Type] [:Description] ]\n    [: [:float] [:64 bit floating point number] ]\n]",
        &mut string_table,
    );
    let context = constant_template_context(&token_stream.src_path, &[declaration]);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("slot child wrapper application should parse");
    let folded = fold_template_in_context(&template, &context, &mut string_table);
    let rendered = string_table.resolve(folded);

    assert_eq!(rendered.matches("<tr>").count(), 2);
    assert!(rendered.contains("<td>Type</td>"));
    assert!(rendered.contains("<td>Description</td>"));
    assert!(rendered.contains("<td>float</td>"));
    assert_eq!(rendered.matches("<td>").count(), 4);
}

#[test]
fn markdown_parent_keeps_table_rows_and_cells_inside_table() {
    let mut string_table = StringTable::new();
    let wrapper_scope =
        InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let mut wrapper_tokens = template_tokens_from_source(
        "[$children([:<tr>[$slot]</tr>]): <table style=\"[$slot(\"style\")]\">[$children([:<td>[$slot]</td>]):[$slot]]</table>]",
        &mut string_table,
    );
    let wrapper_context = new_constant_context(wrapper_tokens.src_path.to_owned());
    let wrapper = Template::new(
        &mut wrapper_tokens,
        &wrapper_context,
        vec![],
        &mut string_table,
    )
    .expect("table wrapper should parse");

    let declaration = Declaration {
        id: wrapper_scope.append(string_table.intern("table_wrapper")),
        value: Expression::template(wrapper, Ownership::ImmutableOwned),
    };

    let mut token_stream = template_tokens_from_source(
        "[$markdown:\n[table_wrapper:\n    [: [:Type] [:Description] ]\n    [: [:float] [:64 bit floating point number] ]\n]\n]",
        &mut string_table,
    );
    let context = constant_template_context(&token_stream.src_path, &[declaration]);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("markdown table usage should parse");
    let folded = fold_template_in_context(&template, &context, &mut string_table);
    let rendered = string_table.resolve(folded);

    assert!(!rendered.contains('\u{FFFC}'));
    assert_eq!(rendered.matches("<tr>").count(), 2);
    assert_eq!(rendered.matches("<td>").count(), 4);
}

#[test]
fn markdown_page_wrapper_keeps_table_rows_and_cells_inside_table() {
    let mut string_table = StringTable::new();
    let wrapper_scope =
        InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);

    let mut table_tokens = template_tokens_from_source(
        "[$children([:<tr>[$slot]</tr>]): <table style=\"[$slot(\"style\")]\">[$children([:<td>[$slot]</td>]):[$slot]]</table>]",
        &mut string_table,
    );
    let wrapper_context = new_constant_context(table_tokens.src_path.to_owned());
    let table_wrapper = Template::new(
        &mut table_tokens,
        &wrapper_context,
        vec![],
        &mut string_table,
    )
    .expect("table wrapper should parse");

    let mut page_tokens =
        template_tokens_from_source("[: <body>[$slot]</body>]", &mut string_table);
    let page_wrapper = Template::new(
        &mut page_tokens,
        &wrapper_context,
        vec![],
        &mut string_table,
    )
    .expect("page wrapper should parse");

    let declarations = vec![
        Declaration {
            id: wrapper_scope.append(string_table.intern("table_wrapper")),
            value: Expression::template(table_wrapper, Ownership::ImmutableOwned),
        },
        Declaration {
            id: wrapper_scope.append(string_table.intern("page_wrapper")),
            value: Expression::template(page_wrapper, Ownership::ImmutableOwned),
        },
    ];

    let mut token_stream = template_tokens_from_source(
        "[page_wrapper, $markdown:\n[table_wrapper:\n    [: [:Type] [:Description] ]\n    [: [:float] [:64 bit floating point number] ]\n]\n]",
        &mut string_table,
    );
    let context = constant_template_context(&token_stream.src_path, &declarations);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("markdown page wrapper table usage should parse");
    let folded = fold_template_in_context(&template, &context, &mut string_table);
    let rendered = string_table.resolve(folded);

    assert!(!rendered.contains('\u{FFFC}'));
    assert_eq!(rendered.matches("<tr>").count(), 2);
    assert_eq!(rendered.matches("<td>").count(), 4);
}

#[test]
fn markdown_parent_with_fresh_row_wrapper_renders_plain_cells() {
    let mut string_table = StringTable::new();
    let declarations = docs_style_wrapper_declarations(&mut string_table);

    let mut token_stream = template_tokens_from_source(
        "[$markdown:\n[row: [: Type] [: Description] ]\n]",
        &mut string_table,
    );
    let context = constant_template_context(&token_stream.src_path, &declarations);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("markdown row usage should parse");
    let folded = fold_template_in_context(&template, &context, &mut string_table);
    let rendered = string_table.resolve(folded);

    assert_eq!(rendered.matches("<td>").count(), 2);
    assert!(rendered.contains("Type"));
    assert!(rendered.contains("Description"));
    assert!(!rendered.contains("<p>"));
}

#[test]
fn markdown_parent_with_fresh_header_row_wrapper_renders_plain_headers() {
    let mut string_table = StringTable::new();
    let declarations = docs_style_wrapper_declarations(&mut string_table);

    let mut token_stream = template_tokens_from_source(
        "[$markdown:\n[header_row: [: Type] [: Description] ]\n]",
        &mut string_table,
    );
    let context = constant_template_context(&token_stream.src_path, &declarations);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("markdown header row usage should parse");

    let folded = fold_template_in_context(&template, &context, &mut string_table);
    let rendered = string_table.resolve(folded);

    assert_eq!(
        rendered
            .matches("<th style=\"border: 1px solid; padding: 0.5em; text-align: left;\">")
            .count(),
        2
    );
    assert!(rendered.contains("Type</th>"));
    assert!(rendered.contains("Description</th>"));
    assert!(!rendered.contains("<p>"));
}
