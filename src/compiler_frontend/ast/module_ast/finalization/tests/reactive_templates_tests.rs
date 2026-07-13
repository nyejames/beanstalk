//! Tests for AST reactive template metadata propagation.

use super::propagate_reactive_template_metadata_in_ast;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::call_argument::{CallAccessMode, CallArgument};
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, ReactiveSource, ReactiveSourceKind, ReactiveTemplateMetadata,
};
use crate::compiler_frontend::ast::expressions::expression_rpn::{
    ExpressionRpn, ExpressionRpnItem,
};
use crate::compiler_frontend::ast::expressions::expression_types::ConstRecordState;
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnChannel, ReturnSlot,
};
use crate::compiler_frontend::ast::statements::match_patterns::MatchPattern;
use crate::compiler_frontend::ast::templates::runtime_handoff::OwnedRuntimeSlotSiteRenderPlan;
use crate::compiler_frontend::ast::templates::template::{
    ReactiveSubscription, SlotKey, Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchSelector, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_slots::RuntimeSlotSiteId;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::tir::{
    ExpressionSiteId, TemplateIr, TemplateIrBranch, TemplateIrNode, TemplateIrNodeKind,
    TemplateIrRegistry, TemplateIrStore, TemplateIrSummary, TemplateLoopHeaderExpressionSites,
    TemplateOverlaySet, TemplateOverlaySetId, TemplateRef, TemplateTirChildReference,
    TemplateTirPhase, TemplateTirReference, TirExpressionOverlay,
};
use crate::compiler_frontend::ast::templates::{
    OwnedRuntimeSlotApplicationHandoff, OwnedRuntimeSlotSite, OwnedRuntimeSlotSiteRenderPiece,
    OwnedRuntimeTemplateNode,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::external_packages::ExternalFunctionId;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tests::ast_fixture_support::{
    function_node, node, symbol, test_location,
};
use crate::compiler_frontend::value_mode::ValueMode;

fn string_return_slot() -> ReturnSlot {
    ReturnSlot {
        value: FunctionReturn::Value(DataType::StringSlice),
        type_id: Some(builtin_type_ids::STRING),
        reactive_template: None,
        channel: ReturnChannel::Success,
    }
}

fn no_value_declaration(
    path: InternedPath,
    data_type: DataType,
    type_id: crate::compiler_frontend::datatypes::TypeId,
) -> Declaration {
    Declaration {
        id: path,
        value: Expression::no_value_with_type_id(
            test_location(1),
            data_type,
            type_id,
            ValueMode::ImmutableReference,
        ),
    }
}

fn reference_expression(
    path: InternedPath,
    data_type: DataType,
    type_id: crate::compiler_frontend::datatypes::TypeId,
) -> Expression {
    Expression::reference_with_type_id(
        path,
        data_type,
        type_id,
        test_location(1),
        ValueMode::ImmutableReference,
        ConstRecordState::RuntimeValue,
    )
}

fn reactive_source(path: InternedPath, kind: ReactiveSourceKind) -> ReactiveSource {
    ReactiveSource { path, kind }
}

fn template_with_subscription(store: &mut TemplateIrStore, source: ReactiveSource) -> Expression {
    let source_expression =
        reference_expression(source.path.clone(), DataType::Int, builtin_type_ids::INT)
            .with_reactive_source(source.clone());
    let subscription = ReactiveSubscription {
        source,
        type_id: builtin_type_ids::INT,
        location: test_location(2),
    };

    template_expression_from_tir(store, source_expression, Some(subscription))
}

fn template_expression_from_tir(
    store: &mut TemplateIrStore,
    expression: Expression,
    reactive_subscription: Option<ReactiveSubscription>,
) -> Expression {
    let mut template = Template::empty();
    template.location = test_location(2);

    let site_id = store.next_expression_site_id();
    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::DynamicExpression {
            expression: Box::new(expression),
            origin: TemplateSegmentOrigin::Body,
            reactive_subscription,
            site_id,
        },
        template.location.clone(),
    ));
    let template_id = store.push_template(TemplateIr::new(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        template.location.clone(),
    ));
    template.tir_reference = Some(TemplateTirReference {
        root: TemplateRef::new(store.store_id(), template_id),
        store_owner: store.owner(),
        is_composed: true,
        phase: TemplateTirPhase::Composed,
        overlay_set_id:
            crate::compiler_frontend::ast::templates::tir::TemplateOverlaySetId::empty_for_test(),
    });

    Expression::template(template, ValueMode::ImmutableOwned)
}

fn linear_tir_expression_overlay_metadata<'a>(
    registry: &'a TemplateIrRegistry,
    template: &Template,
    site_id: ExpressionSiteId,
) -> Option<&'a ReactiveTemplateMetadata> {
    let reference = template.tir_reference.as_ref()?;
    let overlay_set = registry.overlay_set(reference.overlay_set_id)?;
    let expression_overlay_id = overlay_set.expression_overrides?;
    let expression_overlay = registry.expression_overlay(expression_overlay_id)?;
    let expression = expression_overlay.expression_for_site(site_id)?;

    expression.reactive_template.as_ref()
}

