use super::*;
use crate::compiler_frontend::ast::expressions::expression_types::ConstRecordState;
use crate::compiler_frontend::ast::statements::match_patterns::MatchPattern;
use crate::compiler_frontend::ast::templates::template::{
    TemplateConstValueKind, TemplateContent, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_body_parser::{
    NestedTemplateParseOptions, TemplateBodyParseRequest, parse_template_body,
};
use crate::compiler_frontend::ast::templates::template_body_sentinels::TemplateBodyControlContext;
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateAggregatePiece, TemplateAggregateRenderPlan, TemplateBranchChain,
    TemplateBranchSelector, TemplateConditionalBranch, TemplateControlFlow,
    TemplateControlFlowValidationMode, TemplateFallbackBranch, TemplateLoopHeader,
    validate_const_required_template_control_flow,
};
use crate::compiler_frontend::ast::templates::template_head_parser::parse_template_head;
use crate::compiler_frontend::ast::templates::template_render_plan::RenderPiece;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_messages::{
    DiagnosticPayload, InvalidTemplateStructureReason,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::definitions::{FieldDefinition, StructTypeDefinition};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::{NominalTypeId, TypeId, builtin_type_ids};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::compatibility::TypeCompatibilityCache;
use rustc_hash::{FxHashMap, FxHashSet};

#[test]
fn truncated_template_head_stream_returns_missing_closing_delimiter() {
    let mut string_table = StringTable::new();
    let scope = InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let context = new_constant_context(scope.to_owned());

    let mut token_stream = FileTokens::new(
        scope,
        vec![
            token(TokenKind::TemplateHead, 1),
            token(TokenKind::IntLiteral(3), 1),
        ],
    );

    let result = Template::new(&mut token_stream, &context, vec![], &mut string_table);
    assert!(
        result.is_err(),
        "truncated template-head stream without closing delimiter should produce an error"
    );
}

#[test]
fn single_item_template_head_with_close_is_foldable() {
    let mut string_table = StringTable::new();
    let scope = InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let context = new_constant_context(scope.to_owned());

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
    let folded = fold_template_in_context(&template, &context, &mut string_table);
    assert_eq!(string_table.resolve(folded), "3");
}

#[test]
fn const_required_template_head_folds_const_record_instance_field() {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source("[html_defaults.color]", &mut string_table);
    let scope = token_stream.src_path.clone();

    let mut type_environment = TypeEnvironment::new();
    let string_type_id = type_environment.builtins().string;
    let struct_name = string_table.intern("HtmlDefaults");
    let field_name = string_table.intern("color");
    let struct_path = scope.append(struct_name);
    let field_path = struct_path.append(field_name);
    let (_, struct_type_id) = type_environment.register_nominal_struct(StructTypeDefinition {
        id: NominalTypeId(0),
        path: struct_path.clone(),
        fields: vec![FieldDefinition {
            name: field_path.clone(),
            type_id: string_type_id,
            location: SourceLocation::default(),
        }]
        .into_boxed_slice(),
        generic_parameters: None,
        const_record: false,
    });

    let field_value = Expression::string_slice(
        string_table.intern("green"),
        SourceLocation::default(),
        ValueMode::ImmutableOwned,
    );
    let record_value = Expression::struct_instance(
        struct_path,
        vec![Declaration {
            id: field_path,
            value: field_value,
        }],
        SourceLocation::default(),
        ValueMode::ImmutableOwned,
        true,
        None,
        struct_type_id,
    );
    let record_name = string_table.intern("html_defaults");
    let declaration = Declaration {
        id: scope.append(record_name),
        value: record_value,
    };
    let context = constant_template_context(&scope, &[declaration]);
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);

    let template = Template::new_const_required_with_type_interner(
        &mut token_stream,
        &context,
        &mut type_interner,
        vec![],
        &mut string_table,
    )
    .expect("const-required template head should project const-record field values");
    let folded = fold_template_in_context(&template, &context, &mut string_table);

    assert_eq!(string_table.resolve(folded), "green");
}

#[test]
fn source_authored_template_if_suffix_reaches_ast() {
    let (template, string_table) = parse_runtime_template("[if true: Visible]");

    let branch_chain = expect_branch_chain(&template);

    assert_static_content_contains(first_branch_content(branch_chain), &string_table, "Visible");
}

#[test]
fn source_authored_template_option_capture_if_suffix_reaches_ast() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[if maybe_name is |name|: [name]]", &mut string_table);
    let mut context = runtime_template_context(&token_stream.src_path.clone(), &mut string_table);

    let mut type_environment = TypeEnvironment::new();
    let maybe_name_type_id = type_environment.intern_option(type_environment.builtins().string);
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);

    let maybe_name = string_table.intern("maybe_name");
    let declaration = Declaration {
        id: token_stream.src_path.append(maybe_name),
        value: Expression::new(
            ExpressionKind::NoValue,
            token_stream.current_location(),
            maybe_name_type_id,
            DataType::Option(Box::new(DataType::StringSlice)),
            ValueMode::ImmutableOwned,
        ),
    };
    context.add_var(declaration);

    let template = Template::new_with_type_interner(
        &mut token_stream,
        &context,
        &mut type_interner,
        vec![],
        &mut string_table,
    )
    .expect("option-present template if should reach AST");

    let branch_chain = expect_branch_chain(&template);
    let selector = &branch_chain
        .branches
        .first()
        .expect("template if should contain a primary branch")
        .selector;

    assert!(matches!(
        selector,
        TemplateBranchSelector::OptionPresentCapture { .. }
    ));
}

#[test]
fn template_option_capture_binding_is_not_visible_in_else_branch() {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(
        "[if maybe_name is |name|:
            [name]
        [else]
            [name]
        ]",
        &mut string_table,
    );
    let mut context = runtime_template_context(&token_stream.src_path.clone(), &mut string_table);

    let mut type_environment = TypeEnvironment::new();
    let maybe_name_type_id = type_environment.intern_option(type_environment.builtins().string);
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);

    let maybe_name = string_table.intern("maybe_name");
    let capture_name = string_table.intern("name");
    let declaration = Declaration {
        id: token_stream.src_path.append(maybe_name),
        value: Expression::new(
            ExpressionKind::NoValue,
            token_stream.current_location(),
            maybe_name_type_id,
            DataType::Option(Box::new(DataType::StringSlice)),
            ValueMode::ImmutableOwned,
        ),
    };
    context.add_var(declaration);

    let diagnostic = Template::new_with_type_interner(
        &mut token_stream,
        &context,
        &mut type_interner,
        vec![],
        &mut string_table,
    )
    .expect_err("option-present capture should not be visible in template else branch");

    assert!(
        matches!(
            diagnostic.payload,
            DiagnosticPayload::UnexpectedToken {
                found: TokenKind::Symbol(name)
            } if name == capture_name
        ),
        "unexpected payload: {:?}",
        diagnostic.payload
    );
}

#[test]
fn source_authored_template_range_loop_suffix_reaches_ast() {
    let (template, _unused_table) = parse_runtime_template("[loop 0 to 3 |i|: [i]]");

    assert!(matches!(
        template.control_flow,
        Some(TemplateControlFlow::Loop(_))
    ));
}

#[test]
fn source_authored_template_conditional_loop_suffix_reaches_ast() {
    let (template, _unused_table) = parse_runtime_template("[loop true: Waiting]");

    let Some(TemplateControlFlow::Loop(template_loop)) = &template.control_flow else {
        panic!("expected template loop control flow");
    };

    assert!(matches!(
        template_loop.header,
        TemplateLoopHeader::Conditional { .. }
    ));
}

#[test]
fn template_control_flow_suffix_requires_comma_after_head_items() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[value if true: Visible]", &mut string_table);
    let context = runtime_template_context(&token_stream.src_path.clone(), &mut string_table);

    let diagnostic = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("missing comma before template control-flow suffix should fail");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidTemplateStructure {
            reason: InvalidTemplateStructureReason::MissingCommaBeforeControlFlowSuffix
        }
    ));
}

