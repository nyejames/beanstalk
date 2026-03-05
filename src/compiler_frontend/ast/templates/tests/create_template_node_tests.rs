use super::*;
use crate::compiler_frontend::ast::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::ast::templates::code::{
    CodeLanguage, code_formatter, highlight_code_html,
};
use crate::compiler_frontend::ast::templates::markdown::markdown_formatter;
use crate::compiler_frontend::ast::templates::template::{
    TemplateAtom, TemplateSegment, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokenizer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TextLocation, Token, TokenKind};

fn token(kind: TokenKind, line: i32) -> Token {
    Token::new(kind, TextLocation::new_just_line(line))
}

fn template_tokens_from_source(source: &str, string_table: &mut StringTable) -> FileTokens {
    let scope = InternedPath::from_single_str("main.bst/#const_template0", string_table);
    let mut tokens = tokenize(
        source,
        &scope,
        crate::compiler_frontend::tokenizer::tokens::TokenizeMode::Normal,
        string_table,
    )
    .expect("tokenization should succeed");

    tokens.index = tokens
        .tokens
        .iter()
        .position(|token| {
            matches!(
                token.kind,
                TokenKind::TemplateHead | TokenKind::StyleTemplateHead
            )
        })
        .expect("expected a template opener");

    tokens
}

fn runtime_template_context(scope: &InternedPath, string_table: &mut StringTable) -> ScopeContext {
    let value_name = string_table.intern("value");
    let declaration = Declaration {
        id: scope.append(value_name),
        value: Expression::string_slice(
            string_table.intern("dynamic"),
            TextLocation::new_just_line(1),
            Ownership::ImmutableOwned,
        ),
    };

    ScopeContext::new(
        ContextKind::Template,
        scope.to_owned(),
        &[declaration],
        HostRegistry::default(),
        vec![],
    )
}

fn constant_template_context(scope: &InternedPath, declarations: &[Declaration]) -> ScopeContext {
    ScopeContext::new(
        ContextKind::Constant,
        scope.to_owned(),
        declarations,
        HostRegistry::default(),
        vec![],
    )
}

fn folded_template_output(source: &str) -> String {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(source, &mut string_table);
    let context = ScopeContext::new_constant(token_stream.src_path.to_owned());

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("template should parse");
    let folded = template
        .fold_into_stringid(&None, &mut string_table)
        .expect("folding should succeed");

    string_table.resolve(folded).to_owned()
}

fn template_parse_error(source: &str) -> String {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(source, &mut string_table);
    let context = ScopeContext::new_constant(token_stream.src_path.to_owned());

    Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("template should fail to parse")
        .msg
}

fn template_segments(template: &Template) -> Vec<&TemplateSegment> {
    template
        .content
        .atoms
        .iter()
        .filter_map(|atom| match atom {
            TemplateAtom::Content(segment) => Some(segment),
            TemplateAtom::Slot => None,
        })
        .collect()
}

fn collect_static_template_fragments(
    atoms: &[TemplateAtom],
    string_table: &StringTable,
    output: &mut String,
) {
    for atom in atoms {
        let TemplateAtom::Content(segment) = atom else {
            continue;
        };

        match &segment.expression.kind {
            ExpressionKind::StringSlice(value) => output.push_str(string_table.resolve(*value)),
            ExpressionKind::Template(template) => {
                collect_static_template_fragments(&template.content.atoms, string_table, output)
            }
            _ => {}
        }
    }
}

fn render_static_template_fragments(template: &Template, string_table: &StringTable) -> String {
    let mut rendered = String::new();
    collect_static_template_fragments(&template.content.atoms, string_table, &mut rendered);
    rendered
}

#[test]
fn parse_template_head_handles_truncated_stream_without_panicking() {
    let mut string_table = StringTable::new();
    let scope = InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let context = ScopeContext::new_constant(scope.to_owned());

    let mut token_stream = FileTokens::new(
        scope,
        vec![
            token(TokenKind::TemplateHead, 1),
            token(TokenKind::IntLiteral(3), 1),
        ],
    );

    let result = Template::new(&mut token_stream, &context, vec![], &mut string_table);
    assert!(
        result.is_ok(),
        "truncated template-head streams should not panic the parser"
    );
}

#[test]
fn single_item_template_head_with_close_is_foldable() {
    let mut string_table = StringTable::new();
    let scope = InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let context = ScopeContext::new_constant(scope.to_owned());

    let mut token_stream = FileTokens::new(
        scope,
        vec![
            token(TokenKind::TemplateHead, 1),
            token(TokenKind::IntLiteral(3), 1),
            token(TokenKind::TemplateClose, 1),
            token(TokenKind::Eof, 1),
        ],
    );

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("single-item head template should parse");

    assert!(matches!(template.kind, TemplateType::String));
    let folded = template
        .fold_into_stringid(&None, &mut string_table)
        .expect("folding should succeed");
    assert_eq!(string_table.resolve(folded), "3");
}