fn call_expression(function_path: InternedPath, arguments: Vec<CallArgument>) -> Expression {
    let mut type_environment = TypeEnvironment::new();
    Expression::function_call_with_typed_arguments(
        function_path,
        arguments,
        vec![builtin_type_ids::STRING],
        &mut type_environment,
        test_location(3),
    )
}

fn first_expression_statement_metadata(ast: &[AstNode]) -> &ReactiveTemplateMetadata {
    let NodeKind::ExpressionStatement(expression) = &ast[1].kind else {
        panic!("expected expression statement node");
    };

    expression
        .reactive_template
        .as_ref()
        .expect("expected reactive template metadata")
}

fn template_from_expression_statement(ast: &[AstNode]) -> &Template {
    let NodeKind::ExpressionStatement(expression) = &ast[0].kind else {
        panic!("expected template expression statement");
    };
    let ExpressionKind::Template(template) = &expression.kind else {
        panic!("expected template expression");
    };

    template
}

#[test]
fn rebases_reactive_parameter_subscription_to_call_argument_source() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let function_path = symbol("render_count", &mut string_table);
    let parameter_path = symbol("source", &mut string_table);
    let count_path = symbol("count", &mut string_table);

    let mut parameter =
        no_value_declaration(parameter_path.clone(), DataType::Int, builtin_type_ids::INT);
    parameter.value.reactive_source = Some(reactive_source(
        parameter_path.clone(),
        ReactiveSourceKind::Parameter,
    ));

    let signature = FunctionSignature {
        parameters: vec![parameter],
        returns: vec![string_return_slot()],
    };
    let body = vec![node(
        NodeKind::Return(vec![template_with_subscription(
            &mut store,
            reactive_source(parameter_path, ReactiveSourceKind::Parameter),
        )]),
        test_location(2),
    )];

    let mut argument =
        reference_expression(count_path.clone(), DataType::Int, builtin_type_ids::INT)
            .with_reactive_source(reactive_source(
                count_path.clone(),
                ReactiveSourceKind::Declaration,
            ));
    argument.reactive_template = None;

    let mut ast = vec![
        function_node(function_path.clone(), signature, body, test_location(1)),
        node(
            NodeKind::ExpressionStatement(call_expression(
                function_path,
                vec![CallArgument::positional(
                    argument,
                    CallAccessMode::Shared,
                    test_location(3),
                )],
            )),
            test_location(3),
        ),
    ];

    let mut registry = TemplateIrRegistry::new();
    propagate_reactive_template_metadata_in_ast(&mut ast, &mut store, &mut registry);

    let metadata = first_expression_statement_metadata(&ast);
    assert_eq!(metadata.subscriptions.len(), 1);
    assert_eq!(metadata.subscriptions[0].source.path, count_path);
    assert_eq!(
        metadata.subscriptions[0].source.kind,
        ReactiveSourceKind::Declaration
    );
}

#[test]
fn substitutes_string_parameter_template_value_from_call_argument() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let function_path = symbol("wrap", &mut string_table);
    let parameter_path = symbol("content", &mut string_table);
    let count_path = symbol("count", &mut string_table);

    let mut parameter = no_value_declaration(
        parameter_path.clone(),
        DataType::StringSlice,
        builtin_type_ids::STRING,
    );
    parameter.value.reactive_template =
        Some(ReactiveTemplateMetadata::from_template_value_parameter(
            parameter_path.clone(),
            test_location(1),
        ));

    let mut inserted_parameter = reference_expression(
        parameter_path.clone(),
        DataType::StringSlice,
        builtin_type_ids::STRING,
    );
    inserted_parameter.reactive_template = parameter.value.reactive_template.clone();

    let wrapper_template = template_expression_from_tir(&mut store, inserted_parameter, None);

    let signature = FunctionSignature {
        parameters: vec![parameter],
        returns: vec![string_return_slot()],
    };
    let body = vec![node(
        NodeKind::Return(vec![wrapper_template]),
        test_location(2),
    )];

    let argument_template = template_with_subscription(
        &mut store,
        reactive_source(count_path.clone(), ReactiveSourceKind::Declaration),
    );

    let mut ast = vec![
        function_node(function_path.clone(), signature, body, test_location(1)),
        node(
            NodeKind::ExpressionStatement(call_expression(
                function_path,
                vec![CallArgument::positional(
                    argument_template,
                    CallAccessMode::Shared,
                    test_location(3),
                )],
            )),
            test_location(3),
        ),
    ];

    let mut registry = TemplateIrRegistry::new();
    propagate_reactive_template_metadata_in_ast(&mut ast, &mut store, &mut registry);

    let metadata = first_expression_statement_metadata(&ast);
    assert_eq!(metadata.subscriptions.len(), 1);
    assert_eq!(metadata.subscriptions[0].source.path, count_path);
    assert!(metadata.template_value_parameters.is_empty());
}

