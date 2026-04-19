use super::*;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::template::TemplateType;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::datatypes::Ownership;
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{CharPosition, SourceLocation};

#[test]
fn fills_single_slot_templates_in_source_order() {
    let mut string_table = StringTable::new();
    let wrapper_scope =
        InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let mut wrapper_tokens =
        template_tokens_from_source("[: before [$slot] after]", &mut string_table);
    let wrapper_context = new_constant_context(wrapper_tokens.src_path.to_owned());
    let wrapper = Template::new(
        &mut wrapper_tokens,
        &wrapper_context,
        vec![],
        &mut string_table,
    )
    .expect("wrapper should parse");

    let declaration = Declaration {
        id: wrapper_scope.append(string_table.intern("single_slot")),
        value: Expression::template(wrapper, Ownership::ImmutableOwned),
    };

    let mut token_stream = template_tokens_from_source(
        "[single_slot: this content is now wrapped]",
        &mut string_table,
    );
    let context = constant_template_context(&token_stream.src_path, &[declaration]);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("slot application should parse");
    let folded = fold_template_in_context(&template, &context, &mut string_table);
    let rendered = string_table.resolve(folded);
    let before = rendered
        .find("before")
        .expect("wrapper prefix should exist");
    let wrapped = rendered
        .find("this content is now wrapped")
        .expect("inserted slot content should exist");
    let after = rendered.find("after").expect("wrapper suffix should exist");

    assert!(before < wrapped);
    assert!(wrapped < after);
}

#[test]
fn fills_multiple_named_slots_with_ordered_inserts() {
    let mut string_table = StringTable::new();
    let wrapper_scope =
        InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let mut wrapper_tokens = template_tokens_from_source(
        "[: before [$slot(\"first\")] in the middle [$slot(\"second\")] afterwards]",
        &mut string_table,
    );
    let wrapper_context = new_constant_context(wrapper_tokens.src_path.to_owned());
    let wrapper = Template::new(
        &mut wrapper_tokens,
        &wrapper_context,
        vec![],
        &mut string_table,
    )
    .expect("wrapper should parse");

    let declaration = Declaration {
        id: wrapper_scope.append(string_table.intern("basic_slots")),
        value: Expression::template(wrapper, Ownership::ImmutableOwned),
    };

    let mut token_stream = template_tokens_from_source(
        "[basic_slots:[$insert(\"first\"): This goes into the first slot][$insert(\"second\"): This goes into the second slot]]",
        &mut string_table,
    );
    let context = constant_template_context(&token_stream.src_path, &[declaration]);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("ordered slot application should parse");
    let folded = fold_template_in_context(&template, &context, &mut string_table);
    let rendered = string_table.resolve(folded);

    let first_slot = rendered
        .find("This goes into the first slot")
        .expect("first slot content should be present");
    let middle = rendered
        .find("in the middle")
        .expect("wrapper middle should be present");
    let second_slot = rendered
        .find("This goes into the second slot")
        .expect("second slot content should be present");

    assert!(first_slot < middle);
    assert!(middle < second_slot);
    assert!(rendered.contains("afterwards"));
}

#[test]
fn allows_explicitly_empty_named_slot_insertions() {
    let mut string_table = StringTable::new();
    let wrapper_scope =
        InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let mut wrapper_tokens = template_tokens_from_source(
        "[: before [$slot(\"first\")] in the middle [$slot(\"second\")] afterwards]",
        &mut string_table,
    );
    let wrapper_context = new_constant_context(wrapper_tokens.src_path.to_owned());
    let wrapper = Template::new(
        &mut wrapper_tokens,
        &wrapper_context,
        vec![],
        &mut string_table,
    )
    .expect("wrapper should parse");

    let declaration = Declaration {
        id: wrapper_scope.append(string_table.intern("basic_slots")),
        value: Expression::template(wrapper, Ownership::ImmutableOwned),
    };

    let mut token_stream = template_tokens_from_source(
        "[basic_slots:[$insert(\"first\"): first][$insert(\"second\")]]",
        &mut string_table,
    );
    let context = constant_template_context(&token_stream.src_path, &[declaration]);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("empty slot markers should still count as used");
    let folded = fold_template_in_context(&template, &context, &mut string_table);
    let rendered = string_table.resolve(folded);

    assert!(rendered.contains("first"));
    assert!(rendered.contains("in the middle"));
    assert!(rendered.contains("afterwards"));
}

