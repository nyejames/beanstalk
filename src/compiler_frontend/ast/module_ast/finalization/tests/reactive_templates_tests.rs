//! Tests for AST reactive template metadata propagation.

use super::propagate_reactive_template_metadata_in_ast;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::call_argument::{CallAccessMode, CallArgument};
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ReactiveSource, ReactiveSourceKind, ReactiveTemplateMetadata,
};
use crate::compiler_frontend::ast::expressions::expression_types::ConstRecordState;
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnChannel, ReturnSlot,
};
use crate::compiler_frontend::ast::templates::template::{
    ReactiveSubscription, TemplateSegmentOrigin,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
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

fn template_with_subscription(source: ReactiveSource) -> Expression {
    let mut template = Template::empty();
    template.location = test_location(2);

    let source_expression =
        reference_expression(source.path.clone(), DataType::Int, builtin_type_ids::INT)
            .with_reactive_source(source.clone());

    let subscription = ReactiveSubscription {
        source,
        type_id: builtin_type_ids::INT,
        location: test_location(2),
    };

    template.content.add_reactive_subscription(
        source_expression.clone(),
        TemplateSegmentOrigin::Body,
        subscription,
    );

    Expression::template(template, ValueMode::ImmutableOwned)
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

fn first_rvalue_metadata(ast: &[AstNode]) -> &ReactiveTemplateMetadata {
    let NodeKind::Rvalue(expression) = &ast[1].kind else {
        panic!("expected rvalue node");
    };

    expression
        .reactive_template
        .as_ref()
        .expect("expected reactive template metadata")
}

#[test]
fn rebases_reactive_parameter_subscription_to_call_argument_source() {
    let mut string_table = StringTable::new();
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
        NodeKind::Return(vec![template_with_subscription(reactive_source(
            parameter_path,
            ReactiveSourceKind::Parameter,
        ))]),
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
            NodeKind::Rvalue(call_expression(
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

    propagate_reactive_template_metadata_in_ast(&mut ast);

    let metadata = first_rvalue_metadata(&ast);
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

    let mut wrapper_template = Template::empty();
    wrapper_template.location = test_location(2);
    wrapper_template.content.add(inserted_parameter);

    let signature = FunctionSignature {
        parameters: vec![parameter],
        returns: vec![string_return_slot()],
    };
    let body = vec![node(
        NodeKind::Return(vec![Expression::template(
            wrapper_template,
            ValueMode::ImmutableOwned,
        )]),
        test_location(2),
    )];

    let argument_template = template_with_subscription(reactive_source(
        count_path.clone(),
        ReactiveSourceKind::Declaration,
    ));

    let mut ast = vec![
        function_node(function_path.clone(), signature, body, test_location(1)),
        node(
            NodeKind::Rvalue(call_expression(
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

    propagate_reactive_template_metadata_in_ast(&mut ast);

    let metadata = first_rvalue_metadata(&ast);
    assert_eq!(metadata.subscriptions.len(), 1);
    assert_eq!(metadata.subscriptions[0].source.path, count_path);
    assert!(metadata.template_value_parameters.is_empty());
}

#[test]
fn references_use_metadata_computed_for_prior_declarations() {
    let mut string_table = StringTable::new();
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
        NodeKind::Return(vec![template_with_subscription(reactive_source(
            parameter_path,
            ReactiveSourceKind::Parameter,
        ))]),
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
            NodeKind::Rvalue(reference_expression(
                view_path,
                DataType::StringSlice,
                builtin_type_ids::STRING,
            )),
            test_location(4),
        ),
    ];

    propagate_reactive_template_metadata_in_ast(&mut ast);

    let NodeKind::Rvalue(expression) = &ast[2].kind else {
        panic!("expected rvalue reference");
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
    let count_path = symbol("count", &mut string_table);
    let template =
        template_with_subscription(reactive_source(count_path, ReactiveSourceKind::Declaration));
    assert!(template.reactive_template.is_some());

    let runtime_expression = Expression::runtime_with_type_id(
        vec![node(NodeKind::Rvalue(template), test_location(1))],
        DataType::StringSlice,
        builtin_type_ids::STRING,
        test_location(1),
        ValueMode::ImmutableOwned,
    );

    let mut ast = vec![node(NodeKind::Rvalue(runtime_expression), test_location(1))];
    propagate_reactive_template_metadata_in_ast(&mut ast);

    let NodeKind::Rvalue(expression) = &ast[0].kind else {
        panic!("expected rvalue node");
    };
    assert!(expression.reactive_template.is_none());
}

#[test]
fn sink_operand_expressions_keep_reactive_template_metadata() {
    let mut string_table = StringTable::new();
    let count_path = symbol("count", &mut string_table);
    let fragment_template = template_with_subscription(reactive_source(
        count_path.clone(),
        ReactiveSourceKind::Declaration,
    ));
    let host_call_template =
        template_with_subscription(reactive_source(count_path, ReactiveSourceKind::Declaration));

    let mut ast = vec![
        node(
            NodeKind::PushStartRuntimeFragment(fragment_template),
            test_location(1),
        ),
        node(
            NodeKind::HostFunctionCall {
                name: ExternalFunctionId::Io,
                args: vec![CallArgument::positional(
                    host_call_template,
                    CallAccessMode::Shared,
                    test_location(2),
                )],
                result_type_ids: Vec::new(),
                location: test_location(2),
            },
            test_location(2),
        ),
    ];

    propagate_reactive_template_metadata_in_ast(&mut ast);

    let NodeKind::PushStartRuntimeFragment(fragment) = &ast[0].kind else {
        panic!("expected runtime fragment push");
    };
    assert!(fragment.reactive_template.is_some());

    let NodeKind::HostFunctionCall { args, .. } = &ast[1].kind else {
        panic!("expected host function call");
    };
    assert!(args[0].value.reactive_template.is_some());
}