#[test]
fn references_use_metadata_computed_for_prior_declarations() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let function_path = symbol("render_count", &mut string_table);
    let parameter_path = symbol("source", &mut string_table);
    let count_path = symbol("count", &mut string_table);
    let view_path = symbol("view", &mut string_table);

    let mut parameter =
        no_value_declaration(parameter_path.clone(), DataType::Int, builtin_type_ids::INT);
    parameter.value.reactive_source = Some(reactive_source(
        parameter_path.clone(),
        ReactiveSourceKind::Parameter,
    ));

    let signature = FunctionSignature {
        parameters: vec![parameter],
        returns: vec![string_return_slot()],
    };
    let body = vec![node(
        NodeKind::Return(vec![template_with_subscription(
            &mut store,
            reactive_source(parameter_path, ReactiveSourceKind::Parameter),
        )]),
        test_location(2),
    )];

    let argument = reference_expression(count_path.clone(), DataType::Int, builtin_type_ids::INT)
        .with_reactive_source(reactive_source(
            count_path.clone(),
            ReactiveSourceKind::Declaration,
        ));
    let declaration = Declaration {
        id: view_path.clone(),
        value: call_expression(
            function_path.clone(),
            vec![CallArgument::positional(
                argument,
                CallAccessMode::Shared,
                test_location(3),
            )],
        ),
    };

    let mut ast = vec![
        function_node(function_path, signature, body, test_location(1)),
        node(NodeKind::VariableDeclaration(declaration), test_location(3)),
        node(
            NodeKind::ExpressionStatement(reference_expression(
                view_path,
                DataType::StringSlice,
                builtin_type_ids::STRING,
            )),
            test_location(4),
        ),
    ];

    let mut registry = TemplateIrRegistry::new();
    propagate_reactive_template_metadata_in_ast(&mut ast, &mut store, &mut registry);

    let NodeKind::ExpressionStatement(expression) = &ast[2].kind else {
        panic!("expected expression statement reference");
    };
    let metadata = expression
        .reactive_template
        .as_ref()
        .expect("expected reference to use declaration metadata");
    assert_eq!(metadata.subscriptions.len(), 1);
    assert_eq!(metadata.subscriptions[0].source.path, count_path);
}

#[test]
fn runtime_string_operations_do_not_inherit_nested_template_metadata() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let count_path = symbol("count", &mut string_table);
    let template = template_with_subscription(
        &mut store,
        reactive_source(count_path, ReactiveSourceKind::Declaration),
    );
    assert!(
        template.reactive_template.is_none(),
        "template metadata should be deferred until store-aware finalization"
    );

    let runtime_expression = Expression::runtime_with_type_id(
        ExpressionRpn {
            items: vec![ExpressionRpnItem::Operand(template)],
        },
        DataType::StringSlice,
        builtin_type_ids::STRING,
        test_location(1),
        ValueMode::ImmutableOwned,
    );

    let mut ast = vec![node(
        NodeKind::ExpressionStatement(runtime_expression),
        test_location(1),
    )];
    let mut registry = TemplateIrRegistry::new();
    propagate_reactive_template_metadata_in_ast(&mut ast, &mut store, &mut registry);

    let NodeKind::ExpressionStatement(expression) = &ast[0].kind else {
        panic!("expected expression statement node");
    };
    assert!(expression.reactive_template.is_none());
}

#[test]
fn annotates_same_store_branch_body_tir_root_metadata() {
    let mut string_table = StringTable::new();
    let count_path = symbol("count", &mut string_table);
    let location = test_location(2);
    let mut store = TemplateIrStore::new();

    let body_expression = template_with_subscription(
        &mut store,
        reactive_source(count_path.clone(), ReactiveSourceKind::Declaration),
    );

    let body_site_id = store.next_expression_site_id();
    let body_expr_node = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::DynamicExpression {
            expression: Box::new(body_expression),
            origin: TemplateSegmentOrigin::Body,
            reactive_subscription: None,
            site_id: body_site_id,
        },
        location.clone(),
    ));
    let body_root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence {
            children: vec![body_expr_node],
        },
        location.clone(),
    ));

    let selector_site_id = store.next_expression_site_id();
    let branch = TemplateIrBranch::new(
        TemplateBranchSelector::Bool(Expression::bool(
            true,
            location.clone(),
            ValueMode::ImmutableOwned,
        )),
        body_root,
        location.clone(),
    )
    .with_selector_site_id(selector_site_id);

    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::BranchChain {
            branches: vec![branch],
            fallback: None,
        },
        location.clone(),
    ));
    let template_id = store.push_template(TemplateIr::new(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        location.clone(),
    ));

    let mut template = Template::empty();
    template.location = location.clone();
    template.tir_reference = Some(TemplateTirReference {
        root: TemplateRef::new(store.store_id(), template_id),
        store_owner: store.owner(),
        is_composed: true,
        phase: TemplateTirPhase::Composed,
        overlay_set_id: TemplateOverlaySetId::empty_for_test(),
    });

    let mut ast = vec![node(
        NodeKind::ExpressionStatement(Expression::template(template, ValueMode::ImmutableOwned)),
        location,
    )];

    let mut registry = TemplateIrRegistry::new();
    registry.allocate_overlay_set(TemplateOverlaySet::empty());
    propagate_reactive_template_metadata_in_ast(&mut ast, &mut store, &mut registry);

    let template = template_from_expression_statement(&ast);
    let metadata = linear_tir_expression_overlay_metadata(&registry, template, body_site_id)
        .expect("expected TIR root expression overlay metadata for branch body");
    assert_eq!(metadata.subscriptions.len(), 1);
    assert_eq!(metadata.subscriptions[0].source.path, count_path);
}

