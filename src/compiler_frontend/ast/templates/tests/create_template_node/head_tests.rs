use super::*;
use crate::compiler_frontend::ast::const_values::resolver::classify_template_from_effective_tir;
use crate::compiler_frontend::ast::expressions::expression_types::ConstRecordState;
use crate::compiler_frontend::ast::statements::match_patterns::MatchPattern;
use crate::compiler_frontend::ast::templates::template::{
    SlotKey, TemplateConstValueKind, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_body_parser::{
    NestedTemplateParseOptions, TemplateBodyParseRequest, parse_template_body,
};
use crate::compiler_frontend::ast::templates::template_body_sentinels::TemplateBodyControlContext;
use crate::compiler_frontend::ast::templates::template_build_state::TemplateBuildState;
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchChain, TemplateBranchSelector, TemplateControlFlow,
    TemplateControlFlowValidationMode, TemplateLoopControlFlow, TemplateLoopHeader,
    validate_const_required_template_control_flow,
    validate_runtime_template_control_flow_slot_artifacts,
};
use crate::compiler_frontend::ast::templates::template_head_parser::{
    TemplateHeadParseRequest, parse_template_head,
};
use crate::compiler_frontend::ast::templates::tir::{
    ExpressionSiteId, SlotOccurrenceId, TemplateConstructionContext, TemplateIrBranch,
    TemplateIrBuilder, TemplateIrNodeId, TemplateIrNodeKind, TemplateIrRegistry, TemplateIrStore,
    TemplateIrSummary, TemplateLoopHeaderExpressionSites, TemplateOverlaySet, TemplateRef,
    TemplateTirBodyReference, TemplateTirPhase, TemplateTirReference, TirExpressionOverlay,
    TirSlotResolution, TirSlotResolutionOverlay,
};
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_messages::{
    DiagnosticPayload, InvalidTemplateStructureReason, NameNamespace,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::definitions::{FieldDefinition, StructTypeDefinition};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::{NominalTypeId, TypeId, builtin_type_ids};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{CharPosition, FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::compatibility::TypeCompatibilityCache;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

#[test]
fn template_head_unknown_symbol_reports_unknown_value_name_not_unexpected_token() {
    // Unknown names in a template head should produce a structured UnknownName
    // diagnostic, not a generic UnexpectedToken. This is the improvement from
    // routing symbol-led head items through the ordinary expression parser.
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source("[unknown_name]", &mut string_table);
    let context = runtime_template_context(&token_stream.src_path.clone(), &mut string_table);

    let diagnostic = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("unknown name in template head should fail");

    let unknown_name = string_table.intern("unknown_name");
    assert!(
        matches!(
            diagnostic.payload,
            DiagnosticPayload::UnknownName {
                name,
                namespace: NameNamespace::Value,
            } if name == unknown_name
        ),
        "expected UnknownName for unknown symbol in template head, got: {:?}",
        diagnostic.payload
    );
}

#[test]
fn truncated_template_head_stream_returns_missing_closing_delimiter() {
    let mut string_table = StringTable::new();
    let scope = InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let context = new_constant_context(scope.to_owned());

    let mut token_stream = FileTokens::new(
        scope,
        vec![
            token(TokenKind::TemplateHead, 1),
            numeric_token("3", 1, &mut string_table),
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
            numeric_token("3", 1, &mut string_table),
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
fn parsed_template_tir_reference_carries_registry_empty_overlay_set() {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source("[: body]", &mut string_table);
    let context = new_constant_context(token_stream.src_path.clone());

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("template source should parse");
    let reference = template
        .tir_reference
        .as_ref()
        .expect("parsed template should carry a TIR reference");

    let registry = context.template_ir_registry.borrow();
    let overlay_set = registry
        .overlay_set(reference.overlay_set_id)
        .expect("parsed reference overlay set should resolve in the registry");

    assert_eq!(overlay_set, &TemplateOverlaySet::empty());
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
    let (template, context, string_table) = parse_runtime_template("[if true: Visible]");

    let branch_chain = expect_branch_chain(&template);

    assert_body_ref_static_contains(
        first_branch_body_ref(branch_chain),
        &context,
        &string_table,
        "Visible",
    );
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

    // Unknown names in template heads now produce a structured UnknownName
    // diagnostic instead of a generic UnexpectedToken, which is the intended
    // improvement from routing symbol-led head items through create_expression.
    assert!(
        matches!(
            diagnostic.payload,
            DiagnosticPayload::UnknownName {
                name,
                namespace: NameNamespace::Value,
            } if name == capture_name
        ),
        "unexpected payload: {:?}",
        diagnostic.payload
    );
}

#[test]
fn source_authored_template_range_loop_suffix_reaches_ast() {
    let (template, _context, _unused_table) = parse_runtime_template("[loop 0 to 3 |i|: [i]]");

    assert!(matches!(
        template.control_flow,
        Some(TemplateControlFlow::Loop(_))
    ));
}

#[test]
fn source_authored_template_conditional_loop_suffix_reaches_ast() {
    let (template, _context, _unused_table) = parse_runtime_template("[loop true: Waiting]");

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
    let (template, context, string_table) = parse_control_flow_template_after_body_parse(
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
    assert_body_ref_static_contains(
        branch_body_ref(branch_chain, 0),
        &context,
        &string_table,
        "First",
    );
    assert_body_ref_static_contains(
        branch_body_ref(branch_chain, 1),
        &context,
        &string_table,
        "Second",
    );
    assert_body_ref_static_contains(
        branch_body_ref(branch_chain, 2),
        &context,
        &string_table,
        "Third",
    );
    assert_body_ref_static_contains(
        fallback_body_ref(branch_chain),
        &context,
        &string_table,
        "Fallback",
    );
}

#[test]
fn nested_template_else_if_builds_independent_branch_chains() {
    let (template, context, string_table) = parse_control_flow_template_after_body_parse(
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

    assert_body_ref_static_contains(
        first_branch_body_ref(outer_chain),
        &context,
        &string_table,
        "Inner second",
    );
    assert_body_ref_static_contains(
        first_branch_body_ref(outer_chain),
        &context,
        &string_table,
        "Inner fallback",
    );
    assert_body_ref_static_contains(
        fallback_body_ref(outer_chain),
        &context,
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

    // Unknown names in template heads now produce a structured UnknownName
    // diagnostic instead of a generic UnexpectedToken.
    assert!(
        matches!(
            diagnostic.payload,
            DiagnosticPayload::UnknownName {
                name,
                namespace: NameNamespace::Value,
            } if name == capture_name
        ),
        "unexpected payload: {:?}",
        diagnostic.payload
    );
}

#[test]
fn template_if_body_splits_on_direct_else_sentinel() {
    let (template, context, string_table) = parse_control_flow_template_after_body_parse(
        "[if true:
            Visible
        [else]
            Hidden
        ]",
    );

    let branch_chain = expect_branch_chain(&template);

    assert_body_ref_static_contains(
        first_branch_body_ref(branch_chain),
        &context,
        &string_table,
        "Visible",
    );
    assert_body_ref_static_contains(
        fallback_body_ref(branch_chain),
        &context,
        &string_table,
        "Hidden",
    );
    assert_body_ref_static_excludes(
        first_branch_body_ref(branch_chain),
        &context,
        &string_table,
        "else",
    );
    assert_body_ref_static_excludes(
        fallback_body_ref(branch_chain),
        &context,
        &string_table,
        "else",
    );
}

#[test]
fn nested_template_if_consumes_its_own_else_sentinel() {
    let (template, context, string_table) = parse_control_flow_template_after_body_parse(
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

    assert_body_ref_static_contains(
        first_branch_body_ref(outer_if),
        &context,
        &string_table,
        "Inner else",
    );
    assert_body_ref_static_contains(
        fallback_body_ref(outer_if),
        &context,
        &string_table,
        "Outer else",
    );
}

#[test]
fn template_loop_body_stores_normal_body_content() {
    let (template, context, string_table) =
        parse_control_flow_template_after_body_parse("[loop 0 to 3 |i|: Item [i]]");

    let Some(TemplateControlFlow::Loop(template_loop)) = template.control_flow else {
        panic!("expected template loop control flow");
    };

    assert_body_ref_static_contains(
        loop_body_ref(&template_loop),
        &context,
        &string_table,
        "Item",
    );
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
    let (template, context, _string_table) = parse_control_flow_template_after_body_parse(
        "[if true:
            [$slot]
        [else]
            Hidden
        ]",
    );

    let branch_chain = expect_branch_chain(&template);

    assert!(
        body_ref_contains_unresolved_slots(first_branch_body_ref(branch_chain), &context),
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
    let (template, context, _unused_table) = parse_control_flow_template_after_body_parse(
        "[loop 0 to 2 |i|:
            [break]
        ]",
    );

    let Some(TemplateControlFlow::Loop(template_loop)) = template.control_flow else {
        panic!("expected template loop control flow");
    };

    assert_eq!(
        body_ref_loop_control_signal_count(loop_body_ref(&template_loop), &context),
        1
    );
}

#[test]
fn template_loop_continue_is_structural_body_signal() {
    let (template, context, _unused_table) = parse_control_flow_template_after_body_parse(
        "[loop 0 to 2 |i|:
            [continue]
        ]",
    );

    let Some(TemplateControlFlow::Loop(template_loop)) = template.control_flow else {
        panic!("expected template loop control flow");
    };

    assert_eq!(
        body_ref_loop_control_signal_count(loop_body_ref(&template_loop), &context),
        1
    );
}

#[test]
fn nested_template_if_break_inside_loop_is_structural_signal() {
    let (template, context, _unused_table) = parse_control_flow_template_after_body_parse(
        "[loop 0 to 2 |i|:
            [if true:
                [break]
            ]
        ]",
    );

    let Some(TemplateControlFlow::Loop(template_loop)) = template.control_flow else {
        panic!("expected template loop control flow");
    };
    assert_eq!(
        body_ref_loop_control_signal_count(loop_body_ref(&template_loop), &context),
        1
    );
}

#[test]
fn nested_template_if_continue_inside_loop_is_structural_signal() {
    let (template, context, _unused_table) = parse_control_flow_template_after_body_parse(
        "[loop 0 to 2 |i|:
            [if true:
                [continue]
            ]
        ]",
    );

    let Some(TemplateControlFlow::Loop(template_loop)) = template.control_flow else {
        panic!("expected template loop control flow");
    };
    assert_eq!(
        body_ref_loop_control_signal_count(loop_body_ref(&template_loop), &context),
        1
    );
}

#[test]
fn nested_template_if_inside_loop_consumes_its_own_else_sentinel() {
    let (template, context, string_table) = parse_control_flow_template_after_body_parse(
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

    assert_body_ref_static_contains(
        loop_body_ref(&template_loop),
        &context,
        &string_table,
        "Inner else",
    );
}

#[test]
fn template_if_composition_formats_each_branch_independently() {
    let (template, context, string_table) = parse_control_flow_template_after_composition(
        "[$md, if true:
            # Visible
        [else]
            # Hidden
        ]",
    );

    let branch_chain = expect_branch_chain(&template);

    assert_body_ref_static_contains(
        first_branch_body_ref(branch_chain),
        &context,
        &string_table,
        "<h1>Visible</h1>",
    );

    assert_body_ref_static_contains(
        fallback_body_ref(branch_chain),
        &context,
        &string_table,
        "<h1>Hidden</h1>",
    );
    assert_body_ref_static_excludes(
        first_branch_body_ref(branch_chain),
        &context,
        &string_table,
        "Hidden",
    );
    assert_body_ref_static_excludes(
        fallback_body_ref(branch_chain),
        &context,
        &string_table,
        "Visible",
    );
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
    let context = constant_template_context(&token_stream.src_path, &declarations)
        .with_template_ir_registry(
            Rc::clone(&card_context.template_ir_registry),
            card_context.template_ir_store_id,
            Rc::clone(&card_context.template_ir_store),
        );

    let mut type_environment = TypeEnvironment::new();
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);

    let template = Template::new_nested_template(
        &mut token_stream,
        &context,
        &mut type_interner,
        Vec::new(),
        &mut string_table,
        NestedTemplateParseOptions::runtime_capable(),
    )
    .expect("template if should parse through control-flow composition");

    let branch_chain = expect_branch_chain(&template);

    assert_body_ref_static_contains(
        first_branch_body_ref(branch_chain),
        &context,
        &string_table,
        "<card>",
    );
    assert_body_ref_static_contains(
        first_branch_body_ref(branch_chain),
        &context,
        &string_table,
        "Visible",
    );

    assert_body_ref_static_contains(
        fallback_body_ref(branch_chain),
        &context,
        &string_table,
        "<card>",
    );
    assert_body_ref_static_contains(
        fallback_body_ref(branch_chain),
        &context,
        &string_table,
        "Hidden",
    );
}

#[test]
fn template_loop_composition_formats_body_without_repeating_shared_head_prefix() {
    let (template, context, string_table) = parse_control_flow_template_after_composition(
        "[\"prefix\", $md, loop 0 to 3 |i|:
            # Item
        ]",
    );

    let Some(TemplateControlFlow::Loop(template_loop)) = template.control_flow else {
        panic!("expected template loop control flow");
    };

    assert_body_ref_static_contains(
        template_loop
            .aggregate_wrapper_tir_reference
            .as_ref()
            .expect("loop should preserve its shared head prefix in aggregate-wrapper TIR"),
        &context,
        &string_table,
        "prefix",
    );
    assert_body_ref_static_contains(
        loop_body_ref(&template_loop),
        &context,
        &string_table,
        "<h1>Item</h1>",
    );
    assert_body_ref_static_excludes(
        loop_body_ref(&template_loop),
        &context,
        &string_table,
        "prefix",
    );
}

#[test]
fn parent_children_wrappers_attach_conditionally_to_control_flow_child() {
    let (template, context, _unused_table) = parse_control_flow_template_after_composition(
        "[$children([:<li>[$slot]</li>]):
            [if true:
                item
            ]
        ]",
    );

    // The control-flow child is TIR-owned: the parent's TIR root contains it as
    // a ChildTemplate node, and the $children wrapper is attached via wrapper-
    // context overlays rather than as an external content-mirror wrapper.
    let store = context.template_ir_store.borrow();
    assert!(
        tir_root_has_control_flow_child(&template, &store),
        "TIR root should contain the control-flow child template"
    );
}

#[test]
fn fresh_control_flow_child_skips_parent_children_wrapper() {
    let (template, context, _unused_table) = parse_control_flow_template_after_composition(
        "[$children([:<li>[$slot]</li>]):
            [$fresh, if true:
                item
            ]
        ]",
    );

    let store = context.template_ir_store.borrow();
    assert!(
        tir_root_has_control_flow_child(&template, &store),
        "TIR root should contain the $fresh control-flow child template"
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
fn runtime_template_loop_with_continue_preserves_loop_control_signal() {
    let (template, context, _string_table) = parse_runtime_template(
        "[loop 0 to 2 |i|:
            <li>before [i]</li>
            [continue]
            <li>after [i]</li>
        ]",
    );

    let loop_control_flow = template
        .control_flow
        .as_ref()
        .expect("template should have control flow");
    let TemplateControlFlow::Loop(loop_cf) = loop_control_flow else {
        panic!("expected a loop control flow");
    };
    assert_eq!(
        body_ref_loop_control_signal_count(loop_body_ref(loop_cf), &context),
        1,
        "loop body should contain a loop control signal",
    );
}

#[test]
fn runtime_template_loop_with_continue_inside_parent_parses() {
    let (template, context, _string_table) = parse_runtime_template(
        "[:
            outer
            [loop 0 to 2 |i|:
                <li>before [i]</li>
                [continue]
                <li>after [i]</li>
            ]
        ]",
    );

    let store = context.template_ir_store.borrow();
    assert!(
        tir_root_has_control_flow_child(&template, &store),
        "outer template TIR root should contain the nested loop"
    );
}

