//! TypeId-first test helpers for HIR lowering tests.
//!
//! WHAT: wraps AST construction that still requires `DataType` internally so
//!       HIR test files can remain free of parse-era type-syntax references.
//! WHY: production AST nodes carry `diagnostic_type` for render support;
//!      test fixtures should set canonical `TypeId`s and let this module
//!      handle the diagnostic-only placeholder.

use crate::compiler_frontend::ast::Ast;
use crate::compiler_frontend::ast::ast_nodes::{
    AstNode, Declaration, MultiBindTarget, MultiBindTargetKind, NodeKind, SourceLocation,
};
use crate::compiler_frontend::ast::const_values::facts::AstConstFacts;
use crate::compiler_frontend::ast::expressions::expression::{
    ConstRecordState, Expression, ExpressionKind,
};
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, ReturnChannel, ReturnSlot,
};
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::definitions::{
    ChoiceTypeDefinition, ChoiceVariantDefinition, ChoiceVariantPayloadDefinition, FieldDefinition,
    StructTypeDefinition,
};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::{NominalTypeId, TypeId, builtin_type_ids};
use crate::compiler_frontend::declaration_syntax::choice::ChoiceVariant;
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::module::HirModule;
use crate::compiler_frontend::hir::terminators::HirTerminator;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::value_mode::ValueMode;

// ---------------------------------------------------------------------------
// Choice definition helper used by build_ast_with_choices
// ---------------------------------------------------------------------------

pub(crate) struct HirTestChoiceDefinition {
    pub(crate) nominal_path: InternedPath,
    pub(crate) variants: Vec<ChoiceVariant>,
}

// ---------------------------------------------------------------------------
// Return-slot helpers
// ---------------------------------------------------------------------------

pub(crate) use crate::compiler_frontend::tests::ast_fixture_support::fresh_success_returns;

pub(crate) fn success_return_slot(type_id: TypeId) -> ReturnSlot {
    ReturnSlot {
        value: FunctionReturn::Value(DataType::Inferred),
        type_id: Some(type_id),
        channel: ReturnChannel::Success,
    }
}

pub(crate) fn error_return_slot(type_id: TypeId) -> ReturnSlot {
    ReturnSlot {
        value: FunctionReturn::Value(DataType::Inferred),
        type_id: Some(type_id),
        channel: ReturnChannel::Error,
    }
}

// ---------------------------------------------------------------------------
// Parameter / declaration helpers
// ---------------------------------------------------------------------------

pub(crate) fn param_with_type_id(
    name: InternedPath,
    type_id: TypeId,
    mutable: bool,
    location: SourceLocation,
) -> Declaration {
    param_declaration(name, type_id, mutable, location)
}

pub(crate) fn param_declaration(
    name: InternedPath,
    type_id: TypeId,
    mutable: bool,
    location: SourceLocation,
) -> Declaration {
    let value_mode = if mutable {
        ValueMode::MutableOwned
    } else {
        ValueMode::ImmutableOwned
    };

    Declaration {
        id: name,
        value: Expression::new(
            ExpressionKind::NoValue,
            location,
            type_id,
            DataType::Inferred,
            value_mode,
        ),
    }
}

pub(crate) fn loop_binding_with_type_id(
    name: &str,
    type_id: TypeId,
    string_table: &mut crate::compiler_frontend::symbols::string_interning::StringTable,
) -> Declaration {
    let location = crate::compiler_frontend::tokenizer::tokens::SourceLocation::default();
    param_with_type_id(
        InternedPath::from_single_str(name, string_table),
        type_id,
        false,
        location,
    )
}

// ---------------------------------------------------------------------------
// Expression helpers
// ---------------------------------------------------------------------------

pub(crate) fn reference_expr(
    name: InternedPath,
    type_id: TypeId,
    location: SourceLocation,
    value_mode: ValueMode,
) -> Expression {
    Expression::reference_with_type_id(
        name,
        DataType::Inferred,
        type_id,
        location,
        value_mode,
        ConstRecordState::RuntimeValue,
    )
}

