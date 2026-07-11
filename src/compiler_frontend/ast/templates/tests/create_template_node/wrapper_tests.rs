use super::*;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::template::TemplateAtom;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::value_mode::ValueMode;

#[test]
fn docs_style_data_wrapper_keeps_ast_structure_bounded_for_many_rows() {
    let mut string_table = StringTable::new();
    let declarations = docs_style_table_and_data_declarations(&mut string_table);

    let row_count = 48usize;
    let mut source =
        String::from("[table:\n    [header_row: [: Operator] [: Description] [: Precedence] ]\n");
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

    let mut header_row_tokens = template_tokens_from_source(
        "[$children([:\n            <th style=\"border: 1px solid; padding: 0.5em; text-align: left;\">[$slot]</th>\n        ]):[$slot]]",
        string_table,
    );
    let header_row_context = new_constant_context(header_row_tokens.src_path.to_owned());
    let header_row = Template::new(
        &mut header_row_tokens,
        &header_row_context,
        vec![],
        string_table,
    )
    .expect("docs-style header row wrapper should parse");

    let mut table_tokens = template_tokens_from_source(
        "[:\n    <table style=\"[$slot(\"style\") ]\">\n        <tr style=\"background-color: hsla(107, 100%, 36%, 0.23);\">\n            [$slot(1)]\n        </tr>\n        [$children([:<tr style=\"border-bottom: 1px dotted grey;\">[$slot]</tr>]):\n            [$slot]\n        ]\n    </table>\n]",
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
            id: wrapper_scope.append(string_table.intern("header_row")),
            value: Expression::template(header_row, ValueMode::ImmutableOwned),
        },
        Declaration {
            id: wrapper_scope.append(string_table.intern("table")),
            value: Expression::template(table, ValueMode::ImmutableOwned),
        },
        Declaration {
            id: wrapper_scope.append(string_table.intern("data")),
            value: Expression::template(data, ValueMode::ImmutableOwned),
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
        TemplateAtom::Content(segment) => match &segment.expression.kind {
            ExpressionKind::Template(template) => count_template_structure_nodes(template),
            _ => 0,
        },
    }
}

#[test]
fn child_wrapper_composition_marks_template_tir_reference_composed() {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(
        "[$children([:<b>[$slot]</b>]): hello [:child] ]",
        &mut string_table,
    );
    let context = new_constant_context(token_stream.src_path.to_owned());

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("child-wrapper composition should parse");

    let reference = template
        .tir_reference
        .as_ref()
        .expect("child-wrapper composition should produce a TIR reference");

    assert!(
        reference.is_composed,
        "child-wrapper composition must mark the template's TIR reference as composed"
    );
    assert!(
        reference.phase.is_at_least(TemplateTirPhase::Formatted),
        "wrapper-only composition with TIR-normalized wrappers should reach Formatted through the TIR formatter view"
    );

    let store_owner = context.template_ir_store().borrow().owner();
    assert!(
        std::sync::Arc::ptr_eq(&reference.store_owner, &store_owner),
        "composed reference must carry the same-store owner"
    );

    let registry = context.template_ir_registry.borrow();
    let overlay_set = registry
        .overlay_set(reference.overlay_set_id)
        .expect("composed reference overlay set should resolve in the registry");

    assert!(
        overlay_set.wrapper_context.is_some(),
        "direct-child wrappers must thread a wrapper-context overlay"
    );
    assert!(
        overlay_set.slot_resolution.is_none(),
        "direct-child wrapper removal must not leave structural slot-resolution composition"
    );
}