#[test]
fn runtime_template_loop_with_continue_as_slot_fill_parses() {
    let mut string_table = StringTable::new();
    let mut shell_tokens = template_tokens_from_source("[:<ul>[$slot]</ul>]", &mut string_table);
    let shell_context = new_constant_context(shell_tokens.src_path.to_owned());
    let shell_template =
        Template::new(&mut shell_tokens, &shell_context, vec![], &mut string_table)
            .expect("slot shell should parse");

    let mut token_stream = template_tokens_from_source(
        "[:
            before
            [list_shell, loop keep_going:
                [break]
                <li>hidden</li>
            ]
            after
        ]",
        &mut string_table,
    );
    let scope = token_stream.src_path.clone();
    let list_shell_name = string_table.intern("list_shell");
    let keep_going_name = string_table.intern("keep_going");
    let declaration = Declaration {
        id: scope.append(list_shell_name),
        value: Expression::template(shell_template, ValueMode::ImmutableOwned),
    };
    let condition_declaration = Declaration {
        id: scope.append(keep_going_name),
        value: Expression::new(
            ExpressionKind::NoValue,
            token_stream.current_location(),
            builtin_type_ids::BOOL,
            DataType::Bool,
            ValueMode::ImmutableOwned,
        ),
    };
    let context = with_test_path_context(
        ScopeContext::new(
            ContextKind::Template,
            scope.to_owned(),
            Rc::new(TopLevelDeclarationTable::new(vec![
                declaration,
                condition_declaration,
            ])),
            Arc::new(ExternalPackageRegistry::default()),
            vec![],
            0,
        ),
        &scope,
        &frontend_test_style_directives(),
    )
    .with_template_ir_registry(
        Rc::clone(&shell_context.template_ir_registry),
        shell_context.template_ir_store_id,
        Rc::clone(&shell_context.template_ir_store),
    );

    Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("slot-fill loop should parse");
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
    let (template, context, _string_table) = parse_const_required_template(
        "[if true:
            [$slot]
        ]",
    );

    let branch_chain = expect_branch_chain(&template);

    assert!(
        body_ref_contains_unresolved_slots(first_branch_body_ref(branch_chain), &context),
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
    let mut context = constant_template_context(&token_stream.src_path, &declarations);
    // Production scopes in one module share one TIR registry and store. Keep
    // the declaration fixture on that topology so the wrapper reference stays
    // resolvable without compatibility reconstruction.
    context.template_ir_store = Rc::clone(&card_context.template_ir_store);
    context.template_ir_store_id = card_context.template_ir_store_id;
    context.template_ir_registry = Rc::clone(&card_context.template_ir_registry);
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

    {
        assert_eq!(
            classify_template_from_effective_tir(
                &template,
                &context.template_ir_registry,
                &string_table,
            )
            .expect("const classification should succeed"),
            TemplateConstValueKind::RenderableString
        );
    }

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

    {
        assert_eq!(
            classify_template_from_effective_tir(
                &template,
                &context.template_ir_registry,
                &string_table,
            )
            .expect("const classification should succeed"),
            TemplateConstValueKind::RenderableString
        );
    }

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

    {
        assert_eq!(
            classify_template_from_effective_tir(
                &template,
                &context.template_ir_registry,
                &string_table,
            )
            .expect("const classification should succeed"),
            TemplateConstValueKind::RenderableString
        );
    }

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

    {
        assert_eq!(
            classify_template_from_effective_tir(
                &template,
                &context.template_ir_registry,
                &string_table,
            )
            .expect("const classification should succeed"),
            TemplateConstValueKind::RenderableString
        );
    }

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
    let context = constant_template_context(&token_stream.src_path, &declarations)
        .with_template_ir_registry(
            Rc::clone(&card_context.template_ir_registry),
            card_context.template_ir_store_id,
            Rc::clone(&card_context.template_ir_store),
        );
    let template =
        Template::new_const_required(&mut token_stream, &context, vec![], &mut string_table)
            .expect("const-required zero loop should parse");

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
    let context = constant_template_context(&token_stream.src_path, &declarations)
        .with_template_ir_registry(
            Rc::clone(&card_context.template_ir_registry),
            card_context.template_ir_store_id,
            Rc::clone(&card_context.template_ir_store),
        );
    let template =
        Template::new_const_required(&mut token_stream, &context, vec![], &mut string_table)
            .expect("const-required loop should parse");

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
fn const_required_template_if_validates_from_body_tir_roots() {
    let (template, context, string_table) = parse_const_required_template(
        "[if true:
            Visible
        [else]
            Hidden
        ]",
    );

    let registry = context.template_ir_registry.borrow();
    validate_const_required_template_control_flow(&template, &registry, &string_table)
        .expect("const-required branch validation should use same-store TIR body roots");
}