#[test]
fn rejects_loose_content_for_named_only_slots_without_default() {
    let mut string_table = StringTable::new();
    let wrapper_scope =
        InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let mut wrapper_tokens =
        template_tokens_from_source("[: before [$slot(\"title\")] after]", &mut string_table);
    let wrapper_context = new_constant_context(wrapper_tokens.src_path.to_owned());
    let wrapper = Template::new(
        &mut wrapper_tokens,
        &wrapper_context,
        vec![],
        &mut string_table,
    )
    .expect("wrapper should parse");

    let declaration = Declaration {
        id: wrapper_scope.append(string_table.intern("named_only_slots")),
        value: Expression::template(wrapper, Ownership::ImmutableOwned),
    };

    let mut token_stream =
        template_tokens_from_source("[named_only_slots: loose content]", &mut string_table);
    let context = constant_template_context(&token_stream.src_path, &[declaration]);

    let error = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("named-only slots should reject loose content");

    assert!(error.msg.contains("Loose content is not allowed"));
}

#[test]
fn rejects_unknown_named_insert_targets() {
    let mut string_table = StringTable::new();
    let wrapper_scope =
        InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let mut wrapper_tokens =
        template_tokens_from_source("[: before [$slot(\"title\")] after]", &mut string_table);
    let wrapper_context = new_constant_context(wrapper_tokens.src_path.to_owned());
    let wrapper = Template::new(
        &mut wrapper_tokens,
        &wrapper_context,
        vec![],
        &mut string_table,
    )
    .expect("wrapper should parse");

    let declaration = Declaration {
        id: wrapper_scope.append(string_table.intern("named_only_slots")),
        value: Expression::template(wrapper, Ownership::ImmutableOwned),
    };

    let mut token_stream = template_tokens_from_source(
        "[named_only_slots:[$insert(\"missing\"): nope]]",
        &mut string_table,
    );
    let context = constant_template_context(&token_stream.src_path, &[declaration]);

    let error = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("unknown named inserts should fail");

    assert!(error.msg.contains("named slot that does not exist"));
}

#[test]
fn rejects_duplicate_default_slot_definitions() {
    let mut string_table = StringTable::new();
    let wrapper_scope =
        InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let mut wrapper_tokens =
        template_tokens_from_source("[: before [$slot] middle [$slot] after]", &mut string_table);
    let wrapper_context = new_constant_context(wrapper_tokens.src_path.to_owned());
    let wrapper = Template::new(
        &mut wrapper_tokens,
        &wrapper_context,
        vec![],
        &mut string_table,
    )
    .expect("wrapper should parse before composition");

    let declaration = Declaration {
        id: wrapper_scope.append(string_table.intern("duplicate_default")),
        value: Expression::template(wrapper, Ownership::ImmutableOwned),
    };

    let mut token_stream =
        template_tokens_from_source("[duplicate_default: content]", &mut string_table);
    let context = constant_template_context(&token_stream.src_path, &[declaration]);
    let error = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("duplicate default slots should fail when wrapper is composed");

    assert!(error.msg.contains("only define one default '$slot'"));
}

#[test]