#[test]
fn template_if_suffix_must_be_final() {
    let diagnostic = parse_template_error("[if true, $raw: Visible]");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidTemplateStructure {
            reason: InvalidTemplateStructureReason::ControlFlowSuffixNotFinal
        }
    ));
}

#[test]
fn template_loop_suffix_must_be_final() {
    let diagnostic = parse_template_error("[loop 0 to 3 |i|, value: [i]]");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidTemplateStructure {
            reason: InvalidTemplateStructureReason::ControlFlowSuffixNotFinal
        }
    ));
}

#[test]
fn template_if_suffix_requires_condition() {
    let diagnostic = parse_template_error("[if: Visible]");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidTemplateStructure {
            reason: InvalidTemplateStructureReason::MissingTemplateIfCondition
        }
    ));
}

#[test]
fn template_loop_suffix_requires_header() {
    let diagnostic = parse_template_error("[loop: Visible]");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidTemplateStructure {
            reason: InvalidTemplateStructureReason::MissingTemplateLoopHeader
        }
    ));
}

#[test]
fn match_style_template_if_is_rejected() {
    let diagnostic = parse_template_error("[if true is: Visible]");

    assert!(
        matches!(
            diagnostic.payload,
            DiagnosticPayload::InvalidTemplateStructure {
                reason: InvalidTemplateStructureReason::TemplateMatchStyleControlFlowUnsupported
            }
        ),
        "unexpected payload: {:?}",
        diagnostic.payload
    );
}

#[test]
fn match_style_template_else_if_is_rejected() {
    let diagnostic =
        parse_template_error("[if false:\n    First\n[else if true is]\n    Second\n]");

    assert!(
        matches!(
            diagnostic.payload,
            DiagnosticPayload::InvalidTemplateStructure {
                reason: InvalidTemplateStructureReason::TemplateMatchStyleControlFlowUnsupported
            }
        ),
        "unexpected payload: {:?}",
        diagnostic.payload
    );
}

#[test]
fn else_in_template_head_is_rejected_until_body_sentinel_parsing() {
    let diagnostic = parse_template_error("[else]");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidTemplateStructure {
            reason: InvalidTemplateStructureReason::ElseInTemplateHead
        }
    ));
}

#[test]
fn template_else_if_adds_conditional_branch_in_source_order() {
    let (template, string_table) = parse_control_flow_template_after_body_parse(
        "[if false:
            First
        [else if true]
            Second
        [else if false]
            Third
        [else]
            Fallback
        ]",
    );

    let branch_chain = expect_branch_chain(&template);

    assert_eq!(
        branch_chain.branches.len(),
        3,
        "`else if` sentinels should add branches to the same chain"
    );
    assert_static_content_contains(&branch_chain.branches[0].content, &string_table, "First");
    assert_static_content_contains(&branch_chain.branches[1].content, &string_table, "Second");
    assert_static_content_contains(&branch_chain.branches[2].content, &string_table, "Third");
    assert_static_content_contains(fallback_content(branch_chain), &string_table, "Fallback");
}

#[test]
fn nested_template_else_if_builds_independent_branch_chains() {
    let (template, string_table) = parse_control_flow_template_after_body_parse(
        "[if true:
            Outer first
            [if false:
                Inner first
            [else if true]
                Inner second
            [else]
                Inner fallback
            ]
        [else if false]
            Outer second
        [else]
            Outer fallback
        ]",
    );

    let outer_chain = expect_branch_chain(&template);
    assert_eq!(
        outer_chain.branches.len(),
        2,
        "outer else-if should extend only the outer chain"
    );

    let nested_control_flow = find_first_control_flow_child(first_branch_content(outer_chain))
        .expect("outer branch should contain nested template control flow");
    let nested_chain = expect_branch_chain_control_flow(nested_control_flow);
    assert_eq!(
        nested_chain.branches.len(),
        2,
        "nested else-if should extend only the nested chain"
    );

    assert_static_content_contains(
        &nested_chain.branches[1].content,
        &string_table,
        "Inner second",
    );
    assert_static_content_contains(
        fallback_content(nested_chain),
        &string_table,
        "Inner fallback",
    );
    assert_static_content_contains(
        fallback_content(outer_chain),
        &string_table,
        "Outer fallback",
    );
}

#[test]
fn template_else_if_option_capture_binding_is_branch_local() {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(
        "[if false:
            hidden
        [else if maybe_name is |name|]
            [name]
        [else]
            [name]
        ]",
        &mut string_table,
    );
    let mut context = runtime_template_context(&token_stream.src_path.clone(), &mut string_table);

    let mut type_environment = TypeEnvironment::new();
    let maybe_name_type_id = type_environment.intern_option(type_environment.builtins().string);
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);

    let maybe_name = string_table.intern("maybe_name");
    let capture_name = string_table.intern("name");
    let declaration = Declaration {
        id: token_stream.src_path.append(maybe_name),
        value: Expression::new(
            ExpressionKind::NoValue,
            token_stream.current_location(),
            maybe_name_type_id,
            DataType::Option(Box::new(DataType::StringSlice)),
            ValueMode::ImmutableOwned,
        ),
    };
    context.add_var(declaration);

    let diagnostic = Template::new_with_type_interner(
        &mut token_stream,
        &context,
        &mut type_interner,
        vec![],
        &mut string_table,
    )
    .expect_err("else-if option capture should not be visible in the fallback branch");

    assert!(
        matches!(
            diagnostic.payload,
            DiagnosticPayload::UnexpectedToken {
                found: TokenKind::Symbol(name)
            } if name == capture_name
        ),
        "unexpected payload: {:?}",
        diagnostic.payload
    );
}

#[test]
fn template_if_body_splits_on_direct_else_sentinel() {
    let (template, string_table) = parse_control_flow_template_after_body_parse(
        "[if true:
            Visible
        [else]
            Hidden
        ]",
    );

    let branch_chain = expect_branch_chain(&template);

    assert_static_content_contains(first_branch_content(branch_chain), &string_table, "Visible");

    let else_content = fallback_content(branch_chain);
    assert_static_content_contains(else_content, &string_table, "Hidden");
    assert_static_content_excludes(first_branch_content(branch_chain), &string_table, "else");
    assert_static_content_excludes(else_content, &string_table, "else");
}

#[test]
fn nested_template_if_consumes_its_own_else_sentinel() {
    let (template, string_table) = parse_control_flow_template_after_body_parse(
        "[if true:
            [if false:
                Then
            [else]
                Inner else
            ]
        [else]
            Outer else
        ]",
    );

    let outer_if = expect_branch_chain(&template);

    let nested_if = find_first_control_flow_child(first_branch_content(outer_if))
        .expect("outer then branch should keep nested template if as a child");

    let nested_if = expect_branch_chain_control_flow(nested_if);

    let nested_else = fallback_content(nested_if);
    assert_static_content_contains(nested_else, &string_table, "Inner else");

    let outer_else = fallback_content(outer_if);
    assert_static_content_contains(outer_else, &string_table, "Outer else");
}

#[test]
fn template_loop_body_stores_normal_body_content() {
    let (template, string_table) =
        parse_control_flow_template_after_body_parse("[loop 0 to 3 |i|: Item [i]]");

    let Some(TemplateControlFlow::Loop(template_loop)) = template.control_flow else {
        panic!("expected template loop control flow");
    };

    assert_static_content_contains(&template_loop.body_content, &string_table, "Item");
}

#[test]
fn orphan_template_else_in_normal_body_is_rejected_before_nested_template_parsing() {
    let diagnostic = parse_template_error("[: Before [else] After]");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidTemplateStructure {
            reason: InvalidTemplateStructureReason::OrphanTemplateElse
        }
    ));
}

#[test]
fn duplicate_template_else_is_rejected() {
    let diagnostic = parse_template_error(
        "[if true:
            Then
        [else]
            Else
        [else]
            Again
        ]",
    );

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidTemplateStructure {
            reason: InvalidTemplateStructureReason::DuplicateTemplateElse
        }
    ));
}