#[test]
fn const_required_template_loop_validates_from_body_tir_root() {
    let (template, context, string_table) = parse_const_required_template(
        "[loop 0 to 1 |i|:
            [i]
        ]",
    );

    let registry = context.template_ir_registry.borrow();
    validate_const_required_template_control_flow(&template, &registry, &string_table)
        .expect("const-required loop validation should use same-store TIR body roots");
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

    let template = const_required_option_capture_template_with_direct_tir(
        scrutinee,
        capture_name,
        capture_path,
        string_type_id,
        &context,
        &mut string_table,
    );

    {
        let registry = context.template_ir_registry.borrow();
        validate_const_required_template_control_flow(&template, &registry, &string_table)
            .expect("present const option capture should validate");
    }

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

    let template = const_required_option_capture_template_with_direct_tir(
        scrutinee,
        capture_name,
        capture_path,
        string_type_id,
        &context,
        &mut string_table,
    );

    {
        let registry = context.template_ir_registry.borrow();
        validate_const_required_template_control_flow(&template, &registry, &string_table)
            .expect("absent const option capture should validate");
    }

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
fn const_required_option_capture_classifies_foreign_source_const_template_through_registry() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[if maybe_name is |name|:Hello [name]]", &mut string_table);
    let maybe_name = string_table.intern("maybe_name");

    let mut type_environment = TypeEnvironment::new();
    let string_type_id = type_environment.builtins().string;
    let option_string_type_id = type_environment.intern_option(string_type_id);
    let mut template_ir_registry = TemplateIrRegistry::new();
    let primary_store_id = template_ir_registry.allocate_store();
    let primary_store = template_ir_registry
        .store_handle(primary_store_id)
        .expect("test registry should own its primary store");
    let foreign_store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let (foreign_store_id, overlay_set_id) = {
        let store_id = template_ir_registry.adopt_store(Rc::clone(&foreign_store));
        let overlay_set_id = template_ir_registry.allocate_overlay_set(TemplateOverlaySet::empty());
        (store_id, overlay_set_id)
    };

    let payload_text = string_table.intern("Ada");
    let payload_template_id = {
        let mut foreign_store = foreign_store.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut foreign_store);
        let root = builder.push_text_node(
            payload_text,
            3,
            TemplateSegmentOrigin::Body,
            token_stream.current_location(),
        );
        builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::default(),
            token_stream.current_location(),
        )
    };

    let mut payload_template = Template::empty();
    payload_template.kind = TemplateType::String;
    payload_template.tir_reference = Some(TemplateTirReference {
        root: TemplateRef::new(foreign_store_id, payload_template_id),
        store_owner: foreign_store.borrow().owner(),
        is_composed: true,
        phase: TemplateTirPhase::Composed,
        overlay_set_id,
    });

    let declaration = Declaration {
        id: token_stream.src_path.append(maybe_name),
        value: Expression::coerced(
            Expression::template(payload_template, ValueMode::ImmutableOwned),
            option_string_type_id,
        ),
    };
    let context = constant_template_context(&token_stream.src_path, &[declaration])
        .with_template_ir_registry(
            Rc::new(RefCell::new(template_ir_registry)),
            primary_store_id,
            primary_store,
        );
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);

    let template = Template::new_const_required_with_type_interner(
        &mut token_stream,
        &context,
        &mut type_interner,
        vec![],
        &mut string_table,
    )
    .expect("registry-backed source const template should be accepted through effective TIR");

    assert!(
        template.tir_reference.is_some(),
        "accepted const-required template should retain its authoritative TIR reference"
    );
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