#[test]
fn annotates_same_store_fallback_body_tir_root_metadata() {
    let mut string_table = StringTable::new();
    let fallback_path = symbol("fallback_count", &mut string_table);
    let location = test_location(2);
    let mut store = TemplateIrStore::new();

    let branch_body_node = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Text {
            text: string_table.intern("branch"),
            byte_len: 6,
            origin: TemplateSegmentOrigin::Body,
        },
        location.clone(),
    ));

    let fallback_expression = template_with_subscription(
        &mut store,
        reactive_source(fallback_path.clone(), ReactiveSourceKind::Declaration),
    );
    let fallback_site_id = store.next_expression_site_id();
    let fallback_expr_node = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::DynamicExpression {
            expression: Box::new(fallback_expression),
            origin: TemplateSegmentOrigin::Body,
            reactive_subscription: None,
            site_id: fallback_site_id,
        },
        location.clone(),
    ));
    let fallback_body_root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence {
            children: vec![fallback_expr_node],
        },
        location.clone(),
    ));

    let selector_site_id = store.next_expression_site_id();
    let branch = TemplateIrBranch::new(
        TemplateBranchSelector::Bool(Expression::bool(
            true,
            location.clone(),
            ValueMode::ImmutableOwned,
        )),
        branch_body_node,
        location.clone(),
    )
    .with_selector_site_id(selector_site_id);

    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::BranchChain {
            branches: vec![branch],
            fallback: Some(fallback_body_root),
        },
        location.clone(),
    ));
    let template_id = store.push_template(TemplateIr::new(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        location.clone(),
    ));

    let mut template = Template::empty();
    template.location = location.clone();
    template.tir_reference = Some(TemplateTirReference {
        root: TemplateRef::new(store.store_id(), template_id),
        store_owner: store.owner(),
        is_composed: true,
        phase: TemplateTirPhase::Composed,
        overlay_set_id: TemplateOverlaySetId::empty_for_test(),
    });

    let mut ast = vec![node(
        NodeKind::ExpressionStatement(Expression::template(template, ValueMode::ImmutableOwned)),
        location,
    )];

    let mut registry = TemplateIrRegistry::new();
    registry.allocate_overlay_set(TemplateOverlaySet::empty());
    propagate_reactive_template_metadata_in_ast(&mut ast, &mut store, &mut registry);

    let template = template_from_expression_statement(&ast);
    let metadata = linear_tir_expression_overlay_metadata(&registry, template, fallback_site_id)
        .expect("expected TIR root expression overlay metadata for fallback body");
    assert_eq!(metadata.subscriptions.len(), 1);
    assert_eq!(metadata.subscriptions[0].source.path, fallback_path);
}

#[test]
fn annotates_same_store_loop_body_tir_root_metadata() {
    let mut string_table = StringTable::new();
    let count_path = symbol("count", &mut string_table);
    let location = test_location(2);
    let mut store = TemplateIrStore::new();

    let body_expression = template_with_subscription(
        &mut store,
        reactive_source(count_path.clone(), ReactiveSourceKind::Declaration),
    );

    let body_site_id = store.next_expression_site_id();
    let body_expr_node = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::DynamicExpression {
            expression: Box::new(body_expression),
            origin: TemplateSegmentOrigin::Body,
            reactive_subscription: None,
            site_id: body_site_id,
        },
        location.clone(),
    ));
    let body_root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence {
            children: vec![body_expr_node],
        },
        location.clone(),
    ));

    let condition_site_id = store.next_expression_site_id();
    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Loop {
            header: TemplateLoopHeader::Conditional {
                condition: Box::new(Expression::bool(
                    true,
                    location.clone(),
                    ValueMode::ImmutableOwned,
                )),
            },
            header_sites: TemplateLoopHeaderExpressionSites::Conditional {
                condition: condition_site_id,
            },
            body: body_root,
            aggregate_wrapper: None,
        },
        location.clone(),
    ));
    let template_id = store.push_template(TemplateIr::new(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        location.clone(),
    ));

    let mut template = Template::empty();
    template.location = location.clone();
    template.tir_reference = Some(TemplateTirReference {
        root: TemplateRef::new(store.store_id(), template_id),
        store_owner: store.owner(),
        is_composed: true,
        phase: TemplateTirPhase::Composed,
        overlay_set_id: TemplateOverlaySetId::empty_for_test(),
    });

    let mut ast = vec![node(
        NodeKind::ExpressionStatement(Expression::template(template, ValueMode::ImmutableOwned)),
        location,
    )];

    let mut registry = TemplateIrRegistry::new();
    registry.allocate_overlay_set(TemplateOverlaySet::empty());
    propagate_reactive_template_metadata_in_ast(&mut ast, &mut store, &mut registry);

    let template = template_from_expression_statement(&ast);
    let metadata = linear_tir_expression_overlay_metadata(&registry, template, body_site_id)
        .expect("expected TIR root expression overlay metadata for loop body");
    assert_eq!(metadata.subscriptions.len(), 1);
    assert_eq!(metadata.subscriptions[0].source.path, count_path);
}