pub(crate) fn const_record_reference_expr(
    name: InternedPath,
    type_id: TypeId,
    location: SourceLocation,
    value_mode: ValueMode,
) -> Expression {
    Expression::reference_with_type_id(
        name,
        DataType::Inferred,
        type_id,
        location,
        value_mode,
        ConstRecordState::ConstRecord,
    )
}

pub(crate) fn no_value_expr(
    type_id: TypeId,
    location: SourceLocation,
    value_mode: ValueMode,
) -> Expression {
    Expression::new(
        ExpressionKind::NoValue,
        location,
        type_id,
        DataType::Inferred,
        value_mode,
    )
}

pub(crate) fn runtime_expr(
    nodes: Vec<AstNode>,
    type_id: TypeId,
    location: SourceLocation,
    value_mode: ValueMode,
) -> Expression {
    let contains_regular_division = nodes.iter().any(node_has_regular_division);
    Expression::new(
        ExpressionKind::Runtime(nodes),
        location,
        type_id,
        DataType::Inferred,
        value_mode,
    )
    .with_regular_division_provenance(contains_regular_division)
}

pub(crate) fn collection_expr(
    items: Vec<Expression>,
    location: SourceLocation,
    value_mode: ValueMode,
) -> Expression {
    let contains_regular_division = items.iter().any(|item| item.contains_regular_division);
    Expression::new(
        ExpressionKind::Collection(items),
        location,
        builtin_type_ids::NONE,
        DataType::Inferred,
        value_mode,
    )
    .with_regular_division_provenance(contains_regular_division)
}

fn node_has_regular_division(node: &AstNode) -> bool {
    use crate::compiler_frontend::ast::ast_nodes::NodeKind;
    matches!(
        node.kind,
        NodeKind::Operator(
            crate::compiler_frontend::ast::expressions::expression::Operator::Divide
        )
    )
}

pub(crate) fn multi_bind_target(
    id: InternedPath,
    type_id: TypeId,
    value_mode: ValueMode,
    kind: MultiBindTargetKind,
    location: SourceLocation,
) -> MultiBindTarget {
    MultiBindTarget {
        id,
        diagnostic_type: DataType::Inferred,
        type_id,
        value_mode,
        kind,
        location,
    }
}

pub(crate) fn field_access_node(
    base: AstNode,
    field: crate::compiler_frontend::symbols::string_interning::StringId,
    type_id: TypeId,
    const_record_state: ConstRecordState,
    value_mode: ValueMode,
    location: SourceLocation,
) -> AstNode {
    AstNode {
        kind: NodeKind::FieldAccess {
            base: Box::new(base),
            field,
            diagnostic_type: DataType::Inferred,
            type_id,
            const_record_state,
            value_mode,
        },
        location,
        scope: InternedPath::new(),
    }
}

pub(crate) fn choice_construct_expr(
    nominal_path: InternedPath,
    variant: crate::compiler_frontend::symbols::string_interning::StringId,
    tag: usize,
    fields: Vec<Declaration>,
    type_id: TypeId,
    location: SourceLocation,
    value_mode: ValueMode,
) -> Expression {
    Expression::choice_construct(
        crate::compiler_frontend::ast::expressions::expression::ChoiceConstructInput {
            nominal_path,
            variant,
            tag,
            fields,
            diagnostic_type: DataType::Inferred,
            type_id,
            location,
            value_mode,
        },
    )
}

pub(crate) fn option_none_expr(
    inner_type_id: TypeId,
    type_environment: &mut TypeEnvironment,
    location: SourceLocation,
) -> Expression {
    Expression::option_none_with_type_id(
        inner_type_id,
        DataType::Inferred,
        type_environment,
        location,
    )
}

pub(crate) fn result_carrier_type_id(
    type_environment: &mut TypeEnvironment,
    success_type_id: TypeId,
    error_type_id: TypeId,
) -> TypeId {
    type_environment.intern_fallible_carrier(success_type_id, error_type_id)
}

pub(crate) fn handled_result_expr(
    value: Expression,
    handling: crate::compiler_frontend::ast::expressions::expression::FallibleHandling,
    result_type_id: TypeId,
    location: SourceLocation,
) -> Expression {
    Expression::handled_result_with_type_id(
        value,
        handling,
        result_type_id,
        DataType::Inferred,
        location,
    )
}