/// Builds a const-required option-capture template fixture directly as a
/// same-store TIR branch-chain root in the context's module store.
///
/// WHAT: constructs the branch body (text "Hello " plus the capture reference)
///       and fallback body (text "Guest") as TIR nodes, wraps them in a
///       `BranchChain` node, finishes the template record, and returns a
///       `Template` whose `tir_reference` points at that root.
/// WHY: manual fixtures no longer need detached content or its TIR materializer.
///      Validation and folding already consume the authoritative
///      branch-chain root through the registry-backed `TirView`.
fn const_required_option_capture_template_with_direct_tir(
    scrutinee: Expression,
    capture_name: StringId,
    capture_path: InternedPath,
    inner_type_id: TypeId,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Template {
    let location = SourceLocation::default();

    let capture_reference = Expression::reference_with_type_id(
        capture_path.clone(),
        DataType::StringSlice,
        inner_type_id,
        location.clone(),
        ValueMode::ImmutableOwned,
        ConstRecordState::RuntimeValue,
    );

    let hello_id = string_table.intern("Hello ");
    let guest_id = string_table.intern("Guest");

    let store_handle = context.template_ir_store();

    let (template_id, store_owner, store_id) = {
        let mut store = store_handle.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut store);

        let hello_node = builder.push_text_node(
            hello_id,
            "Hello ".len() as u32,
            TemplateSegmentOrigin::Body,
            location.clone(),
        );
        let capture_node = builder.push_dynamic_expression_node(
            capture_reference,
            TemplateSegmentOrigin::Body,
            None,
            location.clone(),
        );
        let branch_body =
            builder.push_sequence_node(vec![hello_node, capture_node], location.clone());

        let guest_node = builder.push_text_node(
            guest_id,
            "Guest".len() as u32,
            TemplateSegmentOrigin::Body,
            location.clone(),
        );
        let fallback_body = builder.push_sequence_node(vec![guest_node], location.clone());

        let selector = TemplateBranchSelector::OptionPresentCapture {
            scrutinee,
            pattern: Box::new(MatchPattern::OptionPresentCapture {
                name: capture_name,
                binding_path: capture_path,
                inner_type_id,
                location: location.clone(),
                binding_location: location.clone(),
            }),
        };
        let branch = TemplateIrBranch::new(selector, branch_body, location.clone());
        let branch_chain_root =
            builder.push_branch_chain_node(vec![branch], Some(fallback_body), location.clone());

        let summary = TemplateIrSummary {
            estimated_output_bytes: "Hello ".len() + "Guest".len(),
            text_node_count: 2,
            text_byte_count: "Hello ".len() + "Guest".len(),
            dynamic_expression_count: 1,
            max_depth: 2,
            has_control_flow: true,
            is_const_evaluable_shape: false,
            ..TemplateIrSummary::default()
        };

        let template_id = builder.finish_template(
            branch_chain_root,
            Style::default(),
            TemplateType::String,
            summary,
            location,
        );

        let store_owner = Arc::clone(&store.owner());
        let store_id = store.store_id();
        (template_id, store_owner, store_id)
    };

    let overlay_set_id = context
        .template_ir_registry
        .borrow_mut()
        .allocate_overlay_set(TemplateOverlaySet::empty());

    let mut template = Template::empty();
    template.kind = TemplateType::String;
    template.tir_reference = Some(TemplateTirReference {
        root: TemplateRef::new(store_id, template_id),
        store_owner,
        is_composed: false,
        phase: TemplateTirPhase::Composed,
        overlay_set_id,
    });

    template
}