#[test]
fn markdown_formats_only_template_body_content() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[\"prefix\", $markdown:\n# Hello\n]", &mut string_table);
    let context = ScopeContext::new_constant(token_stream.src_path.to_owned());

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("template should parse");

    assert!(matches!(template.kind, TemplateType::String));
    let folded = template
        .fold_into_stringid(&None, &mut string_table)
        .expect("folding should succeed");
    let rendered = string_table.resolve(folded);

    assert!(rendered.starts_with("prefix"));
    assert!(rendered.contains("<h1>Hello</h1>"));
    assert!(!rendered.starts_with("<p>prefix"));
}

#[test]
fn runtime_templates_format_static_body_strings_only() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[value, $markdown:\n# Hello\n]", &mut string_table);
    let context = runtime_template_context(&token_stream.src_path, &mut string_table);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("template should parse");

    assert!(matches!(template.kind, TemplateType::StringFunction));
    assert!(template_segments(&template).iter().any(|segment| {
        segment.origin == TemplateSegmentOrigin::Head
            && matches!(segment.expression.kind, ExpressionKind::Reference(_))
    }));

    let formatted_body = template_segments(&template)
        .into_iter()
        .find_map(
            |segment| match (&segment.origin, &segment.expression.kind) {
                (TemplateSegmentOrigin::Body, ExpressionKind::StringSlice(text)) => Some(*text),
                _ => None,
            },
        )
        .expect("expected a formatted body segment");

    assert!(
        string_table
            .resolve(formatted_body)
            .contains("<h1>Hello</h1>")
    );
}

#[test]
fn ignore_clears_inherited_style_before_reapplying_markdown() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[$ignore, $markdown:\n# Hello\n]", &mut string_table);
    let context = ScopeContext::new_constant(token_stream.src_path.to_owned());

    let mut inherited = Template::create_default(vec![]);
    inherited.style.formatter = Some(markdown_formatter());
    inherited.style.formatter_precedence = 0;
    inherited
        .style
        .child_templates
        .push(Template::create_default(vec![]));

    let template = Template::new(
        &mut token_stream,
        &context,
        vec![inherited],
        &mut string_table,
    )
    .expect("template should parse");

    assert!(template.style.formatter.is_some());
    assert!(template.style.child_templates.is_empty());
}

#[test]
fn stores_style_child_templates_from_dollar_bracket_syntax() {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source("[$[:prefix], : body]", &mut string_table);
    let context = ScopeContext::new_constant(token_stream.src_path.to_owned());

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("template should parse");

    assert_eq!(template.style.child_templates.len(), 1);
}

#[test]
fn formatter_directive_errors_until_implemented() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[$formatter(markdown, 10): body]", &mut string_table);
    let context = ScopeContext::new_constant(token_stream.src_path.to_owned());

    let error = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("$formatter should not be implemented yet");

    assert!(error.msg.contains("$formatter"));
    assert!(error.msg.contains("not implemented yet"));
}

#[test]
fn unknown_style_directives_error_cleanly() {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source("[$unknown: body]", &mut string_table);
    let context = ScopeContext::new_constant(token_stream.src_path.to_owned());

    let error = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("unknown directives should fail");

    assert!(error.msg.contains("Unsupported style directive"));
    assert!(error.msg.contains("$unknown"));
}

#[test]
fn code_without_argument_uses_generic_highlighting() {
    let rendered = folded_template_output("[$code:\nloop(x + 1)\n]");

    assert!(rendered.contains("<code class='codeblock'>"));
    assert!(rendered.contains("<span class='bs-code-parenthesis'>(</span>"));
    assert!(!rendered.contains("bs-code-keyword"));
}

#[test]
fn code_bst_argument_highlights_beanstalk_rules() {
    let rendered = folded_template_output("[$code(\"bst\"):\nloop x\n-- hi\n]");

    assert!(rendered.contains("<span class='bs-code-keyword'>loop</span>"));
    assert!(rendered.contains("<span class='bs-code-comment'>-- hi</span>"));
}

#[test]
fn code_javascript_argument_highlights_js_comments() {
    let rendered = folded_template_output("[$code(\"js\"):\nconst x = 1\n// hi\n]");

    assert!(rendered.contains("<span class='bs-code-keyword'>const</span>"));
    assert!(rendered.contains("<span class='bs-code-comment'>// hi</span>"));
}