pub(crate) fn alias_candidates_return_slot(
    parameter_indices: Vec<usize>,
    type_id: TypeId,
) -> ReturnSlot {
    ReturnSlot {
        value: FunctionReturn::AliasCandidates {
            parameter_indices,
            data_type: DataType::Inferred,
        },
        type_id: Some(type_id),
        channel: ReturnChannel::Success,
    }
}

// ---------------------------------------------------------------------------
// AST construction
// ---------------------------------------------------------------------------

fn register_collection_types_from_nodes(
    nodes: &mut [AstNode],
    type_environment: &mut TypeEnvironment,
) {
    for node in nodes.iter_mut() {
        register_collection_types_from_node(node, type_environment);
    }
}

fn register_collection_types_from_node(node: &mut AstNode, type_environment: &mut TypeEnvironment) {
    match &mut node.kind {
        NodeKind::Return(exprs) => {
            for expr in exprs {
                register_collection_types_from_expression(expr, type_environment);
            }
        }
        NodeKind::ReturnError(expr) => {
            register_collection_types_from_expression(expr, type_environment);
        }
        NodeKind::If(condition, then_body, else_body) => {
            register_collection_types_from_expression(condition, type_environment);
            register_collection_types_from_nodes(then_body, type_environment);
            if let Some(else_nodes) = else_body {
                register_collection_types_from_nodes(else_nodes, type_environment);
            }
        }
        NodeKind::Match {
            scrutinee,
            arms,
            default,
            ..
        } => {
            register_collection_types_from_expression(scrutinee, type_environment);
            for arm in arms {
                register_collection_types_from_nodes(&mut arm.body, type_environment);
            }
            if let Some(default_nodes) = default {
                register_collection_types_from_nodes(default_nodes, type_environment);
            }
        }
        NodeKind::ScopedBlock { body } => {
            register_collection_types_from_nodes(body, type_environment);
        }
        NodeKind::RangeLoop { range, body, .. } => {
            register_collection_types_from_expression(&mut range.start, type_environment);
            register_collection_types_from_expression(&mut range.end, type_environment);
            if let Some(step) = &mut range.step {
                register_collection_types_from_expression(step, type_environment);
            }
            register_collection_types_from_nodes(body, type_environment);
        }
        NodeKind::CollectionLoop { iterable, body, .. } => {
            register_collection_types_from_expression(iterable, type_environment);
            register_collection_types_from_nodes(body, type_environment);
        }
        NodeKind::WhileLoop(condition, body) => {
            register_collection_types_from_expression(condition, type_environment);
            register_collection_types_from_nodes(body, type_environment);
        }
        NodeKind::VariableDeclaration(Declaration { value, .. }) => {
            register_collection_types_from_expression(value, type_environment);
        }
        NodeKind::PushStartRuntimeFragment(expr) => {
            register_collection_types_from_expression(expr, type_environment);
        }
        NodeKind::FieldAccess { base, .. } => {
            register_collection_types_from_node(base, type_environment);
        }
        NodeKind::MethodCall { receiver, args, .. }
        | NodeKind::DynamicTraitMethodCall { receiver, args, .. } => {
            register_collection_types_from_node(receiver, type_environment);
            for arg in args {
                register_collection_types_from_expression(&mut arg.value, type_environment);
            }
        }
        NodeKind::CollectionBuiltinCall { receiver, args, .. }
        | NodeKind::MapBuiltinCall { receiver, args, .. } => {
            register_collection_types_from_node(receiver, type_environment);
            for arg in args {
                register_collection_types_from_expression(&mut arg.value, type_environment);
            }
        }
        NodeKind::FunctionCall { args, .. } => {
            for arg in args {
                register_collection_types_from_expression(&mut arg.value, type_environment);
            }
        }
        NodeKind::HandledFallibleFunctionCall { args, .. }
        | NodeKind::HandledFallibleHostFunctionCall { args, .. }
        | NodeKind::HostFunctionCall { args, .. } => {
            for arg in args {
                register_collection_types_from_expression(&mut arg.value, type_environment);
            }
        }
        NodeKind::StructDefinition(_, fields) => {
            for field in fields.iter_mut() {
                register_collection_types_from_expression(&mut field.value, type_environment);
            }
        }
        NodeKind::Function(_, _, body) => {
            register_collection_types_from_nodes(body, type_environment);
        }
        NodeKind::ThenValue(produced_values) => {
            for expr in &mut produced_values.expressions {
                register_collection_types_from_expression(expr, type_environment);
            }
        }
        _ => {}
    }
}