fn parse_template_error(
    source: &str,
) -> crate::compiler_frontend::compiler_messages::CompilerDiagnostic {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(source, &mut string_table);
    let context = new_constant_context(token_stream.src_path.clone());

    *Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("template source should fail")
}

fn parse_runtime_template(source: &str) -> (Template, ScopeContext, StringTable) {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(source, &mut string_table);
    let context = new_constant_context(token_stream.src_path.clone());

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("template source should parse");

    (template, context, string_table)
}

fn parse_control_flow_template_after_body_parse(
    source: &str,
) -> (Template, ScopeContext, StringTable) {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(source, &mut string_table);
    let context = new_constant_context(token_stream.src_path.clone());

    let mut type_environment = TypeEnvironment::new();
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);

    let mut build_state = TemplateBuildState::new();

    let mut construction_context = TemplateConstructionContext::new(
        Rc::clone(&context.template_ir_store),
        context.template_ir_store_id,
        Rc::clone(&context.template_ir_registry),
        token_stream.current_location(),
    );

    let parsed_head = parse_template_head(
        &mut token_stream,
        TemplateHeadParseRequest {
            context: &context,
            type_interner: &mut type_interner,
            build_state: &mut build_state,
            construction_context: &mut construction_context,
            control_flow_validation: TemplateControlFlowValidationMode::RuntimeCapable,
            string_table: &mut string_table,
        },
    )
    .expect("template head should parse");

    parse_template_body(
        &mut token_stream,
        &mut build_state,
        &mut construction_context,
        TemplateBodyParseRequest {
            context: &context,
            type_interner: &mut type_interner,
            body_mode: parsed_head.body_mode,
            direct_child_wrappers: &[],
            control_flow_validation: TemplateControlFlowValidationMode::RuntimeCapable,
            control_context: TemplateBodyControlContext::normal(),
            string_table: &mut string_table,
            default_style: None,
        },
    )
    .expect("template body should parse");

    let template = Template {
        control_flow: build_state.control_flow,
        kind: build_state.kind,
        style: build_state.style,
        child_wrappers: build_state.child_wrappers,
        tir_reference: None,
        id: build_state.id,
        location: construction_context.location().to_owned(),
    };

    (template, context, string_table)
}

fn parse_control_flow_template_after_composition(
    source: &str,
) -> (Template, ScopeContext, StringTable) {
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
        Vec::new(),
        &mut string_table,
        NestedTemplateParseOptions::runtime_capable(),
    )
    .expect("control-flow template should parse through composition");

    (template, context, string_table)
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

    *Template::new_nested_template(
        &mut token_stream,
        &context,
        &mut type_interner,
        Vec::new(),
        &mut string_table,
        NestedTemplateParseOptions::runtime_capable(),
    )
    .expect_err("control-flow template should fail during composition")
}

fn parse_runtime_template_without_validation(
    source: &str,
) -> (Template, ScopeContext, StringTable) {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(source, &mut string_table);
    let context = new_constant_context(token_stream.src_path.clone());

    let mut type_environment = TypeEnvironment::new();
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);

    let mut build_state = TemplateBuildState::new();

    let mut construction_context = TemplateConstructionContext::new(
        Rc::clone(&context.template_ir_store),
        context.template_ir_store_id,
        Rc::clone(&context.template_ir_registry),
        token_stream.current_location(),
    );

    let parsed_head = parse_template_head(
        &mut token_stream,
        TemplateHeadParseRequest {
            context: &context,
            type_interner: &mut type_interner,
            build_state: &mut build_state,
            construction_context: &mut construction_context,
            control_flow_validation: TemplateControlFlowValidationMode::RuntimeCapable,
            string_table: &mut string_table,
        },
    )
    .expect("template head should parse");

    parse_template_body(
        &mut token_stream,
        &mut build_state,
        &mut construction_context,
        TemplateBodyParseRequest {
            context: &context,
            type_interner: &mut type_interner,
            body_mode: parsed_head.body_mode,
            direct_child_wrappers: &[],
            control_flow_validation: TemplateControlFlowValidationMode::RuntimeCapable,
            control_context: TemplateBodyControlContext::normal(),
            string_table: &mut string_table,
            default_style: None,
        },
    )
    .expect("template body should parse");

    // Finish the construction context to install a registry-backed `tir_reference`
    // without running render-unit preparation or runtime validation. This lets
    // focused tests call the view-based runtime validator directly.
    let style = build_state.style.to_owned();
    let kind = build_state.kind.to_owned();
    let location = construction_context.location().to_owned();
    let tir_reference =
        construction_context.finish(style, kind, TemplateTirPhase::Parsed, location);

    let template = Template {
        control_flow: build_state.control_flow,
        kind: build_state.kind,
        style: build_state.style,
        child_wrappers: build_state.child_wrappers,
        tir_reference: Some(tir_reference),
        id: build_state.id,
        location: construction_context.location().to_owned(),
    };

    (template, context, string_table)
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

    *Template::new_const_required(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("const-required template source should fail")
}

