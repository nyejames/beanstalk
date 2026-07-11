use super::*;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::template::TemplateSegmentOrigin;
use crate::compiler_frontend::ast::templates::tir::TemplateIrNodeKind;
use crate::compiler_frontend::compiler_messages::{
    DiagnosticPayload, InvalidTemplateDirectiveReason,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{CharPosition, SourceLocation};

#[test]
fn fresh_marks_template_to_skip_parent_child_wrappers() {
    let mut string_table = StringTable::new();
    let mut wrapper_tokens = template_tokens_from_source("[: inherited]", &mut string_table);
    let context = new_constant_context(wrapper_tokens.src_path.to_owned());
    let wrapper = Template::new(&mut wrapper_tokens, &context, vec![], &mut string_table)
        .expect("inherited wrapper should parse");
    let inherited_wrapper = wrapper
        .tir_reference
        .as_ref()
        .map(|reference| {
            TemplateWrapperReference::new(reference.root, reference.phase, reference.overlay_set_id)
        })
        .expect("inherited wrapper should have durable TIR authority");

    let mut token_stream =
        template_tokens_from_source("[$fresh, $md:\n# Hello\n]", &mut string_table);
    let context = new_constant_context(token_stream.src_path.to_owned());

    let template = Template::new(
        &mut token_stream,
        &context,
        vec![inherited_wrapper],
        &mut string_table,
    )
    .expect("template should parse");

    assert!(template.style.formatter.is_some());
    assert!(template.style.skip_parent_child_wrappers);
    assert!(template.child_wrappers.is_empty());
}

#[test]
fn stores_style_child_templates_from_children_directive() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[$children([:prefix]), : body]", &mut string_table);
    let context = new_constant_context(token_stream.src_path.to_owned());

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("template should parse");

    assert_eq!(template.child_wrappers.len(), 1);
}

#[test]
fn children_directive_classifies_foreign_slot_wrapper_from_registry() {
    let mut string_table = StringTable::new();
    let mut wrapper_tokens =
        template_tokens_from_source("[:before[$slot]after]", &mut string_table);
    let wrapper_context = new_constant_context(wrapper_tokens.src_path.to_owned());
    let wrapper = Template::new(
        &mut wrapper_tokens,
        &wrapper_context,
        vec![],
        &mut string_table,
    )
    .expect("slot wrapper should parse");
    let wrapper_reference = wrapper
        .tir_reference
        .as_ref()
        .expect("slot wrapper should retain its TIR identity")
        .clone();

    let wrapper_name = string_table.intern("wrapper");
    let declarations = vec![Declaration {
        id: wrapper_tokens.src_path.append(wrapper_name),
        value: Expression::template(wrapper, ValueMode::ImmutableOwned),
    }];

    let directive_store_id = wrapper_context
        .template_ir_registry
        .borrow_mut()
        .allocate_store();
    let directive_store = wrapper_context
        .template_ir_registry
        .borrow()
        .store_handle(directive_store_id)
        .expect("directive store should exist");

    let mut token_stream =
        template_tokens_from_source("[$children(wrapper): [: child]]", &mut string_table);
    let context = constant_template_context(&token_stream.src_path, &declarations)
        .with_template_ir_registry(
            Rc::clone(&wrapper_context.template_ir_registry),
            directive_store_id,
            directive_store,
        );

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("children directive should accept a registry-backed slot wrapper");

    let reference = template
        .child_wrappers
        .first()
        .expect("children directive should record the slot wrapper");

    assert!(reference.phase.is_at_least(TemplateTirPhase::Composed));
    assert_eq!(reference.root, wrapper_reference.root);
    assert_ne!(reference.root.store_id, context.template_ir_store_id);
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

    let wrapper_reference = template
        .child_wrappers
        .first()
        .expect("children directive should record one wrapper");

    let store = context.template_ir_store.borrow();
    let wrapper_id = wrapper_reference.root.template_id;
    let wrapper_tir = store
        .get_template(wrapper_id)
        .expect("normalized string wrapper TIR should exist in the module store");
    let root = store
        .get_node(wrapper_tir.root)
        .expect("normalized string wrapper root should exist");
    let TemplateIrNodeKind::Sequence { children } = &root.kind else {
        panic!("normalized string wrapper should have a sequence root");
    };
    let [literal_id] = children.as_slice() else {
        panic!("normalized string wrapper should contain one literal node");
    };
    let literal = store
        .get_node(*literal_id)
        .expect("normalized string wrapper literal should exist");
    let TemplateIrNodeKind::Text { text, origin, .. } = &literal.kind else {
        panic!("normalized string wrapper should contain literal TIR text");
    };

    assert_eq!(string_table.resolve(*text), "prefix: ");
    assert_eq!(*origin, TemplateSegmentOrigin::Body);
}

#[test]
fn children_directive_rejects_runtime_values() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[$children(value): [: child]]", &mut string_table);
    let context = runtime_template_context(&token_stream.src_path, &mut string_table);

    let error = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("children directive should reject runtime values");

    match &error.payload {
        DiagnosticPayload::InvalidTemplateDirective {
            reason: InvalidTemplateDirectiveReason::InvalidChildrenArgument,
            ..
        } => {}
        payload => panic!("expected invalid template directive payload, found {payload:?}"),
    }
}

#[test]
fn children_wrappers_are_applied_to_direct_children() {
    let rendered = folded_template_output("[$children([: pref[$slot]suf]): [: body]]");
    assert!(rendered.contains("pref"));
    assert!(rendered.contains("body"));
    assert!(rendered.contains("suf"));
}

#[test]
fn children_string_wrappers_preserve_prepend_semantics() {
    let rendered = folded_template_output("[$children(\"prefix: \"): [: child]]");

    let prefix_index = rendered.find("prefix: ").expect("wrapper should render");
    let child_index = rendered.find("child").expect("child should render");
    assert!(prefix_index < child_index);
}

#[test]
fn children_wrappers_do_not_apply_to_grandchildren() {
    let rendered = folded_template_output("[$children([:<wrap>[$slot]</wrap>]): [:[ : body]]]");

    assert!(rendered.contains("body"));
    assert_eq!(rendered.matches("<wrap>").count(), 1);
}

#[test]
fn markdown_is_not_inherited_by_grandchildren() {
    let rendered = folded_template_output("[$md:\n[: [: <b>grandchild-body</b> ]]\n]");

    assert!(rendered.contains("<b>grandchild-body</b>"));
    assert!(!rendered.contains("&lt;b&gt;grandchild-body&lt;/b&gt;"));
}

#[test]
fn markdown_must_be_redeclared_at_each_nested_template_level() {
    let rendered = folded_template_output("[$md:\n[$md:\n[$md: <b>grandchild-body</b>]\n]\n]");

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