#[test]
fn code_python_argument_highlights_python_comments() {
    let rendered = folded_template_output("[$code(\"py\"):\ndef run():\n# hi\n]");

    assert!(rendered.contains("<span class='bs-code-keyword'>def</span>"));
    assert!(rendered.contains("<span class='bs-code-comment'># hi</span>"));
}

#[test]
fn code_typescript_argument_highlights_typescript_types() {
    let rendered = folded_template_output("[$code(\"ts\"):\ntype Name = string\n]");

    assert!(rendered.contains("<span class='bs-code-keyword'>type</span>"));
    assert!(rendered.contains("<span class='bs-code-type'>string</span>"));
}

#[test]
fn code_empty_parentheses_error_cleanly() {
    let error = template_parse_error("[$code(): body]");

    assert!(error.contains("$code()"));
    assert!(error.contains("generic highlighting"));
}

#[test]
fn code_requires_a_quoted_string_literal_argument() {
    let error = template_parse_error("[$code(lang): body]");

    assert!(error.contains("quoted string literal"));
}

#[test]
fn code_rejects_unknown_language_aliases() {
    let error = template_parse_error("[$code(\"unknown\"): body]");

    assert!(error.contains("Unsupported '$code(...)' language"));
    assert!(error.contains("\"unknown\""));
}

#[test]
fn code_rejects_multiple_language_arguments() {
    let error = template_parse_error("[$code(\"bst\", \"js\"): body]");

    assert!(error.contains("only one language argument"));
}

#[test]
fn runtime_templates_with_code_format_only_static_body_strings() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[value, $code(\"bst\"):\nloop x\n]", &mut string_table);
    let context = runtime_template_context(&token_stream.src_path, &mut string_table);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("template should parse");

    assert!(template_segments(&template).iter().any(|segment| {
        segment.origin == TemplateSegmentOrigin::Head
            && matches!(segment.expression.kind, ExpressionKind::Reference(_))
    }));

    let formatted_body = template_segments(&template)
        .into_iter()
        .find_map(
            |segment| match (&segment.origin, &segment.expression.kind) {
                (TemplateSegmentOrigin::Body, ExpressionKind::StringSlice(text)) => Some(*text),
                _ => None,
            },
        )
        .expect("expected a formatted body segment");

    let rendered = string_table.resolve(formatted_body);
    assert!(rendered.contains("<code class='codeblock'>"));
    assert!(rendered.contains("<span class='bs-code-keyword'>loop</span>"));
}

#[test]
fn slot_wrappers_remain_compile_time_templates_until_filled() {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source("[: before [..] after]", &mut string_table);
    let context = ScopeContext::new_constant(token_stream.src_path.to_owned());

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("wrapper template should parse");

    assert!(matches!(template.kind, TemplateType::String));
    assert!(template.has_unresolved_slots());
    assert!(Expression::template(template, Ownership::ImmutableOwned).is_compile_time_constant());
}

#[test]
fn wrapper_templates_with_runtime_references_are_not_compile_time_constants() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[value: before [..] after]", &mut string_table);
    let context = runtime_template_context(&token_stream.src_path, &mut string_table);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("runtime wrapper template should parse");

    assert!(matches!(template.kind, TemplateType::StringFunction));
    assert!(template.has_unresolved_slots());
    assert!(!Expression::template(template, Ownership::ImmutableOwned).is_compile_time_constant());
}

#[test]
fn constant_context_template_head_with_constant_references_folds_to_string_slice() {
    let mut string_table = StringTable::new();
    let scope = InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let const_before = string_table.intern("const_before");
    let const_after = string_table.intern("const_after");
    let declarations = vec![
        Declaration {
            id: scope.append(const_before),
            value: Expression::string_slice(
                string_table.intern("Hello "),
                TextLocation::new_just_line(1),
                Ownership::ImmutableOwned,
            ),
        },
        Declaration {
            id: scope.append(const_after),
            value: Expression::string_slice(
                string_table.intern("World!"),
                TextLocation::new_just_line(1),
                Ownership::ImmutableOwned,
            ),
        },
    ];

    let context = ScopeContext::new(
        ContextKind::Constant,
        scope,
        &declarations,
        HostRegistry::default(),
        vec![],
    );
    let mut token_stream =
        template_tokens_from_source("[const_before, const_after]", &mut string_table);
    let mut expected_type = DataType::Inferred;

    let expression = create_expression(
        &mut token_stream,
        &context,
        &mut expected_type,
        &Ownership::ImmutableOwned,
        false,
        &mut string_table,
    )
    .expect("constant template references should fold");

    let ExpressionKind::StringSlice(value) = expression.kind else {
        panic!("expected folded StringSlice expression in constant context");
    };

    assert_eq!(string_table.resolve(value), "Hello World!");
}