#[test]
fn const_required_template_if_validates_branch_condition_through_tir_view_overlay() {
    let (mut template, context, mut string_table) = parse_const_required_template(
        "[if true:
            Visible
        ]",
    );

    let mut registry = context.template_ir_registry.borrow_mut();
    let site_id = find_first_branch_selector_site_id(&template, &registry)
        .expect("parsed const-required branch should have a selector site");

    let override_location = SourceLocation::new(
        template.location.scope.clone(),
        CharPosition {
            line_number: 99,
            char_column: 1,
        },
        CharPosition {
            line_number: 99,
            char_column: 5,
        },
    );
    let runtime_condition = Expression::reference_with_type_id(
        InternedPath::from_single_str("runtime_condition", &mut string_table),
        DataType::Bool,
        builtin_type_ids::BOOL,
        override_location.clone(),
        ValueMode::ImmutableReference,
        ConstRecordState::RuntimeValue,
    );

    install_expression_overlay_on_template(
        &mut template,
        &mut registry,
        site_id,
        runtime_condition,
    );

    drop(registry);
    let registry = context.template_ir_registry.borrow();
    let error = validate_const_required_template_control_flow(&template, &registry, &string_table)
        .expect_err("TirView overlay should make the branch condition non-const");

    assert_invalid_template_structure(
        &error,
        InvalidTemplateStructureReason::TemplateIfConditionNotConst,
    );
    assert_eq!(error.primary_location, override_location);
}

#[test]
fn const_required_template_loop_validates_header_through_tir_view_overlay() {
    let (mut template, context, string_table) = parse_const_required_template(
        "[loop false:
            body
        ]",
    );

    let mut registry = context.template_ir_registry.borrow_mut();
    let site_id = find_first_loop_header_site_id(&template, &registry)
        .expect("parsed const-required conditional loop should have a header site");

    let override_location = SourceLocation::new(
        template.location.scope.clone(),
        CharPosition {
            line_number: 99,
            char_column: 1,
        },
        CharPosition {
            line_number: 99,
            char_column: 5,
        },
    );
    let const_true_condition =
        Expression::bool(true, override_location.clone(), ValueMode::ImmutableOwned);

    install_expression_overlay_on_template(
        &mut template,
        &mut registry,
        site_id,
        const_true_condition,
    );

    drop(registry);
    let registry = context.template_ir_registry.borrow();
    let error = validate_const_required_template_control_flow(&template, &registry, &string_table)
        .expect_err("TirView overlay should turn the conditional loop into const true");

    assert_invalid_template_structure(
        &error,
        InvalidTemplateStructureReason::TemplateConditionalLoopConstTrue,
    );
    assert_eq!(error.primary_location, override_location);
}

#[test]
fn runtime_template_if_rejects_unresolved_slot_through_tir_view() {
    let (template, context, _string_table) =
        parse_runtime_template_without_validation("[if true:\n            [$slot]\n        ]");

    let registry = context.template_ir_registry.borrow();
    let store = context.template_ir_store.borrow();
    let expected_location = find_first_branch_location(&template, &registry)
        .expect("runtime branch should have a stable source location");

    let error =
        validate_runtime_template_control_flow_slot_artifacts(&template, &registry, &store, None)
            .expect_err("TirView path should report the unresolved slot in the branch body");

    let diagnostic = error.into_diagnostic();
    assert_invalid_template_structure(
        &diagnostic,
        InvalidTemplateStructureReason::RuntimeControlFlowUnresolvedSlot,
    );
    assert_eq!(diagnostic.primary_location, expected_location);
}

#[test]
fn runtime_template_if_rejects_unresolved_insert_through_tir_view() {
    let (template, context, _string_table) = parse_runtime_template_without_validation(
        "[if true:\n            [$insert(\"style\"): color: red;]\n        ]",
    );

    let registry = context.template_ir_registry.borrow();
    let store = context.template_ir_store.borrow();
    let expected_location = find_first_branch_location(&template, &registry)
        .expect("runtime branch should have a stable source location");

    let error =
        validate_runtime_template_control_flow_slot_artifacts(&template, &registry, &store, None)
            .expect_err("TirView path should report the escaped insert in the branch body");

    let diagnostic = error.into_diagnostic();
    assert_invalid_template_structure(
        &diagnostic,
        InvalidTemplateStructureReason::RuntimeControlFlowUnresolvedInsert,
    );
    assert_eq!(diagnostic.primary_location, expected_location);
}

#[test]
fn runtime_template_if_allows_resolved_slot_through_tir_view_overlay() {
    let (mut template, context, _string_table) =
        parse_runtime_template_without_validation("[if true:\n            [$slot]\n        ]");

    let mut registry = context.template_ir_registry.borrow_mut();
    let (occurrence_id, key) = find_first_slot_occurrence_id(&template, &registry)
        .expect("parsed runtime branch body should contain a slot occurrence");

    install_slot_resolution_overlay_on_template(
        &mut template,
        &mut registry,
        occurrence_id,
        TirSlotResolution::missing(key.clone()),
    );

    drop(registry);
    let registry = context.template_ir_registry.borrow();
    let store = context.template_ir_store.borrow();
    validate_runtime_template_control_flow_slot_artifacts(&template, &registry, &store, None)
        .expect("resolved slot overlay should suppress the unresolved-slot artifact");
}

fn find_first_branch_selector_site_id(
    template: &Template,
    registry: &TemplateIrRegistry,
) -> Option<ExpressionSiteId> {
    let reference = template.tir_reference.as_ref()?;
    let store = registry.store(reference.root.store_id)?;
    let template_ir = store.get_template(reference.root.template_id)?;
    find_branch_selector_site_id_in_subtree(&store, template_ir.root)
}

fn find_branch_selector_site_id_in_subtree(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
) -> Option<ExpressionSiteId> {
    let node = store.get_node(node_id)?;
    match &node.kind {
        TemplateIrNodeKind::BranchChain { branches, .. } => {
            branches.first().map(|branch| branch.selector_site_id)
        }
        TemplateIrNodeKind::Sequence { children } => children
            .iter()
            .find_map(|child| find_branch_selector_site_id_in_subtree(store, *child)),
        _ => None,
    }
}

fn find_first_branch_location(
    template: &Template,
    registry: &TemplateIrRegistry,
) -> Option<SourceLocation> {
    let reference = template.tir_reference.as_ref()?;
    let store = registry.store(reference.root.store_id)?;
    let template_ir = store.get_template(reference.root.template_id)?;
    find_branch_location_in_subtree(&store, template_ir.root)
}