#[test]
fn template_else_in_literal_body_template_if_is_rejected() {
    let diagnostic = parse_template_error(
        "[$doc, if true:
            literal then
        [else]
            literal else
        ]",
    );

    assert_invalid_template_structure(
        &diagnostic,
        InvalidTemplateStructureReason::TemplateElseInLiteralBody,
    );
}

#[test]
fn template_else_if_in_literal_body_template_if_is_rejected() {
    let diagnostic = parse_template_error(
        "[$doc, if true:
            literal then
        [else if false]
            literal else if
        ]",
    );

    assert_invalid_template_structure(
        &diagnostic,
        InvalidTemplateStructureReason::TemplateElseIfInLiteralBody,
    );
}

#[test]
fn malformed_template_else_forms_are_rejected() {
    for source in [
        "[if true:\nThen\n[else: nope]\n]",
        "[if true:\nThen\n[else, nope]\n]",
        "[if true:\nThen\n[else nope]\n]",
    ] {
        let diagnostic = parse_template_error(source);

        assert!(
            matches!(
                diagnostic.payload,
                DiagnosticPayload::InvalidTemplateStructure {
                    reason: InvalidTemplateStructureReason::MalformedTemplateElse
                }
            ),
            "unexpected payload for {source:?}: {:?}",
            diagnostic.payload
        );
    }
}

#[test]
fn malformed_template_else_if_forms_are_rejected() {
    for source in [
        "[if true:\nThen\n[else if false:]\n]",
        "[if true:\nThen\n[else if false, nope]\n]",
    ] {
        let diagnostic = parse_template_error(source);

        assert!(
            matches!(
                diagnostic.payload,
                DiagnosticPayload::InvalidTemplateStructure {
                    reason: InvalidTemplateStructureReason::MalformedTemplateElseIf
                }
            ),
            "unexpected payload for {source:?}: {:?}",
            diagnostic.payload
        );
    }
}

#[test]
fn template_else_if_requires_condition() {
    let diagnostic = parse_template_error(
        "[if true:
            Then
        [else if]
            Hidden
        ]",
    );

    assert_invalid_template_structure(
        &diagnostic,
        InvalidTemplateStructureReason::MissingTemplateElseIfCondition,
    );
}

#[test]
fn orphan_template_else_if_is_rejected_before_nested_template_parsing() {
    let diagnostic = parse_template_error("[: Before [else if true] After]");

    assert_invalid_template_structure(
        &diagnostic,
        InvalidTemplateStructureReason::OrphanTemplateElseIf,
    );
}

#[test]
fn template_else_if_after_else_is_rejected() {
    let diagnostic = parse_template_error(
        "[if true:
            Then
        [else]
            Else
        [else if false]
            Too late
        ]",
    );

    assert_invalid_template_structure(
        &diagnostic,
        InvalidTemplateStructureReason::TemplateElseIfAfterElse,
    );
}

#[test]
fn inline_template_else_boundary_text_is_rejected() {
    for source in [
        "[if true: Visible [else]\nHidden]",
        "[if true:\nVisible\n[else] Hidden]",
        "[if true: [$slot][else]\nHidden]",
    ] {
        let diagnostic = parse_template_error(source);

        assert!(
            matches!(
                diagnostic.payload,
                DiagnosticPayload::InvalidTemplateStructure {
                    reason: InvalidTemplateStructureReason::InlineTemplateElse
                }
            ),
            "unexpected payload for {source:?}: {:?}",
            diagnostic.payload
        );
    }
}

#[test]
fn inline_template_else_if_boundary_text_is_rejected() {
    for source in [
        "[if true: Visible [else if false]\nHidden]",
        "[if true:\nVisible\n[else if false] Hidden]",
    ] {
        let diagnostic = parse_template_error(source);

        assert!(
            matches!(
                diagnostic.payload,
                DiagnosticPayload::InvalidTemplateStructure {
                    reason: InvalidTemplateStructureReason::InlineTemplateElseIf
                }
            ),
            "unexpected payload for {source:?}: {:?}",
            diagnostic.payload
        );
    }
}

#[test]
fn template_if_allows_slot_on_previous_line_before_else_sentinel() {
    let (template, _unused_table) = parse_control_flow_template_after_body_parse(
        "[if true:
            [$slot]
        [else]
            Hidden
        ]",
    );

    let branch_chain = expect_branch_chain(&template);

    assert!(
        first_branch_content(branch_chain).has_unresolved_slots(),
        "slot placeholders before a next-line else sentinel should remain valid branch content"
    );
}

#[test]
fn direct_template_else_inside_loop_body_is_rejected() {
    let diagnostic = parse_template_error("[loop 0 to 3 |i|:\n[else]\n]");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidTemplateStructure {
            reason: InvalidTemplateStructureReason::TemplateElseInLoopBody
        }
    ));
}

#[test]
fn direct_template_else_if_inside_loop_body_is_rejected() {
    let diagnostic = parse_template_error("[loop 0 to 3 |i|:\n[else if true]\n]");

    assert_invalid_template_structure(
        &diagnostic,
        InvalidTemplateStructureReason::TemplateElseIfInLoopBody,
    );
}

#[test]
fn template_loop_break_is_structural_body_signal() {
    let (template, _unused_table) = parse_control_flow_template_after_body_parse(
        "[loop 0 to 2 |i|:
            [break]
        ]",
    );

    let Some(TemplateControlFlow::Loop(template_loop)) = template.control_flow else {
        panic!("expected template loop control flow");
    };

    assert_eq!(count_loop_control_signals(&template_loop.body_content), 1);
}

#[test]
fn template_loop_continue_is_structural_body_signal() {
    let (template, _unused_table) = parse_control_flow_template_after_body_parse(
        "[loop 0 to 2 |i|:
            [continue]
        ]",
    );

    let Some(TemplateControlFlow::Loop(template_loop)) = template.control_flow else {
        panic!("expected template loop control flow");
    };

    assert_eq!(count_loop_control_signals(&template_loop.body_content), 1);
}

#[test]
fn nested_template_if_break_inside_loop_is_structural_signal() {
    let (template, _unused_table) = parse_control_flow_template_after_body_parse(
        "[loop 0 to 2 |i|:
            [if true:
                [break]
            ]
        ]",
    );

    let Some(TemplateControlFlow::Loop(template_loop)) = template.control_flow else {
        panic!("expected template loop control flow");
    };
    let nested_if = find_first_control_flow_child(&template_loop.body_content)
        .expect("loop body should keep nested template if as a child");
    let branch_chain = expect_branch_chain_control_flow(nested_if);

    assert_eq!(
        count_loop_control_signals(first_branch_content(branch_chain)),
        1
    );
}

#[test]
fn nested_template_if_continue_inside_loop_is_structural_signal() {
    let (template, _unused_table) = parse_control_flow_template_after_body_parse(
        "[loop 0 to 2 |i|:
            [if true:
                [continue]
            ]
        ]",
    );

    let Some(TemplateControlFlow::Loop(template_loop)) = template.control_flow else {
        panic!("expected template loop control flow");
    };
    let nested_if = find_first_control_flow_child(&template_loop.body_content)
        .expect("loop body should keep nested template if as a child");
    let branch_chain = expect_branch_chain_control_flow(nested_if);

    assert_eq!(
        count_loop_control_signals(first_branch_content(branch_chain)),
        1
    );
}

#[test]
fn nested_template_if_inside_loop_consumes_its_own_else_sentinel() {
    let (template, string_table) = parse_control_flow_template_after_body_parse(
        "[loop 0 to 3 |i|:
            [if true:
                Inner then
            [else]
                Inner else
            ]
        ]",
    );

    let Some(TemplateControlFlow::Loop(template_loop)) = template.control_flow else {
        panic!("expected template loop control flow");
    };

    let nested_if = find_first_control_flow_child(&template_loop.body_content)
        .expect("loop body should keep nested template if as a child");

    let nested_if = expect_branch_chain_control_flow(nested_if);

    let nested_else = fallback_content(nested_if);
    assert_static_content_contains(nested_else, &string_table, "Inner else");
}