#[test]
fn annotates_branch_selector_and_body_through_one_root_overlay() {
    let mut string_table = StringTable::new();
    let count_path = symbol("count", &mut string_table);
    let show_path = symbol("show", &mut string_table);
    let location = test_location(2);
    let mut store = TemplateIrStore::new();

    let show_template = template_with_subscription(
        &mut store,
        reactive_source(count_path.clone(), ReactiveSourceKind::Declaration),
    );
    let show_declaration = Declaration {
        id: show_path.clone(),
        value: show_template,
    };

    let body_expression = template_with_subscription(
        &mut store,
        reactive_source(count_path.clone(), ReactiveSourceKind::Declaration),
    );

    let body_site_id = store.next_expression_site_id();
    let body_expr_node = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::DynamicExpression {
            expression: Box::new(body_expression),
            origin: TemplateSegmentOrigin::Body,
            reactive_subscription: None,
            site_id: body_site_id,
        },
        location.clone(),
    ));
    let body_root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence {
            children: vec![body_expr_node],
        },
        location.clone(),
    ));

    let selector_expression = reference_expression(
        show_path.clone(),
        DataType::StringSlice,
        builtin_type_ids::STRING,
    );
    let selector_site_id = store.next_expression_site_id();
    let branch = TemplateIrBranch::new(
        TemplateBranchSelector::Bool(selector_expression),
        body_root,
        location.clone(),
    )
    .with_selector_site_id(selector_site_id);

    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::BranchChain {
            branches: vec![branch],
            fallback: None,
        },
        location.clone(),
    ));
    let template_id = store.push_template(TemplateIr::new(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        location.clone(),
    ));

    let mut template = Template::empty();
    template.location = location.clone();
    template.tir_reference = Some(TemplateTirReference {
        root: TemplateRef::new(store.store_id(), template_id),
        store_owner: store.owner(),
        is_composed: true,
        phase: TemplateTirPhase::Composed,
        overlay_set_id: TemplateOverlaySetId::empty_for_test(),
    });

    let mut ast = vec![
        node(
            NodeKind::VariableDeclaration(show_declaration),
            test_location(1),
        ),
        node(
            NodeKind::ExpressionStatement(Expression::template(
                template,
                ValueMode::ImmutableOwned,
            )),
            location,
        ),
    ];

    let mut registry = TemplateIrRegistry::new();
    registry.allocate_overlay_set(TemplateOverlaySet::empty());
    propagate_reactive_template_metadata_in_ast(&mut ast, &mut store, &mut registry);

    let NodeKind::ExpressionStatement(Expression {
        kind: ExpressionKind::Template(template),
        ..
    }) = &ast[1].kind
    else {
        panic!("expected template expression statement at ast[1]");
    };

    let selector_metadata =
        linear_tir_expression_overlay_metadata(&registry, template, selector_site_id)
            .expect("expected TIR root expression overlay metadata for branch selector");
    assert_eq!(
        selector_metadata.subscriptions.len(),
        1,
        "branch selector reference should inherit reactive metadata from the prior declaration through the root overlay"
    );
    assert_eq!(selector_metadata.subscriptions[0].source.path, count_path);

    let body_metadata = linear_tir_expression_overlay_metadata(&registry, template, body_site_id)
        .expect("expected TIR root expression overlay metadata for branch body");
    assert_eq!(body_metadata.subscriptions.len(), 1);
    assert_eq!(body_metadata.subscriptions[0].source.path, count_path);
}