fn find_branch_location_in_subtree(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
) -> Option<SourceLocation> {
    let node = store.get_node(node_id)?;
    match &node.kind {
        TemplateIrNodeKind::BranchChain { branches, .. } => {
            branches.first().map(|branch| branch.location.clone())
        }
        TemplateIrNodeKind::Sequence { children } => children
            .iter()
            .find_map(|child| find_branch_location_in_subtree(store, *child)),
        _ => None,
    }
}

fn find_first_loop_header_site_id(
    template: &Template,
    registry: &TemplateIrRegistry,
) -> Option<ExpressionSiteId> {
    let reference = template.tir_reference.as_ref()?;
    let store = registry.store(reference.root.store_id)?;
    let template_ir = store.get_template(reference.root.template_id)?;
    find_loop_header_site_id_in_subtree(&store, template_ir.root)
}

fn find_loop_header_site_id_in_subtree(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
) -> Option<ExpressionSiteId> {
    let node = store.get_node(node_id)?;
    match &node.kind {
        TemplateIrNodeKind::Loop {
            header_sites: TemplateLoopHeaderExpressionSites::Conditional { condition },
            ..
        } => Some(*condition),
        TemplateIrNodeKind::Sequence { children } => children
            .iter()
            .find_map(|child| find_loop_header_site_id_in_subtree(store, *child)),
        _ => None,
    }
}

fn install_expression_overlay_on_template(
    template: &mut Template,
    registry: &mut TemplateIrRegistry,
    site_id: ExpressionSiteId,
    expression: Expression,
) {
    let overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![(site_id, Box::new(expression))],
    });
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    });

    if let Some(reference) = &mut template.tir_reference {
        reference.phase = TemplateTirPhase::Finalized;
        reference.overlay_set_id = overlay_set_id;
    }
}

fn find_first_slot_occurrence_id(
    template: &Template,
    registry: &TemplateIrRegistry,
) -> Option<(SlotOccurrenceId, SlotKey)> {
    let reference = template.tir_reference.as_ref()?;
    let store = registry.store(reference.root.store_id)?;
    let template_ir = store.get_template(reference.root.template_id)?;
    find_slot_occurrence_id_in_subtree(&store, template_ir.root)
}

fn find_slot_occurrence_id_in_subtree(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
) -> Option<(SlotOccurrenceId, SlotKey)> {
    let node = store.get_node(node_id)?;
    match &node.kind {
        TemplateIrNodeKind::Slot { placeholder } => {
            Some((placeholder.occurrence_id, placeholder.key.clone()))
        }
        TemplateIrNodeKind::Sequence { children } => children
            .iter()
            .find_map(|child| find_slot_occurrence_id_in_subtree(store, *child)),
        TemplateIrNodeKind::BranchChain { branches, fallback } => branches
            .iter()
            .find_map(|branch| find_slot_occurrence_id_in_subtree(store, branch.body))
            .or_else(|| {
                fallback.and_then(|fallback| find_slot_occurrence_id_in_subtree(store, fallback))
            }),
        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => find_slot_occurrence_id_in_subtree(store, *body).or_else(|| {
            aggregate_wrapper.and_then(|wrapper| find_slot_occurrence_id_in_subtree(store, wrapper))
        }),
        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            let template_id = reference.template_id_in_store(store.store_id())?;
            let template_ir = store.get_template(template_id)?;
            find_slot_occurrence_id_in_subtree(store, template_ir.root)
        }
        TemplateIrNodeKind::InsertContribution { template } => {
            let template_ir = store.get_template(*template)?;
            find_slot_occurrence_id_in_subtree(store, template_ir.root)
        }
        _ => None,
    }
}

fn install_slot_resolution_overlay_on_template(
    template: &mut Template,
    registry: &mut TemplateIrRegistry,
    occurrence_id: SlotOccurrenceId,
    resolution: TirSlotResolution,
) {
    let overlay_id = registry.allocate_slot_resolution_overlay(TirSlotResolutionOverlay {
        resolutions: vec![(occurrence_id, resolution)],
    });
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: Some(overlay_id),
        wrapper_context: None,
    });

    if let Some(reference) = &mut template.tir_reference {
        reference.phase = TemplateTirPhase::Finalized;
        reference.overlay_set_id = overlay_set_id;
    }
}

fn body_ref_static_text(
    body_ref: &TemplateTirBodyReference,
    context: &ScopeContext,
    string_table: &StringTable,
) -> String {
    let store = context.template_ir_store.borrow();
    let root = body_ref
        .same_store_root(&store)
        .expect("control-flow body should resolve in the parser test store");
    let mut rendered = String::new();
    collect_static_tir_fragments(root, &store, string_table, &mut rendered);
    rendered
}

fn collect_static_tir_fragments(
    node_id: crate::compiler_frontend::ast::templates::tir::TemplateIrNodeId,
    store: &TemplateIrStore,
    string_table: &StringTable,
    output: &mut String,
) {
    let Some(node) = store.get_node(node_id) else {
        return;
    };

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            for child in children {
                collect_static_tir_fragments(*child, store, string_table, output);
            }
        }

        TemplateIrNodeKind::Text { text, .. } => output.push_str(string_table.resolve(*text)),

        TemplateIrNodeKind::DynamicExpression { expression, .. } => {
            if let ExpressionKind::StringSlice(value) = &expression.kind {
                output.push_str(string_table.resolve(*value));
            }
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            if let Some(child_id) = reference.template_id_in_store(store.store_id())
                && let Some(template) = store.get_template(child_id)
            {
                collect_static_tir_fragments(template.root, store, string_table, output);
            }
        }
        TemplateIrNodeKind::InsertContribution { template } => {
            if let Some(template) = store.get_template(*template) {
                collect_static_tir_fragments(template.root, store, string_table, output);
            }
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            for branch in branches {
                collect_static_tir_fragments(branch.body, store, string_table, output);
            }
            if let Some(fallback) = fallback {
                collect_static_tir_fragments(*fallback, store, string_table, output);
            }
        }

        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            collect_static_tir_fragments(*body, store, string_table, output);
            if let Some(aggregate_wrapper) = aggregate_wrapper {
                collect_static_tir_fragments(*aggregate_wrapper, store, string_table, output);
            }
        }

        TemplateIrNodeKind::Slot { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::LoopControl { .. }
        | TemplateIrNodeKind::RuntimeSlotSite { .. } => {}
    }
}

fn body_ref_contains_unresolved_slots(
    body_ref: &TemplateTirBodyReference,
    context: &ScopeContext,
) -> bool {
    let store = context.template_ir_store.borrow();
    let root = body_ref
        .same_store_root(&store)
        .expect("control-flow body should resolve in the parser test store");

    tir_subtree_contains_slot(root, &store)
}

