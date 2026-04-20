use super::*;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::template::TemplateAtom;
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

#[test]
fn docs_style_data_wrapper_renders_expected_table_structure() {
    let mut string_table = StringTable::new();
    let declarations = docs_style_table_and_data_declarations(&mut string_table);

    let mut token_stream = template_tokens_from_source(
        "[table:\n    [: [: Operator] [: Description] [: Precedence] ]\n    [data: [: +] [: Sum] [: 2] ]\n    [data: [: -] [: Subtraction] [: 2] ]\n]",
        &mut string_table,
    );
    let context = constant_template_context(&token_stream.src_path, &declarations);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("docs-style table usage should parse");
    let folded = fold_template_in_context(&template, &context, &mut string_table);
    let rendered = string_table.resolve(folded);

    assert_eq!(rendered.matches("<th").count(), 3);
    assert_eq!(
        rendered
            .matches("<td style=\"padding: 0.2em 0.5em;\">")
            .count(),
        6
    );
    assert_eq!(
        rendered
            .matches("<tr style=\"border-bottom: 1px dotted grey;\">")
            .count(),
        2
    );
    assert!(rendered.contains("> +</td>"));
    assert!(rendered.contains("> -</td>"));
}

#[test]
fn docs_style_data_wrapper_keeps_cell_counts_linear_with_many_rows() {
    let mut string_table = StringTable::new();
    let declarations = docs_style_table_and_data_declarations(&mut string_table);

    let mut token_stream = template_tokens_from_source(
        "[table:\n    [: [: Operator] [: Description] [: Precedence] ]\n    [data: [: ^]  [: Exponent]            [: 4] ]\n    [data: [: *]  [: Multiplication]      [: 3] ]\n    [data: [: /]  [: Division]            [: 3] ]\n    [data: [: //] [: Integer Division]    [: 3] ]\n    [data: [: %]  [: Modulo (truncated)]  [: 3] ]\n    [data: [: %%] [: Remainder (floored)] [: 3] ]\n    [data: [: +]  [: Sum]                 [: 2] ]\n    [data: [: -]  [: Subtraction]         [: 2] ]\n]",
        &mut string_table,
    );
    let context = constant_template_context(&token_stream.src_path, &declarations);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("docs-style operator table should parse");
    let folded = fold_template_in_context(&template, &context, &mut string_table);
    let rendered = string_table.resolve(folded);

    assert_eq!(
        rendered
            .matches("<td style=\"padding: 0.2em 0.5em;\">")
            .count(),
        24
    );
    assert_eq!(
        rendered
            .matches("<tr style=\"border-bottom: 1px dotted grey;\">")
            .count(),
        8
    );
    assert!(!rendered.contains("<td style=\"padding: 0.2em 0.5em;\"><td"));
}

#[test]
fn docs_style_data_wrapper_keeps_ast_structure_bounded_for_many_rows() {
    let mut string_table = StringTable::new();
    let declarations = docs_style_table_and_data_declarations(&mut string_table);

    let row_count = 48usize;
    let mut source =
        String::from("[table:\n    [: [: Operator] [: Description] [: Precedence] ]\n");
    for index in 0..row_count {
        source.push_str(&format!(
            "    [data: [: op-{index}] [: desc-{index}] [: {index}] ]\n"
        ));
    }
    source.push(']');

    let mut token_stream = template_tokens_from_source(&source, &mut string_table);
    let context = constant_template_context(&token_stream.src_path, &declarations);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("docs-style table with many rows should parse");
    let structural_nodes = count_template_structure_nodes(&template);

    // Guard against composition regressions that re-introduce runaway structural growth.
    assert!(
        structural_nodes <= 3000,
        "unexpectedly large composed template structure: {structural_nodes} nodes for {row_count} rows"
    );
}

fn docs_style_table_and_data_declarations(string_table: &mut StringTable) -> Vec<Declaration> {
    let wrapper_scope = InternedPath::from_single_str("main.bst/#const_template0", string_table);

    let mut table_tokens = template_tokens_from_source(
        "[:\n    <table style=\"[$slot(\"style\") ]\">\n        <tr style=\"background-color: hsla(107, 100%, 36%, 0.23);\">\n            [$children([:\n                <th style=\"border: 1px solid; padding: 0.5em; text-align: left;\">[$slot]</th>\n            ]):[$slot(1)]]\n        </tr>\n        [$children([:<tr style=\"border-bottom: 1px dotted grey;\">[$slot]</tr>]):\n            [$slot]\n        ]\n    </table>\n]",
        string_table,
    );
    let table_context = new_constant_context(table_tokens.src_path.to_owned());
    let table = Template::new(&mut table_tokens, &table_context, vec![], string_table)
        .expect("docs-style table wrapper should parse");

    let mut data_tokens = template_tokens_from_source(
        "[$children([: <td style=\"padding: 0.2em 0.5em;\">[$slot]</td>]):\n    [$slot]\n]",
        string_table,
    );
    let data_context = new_constant_context(data_tokens.src_path.to_owned());
    let data = Template::new(&mut data_tokens, &data_context, vec![], string_table)
        .expect("docs-style data wrapper should parse");

    vec![
        Declaration {
            id: wrapper_scope.append(string_table.intern("table")),
            value: Expression::template(table, Ownership::ImmutableOwned),
        },
        Declaration {
            id: wrapper_scope.append(string_table.intern("data")),
            value: Expression::template(data, Ownership::ImmutableOwned),
        },
    ]
}

fn count_template_structure_nodes(template: &Template) -> usize {
    let mut total = template.content.atoms.len();

    for atom in &template.content.atoms {
        total += count_atom_structure_nodes(atom);
    }

    total
}

fn count_atom_structure_nodes(atom: &TemplateAtom) -> usize {
    match atom {
        TemplateAtom::Slot(slot) => {
            slot.applied_child_wrappers
                .iter()
                .map(count_template_structure_nodes)
                .sum::<usize>()
                + slot
                    .child_wrappers
                    .iter()
                    .map(count_template_structure_nodes)
                    .sum::<usize>()
        }
        TemplateAtom::Content(segment) => {
            let source_nodes = segment
                .source_child_template
                .as_ref()
                .map(|template| count_template_structure_nodes(template.as_ref()))
                .unwrap_or(0);
            let expression_nodes = match &segment.expression.kind {
                ExpressionKind::Template(template) => count_template_structure_nodes(template),
                _ => 0,
            };

            source_nodes + expression_nodes
        }
    }
}
