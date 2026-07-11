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
use crate::compiler_frontend::ast::templates::runtime_handoff::OwnedRuntimeSlotSiteRenderPlan;
use crate::compiler_frontend::ast::templates::template::{
    ReactiveSubscription, SlotKey, Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchChain, TemplateBranchSelector, TemplateConditionalBranch, TemplateControlFlow,
    TemplateControlFlowTirReference, TemplateFallbackBranch, TemplateLoopControlFlow,
    TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_slots::RuntimeSlotSiteId;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::tir::{
    ExpressionSiteId, TemplateIr, TemplateIrNode, TemplateIrNodeKind, TemplateIrRegistry,
    TemplateIrStore, TemplateIrSummary, TemplateOverlaySet, TemplateRef, TemplateTirPhase,
    TemplateTirReference,
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

fn push_tir_body_root_for_expression(
    store: &mut TemplateIrStore,
    expression: Expression,
) -> (TemplateControlFlowTirReference, ExpressionSiteId) {
    let location = expression.location.clone();
    let site_id = store.next_expression_site_id();
    let expression_node = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::DynamicExpression {
            expression: Box::new(expression),
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
        location,
    ));

    (TemplateControlFlowTirReference::new(store, root), site_id)
}

fn tir_body_overlay_expression_metadata<'a>(
    registry: &'a TemplateIrRegistry,
    body_ref: &TemplateControlFlowTirReference,
    site_id: ExpressionSiteId,
) -> Option<&'a ReactiveTemplateMetadata> {
    let overlay_set = registry.overlay_set(body_ref.overlay_set_id())?;
    let expression_overlay_id = overlay_set.expression_overrides?;
    let expression_overlay = registry.expression_overlay(expression_overlay_id)?;
    expression_overlay
        .expression_for_site(site_id)?
        .reactive_template
        .as_ref()
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
    let (body_ref, body_site_id) = push_tir_body_root_for_expression(&mut store, body_expression);

    let mut template = Template::empty();
    template.location = location.clone();
    template.control_flow = Some(TemplateControlFlow::BranchChain(Box::new(
        TemplateBranchChain {
            branches: vec![TemplateConditionalBranch {
                body_tir_reference: Some(body_ref.clone()),
                selector: TemplateBranchSelector::Bool(Expression::bool(
                    true,
                    location.clone(),
                    ValueMode::ImmutableOwned,
                )),
                location: location.clone(),
            }],
            fallback: None,
            location: location.clone(),
        },
    )));

    let mut ast = vec![node(
        NodeKind::ExpressionStatement(Expression::template(template, ValueMode::ImmutableOwned)),
        location,
    )];

    let mut registry = TemplateIrRegistry::new();
    propagate_reactive_template_metadata_in_ast(&mut ast, &mut store, &mut registry);

    let template = template_from_expression_statement(&ast);
    let Some(TemplateControlFlow::BranchChain(branch_chain)) = &template.control_flow else {
        panic!("expected branch chain");
    };
    let body_ref = branch_chain.branches[0]
        .body_tir_reference
        .as_ref()
        .expect("expected branch body reference");
    let metadata = tir_body_overlay_expression_metadata(&registry, body_ref, body_site_id)
        .expect("expected TIR body expression metadata");
    assert_eq!(metadata.subscriptions.len(), 1);
    assert_eq!(metadata.subscriptions[0].source.path, count_path);
}

#[test]
fn annotates_same_store_fallback_body_tir_root_metadata() {
    let mut string_table = StringTable::new();
    let fallback_path = symbol("fallback_count", &mut string_table);
    let location = test_location(2);
    let mut store = TemplateIrStore::new();

    let (branch_body_ref, _) = push_tir_body_root_for_expression(
        &mut store,
        Expression::string_slice(
            string_table.intern("branch"),
            location.clone(),
            ValueMode::ImmutableOwned,
        ),
    );
    let fallback_expression = template_with_subscription(
        &mut store,
        reactive_source(fallback_path.clone(), ReactiveSourceKind::Declaration),
    );
    let (fallback_body_ref, fallback_site_id) =
        push_tir_body_root_for_expression(&mut store, fallback_expression);

    let mut template = Template::empty();
    template.location = location.clone();
    template.control_flow = Some(TemplateControlFlow::BranchChain(Box::new(
        TemplateBranchChain {
            branches: vec![TemplateConditionalBranch {
                body_tir_reference: Some(branch_body_ref),
                selector: TemplateBranchSelector::Bool(Expression::bool(
                    true,
                    location.clone(),
                    ValueMode::ImmutableOwned,
                )),
                location: location.clone(),
            }],
            fallback: Some(TemplateFallbackBranch {
                body_tir_reference: Some(fallback_body_ref.clone()),
                location: location.clone(),
            }),
            location: location.clone(),
        },
    )));

    let mut ast = vec![node(
        NodeKind::ExpressionStatement(Expression::template(template, ValueMode::ImmutableOwned)),
        location,
    )];

    let mut registry = TemplateIrRegistry::new();
    propagate_reactive_template_metadata_in_ast(&mut ast, &mut store, &mut registry);

    let template = template_from_expression_statement(&ast);
    let Some(TemplateControlFlow::BranchChain(branch_chain)) = &template.control_flow else {
        panic!("expected branch chain");
    };
    let fallback_ref = branch_chain
        .fallback
        .as_ref()
        .and_then(|fallback| fallback.body_tir_reference.as_ref())
        .expect("expected fallback body reference");
    let metadata = tir_body_overlay_expression_metadata(&registry, fallback_ref, fallback_site_id)
        .expect("expected fallback TIR body expression metadata");
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
    let (body_ref, body_site_id) = push_tir_body_root_for_expression(&mut store, body_expression);

    let mut template = Template::empty();
    template.location = location.clone();
    template.control_flow = Some(TemplateControlFlow::Loop(Box::new(
        TemplateLoopControlFlow {
            header: TemplateLoopHeader::Conditional {
                condition: Box::new(Expression::bool(
                    true,
                    location.clone(),
                    ValueMode::ImmutableOwned,
                )),
            },
            body_tir_reference: Some(body_ref.clone()),
            aggregate_wrapper_tir_reference: None,
            location: location.clone(),
        },
    )));

    let mut ast = vec![node(
        NodeKind::ExpressionStatement(Expression::template(template, ValueMode::ImmutableOwned)),
        location,
    )];

    let mut registry = TemplateIrRegistry::new();
    propagate_reactive_template_metadata_in_ast(&mut ast, &mut store, &mut registry);

    let template = template_from_expression_statement(&ast);
    let Some(TemplateControlFlow::Loop(template_loop)) = &template.control_flow else {
        panic!("expected template loop");
    };
    let body_ref = template_loop
        .body_tir_reference
        .as_ref()
        .expect("expected loop body reference");
    let metadata = tir_body_overlay_expression_metadata(&registry, body_ref, body_site_id)
        .expect("expected loop TIR body expression metadata");
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