fn register_collection_types_from_expression(
    expr: &mut Expression,
    type_environment: &mut TypeEnvironment,
) {
    if let ExpressionKind::Collection(items) = &mut expr.kind {
        if let Some(first) = items.first() {
            let collection_type_id = type_environment.intern_collection(first.type_id, None);
            expr.type_id = collection_type_id;
        }
        for item in items {
            register_collection_types_from_expression(item, type_environment);
        }
        return;
    }

    match &mut expr.kind {
        ExpressionKind::Runtime(nodes) => {
            register_collection_types_from_nodes(nodes, type_environment);
        }
        ExpressionKind::Copy(base) => {
            register_collection_types_from_node(base, type_environment);
        }
        ExpressionKind::Function(_, body) => {
            register_collection_types_from_nodes(body, type_environment);
        }
        ExpressionKind::FunctionCall { args, .. }
        | ExpressionKind::HandledFallibleFunctionCall { args, .. }
        | ExpressionKind::HandledFallibleHostFunctionCall { args, .. }
        | ExpressionKind::HostFunctionCall { args, .. } => {
            for arg in args {
                register_collection_types_from_expression(&mut arg.value, type_environment);
            }
        }
        ExpressionKind::StructDefinition(fields)
        | ExpressionKind::StructInstance(fields)
        | ExpressionKind::ChoiceConstruct { fields, .. } => {
            for field in fields {
                register_collection_types_from_expression(&mut field.value, type_environment);
            }
        }
        ExpressionKind::Range(start, end) => {
            register_collection_types_from_expression(start, type_environment);
            register_collection_types_from_expression(end, type_environment);
        }
        _ => {}
    }
}

pub(crate) fn choice_type_id(
    path: InternedPath,
    variants: &[crate::compiler_frontend::declaration_syntax::choice::ChoiceVariant],
) -> TypeId {
    let mut type_environment = TypeEnvironment::new();
    let variant_definitions = variants
        .iter()
        .enumerate()
        .map(|(tag, variant)| ChoiceVariantDefinition {
            name: variant.id,
            tag,
            payload: match &variant.payload {
                crate::compiler_frontend::declaration_syntax::choice::ChoiceVariantPayload::Unit => {
                    ChoiceVariantPayloadDefinition::Unit
                }
                crate::compiler_frontend::declaration_syntax::choice::ChoiceVariantPayload::Record {
                    fields,
                } => {
                    let field_definitions = fields
                        .iter()
                        .map(|field| FieldDefinition {
                            name: field.id.clone(),
                            type_id: field.value.type_id,
                            location: field.value.location.clone(),
                        })
                        .collect::<Vec<_>>();
                    ChoiceVariantPayloadDefinition::Record {
                        fields: field_definitions.into_boxed_slice(),
                    }
                }
            },
            location: variant.location.clone(),
        })
        .collect::<Vec<_>>();

    let definition = ChoiceTypeDefinition {
        id: NominalTypeId(0),
        path,
        variants: variant_definitions.into_boxed_slice(),
        generic_parameters: None,
    };

    let (_, type_id) = type_environment.register_nominal_choice(definition);
    type_id
}

pub(crate) fn build_ast(nodes: Vec<AstNode>, entry_path: InternedPath) -> Ast {
    build_ast_with_choices(nodes, entry_path, vec![])
}