#[test]
fn template_if_composition_formats_each_branch_independently() {
    let (template, string_table) = parse_control_flow_template_after_composition(
        "[$markdown, if true:
            # Visible
        [else]
            # Hidden
        ]",
    );

    let branch_chain = expect_branch_chain(&template);

    assert_static_content_contains(
        first_branch_content(branch_chain),
        &string_table,
        "<h1>Visible</h1>",
    );

    let else_content = fallback_content(branch_chain);
    assert_static_content_contains(else_content, &string_table, "<h1>Hidden</h1>");
    assert_static_content_excludes(first_branch_content(branch_chain), &string_table, "Hidden");
    assert_static_content_excludes(else_content, &string_table, "Visible");
}

#[test]
fn template_if_composition_applies_shared_head_prefix_to_each_branch() {
    let mut string_table = StringTable::new();
    let wrapper_scope =
        InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);

    let mut card_tokens =
        template_tokens_from_source("[: <card>[$slot]</card>]", &mut string_table);
    let card_context = new_constant_context(card_tokens.src_path.to_owned());
    let card_template = Template::new(&mut card_tokens, &card_context, vec![], &mut string_table)
        .expect("card wrapper should parse");

    let card_name = string_table.intern("card");
    let declarations = vec![Declaration {
        id: wrapper_scope.append(card_name),
        value: Expression::template(card_template, ValueMode::ImmutableOwned),
    }];

    let mut token_stream = template_tokens_from_source(
        "[card, if true:
            Visible
        [else]
            Hidden
        ]",
        &mut string_table,
    );
    let context = constant_template_context(&token_stream.src_path, &declarations);

    let mut type_environment = TypeEnvironment::new();
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);

    let template = Template::new_nested_template(
        &mut token_stream,
        &context,
        &mut type_interner,
        TemplateInheritance::default(),
        &mut string_table,
        NestedTemplateParseOptions::runtime_capable(),
    )
    .expect("template if should parse through control-flow composition");

    let branch_chain = expect_branch_chain(&template);

    assert_static_content_contains(first_branch_content(branch_chain), &string_table, "<card>");
    assert_static_content_contains(first_branch_content(branch_chain), &string_table, "Visible");

    let else_content = fallback_content(branch_chain);
    assert_static_content_contains(else_content, &string_table, "<card>");
    assert_static_content_contains(else_content, &string_table, "Hidden");
}

#[test]
fn template_loop_composition_formats_body_without_repeating_shared_head_prefix() {
    let (template, string_table) = parse_control_flow_template_after_composition(
        "[\"prefix\", $markdown, loop 0 to 3 |i|:
            # Item
        ]",
    );

    let Some(TemplateControlFlow::Loop(template_loop)) = template.control_flow else {
        panic!("expected template loop control flow");
    };

    assert_static_content_contains(&template.content, &string_table, "prefix");
    assert_static_content_contains(&template_loop.body_content, &string_table, "<h1>Item</h1>");
    assert_static_content_excludes(&template_loop.body_content, &string_table, "prefix");
}

#[test]
fn parent_children_wrappers_attach_conditionally_to_control_flow_child() {
    let (template, _unused_table) = parse_control_flow_template_after_composition(
        "[$children([:<li>[$slot]</li>]):
            [if true:
                item
            ]
        ]",
    );

    let child_template =
        find_first_control_flow_template_child(&template.content).expect("expected child if");

    assert_eq!(
        child_template.conditional_child_wrappers.len(),
        1,
        "control-flow child should receive inherited wrapper for conditional emission"
    );

    assert!(
        !template
            .content
            .atoms
            .iter()
            .any(atom_is_external_wrapper_around_control_flow_child),
        "parent should not externally wrap maybe-empty control-flow children"
    );
}

#[test]
fn fresh_control_flow_child_skips_parent_children_wrapper() {
    let (template, _unused_table) = parse_control_flow_template_after_composition(
        "[$children([:<li>[$slot]</li>]):
            [$fresh, if true:
                item
            ]
        ]",
    );

    let child_template =
        find_first_control_flow_template_child(&template.content).expect("expected child if");

    assert!(
        child_template.conditional_child_wrappers.is_empty(),
        "$fresh should opt the whole control-flow child out of immediate parent wrappers"
    );
}

#[test]
fn runtime_template_if_rejects_insert_leaking_from_branch() {
    let error = parse_control_flow_template_after_composition_error(
        "[if true:
            [$insert(\"style\"): color: red;]
        ]",
    );

    assert_invalid_template_structure(
        &error,
        InvalidTemplateStructureReason::RuntimeControlFlowUnresolvedInsert,
    );
}

#[test]
fn runtime_template_loop_rejects_insert_leaking_from_body() {
    let error = parse_control_flow_template_after_composition_error(
        "[loop 0 to 2 |i|:
            [$insert(\"row\"): [i]]
        ]",
    );

    assert_invalid_template_structure(
        &error,
        InvalidTemplateStructureReason::RuntimeControlFlowUnresolvedInsert,
    );
}

#[test]
fn runtime_template_if_rejects_unresolved_slot() {
    let diagnostic = parse_template_error(
        "[if true:
            [$slot]
        ]",
    );

    assert_invalid_template_structure(
        &diagnostic,
        InvalidTemplateStructureReason::RuntimeControlFlowUnresolvedSlot,
    );
}

#[test]
fn runtime_template_if_rejects_unresolved_insert() {
    let diagnostic = parse_template_error(
        "[if true:
            [$insert(\"style\"): color: red;]
        ]",
    );

    assert_invalid_template_structure(
        &diagnostic,
        InvalidTemplateStructureReason::RuntimeControlFlowUnresolvedInsert,
    );
}

#[test]
fn const_required_template_if_allows_unresolved_slot_wrapper() {
    let (template, _context, _unused_table) = parse_const_required_template(
        "[if true:
            [$slot]
        ]",
    );

    let branch_chain = expect_branch_chain(&template);

    assert!(
        first_branch_content(branch_chain).has_unresolved_slots(),
        "const-required helper templates may keep slot structure for later composition"
    );
}

#[test]
fn const_required_template_if_folds_selected_branch() {
    let (template, context, mut string_table) = parse_const_required_template(
        "[if true:
            Visible
        [else]
            Hidden
        ]",
    );

    let folded = fold_template_in_context(&template, &context, &mut string_table);
    let rendered = string_table.resolve(folded);

    assert!(rendered.contains("Visible"));
    assert!(!rendered.contains("Hidden"));
}

#[test]
fn const_required_template_else_if_folds_first_selected_branch() {
    let (template, context, mut string_table) = parse_const_required_template(
        "[if false:
            First
        [else if true]
            Second
        [else if true]
            Third
        [else]
            Fallback
        ]",
    );

    let folded = fold_template_in_context(&template, &context, &mut string_table);
    let rendered = string_table.resolve(folded);

    assert!(rendered.contains("Second"));
    assert!(!rendered.contains("First"));
    assert!(!rendered.contains("Third"));
    assert!(!rendered.contains("Fallback"));
}

#[test]
fn const_required_template_if_inlines_same_file_source_const_bool() {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(
        "[if show_banner:
            Visible
        [else]
            Hidden
        ]",
        &mut string_table,
    );
    let show_banner = string_table.intern("show_banner");
    let declaration = Declaration {
        id: token_stream.src_path.append(show_banner),
        value: Expression::bool(
            true,
            token_stream.current_location(),
            ValueMode::ImmutableOwned,
        ),
    };
    let context = constant_template_context(&token_stream.src_path, &[declaration]);

    let template =
        Template::new_const_required(&mut token_stream, &context, vec![], &mut string_table)
            .expect("const-required template if should inline source const bool");
    let folded = fold_template_in_context(&template, &context, &mut string_table);
    let rendered = string_table.resolve(folded);

    assert!(rendered.contains("Visible"));
    assert!(!rendered.contains("Hidden"));
}