#[test]
fn non_constant_context_template_head_keeps_runtime_template() {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source("[value]", &mut string_table);
    let context = runtime_template_context(&token_stream.src_path, &mut string_table);
    let mut expected_type = DataType::Inferred;

    let expression = create_expression(
        &mut token_stream,
        &context,
        &mut expected_type,
        &Ownership::ImmutableOwned,
        false,
        &mut string_table,
    )
    .expect("runtime template expression should parse");

    let ExpressionKind::Template(template) = expression.kind else {
        panic!("expected runtime template expression");
    };

    assert!(matches!(template.kind, TemplateType::StringFunction));
}

#[test]
fn fills_single_slot_templates_in_source_order() {
    let mut string_table = StringTable::new();
    let wrapper_scope =
        InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let mut wrapper_tokens =
        template_tokens_from_source("[: before [..] after]", &mut string_table);
    let wrapper_context = ScopeContext::new_constant(wrapper_tokens.src_path.to_owned());
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
    let folded = template
        .fold_into_stringid(&None, &mut string_table)
        .expect("filled template should fold");

    assert_eq!(
        string_table.resolve(folded),
        " before  this content is now wrapped after"
    );
}

#[test]
fn fills_multiple_slots_with_ordered_labels() {
    let mut string_table = StringTable::new();
    let wrapper_scope =
        InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let mut wrapper_tokens = template_tokens_from_source(
        "[: before [..] in the middle [..] afterwards]",
        &mut string_table,
    );
    let wrapper_context = ScopeContext::new_constant(wrapper_tokens.src_path.to_owned());
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
        "[basic_slots:\n    this text goes before any slots\n    [$1: This goes into the first slot]\n    this would go in between the slots\n    [$2: This goes into the second slot]\n    this would go after any slots\n]",
        &mut string_table,
    );
    let context = constant_template_context(&token_stream.src_path, &[declaration]);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("ordered slot application should parse");
    let folded = template
        .fold_into_stringid(&None, &mut string_table)
        .expect("filled template should fold");
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
    assert!(rendered.contains("this would go after any slots"));
}

#[test]
fn allows_explicitly_empty_slots() {
    let mut string_table = StringTable::new();
    let wrapper_scope =
        InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let mut wrapper_tokens = template_tokens_from_source(
        "[: before [..] in the middle [..] afterwards]",
        &mut string_table,
    );
    let wrapper_context = ScopeContext::new_constant(wrapper_tokens.src_path.to_owned());
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

    let mut token_stream =
        template_tokens_from_source("[basic_slots: [$1: first][$2]]", &mut string_table);
    let context = constant_template_context(&token_stream.src_path, &[declaration]);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("empty slot markers should still count as used");
    let folded = template
        .fold_into_stringid(&None, &mut string_table)
        .expect("filled template should fold");
    let rendered = string_table.resolve(folded);

    assert!(rendered.contains("first"));
    assert!(rendered.contains("in the middle"));
    assert!(rendered.contains("afterwards"));
}

#[test]
fn rejects_missing_required_slot_use() {
    let mut string_table = StringTable::new();
    let wrapper_scope =
        InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let mut wrapper_tokens = template_tokens_from_source(
        "[: before [..] in the middle [..] afterwards]",
        &mut string_table,
    );
    let wrapper_context = ScopeContext::new_constant(wrapper_tokens.src_path.to_owned());
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

    let mut token_stream =
        template_tokens_from_source("[basic_slots: [$1: only first]]", &mut string_table);
    let context = constant_template_context(&token_stream.src_path, &[declaration]);

    let error = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("missing slot usage should fail");

    assert!(error.msg.contains("must be used exactly once"));
}

#[test]
fn rejects_out_of_order_labeled_slots() {
    let mut string_table = StringTable::new();
    let wrapper_scope =
        InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let mut wrapper_tokens = template_tokens_from_source(
        "[: before [..] in the middle [..] afterwards]",
        &mut string_table,
    );
    let wrapper_context = ScopeContext::new_constant(wrapper_tokens.src_path.to_owned());
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

    let mut token_stream =
        template_tokens_from_source("[basic_slots: [$2: second] [$1: first]]", &mut string_table);
    let context = constant_template_context(&token_stream.src_path, &[declaration]);

    let error = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("out-of-order slots should fail");

    assert!(error.msg.contains("out of order"));
}