#[test]
fn annotates_existing_effective_expression_override_instead_of_structural_payload() {
    let mut string_table = StringTable::new();
    let count_path = symbol("count", &mut string_table);
    let show_path = symbol("show", &mut string_table);
    let location = test_location(2);
    let mut store = TemplateIrStore::new();

    let show_declaration = Declaration {
        id: show_path.clone(),
        value: template_with_subscription(
            &mut store,
            reactive_source(count_path.clone(), ReactiveSourceKind::Declaration),
        ),
    };

    let site_id = store.next_expression_site_id();
    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::DynamicExpression {
            expression: Box::new(Expression::bool(
                false,
                location.clone(),
                ValueMode::ImmutableOwned,
            )),
            origin: TemplateSegmentOrigin::Body,
            reactive_subscription: None,
            site_id,
        },
        location.clone(),
    ));
    let template_id = store.push_template(TemplateIr::new(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        location.clone(),
    ));

    let mut registry = TemplateIrRegistry::new();
    let expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![(
            site_id,
            Box::new(reference_expression(
                show_path,
                DataType::StringSlice,
                builtin_type_ids::STRING,
            )),
        )],
    });
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    });

    let mut template = Template::empty();
    template.location = location.clone();
    template.tir_reference = Some(TemplateTirReference {
        root: TemplateRef::new(store.store_id(), template_id),
        store_owner: store.owner(),
        is_composed: true,
        phase: TemplateTirPhase::Composed,
        overlay_set_id,
    });

    let mut ast = vec![
        node(
            NodeKind::VariableDeclaration(show_declaration),
            test_location(1),
        ),
        node(
            NodeKind::ExpressionStatement(Expression::template(
                template,
                ValueMode::ImmutableOwned,
            )),
            location,
        ),
    ];

    propagate_reactive_template_metadata_in_ast(&mut ast, &mut store, &mut registry);

    let NodeKind::ExpressionStatement(Expression {
        kind: ExpressionKind::Template(template),
        ..
    }) = &ast[1].kind
    else {
        panic!("expected template expression statement at ast[1]");
    };
    let metadata = linear_tir_expression_overlay_metadata(&registry, template, site_id)
        .expect("existing effective expression override should be annotated");

    assert_eq!(metadata.subscriptions.len(), 1);
    assert_eq!(metadata.subscriptions[0].source.path, count_path);
}

#[test]
fn annotates_existing_same_store_child_expression_override() {
    let mut string_table = StringTable::new();
    let count_path = symbol("count", &mut string_table);
    let show_path = symbol("show", &mut string_table);
    let location = test_location(2);
    let mut store = TemplateIrStore::new();

    let show_declaration = Declaration {
        id: show_path.clone(),
        value: template_with_subscription(
            &mut store,
            reactive_source(count_path.clone(), ReactiveSourceKind::Declaration),
        ),
    };

    let child_site_id = store.next_expression_site_id();
    let child_root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::DynamicExpression {
            expression: Box::new(Expression::bool(
                false,
                location.clone(),
                ValueMode::ImmutableOwned,
            )),
            origin: TemplateSegmentOrigin::Body,
            reactive_subscription: None,
            site_id: child_site_id,
        },
        location.clone(),
    ));
    let child_template_id = store.push_template(TemplateIr::new(
        child_root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        location.clone(),
    ));

    let mut registry = TemplateIrRegistry::new();
    let child_expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![(
            child_site_id,
            Box::new(reference_expression(
                show_path,
                DataType::StringSlice,
                builtin_type_ids::STRING,
            )),
        )],
    });
    let child_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(child_expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    });
    let root_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let child_reference = TemplateTirChildReference::same_store(
        child_template_id,
        store.store_id(),
        TemplateTirPhase::Composed,
        child_overlay_set_id,
    );
    let child_occurrence_id = store.next_child_template_occurrence_id();
    let parent_root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::ChildTemplate {
            reference: child_reference,
            occurrence_id: child_occurrence_id,
        },
        location.clone(),
    ));
    let parent_template_id = store.push_template(TemplateIr::new(
        parent_root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        location.clone(),
    ));

    let mut template = Template::empty();
    template.location = location.clone();
    template.tir_reference = Some(TemplateTirReference {
        root: TemplateRef::new(store.store_id(), parent_template_id),
        store_owner: store.owner(),
        is_composed: true,
        phase: TemplateTirPhase::Composed,
        overlay_set_id: root_overlay_set_id,
    });
    let mut ast = vec![
        node(
            NodeKind::VariableDeclaration(show_declaration),
            test_location(1),
        ),
        node(
            NodeKind::ExpressionStatement(Expression::template(
                template,
                ValueMode::ImmutableOwned,
            )),
            location,
        ),
    ];

    propagate_reactive_template_metadata_in_ast(&mut ast, &mut store, &mut registry);

    let NodeKind::ExpressionStatement(Expression {
        kind: ExpressionKind::Template(template),
        ..
    }) = &ast[1].kind
    else {
        panic!("expected template expression statement at ast[1]");
    };
    let metadata = linear_tir_expression_overlay_metadata(&registry, template, child_site_id)
        .expect("same-store child effective override should be annotated on the root overlay");

    assert_eq!(metadata.subscriptions.len(), 1);
    assert_eq!(metadata.subscriptions[0].source.path, count_path);
}