#[test]
fn const_required_template_if_inlines_imported_source_const_bool() {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(
        "[if show_banner:
            Visible
        [else]
            Hidden
        ]",
        &mut string_table,
    );
    let show_banner = string_table.intern("show_banner");
    let flags_scope = InternedPath::from_single_str("flags.bst", &mut string_table);
    let imported_path = flags_scope.append(show_banner);
    let declaration = Declaration {
        id: imported_path.clone(),
        value: Expression::bool(
            true,
            token_stream.current_location(),
            ValueMode::ImmutableOwned,
        ),
    };
    let context = imported_const_template_context(&token_stream.src_path, declaration, show_banner);

    let template =
        Template::new_const_required(&mut token_stream, &context, vec![], &mut string_table)
            .expect("const-required template if should inline imported source const bool");
    let folded = fold_template_in_context(&template, &context, &mut string_table);
    let rendered = string_table.resolve(folded);

    assert!(rendered.contains("Visible"));
    assert!(!rendered.contains("Hidden"));
}

#[test]
fn const_required_template_if_false_without_else_skips_shared_head_output() {
    let mut string_table = StringTable::new();
    let wrapper_scope =
        InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);

    let mut card_tokens = template_tokens_from_source("[:<card>[$slot]</card>]", &mut string_table);
    let card_context = new_constant_context(card_tokens.src_path.to_owned());
    let card_template = Template::new(&mut card_tokens, &card_context, vec![], &mut string_table)
        .expect("card wrapper should parse");

    let card_name = string_table.intern("card");
    let declarations = vec![Declaration {
        id: wrapper_scope.append(card_name),
        value: Expression::template(card_template, ValueMode::ImmutableOwned),
    }];

    let mut token_stream = template_tokens_from_source(
        "[card, if false:
            Visible
        ]",
        &mut string_table,
    );
    let context = constant_template_context(&token_stream.src_path, &declarations);
    let template =
        Template::new_const_required(&mut token_stream, &context, vec![], &mut string_table)
            .expect("const-required template if should parse");

    let folded = fold_template_in_context(&template, &context, &mut string_table);

    assert_eq!(string_table.resolve(folded), "");
}

#[test]
fn const_required_template_if_inspects_inactive_branch_control_flow() {
    let (template, context, mut string_table) = parse_const_required_template(
        "[if true:
            Visible
        [else]
            [loop 0 to 1 |i|:
                Hidden
            ]
        ]",
    );

    let folded = fold_template_in_context(&template, &context, &mut string_table);
    let rendered = string_table.resolve(folded);

    assert!(rendered.contains("Visible"));
    assert!(!rendered.contains("Hidden"));
}

#[test]
fn const_required_template_range_loop_folds_iteration_bindings() {
    let (template, context, mut string_table) = parse_const_required_template(
        "[loop 0 to & 3 |i, index|:
            [index]:[i];
        ]",
    );

    assert_eq!(
        template.const_value_kind(),
        TemplateConstValueKind::RenderableString
    );

    let folded = fold_template_in_context(&template, &context, &mut string_table);

    assert_eq!(string_table.resolve(folded), "0:0;1:1;2:2;3:3;");
}

#[test]
fn const_required_template_range_loop_folds_expressions_with_iteration_bindings() {
    let (template, context, mut string_table) = parse_const_required_template(
        "[loop 0 to 2 |i|:
            [i + 1];
        ]",
    );

    assert_eq!(
        template.const_value_kind(),
        TemplateConstValueKind::RenderableString
    );

    let folded = fold_template_in_context(&template, &context, &mut string_table);

    assert_eq!(string_table.resolve(folded), "1;2;");
}

#[test]
fn const_required_template_loop_allows_nested_if_to_use_iteration_binding() {
    let (template, context, mut string_table) = parse_const_required_template(
        "[loop 0 to 2 |i|:
            [if true:
                [i]
            ]
        ]",
    );

    assert_eq!(
        template.const_value_kind(),
        TemplateConstValueKind::RenderableString
    );

    let folded = fold_template_in_context(&template, &context, &mut string_table);

    assert_eq!(string_table.resolve(folded), "01");
}

#[test]
fn const_required_template_loop_allows_nested_if_condition_to_use_iteration_binding() {
    let (template, context, mut string_table) = parse_const_required_template(
        "[loop 0 to 3 |i|:
            [if i is 1:
                [:T]
            [else]
                [i]
            ]
        ]",
    );

    assert_eq!(
        template.const_value_kind(),
        TemplateConstValueKind::RenderableString
    );

    let folded = fold_template_in_context(&template, &context, &mut string_table);

    assert_eq!(string_table.resolve(folded), "0T2");
}

#[test]
fn const_required_template_loop_body_if_can_use_source_const_condition() {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(
        "[loop 0 to 2 |i|:
            [if show_item:
                [i]
            ]
        ]",
        &mut string_table,
    );
    let show_item = string_table.intern("show_item");
    let declaration = Declaration {
        id: token_stream.src_path.append(show_item),
        value: Expression::bool(
            true,
            token_stream.current_location(),
            ValueMode::ImmutableOwned,
        ),
    };
    let context = constant_template_context(&token_stream.src_path, &[declaration]);

    let template =
        Template::new_const_required(&mut token_stream, &context, vec![], &mut string_table)
            .expect("nested const-required template if should inline source const bool");
    let folded = fold_template_in_context(&template, &context, &mut string_table);

    assert_eq!(string_table.resolve(folded), "01");
}

#[test]
fn const_required_template_collection_loop_folds_iteration_bindings() {
    let (template, context, mut string_table) = parse_const_required_template(
        "[loop {\"Ada\", \"Bo\"} |name, index|:
            [index]-[name];
        ]",
    );

    let folded = fold_template_in_context(&template, &context, &mut string_table);

    assert_eq!(string_table.resolve(folded), "0-Ada;1-Bo;");
}

#[test]
fn const_required_template_zero_iteration_loop_skips_shared_head_output() {
    let mut string_table = StringTable::new();
    let wrapper_scope =
        InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);

    let mut card_tokens = template_tokens_from_source("[:<card>[$slot]</card>]", &mut string_table);
    let card_context = new_constant_context(card_tokens.src_path.to_owned());
    let card_template = Template::new(&mut card_tokens, &card_context, vec![], &mut string_table)
        .expect("card wrapper should parse");

    let card_name = string_table.intern("card");
    let declarations = vec![Declaration {
        id: wrapper_scope.append(card_name),
        value: Expression::template(card_template, ValueMode::ImmutableOwned),
    }];

    let mut token_stream = template_tokens_from_source(
        "[card, loop 0 to 0 |i|:
            [i]
        ]",
        &mut string_table,
    );
    let context = constant_template_context(&token_stream.src_path, &declarations);
    let template =
        Template::new_const_required(&mut token_stream, &context, vec![], &mut string_table)
            .expect("const-required zero loop should parse");

    assert_aggregate_plan_is_structural(&template);

    let folded = fold_template_in_context(&template, &context, &mut string_table);

    assert_eq!(string_table.resolve(folded), "");
}

#[test]
fn const_required_template_loop_wraps_aggregate_once() {
    let mut string_table = StringTable::new();
    let wrapper_scope =
        InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);

    let mut card_tokens = template_tokens_from_source("[:<card>[$slot]</card>]", &mut string_table);
    let card_context = new_constant_context(card_tokens.src_path.to_owned());
    let card_template = Template::new(&mut card_tokens, &card_context, vec![], &mut string_table)
        .expect("card wrapper should parse");

    let card_name = string_table.intern("card");
    let declarations = vec![Declaration {
        id: wrapper_scope.append(card_name),
        value: Expression::template(card_template, ValueMode::ImmutableOwned),
    }];

    let mut token_stream = template_tokens_from_source(
        "[card, loop 0 to 2 |i|:
            [i]
        ]",
        &mut string_table,
    );
    let context = constant_template_context(&token_stream.src_path, &declarations);
    let template =
        Template::new_const_required(&mut token_stream, &context, vec![], &mut string_table)
            .expect("const-required loop should parse");

    assert_aggregate_plan_is_structural(&template);

    let folded = fold_template_in_context(&template, &context, &mut string_table);

    assert_eq!(string_table.resolve(folded), "<card>01</card>");
}