#[test]
fn fills_nested_slots_in_parent_authored_order() {
    let mut string_table = StringTable::new();
    let wrapper_scope =
        InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let mut wrapper_tokens = template_tokens_from_source(
        "[: outer [: inner [..] middle [: deep [..] end] tail] after]",
        &mut string_table,
    );
    let wrapper_context = ScopeContext::new_constant(wrapper_tokens.src_path.to_owned());
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
        "[nested_slots: [$1: first slot] in between [$2: second slot]]",
        &mut string_table,
    );
    let context = constant_template_context(&token_stream.src_path, &[declaration]);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("nested slot application should parse");
    let folded = template
        .fold_into_stringid(&None, &mut string_table)
        .expect("nested slot template should fold");
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
            TextLocation::new_just_line(1),
            Ownership::ImmutableOwned,
        ),
    };

    let wrapper_context = ScopeContext::new(
        ContextKind::Template,
        scope.to_owned(),
        &[value_declaration.to_owned()],
        HostRegistry::default(),
        vec![],
    );
    let mut wrapper_tokens = template_tokens_from_source(
        "[value: outer [: inner [..] middle [: deep [..] end] tail] after]",
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
        &declarations,
        HostRegistry::default(),
        vec![],
    );
    let mut token_stream = template_tokens_from_source(
        "[runtime_wrapper: [$1: first slot] in between [$2: second slot]]",
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
fn generic_code_highlighter_marks_syntax_but_not_keywords() {
    let highlighted = highlight_code_html("loop(x + 1)", CodeLanguage::Generic);

    assert!(highlighted.contains("<span class='bs-code-parenthesis'>(</span>"));
    assert!(highlighted.contains("<span class='bs-code-operator'>+</span>"));
    assert!(highlighted.contains("<span class='bs-code-number'>1</span>"));
    assert!(!highlighted.contains("bs-code-keyword"));
    assert!(highlighted.contains("loop"));
}

#[test]
fn direct_beanstalk_highlighter_marks_comments_and_keywords() {
    let highlighted = highlight_code_html("loop x\n-- hi", CodeLanguage::Beanstalk);

    assert!(highlighted.contains("<span class='bs-code-keyword'>loop</span>"));
    assert!(highlighted.contains("<span class='bs-code-comment'>-- hi</span>"));
}

#[test]
fn direct_javascript_highlighter_marks_line_comments() {
    let highlighted = highlight_code_html("const x = 1\n// hi", CodeLanguage::JavaScript);

    assert!(highlighted.contains("<span class='bs-code-keyword'>const</span>"));
    assert!(highlighted.contains("<span class='bs-code-comment'>// hi</span>"));
}

#[test]
fn direct_python_highlighter_marks_hash_comments() {
    let highlighted = highlight_code_html("def run():\n# hi", CodeLanguage::Python);

    assert!(highlighted.contains("<span class='bs-code-keyword'>def</span>"));
    assert!(highlighted.contains("<span class='bs-code-comment'># hi</span>"));
}

#[test]
fn direct_typescript_highlighter_marks_type_keywords() {
    let highlighted = highlight_code_html("type Name = string", CodeLanguage::TypeScript);

    assert!(highlighted.contains("<span class='bs-code-keyword'>type</span>"));
    assert!(highlighted.contains("<span class='bs-code-type'>string</span>"));
}

#[test]
fn direct_code_highlighter_preserves_trailing_words_at_eof() {
    let highlighted = highlight_code_html("value", CodeLanguage::Generic);

    assert!(highlighted.ends_with("value"));
}

#[test]
fn direct_code_highlighter_preserves_single_quoted_strings() {
    let highlighted = highlight_code_html("'value'", CodeLanguage::Generic);

    assert!(highlighted.contains("&#39;value&#39;"));
}

#[test]
fn direct_code_highlighter_escapes_html_sensitive_content() {
    let highlighted = highlight_code_html("<tag>", CodeLanguage::Generic);

    assert!(highlighted.contains("&lt;"));
    assert!(highlighted.contains("tag"));
    assert!(highlighted.contains("&gt;"));
    assert!(!highlighted.contains("<tag>"));
}

#[test]
fn code_formatter_wrapper_preserves_newlines_after_dedent() {
    let formatter = code_formatter(CodeLanguage::Generic);
    let mut content = String::from("    x\n    y");

    formatter.formatter.format(&mut content);

    assert!(content.starts_with("<code class='codeblock'>"));
    assert!(content.ends_with("</code>"));
    assert!(content.contains("x\ny"));
}