#[test]
fn option_capture_body_uses_scrutinee_reactive_metadata() {
    let mut string_table = StringTable::new();
    let count_path = symbol("count", &mut string_table);
    let optional_path = symbol("optional", &mut string_table);
    let capture_path = symbol("captured", &mut string_table);
    let capture_name = string_table.intern("captured");
    let location = test_location(2);
    let mut store = TemplateIrStore::new();

    let optional_declaration = Declaration {
        id: optional_path.clone(),
        value: template_with_subscription(
            &mut store,
            reactive_source(count_path.clone(), ReactiveSourceKind::Declaration),
        ),
    };

    let body_site_id = store.next_expression_site_id();
    let body_root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::DynamicExpression {
            expression: Box::new(reference_expression(
                capture_path.clone(),
                DataType::StringSlice,
                builtin_type_ids::STRING,
            )),
            origin: TemplateSegmentOrigin::Body,
            reactive_subscription: None,
            site_id: body_site_id,
        },
        location.clone(),
    ));

    let selector_site_id = store.next_expression_site_id();
    let selector = TemplateBranchSelector::OptionPresentCapture {
        scrutinee: reference_expression(
            optional_path,
            DataType::StringSlice,
            builtin_type_ids::STRING,
        ),
        pattern: Box::new(MatchPattern::OptionPresentCapture {
            name: capture_name,
            binding_path: capture_path,
            inner_type_id: builtin_type_ids::STRING,
            location: location.clone(),
            binding_location: location.clone(),
        }),
    };
    let branch = TemplateIrBranch::new(selector, body_root, location.clone())
        .with_selector_site_id(selector_site_id);
    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::BranchChain {
            branches: vec![branch],
            fallback: None,
        },
        location.clone(),
    ));
    let template_id = store.push_template(TemplateIr::new(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        location.clone(),
    ));

    let mut template = Template::empty();
    template.location = location.clone();
    template.tir_reference = Some(TemplateTirReference {
        root: TemplateRef::new(store.store_id(), template_id),
        store_owner: store.owner(),
        is_composed: true,
        phase: TemplateTirPhase::Composed,
        overlay_set_id: TemplateOverlaySetId::empty_for_test(),
    });

    let mut ast = vec![
        node(
            NodeKind::VariableDeclaration(optional_declaration),
            test_location(1),
        ),
        node(
            NodeKind::ExpressionStatement(Expression::template(
                template,
                ValueMode::ImmutableOwned,
            )),
            location,
        ),
    ];

    let mut registry = TemplateIrRegistry::new();
    registry.allocate_overlay_set(TemplateOverlaySet::empty());
    propagate_reactive_template_metadata_in_ast(&mut ast, &mut store, &mut registry);

    let NodeKind::ExpressionStatement(Expression {
        kind: ExpressionKind::Template(template),
        ..
    }) = &ast[1].kind
    else {
        panic!("expected template expression statement at ast[1]");
    };
    let metadata = linear_tir_expression_overlay_metadata(&registry, template, body_site_id)
        .expect("option-capture body reference should inherit scrutinee metadata");

    assert_eq!(metadata.subscriptions.len(), 1);
    assert_eq!(metadata.subscriptions[0].source.path, count_path);
}

#[test]
fn annotates_same_store_linear_tir_root_metadata_through_overlay() {
    let mut string_table = StringTable::new();
    let count_path = symbol("count", &mut string_table);
    let location = test_location(2);
    let mut store = TemplateIrStore::new();

    let source_expression = template_with_subscription(
        &mut store,
        reactive_source(count_path.clone(), ReactiveSourceKind::Declaration),
    );
    let site_id = store.next_expression_site_id();
    let expression_node = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::DynamicExpression {
            expression: Box::new(source_expression),
            origin: TemplateSegmentOrigin::Body,
            reactive_subscription: None,
            site_id,
        },
        location.clone(),
    ));
    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence {
            children: vec![expression_node],
        },
        location.clone(),
    ));
    let template_id = store.push_template(TemplateIr::new(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        location.clone(),
    ));

    let mut registry = TemplateIrRegistry::new();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let mut template = Template::empty();
    template.location = location.clone();
    template.tir_reference = Some(TemplateTirReference {
        root: TemplateRef::new(store.store_id(), template_id),
        store_owner: store.owner(),
        is_composed: true,
        phase: TemplateTirPhase::Composed,
        overlay_set_id,
    });

    let mut ast = vec![node(
        NodeKind::ExpressionStatement(Expression::template(template, ValueMode::ImmutableOwned)),
        location,
    )];

    propagate_reactive_template_metadata_in_ast(&mut ast, &mut store, &mut registry);

    let NodeKind::ExpressionStatement(expression) = &ast[0].kind else {
        panic!("expected expression statement");
    };
    let ExpressionKind::Template(template) = &expression.kind else {
        panic!("expected template expression");
    };

    let metadata = linear_tir_expression_overlay_metadata(&registry, template, site_id)
        .expect("expected TIR expression overlay metadata");
    assert_eq!(metadata.subscriptions.len(), 1);
    assert_eq!(metadata.subscriptions[0].source.path, count_path);
}