#[test]
fn const_required_template_conditional_loop_false_folds_to_no_output() {
    let (template, context, mut string_table) = parse_const_required_template(
        "[loop false:
            Never
        ]",
    );

    let folded = fold_template_in_context(&template, &context, &mut string_table);

    assert_eq!(string_table.resolve(folded), "");
}

#[test]
fn const_required_template_conditional_loop_true_is_rejected() {
    let diagnostic = parse_const_required_template_error(
        "[loop true:
            Never
        ]",
    );

    assert_invalid_template_structure(
        &diagnostic,
        InvalidTemplateStructureReason::TemplateConditionalLoopConstTrue,
    );
}

#[test]
fn const_required_template_conditional_loop_reports_runtime_condition() {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(
        "[loop keep_going:
            Never
        ]",
        &mut string_table,
    );
    let mut context = new_constant_context(token_stream.src_path.clone());
    let mut type_environment = TypeEnvironment::new();
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);
    let keep_going = string_table.intern("keep_going");

    context.add_var(Declaration {
        id: token_stream.src_path.append(keep_going),
        value: Expression::new(
            ExpressionKind::NoValue,
            token_stream.current_location(),
            builtin_type_ids::BOOL,
            DataType::Bool,
            ValueMode::ImmutableOwned,
        ),
    });

    let diagnostic = Template::new_const_required_with_type_interner(
        &mut token_stream,
        &context,
        &mut type_interner,
        vec![],
        &mut string_table,
    )
    .expect_err("const-required conditional loop should reject runtime conditions");

    assert_invalid_template_structure(
        &diagnostic,
        InvalidTemplateStructureReason::TemplateLoopConditionNotConst,
    );
}

#[test]
fn const_required_template_loop_reports_non_const_collection_source() {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(
        "[loop items |item|:
            [item]
        ]",
        &mut string_table,
    );
    let mut context = new_constant_context(token_stream.src_path.clone());
    let mut type_environment = TypeEnvironment::new();
    let collection_type_id =
        type_environment.intern_collection(type_environment.builtins().string, None);
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);
    let items = string_table.intern("items");

    context.add_var(Declaration {
        id: token_stream.src_path.append(items),
        value: Expression::new(
            ExpressionKind::NoValue,
            token_stream.current_location(),
            collection_type_id,
            DataType::collection(DataType::StringSlice),
            ValueMode::ImmutableOwned,
        ),
    });

    let diagnostic = Template::new_const_required_with_type_interner(
        &mut token_stream,
        &context,
        &mut type_interner,
        vec![],
        &mut string_table,
    )
    .expect_err("const-required loop should reject runtime collection source");

    assert_invalid_template_structure(
        &diagnostic,
        InvalidTemplateStructureReason::TemplateLoopSourceNotConst,
    );
}

#[test]
fn const_required_template_loop_reports_non_const_body() {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(
        "[loop 0 to 1 |i|:
            [value]
        ]",
        &mut string_table,
    );
    let mut context = new_constant_context(token_stream.src_path.clone());
    let mut type_environment = TypeEnvironment::new();
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);
    let value = string_table.intern("value");

    context.add_var(Declaration {
        id: token_stream.src_path.append(value),
        value: Expression::new(
            ExpressionKind::NoValue,
            token_stream.current_location(),
            builtin_type_ids::STRING,
            DataType::StringSlice,
            ValueMode::ImmutableOwned,
        ),
    });

    let diagnostic = Template::new_const_required_with_type_interner(
        &mut token_stream,
        &context,
        &mut type_interner,
        vec![],
        &mut string_table,
    )
    .expect_err("const-required loop should reject runtime body content");

    assert_invalid_template_structure(
        &diagnostic,
        InvalidTemplateStructureReason::TemplateLoopBodyNotConst,
    );
}

#[test]
fn const_required_template_loop_reports_expansion_limit() {
    let (template, context, mut string_table) = parse_const_required_template(
        "[loop 0 to & 10000 |i|:
            [i]
        ]",
    );

    let mut fold_context = context
        .new_template_fold_context(&mut string_table, "template tests fold limit")
        .expect("test context should include fold dependencies");
    let error = template
        .fold_into_stringid(&mut fold_context)
        .expect_err("const loop should enforce expansion limit")
        .into_diagnostic();

    assert_invalid_template_structure(
        &error,
        InvalidTemplateStructureReason::TemplateConstLoopExpansionLimitExceeded { limit: 10_000 },
    );
}

#[test]
fn const_required_template_loop_uses_configured_expansion_limit() {
    let (template, context, mut string_table) = parse_const_required_template(
        "[loop 0 to & 10000 |i|:
            [if false:
                hidden
            ]
        ]",
    );

    let mut fold_context = context
        .new_template_fold_context(&mut string_table, "template tests configured fold limit")
        .expect("test context should include fold dependencies");
    fold_context.template_const_loop_iteration_limit = 10_001;

    let folded = template
        .fold_into_stringid(&mut fold_context)
        .expect("configured const loop limit should allow the loop");
    drop(fold_context);

    assert_eq!(string_table.resolve(folded), "");
}

#[test]
fn const_required_template_option_capture_present_folds_then_branch() {
    let mut string_table = StringTable::new();
    let context_scope =
        InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let context = new_constant_context(context_scope.clone());

    let mut type_environment = TypeEnvironment::new();
    let string_type_id = type_environment.builtins().string;
    let option_string_type_id = type_environment.intern_option(string_type_id);
    let capture_name = string_table.intern("name");
    let capture_path = context_scope.append(capture_name);
    let present_value = Expression::string_slice(
        string_table.intern("Ada"),
        SourceLocation::default(),
        ValueMode::ImmutableOwned,
    );
    let scrutinee = Expression::coerced(present_value, option_string_type_id);
    let template = option_capture_template(
        scrutinee,
        capture_name,
        capture_path,
        string_type_id,
        &mut string_table,
    );

    validate_const_required_template_control_flow(&template, &template.location)
        .expect("present const option capture should validate");

    let folded = fold_template_in_context(&template, &context, &mut string_table);

    assert_eq!(string_table.resolve(folded), "Hello Ada");
}

#[test]
fn const_required_template_option_capture_absent_folds_else_branch() {
    let mut string_table = StringTable::new();
    let context_scope =
        InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let context = new_constant_context(context_scope.clone());

    let mut type_environment = TypeEnvironment::new();
    let string_type_id = type_environment.builtins().string;
    let capture_name = string_table.intern("name");
    let capture_path = context_scope.append(capture_name);
    let scrutinee = Expression::option_none_with_type_id(
        string_type_id,
        DataType::StringSlice,
        &mut type_environment,
        SourceLocation::default(),
    );
    let template = option_capture_template(
        scrutinee,
        capture_name,
        capture_path,
        string_type_id,
        &mut string_table,
    );

    validate_const_required_template_control_flow(&template, &template.location)
        .expect("absent const option capture should validate");

    let folded = fold_template_in_context(&template, &context, &mut string_table);

    assert_eq!(string_table.resolve(folded), "Guest");
}

#[test]
fn const_required_template_option_capture_inlines_present_source_const() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[if maybe_name is |name|:Hello [name]]", &mut string_table);
    let maybe_name = string_table.intern("maybe_name");

    let mut type_environment = TypeEnvironment::new();
    let string_type_id = type_environment.builtins().string;
    let option_string_type_id = type_environment.intern_option(string_type_id);
    let present_value = Expression::string_slice(
        string_table.intern("Ada"),
        token_stream.current_location(),
        ValueMode::ImmutableOwned,
    );
    let declaration = Declaration {
        id: token_stream.src_path.append(maybe_name),
        value: Expression::coerced(present_value, option_string_type_id),
    };
    let context = constant_template_context(&token_stream.src_path, &[declaration]);
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);

    let template = Template::new_const_required_with_type_interner(
        &mut token_stream,
        &context,
        &mut type_interner,
        vec![],
        &mut string_table,
    )
    .expect("const-required option capture should inline present source const");
    let folded = fold_template_in_context(&template, &context, &mut string_table);

    assert_eq!(string_table.resolve(folded), "Hello Ada");
}