fn rejects_insert_targeting_non_immediate_parent_slot() {
    let mut string_table = StringTable::new();
    let scope = InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);

    let mut outer_tokens =
        template_tokens_from_source("[: OUTER [$slot(\"outer\")] END]", &mut string_table);
    let outer_scope = outer_tokens.src_path.to_owned();
    let outer = Template::new(
        &mut outer_tokens,
        &new_constant_context(outer_scope),
        vec![],
        &mut string_table,
    )
    .expect("outer wrapper should parse");

    let mut inner_tokens =
        template_tokens_from_source("[: INNER [$slot(\"inner\")] END]", &mut string_table);
    let inner_scope = inner_tokens.src_path.to_owned();
    let inner = Template::new(
        &mut inner_tokens,
        &new_constant_context(inner_scope),
        vec![],
        &mut string_table,
    )
    .expect("inner wrapper should parse");

    let mut insert_tokens = template_tokens_from_source(
        "[$insert(\"outer\"): no-grandparent-matching]",
        &mut string_table,
    );
    let insert_scope = insert_tokens.src_path.to_owned();
    let outer_insert = Template::new(
        &mut insert_tokens,
        &new_constant_context(insert_scope),
        vec![],
        &mut string_table,
    )
    .expect("insert helper should parse");

    let declarations = vec![
        Declaration {
            id: scope.append(string_table.intern("outer_wrapper")),
            value: Expression::template(outer, Ownership::ImmutableOwned),
        },
        Declaration {
            id: scope.append(string_table.intern("inner_wrapper")),
            value: Expression::template(inner, Ownership::ImmutableOwned),
        },
        Declaration {
            id: scope.append(string_table.intern("outer_insert")),
            value: Expression::template(outer_insert, Ownership::ImmutableOwned),
        },
    ];

    let mut token_stream = template_tokens_from_source(
        "[outer_wrapper, inner_wrapper, outer_insert]",
        &mut string_table,
    );
    let context = constant_template_context(&token_stream.src_path, &declarations);
    let error = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("inserts should only target the immediate parent");

    assert!(error.msg.contains("does not exist on the immediate parent"));
}

#[test]
fn fills_nested_slots_in_parent_authored_order() {
    let mut string_table = StringTable::new();
    let wrapper_scope =
        InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let mut wrapper_tokens = template_tokens_from_source(
        "[: outer [: inner [$slot(\"first\")] middle [$slot] [: deep [$slot(\"second\")] end] tail] after]",
        &mut string_table,
    );
    let wrapper_context = new_constant_context(wrapper_tokens.src_path.to_owned());
    let wrapper = Template::new(
        &mut wrapper_tokens,
        &wrapper_context,
        vec![],
        &mut string_table,
    )
    .expect("nested wrapper should parse");

    let declaration = Declaration {
        id: wrapper_scope.append(string_table.intern("nested_slots")),
        value: Expression::template(wrapper, Ownership::ImmutableOwned),
    };

    let mut token_stream = template_tokens_from_source(
        "[nested_slots: [$insert(\"first\"): first slot] in between [$insert(\"second\"): second slot]]",
        &mut string_table,
    );
    let context = constant_template_context(&token_stream.src_path, &[declaration]);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("nested slot application should parse");
    let folded = fold_template_in_context(&template, &context, &mut string_table);
    let rendered = string_table.resolve(folded);

    let first_slot = rendered
        .find("first slot")
        .expect("first slot content should be present");
    let between = rendered
        .find("in between")
        .expect("gap content should be present");
    let second_slot = rendered
        .find("second slot")
        .expect("second slot content should be present");
    let deep = rendered
        .find("deep")
        .expect("nested wrapper text should be present");
    let end = rendered
        .find("end")
        .expect("nested wrapper text should be present");

    assert!(first_slot < between);
    assert!(between < second_slot);
    assert!(deep < second_slot);
    assert!(second_slot < end);
}

