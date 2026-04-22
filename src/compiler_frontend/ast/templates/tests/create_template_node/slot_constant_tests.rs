use super::*;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext, TopLevelDeclarationIndex};
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{CharPosition, SourceLocation};
use std::rc::Rc;

#[test]
fn slot_wrappers_remain_compile_time_templates_until_filled() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[: before [$slot] after]", &mut string_table);
    let context = new_constant_context(token_stream.src_path.to_owned());

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("wrapper template should parse");

    assert!(matches!(template.kind, TemplateType::String));
    assert!(template.has_unresolved_slots());
    assert!(Expression::template(template, Ownership::ImmutableOwned).is_compile_time_constant());
}

#[test]
fn folding_nested_wrapper_constant_with_unfilled_named_slots_renders_empty_strings() {
    let mut string_table = StringTable::new();
    let scope = InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);

    let mut wrapper_tokens = template_tokens_from_source(
        "[:<link rel=\"icon\" href=\"[$slot(\"favicon\")]\"><style>[$slot(\"css\")]</style>]",
        &mut string_table,
    );
    let wrapper_context = new_constant_context(wrapper_tokens.src_path.to_owned());
    let wrapper = Template::new(
        &mut wrapper_tokens,
        &wrapper_context,
        vec![],
        &mut string_table,
    )
    .expect("wrapper template should parse");

    let declarations = vec![Declaration {
        id: scope.append(string_table.intern("header")),
        value: Expression::template(wrapper, Ownership::ImmutableOwned),
    }];

    let mut token_stream = template_tokens_from_source("[header]", &mut string_table);
    let context = constant_template_context(&token_stream.src_path, &declarations);
    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("template using wrapper constant should parse");

    let folded = fold_template_in_context(&template, &context, &mut string_table);
    let rendered = string_table.resolve(folded);

    assert!(rendered.contains("rel=\"icon\""));
    assert!(rendered.contains("href=\"\""));
    assert!(rendered.contains("<style></style>"));
    assert!(!rendered.contains("$slot("));
}

#[test]
fn wrapper_templates_with_runtime_references_are_not_compile_time_constants() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[value: before [$slot] after]", &mut string_table);
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
        },
        Declaration {
            id: scope.append(const_after),
            value: Expression::string_slice(
                string_table.intern("World!"),
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
        },
    ];

    let style_directives = frontend_test_style_directives();
    let context = with_test_path_context(
        ScopeContext::new(
            ContextKind::Constant,
            scope.to_owned(),
            Rc::new(TopLevelDeclarationIndex::new(declarations.clone())),
            HostRegistry::default(),
            vec![],
        ),
        &scope,
        &style_directives,
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