#[test]
fn const_required_template_option_capture_inlines_absent_source_const() {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(
        "[if maybe_name is |name|:
            Hello [name]
        [else]
            Guest
        ]",
        &mut string_table,
    );
    let maybe_name = string_table.intern("maybe_name");

    let mut type_environment = TypeEnvironment::new();
    let string_type_id = type_environment.builtins().string;
    let absent_value = Expression::option_none_with_type_id(
        string_type_id,
        DataType::StringSlice,
        &mut type_environment,
        token_stream.current_location(),
    );
    let declaration = Declaration {
        id: token_stream.src_path.append(maybe_name),
        value: absent_value,
    };
    let context = constant_template_context(&token_stream.src_path, &[declaration]);
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);

    let template = Template::new_const_required_with_type_interner(
        &mut token_stream,
        &context,
        &mut type_interner,
        vec![],
        &mut string_table,
    )
    .expect("const-required option capture should inline absent source const");
    let folded = fold_template_in_context(&template, &context, &mut string_table);

    assert_eq!(string_table.resolve(folded), "Guest");
}

#[test]
fn const_required_template_option_capture_reports_runtime_scrutinee_diagnostic() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[if maybe_name is |name|: [name]]", &mut string_table);
    let mut context = new_constant_context(token_stream.src_path.clone());

    let mut type_environment = TypeEnvironment::new();
    let maybe_name_type_id = type_environment.intern_option(type_environment.builtins().string);
    let maybe_name = string_table.intern("maybe_name");
    let declaration = Declaration {
        id: token_stream.src_path.append(maybe_name),
        value: Expression::new(
            ExpressionKind::NoValue,
            token_stream.current_location(),
            maybe_name_type_id,
            DataType::Option(Box::new(DataType::StringSlice)),
            ValueMode::ImmutableOwned,
        ),
    };
    context.add_var(declaration);

    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);
    let diagnostic = Template::new_const_required_with_type_interner(
        &mut token_stream,
        &context,
        &mut type_interner,
        vec![],
        &mut string_table,
    )
    .expect_err("const-required option capture should be deferred");

    assert_invalid_template_structure(
        &diagnostic,
        InvalidTemplateStructureReason::TemplateOptionCaptureConstDeferred,
    );
}

#[test]
fn const_required_template_if_rejects_runtime_local_condition() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[if show_banner: Visible]", &mut string_table);
    let mut context = new_constant_context(token_stream.src_path.clone());
    let show_banner = string_table.intern("show_banner");
    context.add_var(Declaration {
        id: token_stream.src_path.append(show_banner),
        value: Expression::new(
            ExpressionKind::NoValue,
            token_stream.current_location(),
            builtin_type_ids::BOOL,
            DataType::Bool,
            ValueMode::ImmutableOwned,
        ),
    });

    let diagnostic =
        Template::new_const_required(&mut token_stream, &context, vec![], &mut string_table)
            .expect_err("const-required template if should reject runtime local condition");

    assert_invalid_template_structure(
        &diagnostic,
        InvalidTemplateStructureReason::TemplateIfConditionNotConst,
    );
}

fn imported_const_template_context(
    scope: &InternedPath,
    declaration: Declaration,
    visible_name: StringId,
) -> ScopeContext {
    let mut visible_declarations = FxHashSet::default();
    visible_declarations.insert(declaration.id.clone());

    let mut visible_bindings = FxHashMap::default();
    visible_bindings.insert(visible_name, declaration.id.clone());

    constant_template_context(scope, &[declaration])
        .with_visible_declarations(visible_declarations)
        .with_visible_source_bindings(visible_bindings)
}

fn option_capture_template(
    scrutinee: Expression,
    capture_name: StringId,
    capture_path: InternedPath,
    inner_type_id: TypeId,
    string_table: &mut StringTable,
) -> Template {
    let capture_reference = Expression::reference_with_type_id(
        capture_path.clone(),
        DataType::StringSlice,
        inner_type_id,
        SourceLocation::default(),
        ValueMode::ImmutableOwned,
        ConstRecordState::RuntimeValue,
    );
    let hello = Expression::string_slice(
        string_table.intern("Hello "),
        SourceLocation::default(),
        ValueMode::ImmutableOwned,
    );
    let guest = Expression::string_slice(
        string_table.intern("Guest"),
        SourceLocation::default(),
        ValueMode::ImmutableOwned,
    );

    let mut template = Template::empty();
    template.kind = TemplateType::String;
    template.control_flow = Some(TemplateControlFlow::BranchChain(Box::new(
        TemplateBranchChain {
            branches: vec![TemplateConditionalBranch {
                selector: TemplateBranchSelector::OptionPresentCapture {
                    scrutinee,
                    pattern: Box::new(MatchPattern::OptionPresentCapture {
                        name: capture_name,
                        binding_path: capture_path,
                        inner_type_id,
                        location: SourceLocation::default(),
                        binding_location: SourceLocation::default(),
                    }),
                },
                content: TemplateContent::new(vec![hello, capture_reference]),
                render_plan: None,
                location: SourceLocation::default(),
            }],
            fallback: Some(TemplateFallbackBranch {
                content: TemplateContent::new(vec![guest]),
                render_plan: None,
                location: SourceLocation::default(),
            }),
            location: SourceLocation::default(),
        },
    )));

    template
}

fn parse_template_error(
    source: &str,
) -> crate::compiler_frontend::compiler_messages::CompilerDiagnostic {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(source, &mut string_table);
    let context = new_constant_context(token_stream.src_path.clone());

    Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("template source should fail")
}

fn parse_runtime_template(source: &str) -> (Template, StringTable) {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(source, &mut string_table);
    let context = new_constant_context(token_stream.src_path.clone());

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("template source should parse");

    (template, string_table)
}

fn parse_control_flow_template_after_body_parse(source: &str) -> (Template, StringTable) {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(source, &mut string_table);
    let context = new_constant_context(token_stream.src_path.clone());

    let mut type_environment = TypeEnvironment::new();
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);

    let mut template = Template::empty();
    template.location = token_stream.current_location();
    let mut can_fold = true;

    let parsed_head = parse_template_head(
        &mut token_stream,
        &context,
        &mut type_interner,
        &mut template,
        &mut can_fold,
        TemplateControlFlowValidationMode::RuntimeCapable,
        &mut string_table,
    )
    .expect("template head should parse");

    parse_template_body(
        &mut token_stream,
        &mut template,
        TemplateBodyParseRequest {
            context: &context,
            type_interner: &mut type_interner,
            body_mode: parsed_head.body_mode,
            direct_child_wrappers: &[],
            control_flow_validation: TemplateControlFlowValidationMode::RuntimeCapable,
            control_context: TemplateBodyControlContext::normal(),
            foldable: &mut can_fold,
            string_table: &mut string_table,
        },
    )
    .expect("template body should parse");

    (template, string_table)
}

fn parse_control_flow_template_after_composition(source: &str) -> (Template, StringTable) {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(source, &mut string_table);
    let context = new_constant_context(token_stream.src_path.clone());

    let mut type_environment = TypeEnvironment::new();
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);

    let template = Template::new_nested_template(
        &mut token_stream,
        &context,
        &mut type_interner,
        TemplateInheritance::default(),
        &mut string_table,
        NestedTemplateParseOptions::runtime_capable(),
    )
    .expect("control-flow template should parse through composition");

    (template, string_table)
}

fn parse_control_flow_template_after_composition_error(
    source: &str,
) -> crate::compiler_frontend::compiler_messages::CompilerDiagnostic {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(source, &mut string_table);
    let context = new_constant_context(token_stream.src_path.clone());

    let mut type_environment = TypeEnvironment::new();
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);

    Template::new_nested_template(
        &mut token_stream,
        &context,
        &mut type_interner,
        TemplateInheritance::default(),
        &mut string_table,
        NestedTemplateParseOptions::runtime_capable(),
    )
    .expect_err("control-flow template should fail during composition")
}