#[test]
fn fills_nested_slots_for_runtime_wrappers() {
    let mut string_table = StringTable::new();
    let scope = InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let value_name = string_table.intern("value");
    let value_declaration = Declaration {
        id: scope.append(value_name),
        value: Expression::string_slice(
            string_table.intern("runtime"),
            SourceLocation {
                scope: InternedPath::new(),
                start_pos: CharPosition {
                    line_number: 1,
                    char_column: 0,
                },
                end_pos: CharPosition {
                    line_number: 1,
                    char_column: 120, // Arbitrary number
                },
            },
            Ownership::ImmutableOwned,
        ),
    };

    let wrapper_context = ScopeContext::new(
        ContextKind::Template,
        scope.to_owned(),
        Rc::new(vec![value_declaration.to_owned()]),
        HostRegistry::default(),
        vec![],
    );
    let mut wrapper_tokens = template_tokens_from_source(
        "[value: outer [: inner [$slot(\"first\")] middle [$slot] [: deep [$slot(\"second\")] end] tail] after]",
        &mut string_table,
    );
    let wrapper = Template::new(
        &mut wrapper_tokens,
        &wrapper_context,
        vec![],
        &mut string_table,
    )
    .expect("runtime nested wrapper should parse");
    assert!(matches!(wrapper.kind, TemplateType::StringFunction));

    let wrapper_declaration = Declaration {
        id: scope.append(string_table.intern("runtime_wrapper")),
        value: Expression::template(wrapper, Ownership::ImmutableOwned),
    };
    let declarations = vec![value_declaration, wrapper_declaration];
    let consuming_context = ScopeContext::new(
        ContextKind::Template,
        scope,
        Rc::new(declarations),
        HostRegistry::default(),
        vec![],
    );
    let mut token_stream = template_tokens_from_source(
        "[runtime_wrapper: [$insert(\"first\"): first slot] in between [$insert(\"second\"): second slot]]",
        &mut string_table,
    );

    let template = Template::new(
        &mut token_stream,
        &consuming_context,
        vec![],
        &mut string_table,
    )
    .expect("runtime wrapper slot application should parse");
    assert!(matches!(template.kind, TemplateType::StringFunction));
    assert!(!template.has_unresolved_slots());

    let rendered = render_static_template_fragments(&template, &string_table);
    let first_slot = rendered
        .find("first slot")
        .expect("first slot content should be present");
    let between = rendered
        .find("in between")
        .expect("gap content should be present");
    let second_slot = rendered
        .find("second slot")
        .expect("second slot content should be present");
    let deep = rendered
        .find("deep")
        .expect("nested wrapper text should be present");
    let end = rendered
        .find("end")
        .expect("nested wrapper text should be present");

    assert!(first_slot < between);
    assert!(between < second_slot);
    assert!(deep < second_slot);
    assert!(second_slot < end);
}

#[test]
fn template_with_slot_and_insert_contributes_upward_after_receiving_content() {
    let mut string_table = StringTable::new();
    let scope = InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);

    let mut page_tokens = template_tokens_from_source(
        "[: <h1 style=\"[$slot(\"style\") ]\">[$slot]</h1>]",
        &mut string_table,
    );
    let page_scope = page_tokens.src_path.to_owned();
    let page = Template::new(
        &mut page_tokens,
        &new_constant_context(page_scope),
        vec![],
        &mut string_table,
    )
    .expect("page wrapper should parse");

    let mut style_tokens = template_tokens_from_source(
        "[: [$insert(\"style\"): color: blue;] <em>[$slot]</em>]",
        &mut string_table,
    );
    let style_scope = style_tokens.src_path.to_owned();
    let style_wrapper = Template::new(
        &mut style_tokens,
        &new_constant_context(style_scope),
        vec![],
        &mut string_table,
    )
    .expect("style contributor wrapper should parse");

    let declarations = vec![
        Declaration {
            id: scope.append(string_table.intern("page")),
            value: Expression::template(page, Ownership::ImmutableOwned),
        },
        Declaration {
            id: scope.append(string_table.intern("blue")),
            value: Expression::template(style_wrapper, Ownership::ImmutableOwned),
        },
    ];

    let mut token_stream = template_tokens_from_source("[page, blue: Hello]", &mut string_table);
    let context = constant_template_context(&token_stream.src_path, &declarations);
    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("composed template should parse");
    let folded = fold_template_in_context(&template, &context, &mut string_table);
    let rendered = string_table.resolve(folded);

    assert!(rendered.contains("color: blue;"));
    assert!(rendered.contains("<em>"));
    assert!(rendered.contains("Hello"));
}