fn tir_subtree_contains_slot(
    node_id: crate::compiler_frontend::ast::templates::tir::TemplateIrNodeId,
    store: &TemplateIrStore,
) -> bool {
    let Some(node) = store.get_node(node_id) else {
        return false;
    };

    match &node.kind {
        TemplateIrNodeKind::Slot { .. } => true,

        TemplateIrNodeKind::Sequence { children } => children
            .iter()
            .any(|child| tir_subtree_contains_slot(*child, store)),

        TemplateIrNodeKind::ChildTemplate { reference, .. } => reference
            .template_id_in_store(store.store_id())
            .and_then(|template_id| store.get_template(template_id))
            .is_some_and(|template| tir_subtree_contains_slot(template.root, store)),
        TemplateIrNodeKind::InsertContribution { template } => store
            .get_template(*template)
            .is_some_and(|template| tir_subtree_contains_slot(template.root, store)),

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            branches
                .iter()
                .any(|branch| tir_subtree_contains_slot(branch.body, store))
                || fallback.is_some_and(|fallback| tir_subtree_contains_slot(fallback, store))
        }

        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            tir_subtree_contains_slot(*body, store)
                || aggregate_wrapper
                    .is_some_and(|wrapper| tir_subtree_contains_slot(wrapper, store))
        }

        TemplateIrNodeKind::Text { .. }
        | TemplateIrNodeKind::DynamicExpression { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::LoopControl { .. }
        | TemplateIrNodeKind::RuntimeSlotSite { .. } => false,
    }
}

fn body_ref_loop_control_signal_count(
    body_ref: &TemplateTirBodyReference,
    context: &ScopeContext,
) -> usize {
    let store = context.template_ir_store.borrow();
    let root = body_ref
        .same_store_root(&store)
        .expect("control-flow body should resolve in the parser test store");

    count_tir_loop_control_signals(root, &store)
}

fn count_tir_loop_control_signals(
    node_id: crate::compiler_frontend::ast::templates::tir::TemplateIrNodeId,
    store: &TemplateIrStore,
) -> usize {
    let Some(node) = store.get_node(node_id) else {
        return 0;
    };

    match &node.kind {
        TemplateIrNodeKind::LoopControl { .. } => 1,

        TemplateIrNodeKind::Sequence { children } => children
            .iter()
            .map(|child| count_tir_loop_control_signals(*child, store))
            .sum(),

        TemplateIrNodeKind::ChildTemplate { reference, .. } => reference
            .template_id_in_store(store.store_id())
            .and_then(|template_id| store.get_template(template_id))
            .map_or(0, |template| {
                count_tir_loop_control_signals(template.root, store)
            }),
        TemplateIrNodeKind::InsertContribution { template } => {
            store.get_template(*template).map_or(0, |template| {
                count_tir_loop_control_signals(template.root, store)
            })
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            branches
                .iter()
                .map(|branch| count_tir_loop_control_signals(branch.body, store))
                .sum::<usize>()
                + fallback.map_or(0, |fallback| {
                    count_tir_loop_control_signals(fallback, store)
                })
        }

        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            count_tir_loop_control_signals(*body, store)
                + aggregate_wrapper
                    .map_or(0, |wrapper| count_tir_loop_control_signals(wrapper, store))
        }

        TemplateIrNodeKind::Text { .. }
        | TemplateIrNodeKind::DynamicExpression { .. }
        | TemplateIrNodeKind::Slot { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::RuntimeSlotSite { .. } => 0,
    }
}

fn assert_body_ref_static_contains(
    body_ref: &TemplateTirBodyReference,
    context: &ScopeContext,
    string_table: &StringTable,
    expected: &str,
) {
    let rendered = body_ref_static_text(body_ref, context, string_table);
    assert!(
        rendered.contains(expected),
        "expected {rendered:?} to contain {expected:?}"
    );
}

fn assert_body_ref_static_excludes(
    body_ref: &TemplateTirBodyReference,
    context: &ScopeContext,
    string_table: &StringTable,
    unexpected: &str,
) {
    let rendered = body_ref_static_text(body_ref, context, string_table);
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

fn expect_branch_chain(template: &Template) -> &TemplateBranchChain {
    let Some(TemplateControlFlow::BranchChain(branch_chain)) = &template.control_flow else {
        panic!("expected template branch-chain control flow");
    };

    branch_chain
}

fn first_branch_body_ref(branch_chain: &TemplateBranchChain) -> &TemplateTirBodyReference {
    &branch_chain
        .branches
        .first()
        .expect("branch chain should contain a primary branch")
        .body_tir_reference
}

fn branch_body_ref(branch_chain: &TemplateBranchChain, index: usize) -> &TemplateTirBodyReference {
    &branch_chain
        .branches
        .get(index)
        .expect("branch chain should contain requested branch")
        .body_tir_reference
}

fn fallback_body_ref(branch_chain: &TemplateBranchChain) -> &TemplateTirBodyReference {
    &branch_chain
        .fallback
        .as_ref()
        .expect("branch chain should contain fallback")
        .body_tir_reference
}

fn loop_body_ref(template_loop: &TemplateLoopControlFlow) -> &TemplateTirBodyReference {
    &template_loop.body_tir_reference
}

/// Returns true when the TIR subtree rooted at `node_id` contains a
/// `BranchChain` or `Loop` node (i.e. a control-flow child template).
fn tir_subtree_contains_control_flow(node_id: TemplateIrNodeId, store: &TemplateIrStore) -> bool {
    let Some(node) = store.get_node(node_id) else {
        return false;
    };
    match &node.kind {
        TemplateIrNodeKind::BranchChain { .. } | TemplateIrNodeKind::Loop { .. } => true,
        TemplateIrNodeKind::Sequence { children } => children
            .iter()
            .any(|child| tir_subtree_contains_control_flow(*child, store)),
        TemplateIrNodeKind::ChildTemplate { reference, .. } => reference
            .template_id_in_store(store.store_id())
            .is_some_and(|child_id| {
                store
                    .get_template(child_id)
                    .is_some_and(|child_ir| tir_subtree_contains_control_flow(child_ir.root, store))
            }),
        _ => false,
    }
}

/// Returns true when the template's TIR root contains a `ChildTemplate` node
/// whose referenced child template has control flow.
fn tir_root_has_control_flow_child(template: &Template, store: &TemplateIrStore) -> bool {
    let Some(reference) = template.tir_reference.as_ref() else {
        return false;
    };
    let Some(tir_template) = store.get_template(reference.root.template_id) else {
        return false;
    };
    tir_subtree_contains_control_flow(tir_template.root, store)
}