pub(crate) fn build_ast_with_choices(
    mut nodes: Vec<AstNode>,
    entry_path: InternedPath,
    choice_definitions: Vec<HirTestChoiceDefinition>,
) -> Ast {
    let mut type_environment = TypeEnvironment::new();

    // Register struct definitions from AST nodes so that HIR lowering can
    // resolve frontend TypeIds during declaration registration.
    for node in &nodes {
        if let NodeKind::StructDefinition(name, fields) = &node.kind {
            let field_definitions = fields
                .iter()
                .map(|field| FieldDefinition {
                    name: field.id.clone(),
                    type_id: field.value.type_id,
                    location: field.value.location.clone(),
                })
                .collect::<Vec<_>>();

            let definition = StructTypeDefinition {
                id: NominalTypeId(0),
                path: name.clone(),
                fields: field_definitions.into_boxed_slice(),
                generic_parameters: None,
                const_record: false,
            };

            let _ = type_environment.register_nominal_struct(definition);
        }
    }

    // Register choice definitions so expression canonicalization can resolve
    // choice TypeIds before HIR lowering.
    for choice_def in &choice_definitions {
        let variant_definitions = choice_def
            .variants
            .iter()
            .enumerate()
            .map(|(tag, variant)| ChoiceVariantDefinition {
                name: variant.id,
                tag,
                payload: match &variant.payload {
                    crate::compiler_frontend::declaration_syntax::choice::ChoiceVariantPayload::Unit => {
                        ChoiceVariantPayloadDefinition::Unit
                    }
                    crate::compiler_frontend::declaration_syntax::choice::ChoiceVariantPayload::Record {
                        fields,
                    } => {
                        let field_definitions = fields
                            .iter()
                            .map(|field| FieldDefinition {
                                name: field.id.clone(),
                                type_id: field.value.type_id,
                                location: field.value.location.clone(),
                            })
                            .collect::<Vec<_>>();
                        ChoiceVariantPayloadDefinition::Record {
                            fields: field_definitions.into_boxed_slice(),
                        }
                    }
                },
                location: variant.location.clone(),
            })
            .collect::<Vec<_>>();

        let definition = ChoiceTypeDefinition {
            id: NominalTypeId(0),
            path: choice_def.nominal_path.clone(),
            variants: variant_definitions.into_boxed_slice(),
            generic_parameters: None,
        };

        let _ = type_environment.register_nominal_choice(definition);
    }

    // Scan AST for collection literals and register their constructed types so
    // that HIR loop lowering can resolve collection type identities.
    register_collection_types_from_nodes(&mut nodes, &mut type_environment);

    Ast {
        nodes,
        module_constants: vec![],
        doc_fragments: vec![],
        entry_path,
        const_top_level_fragments: vec![],
        rendered_path_usages: vec![],
        warnings: vec![],
        choice_definitions: choice_definitions
            .into_iter()
            .map(
                |definition| crate::compiler_frontend::ast::AstChoiceDefinition {
                    nominal_path: definition.nominal_path,
                },
            )
            .collect(),
        type_environment,
        trait_environment: crate::compiler_frontend::traits::environment::TraitEnvironment::new(),
        trait_evidence_environment:
            crate::compiler_frontend::traits::evidence::TraitEvidenceEnvironment::new(),
        const_facts: AstConstFacts::default(),
    }
}

/// Lower a test `Ast` into a `HirModule` and its `TypeEnvironment`.
pub(crate) fn lower_ast(
    ast: Ast,
    string_table: &mut crate::compiler_frontend::symbols::string_interning::StringTable,
) -> Result<
    (
        HirModule,
        crate::compiler_frontend::datatypes::environment::TypeEnvironment,
    ),
    CompilerMessages,
> {
    let type_environment = ast.type_environment.clone();
    HirBuilder::new(
        string_table,
        PathStringFormatConfig::default(),
        type_environment,
    )
    .build_hir_module(ast)
}

/// Assert that no block ends with a placeholder `Uninitialized` terminator.
pub(crate) fn assert_no_placeholder_terminators(module: &HirModule) {
    assert!(
        module
            .blocks
            .iter()
            .all(|block| !matches!(block.terminator, HirTerminator::Uninitialized)),
        "expected no placeholder Uninitialized terminators in lowered HIR"
    );
}