#[test]
fn sink_operand_expressions_keep_reactive_template_metadata() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let count_path = symbol("count", &mut string_table);
    let fragment_template = template_with_subscription(
        &mut store,
        reactive_source(count_path.clone(), ReactiveSourceKind::Declaration),
    );
    let host_call_template = template_with_subscription(
        &mut store,
        reactive_source(count_path, ReactiveSourceKind::Declaration),
    );

    let mut ast = vec![
        node(
            NodeKind::PushStartRuntimeFragment(fragment_template),
            test_location(1),
        ),
        node(
            NodeKind::ExpressionStatement(Expression::host_function_call_with_arguments(
                ExternalFunctionId::IoLine,
                vec![CallArgument::positional(
                    host_call_template,
                    CallAccessMode::Shared,
                    test_location(2),
                )],
                Vec::new(),
                test_location(2),
            )),
            test_location(2),
        ),
    ];

    let mut registry = TemplateIrRegistry::new();
    propagate_reactive_template_metadata_in_ast(&mut ast, &mut store, &mut registry);

    let NodeKind::PushStartRuntimeFragment(fragment) = &ast[0].kind else {
        panic!("expected runtime fragment push");
    };
    assert!(fragment.reactive_template.is_some());

    let NodeKind::ExpressionStatement(Expression {
        kind: ExpressionKind::HostFunctionCall { args, .. },
        ..
    }) = &ast[1].kind
    else {
        panic!("expected host function call");
    };
    assert!(args[0].value.reactive_template.is_some());
}

#[test]
fn annotates_reactive_subscription_in_runtime_slot_site_render_piece() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let count_path = symbol("count", &mut string_table);
    let location = test_location(2);

    let inner_expression = template_with_subscription(
        &mut store,
        reactive_source(count_path.clone(), ReactiveSourceKind::Declaration),
    );

    let render_piece_node = OwnedRuntimeTemplateNode::DynamicExpression {
        expression: Box::new(inner_expression),
        reactive_subscription: None,
        location: location.clone(),
    };

    let slot_site = OwnedRuntimeSlotSite {
        site: RuntimeSlotSiteId(0),
        key: SlotKey::Default,
        render_plan: OwnedRuntimeSlotSiteRenderPlan {
            pieces: vec![OwnedRuntimeSlotSiteRenderPiece::Render(render_piece_node)],
        },
        location: location.clone(),
    };

    let handoff = OwnedRuntimeSlotApplicationHandoff {
        wrapper: OwnedRuntimeTemplateNode::RuntimeSlotSite {
            site: RuntimeSlotSiteId(0),
            location: location.clone(),
        },
        contribution_sources: vec![],
        slot_sites: vec![slot_site],
        location: location.clone(),
    };

    let expression =
        Expression::runtime_slot_application_handoff(handoff, ValueMode::ImmutableOwned);
    let mut ast = vec![node(NodeKind::ExpressionStatement(expression), location)];
    let mut registry = TemplateIrRegistry::new();
    propagate_reactive_template_metadata_in_ast(&mut ast, &mut store, &mut registry);

    let NodeKind::ExpressionStatement(expression) = &ast[0].kind else {
        panic!("expected expression statement");
    };

    let outer_metadata = expression
        .reactive_template
        .as_ref()
        .expect("expected outer handoff reactive template metadata");
    assert_eq!(
        outer_metadata.subscriptions.len(),
        1,
        "outer runtime slot handoff should carry the subscription from its slot-site render piece"
    );
    assert_eq!(outer_metadata.subscriptions[0].source.path, count_path);

    let ExpressionKind::RuntimeSlotApplicationHandoff(handoff) = &expression.kind else {
        panic!("expected runtime slot application handoff expression");
    };
    let site = handoff.slot_sites.first().expect("expected one slot site");
    let OwnedRuntimeSlotSiteRenderPiece::Render(node) = &site.render_plan.pieces[0] else {
        panic!("expected render piece");
    };
    let OwnedRuntimeTemplateNode::DynamicExpression {
        expression: inner, ..
    } = node
    else {
        panic!("expected dynamic expression node");
    };
    let inner_metadata = inner
        .reactive_template
        .as_ref()
        .expect("expected inner dynamic expression reactive template metadata");
    assert_eq!(
        inner_metadata.subscriptions.len(),
        1,
        "inner dynamic expression in slot-site render piece should keep its subscription"
    );
    assert_eq!(inner_metadata.subscriptions[0].source.path, count_path);
}