fn parse_const_required_template(source: &str) -> (Template, ScopeContext, StringTable) {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(source, &mut string_table);
    let context = new_constant_context(token_stream.src_path.clone());

    let template =
        Template::new_const_required(&mut token_stream, &context, vec![], &mut string_table)
            .expect("const-required template should parse");

    (template, context, string_table)
}

fn parse_const_required_template_error(
    source: &str,
) -> crate::compiler_frontend::compiler_messages::CompilerDiagnostic {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(source, &mut string_table);
    let context = new_constant_context(token_stream.src_path.clone());

    Template::new_const_required(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("const-required template source should fail")
}

fn static_content_text(content: &TemplateContent, string_table: &StringTable) -> String {
    let mut rendered = String::new();
    collect_static_template_fragments(&content.atoms, string_table, &mut rendered);
    rendered
}

fn assert_static_content_contains(
    content: &TemplateContent,
    string_table: &StringTable,
    expected: &str,
) {
    let rendered = static_content_text(content, string_table);
    assert!(
        rendered.contains(expected),
        "expected {rendered:?} to contain {expected:?}"
    );
}

fn assert_static_content_excludes(
    content: &TemplateContent,
    string_table: &StringTable,
    unexpected: &str,
) {
    let rendered = static_content_text(content, string_table);
    assert!(
        !rendered.contains(unexpected),
        "expected {rendered:?} to exclude {unexpected:?}"
    );
}

fn assert_invalid_template_structure(
    diagnostic: &crate::compiler_frontend::compiler_messages::CompilerDiagnostic,
    expected_reason: InvalidTemplateStructureReason,
) {
    match &diagnostic.payload {
        DiagnosticPayload::InvalidTemplateStructure { reason } => {
            assert_eq!(*reason, expected_reason);
        }
        payload => panic!("expected invalid template structure payload, found {payload:?}"),
    }
}

fn assert_aggregate_plan_is_structural(template: &Template) {
    let Some(TemplateControlFlow::Loop(template_loop)) = &template.control_flow else {
        panic!("expected template loop control flow");
    };
    let aggregate_plan = template_loop
        .aggregate_render_plan
        .as_ref()
        .expect("template loop should have an aggregate render plan");

    assert_eq!(
        count_aggregate_pieces(aggregate_plan),
        1,
        "template aggregate plans should contain one explicit aggregate marker"
    );
    assert!(
        !aggregate_plan_contains_no_value_expression(aggregate_plan),
        "aggregate markers must not be represented as ExpressionKind::NoValue"
    );
    assert!(
        !aggregate_plan_contains_render_slot(aggregate_plan),
        "aggregate markers must be converted before final aggregate render plans"
    );
}

fn count_aggregate_pieces(plan: &TemplateAggregateRenderPlan) -> usize {
    plan.pieces
        .iter()
        .filter(|piece| matches!(piece, TemplateAggregatePiece::Aggregate))
        .count()
}

fn aggregate_plan_contains_no_value_expression(plan: &TemplateAggregateRenderPlan) -> bool {
    plan.pieces.iter().any(|piece| match piece {
        TemplateAggregatePiece::Aggregate => false,
        TemplateAggregatePiece::Render(render_piece) => {
            render_piece_contains_no_value_expression(render_piece)
        }
    })
}

fn aggregate_plan_contains_render_slot(plan: &TemplateAggregateRenderPlan) -> bool {
    plan.pieces.iter().any(|piece| match piece {
        TemplateAggregatePiece::Aggregate => false,
        TemplateAggregatePiece::Render(render_piece) => render_piece_contains_slot(render_piece),
    })
}

fn render_piece_contains_slot(piece: &RenderPiece) -> bool {
    match piece {
        RenderPiece::Slot(_) => true,

        RenderPiece::DynamicExpression(dynamic) => expression_contains_slot(&dynamic.expression),

        RenderPiece::ChildTemplate(child) => expression_contains_slot(&child.expression),

        RenderPiece::Text(_)
        | RenderPiece::HeadContent(_)
        | RenderPiece::LoopControl(_)
        | RenderPiece::RuntimeSlotSite(_) => false,
    }
}

fn render_piece_contains_no_value_expression(piece: &RenderPiece) -> bool {
    match piece {
        RenderPiece::DynamicExpression(dynamic) => {
            expression_contains_no_value(&dynamic.expression)
        }

        RenderPiece::ChildTemplate(child) => expression_contains_no_value(&child.expression),

        RenderPiece::Text(_)
        | RenderPiece::HeadContent(_)
        | RenderPiece::LoopControl(_)
        | RenderPiece::Slot(_)
        | RenderPiece::RuntimeSlotSite(_) => false,
    }
}

fn count_loop_control_signals(content: &TemplateContent) -> usize {
    content
        .atoms
        .iter()
        .filter(|atom| match atom {
            TemplateAtom::Content(segment) => matches!(
                &segment.expression.kind,
                ExpressionKind::Template(template)
                    if matches!(template.control_flow, Some(TemplateControlFlow::LoopControl(_)))
            ),
            TemplateAtom::Slot(_) => false,
        })
        .count()
}

fn expression_contains_slot(expression: &Expression) -> bool {
    let ExpressionKind::Template(template) = &expression.kind else {
        return false;
    };

    template
        .content
        .atoms
        .iter()
        .any(template_atom_contains_slot)
}

fn expression_contains_no_value(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::NoValue => true,

        ExpressionKind::Template(template) => template
            .content
            .atoms
            .iter()
            .any(template_atom_contains_no_value_expression),

        _ => false,
    }
}

fn template_atom_contains_no_value_expression(atom: &TemplateAtom) -> bool {
    let TemplateAtom::Content(segment) = atom else {
        return false;
    };

    expression_contains_no_value(&segment.expression)
}

fn template_atom_contains_slot(atom: &TemplateAtom) -> bool {
    match atom {
        TemplateAtom::Slot(_) => true,
        TemplateAtom::Content(segment) => expression_contains_slot(&segment.expression),
    }
}

fn find_first_control_flow_child(content: &TemplateContent) -> Option<&TemplateControlFlow> {
    content.atoms.iter().find_map(|atom| {
        let TemplateAtom::Content(segment) = atom else {
            return None;
        };

        let ExpressionKind::Template(child_template) = &segment.expression.kind else {
            return None;
        };

        child_template.control_flow.as_ref()
    })
}

fn expect_branch_chain(template: &Template) -> &TemplateBranchChain {
    let Some(TemplateControlFlow::BranchChain(branch_chain)) = &template.control_flow else {
        panic!("expected template branch-chain control flow");
    };

    branch_chain
}

fn expect_branch_chain_control_flow(control_flow: &TemplateControlFlow) -> &TemplateBranchChain {
    let TemplateControlFlow::BranchChain(branch_chain) = control_flow else {
        panic!("expected template branch-chain control flow");
    };

    branch_chain
}

fn first_branch_content(branch_chain: &TemplateBranchChain) -> &TemplateContent {
    &branch_chain
        .branches
        .first()
        .expect("branch chain should contain a primary branch")
        .content
}

fn fallback_content(branch_chain: &TemplateBranchChain) -> &TemplateContent {
    &branch_chain
        .fallback
        .as_ref()
        .expect("branch chain should contain fallback")
        .content
}

fn find_first_control_flow_template_child(content: &TemplateContent) -> Option<&Template> {
    content.atoms.iter().find_map(|atom| {
        let TemplateAtom::Content(segment) = atom else {
            return None;
        };

        let ExpressionKind::Template(child_template) = &segment.expression.kind else {
            return None;
        };

        child_template
            .is_control_flow_template()
            .then_some(child_template.as_ref())
    })
}

fn atom_is_external_wrapper_around_control_flow_child(atom: &TemplateAtom) -> bool {
    let TemplateAtom::Content(segment) = atom else {
        return false;
    };

    let ExpressionKind::Template(template) = &segment.expression.kind else {
        return false;
    };

    if template.is_control_flow_template() {
        return false;
    }

    find_first_control_flow_template_child(&template.content).is_some()
}
