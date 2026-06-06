//! HIR validation regression tests.
//!
//! WHAT: exercises the post-lowering HIR validator against valid and intentionally broken modules.
//! WHY: validator coverage needs focused tests that isolate invariants from the rest of lowering.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind, SourceLocation};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::{AstDocFragment, AstDocFragmentKind};
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorType};
use crate::compiler_frontend::datatypes::definitions::StructTypeDefinition;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::{
    BuiltinTypeConstructor, FunctionTypeKey, GenericParameterId, NominalTypeId, TypeConstructor,
    TypeId, builtin_type_ids,
};
use crate::compiler_frontend::declaration_syntax::choice::{ChoiceVariant, ChoiceVariantPayload};
use crate::compiler_frontend::hir::blocks::HirLocal;
use crate::compiler_frontend::hir::expressions::{
    HirExpression, HirExpressionKind, HirVariantCarrier, HirVariantField, ValueKind,
};
use crate::compiler_frontend::hir::hir_builder::{
    HirTestChoiceDefinition, build_ast, build_ast_with_choices, lower_ast,
    validate_module_for_tests,
};
use crate::compiler_frontend::hir::ids::{
    ChoiceId, FieldId, HirNodeId, HirValueId, LocalId, RegionId, StructId,
};
use crate::compiler_frontend::hir::module::{
    HirChoice, HirChoiceField, HirChoiceVariant, HirModule,
};
use crate::compiler_frontend::hir::patterns::{HirMatchArm, HirPattern};
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::hir::regions::HirRegion;
use crate::compiler_frontend::hir::statements::{
    HirDynamicTraitCallArgumentEffect, HirStatement, HirStatementKind,
};
use crate::compiler_frontend::hir::structs::{HirField, HirStruct};
use crate::compiler_frontend::hir::terminators::HirTerminator;
use crate::compiler_frontend::hir::tests::hir_expression_lowering_tests::location;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tests::type_id_fixture_support::{
    no_value_expr, reference_expr, success_return_slot,
};
use crate::compiler_frontend::traits::evidence::TraitEvidenceDefinition;
use crate::compiler_frontend::traits::evidence::environment::{
    TraitEvidenceKind, TraitRequirementEvidence,
};
use crate::compiler_frontend::traits::ids::{TraitEvidenceId, TraitId, TraitRequirementId};
use crate::compiler_frontend::value_mode::ValueMode;

fn test_location(line: i32) -> SourceLocation {
    location(line)
}

fn node(kind: NodeKind, location: SourceLocation) -> AstNode {
    AstNode {
        kind,
        location,
        scope: InternedPath::new(),
    }
}

fn make_test_variable(name: InternedPath, value: Expression) -> Declaration {
    Declaration { id: name, value }
}

fn param(
    name: InternedPath,
    type_id: TypeId,
    mutable: bool,
    location: SourceLocation,
) -> Declaration {
    crate::compiler_frontend::tests::type_id_fixture_support::param_declaration(
        name, type_id, mutable, location,
    )
}

fn function_node(
    name: InternedPath,
    signature: FunctionSignature,
    body: Vec<AstNode>,
    location: SourceLocation,
) -> AstNode {
    node(NodeKind::Function(name, signature, body), location)
}

// Shared builders for the validation regressions below.
fn generic_parameter_type_id(
    string_table: &mut StringTable,
    type_environment: &mut TypeEnvironment,
) -> TypeId {
    let parameter_name = string_table.intern("T");
    type_environment.intern_generic_parameter(GenericParameterId(0), parameter_name)
}

fn minimal_lowered_hir_module() -> (StringTable, HirModule, TypeEnvironment) {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(1))],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let (module, type_environment) =
        lower_ast(ast, &mut string_table).expect("lowering should succeed");

    (string_table, module, type_environment)
}

fn start_entry_block_index(module: &HirModule) -> usize {
    module.functions[module.start_function.0 as usize].entry.0 as usize
}

fn validation_error_for_injected_local_type(
    build_type: impl FnOnce(&mut StringTable, &mut TypeEnvironment) -> TypeId,
) -> CompilerError {
    let (mut string_table, mut module, mut type_environment) = minimal_lowered_hir_module();
    let local_type_id = build_type(&mut string_table, &mut type_environment);

    let entry_block_index = start_entry_block_index(&module);
    let entry_block = &mut module.blocks[entry_block_index];
    entry_block.locals.push(HirLocal {
        id: LocalId(9000),
        ty: local_type_id,
        mutable: false,
        region: entry_block.region,
        source_info: Some(test_location(20)),
    });

    validate_module_for_tests(&module, &string_table, &type_environment)
        .expect_err("validator should reject unresolved generic parameter inside TypeId")
}

fn inject_collection_expression_statement(
    module: &mut HirModule,
    collection_type_id: TypeId,
    location: SourceLocation,
) {
    let entry_block_index = start_entry_block_index(module);
    let entry_block = &mut module.blocks[entry_block_index];
    let value_id = HirValueId(9000);
    let statement_id = HirNodeId(9000);
    let expression = HirExpression {
        id: value_id,
        kind: HirExpressionKind::Collection(vec![]),
        ty: collection_type_id,
        value_kind: ValueKind::RValue,
        region: entry_block.region,
    };

    let statement = HirStatement {
        id: statement_id,
        kind: HirStatementKind::Expr(expression),
        location: location.clone(),
    };

    module.side_table.map_statement(&location, &statement);
    module.side_table.map_value(&location, value_id, &location);
    entry_block.statements.push(statement);
}

fn register_displayable_evidence_for_type(
    module: &mut HirModule,
    type_environment: &mut TypeEnvironment,
    string_table: &mut StringTable,
    target_type_id: TypeId,
) -> (TypeId, TraitId, TraitEvidenceId, TraitRequirementId) {
    let trait_id = module
        .trait_environment
        .register_core_displayable(type_environment, string_table);
    let trait_definition = module
        .trait_environment
        .get(trait_id)
        .expect("core DISPLAYABLE trait should be registered");
    let requirement = trait_definition
        .requirements
        .first()
        .expect("core DISPLAYABLE should have one requirement");
    let dynamic_trait_type_id =
        type_environment.intern_dynamic_trait(trait_id, trait_definition.name);

    module
        .trait_evidence_environment
        .insert_validated(TraitEvidenceDefinition {
            id: TraitEvidenceId(0),
            kind: TraitEvidenceKind::Canonical,
            target_type_id,
            trait_id,
            source_file: InternedPath::new(),
            declaration_location: test_location(30),
            requirements: vec![TraitRequirementEvidence {
                requirement_id: requirement.id,
                method_path: InternedPath::new(),
            }],
        });

    (
        dynamic_trait_type_id,
        trait_id,
        TraitEvidenceId(0),
        requirement.id,
    )
}

#[test]
fn valid_module_passes_explicit_validation() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(1))],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let (module, type_environment) =
        lower_ast(ast, &mut string_table).expect("lowering should succeed");
    validate_module_for_tests(&module, &string_table, &type_environment)
        .expect("validator should accept a valid lowered module");
}

#[test]
fn validator_rejects_invalid_jump_target() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(1))],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let (mut module, type_environment) =
        lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let entry_block = module.functions[module.start_function.0 as usize].entry;
    module.blocks[entry_block.0 as usize].terminator = HirTerminator::Jump {
        target: crate::compiler_frontend::hir::ids::BlockId(999),
        args: vec![],
    };

    let error = validate_module_for_tests(&module, &string_table, &type_environment)
        .expect_err("validator should reject invalid jump target");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("Unknown HIR block id"));
}

#[test]
fn validator_rejects_non_literal_match_pattern() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let x = super::symbol("x", &mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![param(
                x.clone(),
                builtin_type_ids::INT,
                false,
                test_location(2),
            )],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(3))],
        test_location(2),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let (mut module, type_environment) =
        lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let start = &module.functions[module.start_function.0 as usize];
    let entry_block = &mut module.blocks[start.entry.0 as usize];
    let local_id = start.params[0];
    let local_ty = entry_block.locals[0].ty;
    let region = entry_block.region;
    let scrutinee_id = HirValueId(9000);
    let pattern_id = HirValueId(9001);

    let value_location = test_location(20);
    module
        .side_table
        .map_value(&value_location, scrutinee_id, &value_location);
    module
        .side_table
        .map_value(&value_location, pattern_id, &value_location);

    entry_block.terminator = HirTerminator::Match {
        scrutinee: HirExpression {
            id: scrutinee_id,
            kind: HirExpressionKind::Int(1),
            ty: local_ty,
            value_kind: ValueKind::Const,
            region,
        },
        arms: vec![HirMatchArm {
            pattern: HirPattern::Literal(HirExpression {
                id: pattern_id,
                kind: HirExpressionKind::Load(HirPlace::Local(local_id)),
                ty: local_ty,
                value_kind: ValueKind::Place,
                region,
            }),
            guard: None,
            body: start.entry,
        }],
    };

    let error = validate_module_for_tests(&module, &string_table, &type_environment)
        .expect_err("validator should reject non-literal match pattern");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("Match literal pattern"));
}

#[test]
fn validator_rejects_missing_side_table_mappings() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let x = super::symbol("x", &mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    x,
                    Expression::int(1, test_location(4), ValueMode::ImmutableOwned),
                )),
                test_location(4),
            ),
            node(NodeKind::Return(vec![]), test_location(5)),
        ],
        test_location(3),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let (mut module, type_environment) =
        lower_ast(ast, &mut string_table).expect("lowering should succeed");
    module.side_table.clear();

    let error = validate_module_for_tests(&module, &string_table, &type_environment)
        .expect_err("validator should reject missing side-table mappings");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("side-table mapping"));
}

#[test]
fn validator_rejects_unresolved_generic_parameter_types() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(1))],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let (mut module, mut type_environment) =
        lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let parameter_name = string_table.intern("T");
    let generic_type_id =
        type_environment.intern_generic_parameter(GenericParameterId(0), parameter_name);

    let entry_block =
        &mut module.blocks[module.functions[module.start_function.0 as usize].entry.0 as usize];
    entry_block.locals.push(HirLocal {
        id: LocalId(9000),
        ty: generic_type_id,
        mutable: false,
        region: entry_block.region,
        source_info: Some(test_location(20)),
    });

    let error = validate_module_for_tests(&module, &string_table, &type_environment)
        .expect_err("validator should reject unresolved generic parameter TypeIds");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("Unresolved generic parameter"));
}

#[test]
fn validator_rejects_collection_containing_generic_parameter() {
    let error = validation_error_for_injected_local_type(|string_table, type_environment| {
        let generic_type_id = generic_parameter_type_id(string_table, type_environment);
        type_environment.intern_constructed(
            TypeConstructor::Builtin(BuiltinTypeConstructor::Collection {
                fixed_capacity: None,
            }),
            Box::new([generic_type_id]),
        )
    });

    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("Unresolved generic parameter"));
}

#[test]
fn validator_rejects_option_and_result_containing_generic_parameter() {
    let option_error =
        validation_error_for_injected_local_type(|string_table, type_environment| {
            let generic_type_id = generic_parameter_type_id(string_table, type_environment);
            type_environment.intern_constructed(
                TypeConstructor::Builtin(BuiltinTypeConstructor::Option),
                Box::new([generic_type_id]),
            )
        });
    assert_eq!(option_error.error_type, ErrorType::HirTransformation);
    assert!(option_error.msg.contains("Unresolved generic parameter"));

    let result_error =
        validation_error_for_injected_local_type(|string_table, type_environment| {
            let generic_type_id = generic_parameter_type_id(string_table, type_environment);
            type_environment.intern_constructed(
                TypeConstructor::Builtin(BuiltinTypeConstructor::FallibleCarrier),
                Box::new([type_environment.builtins().int, generic_type_id]),
            )
        });
    assert_eq!(result_error.error_type, ErrorType::HirTransformation);
    assert!(result_error.msg.contains("Unresolved generic parameter"));
}

#[test]
fn validator_rejects_generic_nominal_instance_containing_generic_parameter() {
    let error = validation_error_for_injected_local_type(|string_table, type_environment| {
        let generic_type_id = generic_parameter_type_id(string_table, type_environment);
        let box_path = InternedPath::from_single_str("Box", string_table);
        let (nominal_id, _) = type_environment.register_nominal_struct(StructTypeDefinition {
            id: NominalTypeId(0),
            path: box_path,
            fields: Box::new([]),
            generic_parameters: None,
            const_record: false,
        });

        type_environment.intern_generic_instance(nominal_id, Box::new([generic_type_id]))
    });

    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("Unresolved generic parameter"));
}

#[test]
fn validator_rejects_function_type_containing_generic_parameter() {
    let error = validation_error_for_injected_local_type(|string_table, type_environment| {
        let generic_type_id = generic_parameter_type_id(string_table, type_environment);
        type_environment.intern_function(FunctionTypeKey {
            parameters: Box::new([generic_type_id]),
            returns: Box::new([type_environment.builtins().int]),
            error_return: None,
        })
    });

    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("Unresolved generic parameter"));
}

#[test]
fn validator_rejects_struct_field_type_containing_generic_parameter() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(1))],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let (mut module, mut type_environment) =
        lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let generic_type_id = generic_parameter_type_id(&mut string_table, &mut type_environment);
    let collection_type_id = type_environment.intern_constructed(
        TypeConstructor::Builtin(BuiltinTypeConstructor::Collection {
            fixed_capacity: None,
        }),
        Box::new([generic_type_id]),
    );

    module.structs.push(HirStruct {
        id: StructId(9000),
        frontend_type_id: type_environment.builtins().int,
        fields: vec![HirField {
            id: FieldId(9000),
            ty: collection_type_id,
        }],
    });

    let error = validate_module_for_tests(&module, &string_table, &type_environment)
        .expect_err("validator should reject unresolved generic parameter in HIR field types");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("Unresolved generic parameter"));
}

#[test]
fn validator_rejects_function_return_type_containing_generic_parameter() {
    let (mut string_table, mut module, mut type_environment) = minimal_lowered_hir_module();
    let generic_type_id = generic_parameter_type_id(&mut string_table, &mut type_environment);

    let start_index = module.start_function.0 as usize;
    module.functions[start_index].return_type = generic_type_id;

    let error = validate_module_for_tests(&module, &string_table, &type_environment)
        .expect_err("validator should reject unresolved generic parameter in return types");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("Unresolved generic parameter"));
}

#[test]
fn validator_rejects_function_parameter_type_containing_generic_parameter() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let value_name = super::symbol("value", &mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![param(
                value_name,
                builtin_type_ids::INT,
                false,
                test_location(1),
            )],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(2))],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let (mut module, mut type_environment) =
        lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let generic_type_id = generic_parameter_type_id(&mut string_table, &mut type_environment);

    let entry_block =
        &mut module.blocks[module.functions[module.start_function.0 as usize].entry.0 as usize];
    entry_block.locals[0].ty = generic_type_id;

    let error = validate_module_for_tests(&module, &string_table, &type_environment)
        .expect_err("validator should reject unresolved generic parameter in parameter locals");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("Unresolved generic parameter"));
}

#[test]
fn validator_rejects_choice_payload_type_containing_generic_parameter() {
    let (mut string_table, mut module, mut type_environment) = minimal_lowered_hir_module();
    let generic_type_id = generic_parameter_type_id(&mut string_table, &mut type_environment);
    let field_name = string_table.intern("value");

    module.choices.push(HirChoice {
        id: ChoiceId(9000),
        frontend_type_id: type_environment.builtins().int,
        variants: vec![HirChoiceVariant {
            name: string_table.intern("Some"),
            fields: vec![HirChoiceField {
                name: field_name,
                ty: generic_type_id,
            }],
        }],
    });

    let error = validate_module_for_tests(&module, &string_table, &type_environment)
        .expect_err("validator should reject unresolved generic parameter in choice payloads");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("Unresolved generic parameter"));
}

#[test]
fn validator_rejects_expression_type_containing_generic_parameter() {
    let (mut string_table, mut module, mut type_environment) = minimal_lowered_hir_module();
    let generic_type_id = generic_parameter_type_id(&mut string_table, &mut type_environment);

    let entry_block_index = start_entry_block_index(&module);
    let entry_block = &mut module.blocks[entry_block_index];
    let value_id = HirValueId(9000);
    let statement_id = HirNodeId(9000);
    let location = test_location(20);
    let expression = HirExpression {
        id: value_id,
        kind: HirExpressionKind::Int(1),
        ty: generic_type_id,
        value_kind: ValueKind::Const,
        region: entry_block.region,
    };
    let statement = HirStatement {
        id: statement_id,
        kind: HirStatementKind::Expr(expression),
        location: location.clone(),
    };

    module.side_table.map_statement(&location, &statement);
    module.side_table.map_value(&location, value_id, &location);
    entry_block.statements.push(statement);

    let error = validate_module_for_tests(&module, &string_table, &type_environment)
        .expect_err("validator should reject unresolved generic parameter in expression types");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("Unresolved generic parameter"));
}

#[test]
fn validator_rejects_invalid_doc_fragment_location() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(1))],
        test_location(1),
    );

    let mut ast = build_ast(vec![start_fn], entry_path);
    let mut invalid_location = test_location(10);
    invalid_location.end_pos.line_number = 9;
    ast.doc_fragments.push(AstDocFragment {
        kind: AstDocFragmentKind::Doc,
        value: string_table.intern("broken"),
        location: invalid_location,
    });

    let error = lower_ast(ast, &mut string_table)
        .expect_err("validator should reject invalid doc fragment locations");
    let (_error_type, message, _location) = error
        .first_infrastructure_error_for_tests()
        .expect("HIR validation failure should be wrapped for rendering");
    assert!(message.contains("Doc fragment"));
}

#[test]
fn validator_rejects_placeholder_terminator() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(1))],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let (mut module, type_environment) =
        lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let entry = module.functions[module.start_function.0 as usize].entry;
    module.blocks[entry.0 as usize].terminator = HirTerminator::Uninitialized;

    let error = validate_module_for_tests(&module, &string_table, &type_environment)
        .expect_err("validator should reject placeholder terminators");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("placeholder terminator"));
}

#[test]
fn validator_rejects_region_cycle() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(1))],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let (mut module, type_environment) =
        lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let region_id = module.regions[0].id();
    module.regions[0] = HirRegion::lexical(region_id, Some(region_id));

    let error = validate_module_for_tests(&module, &string_table, &type_environment)
        .expect_err("validator should reject cyclic region parents");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("cycle"));
}

#[test]
fn validator_rejects_missing_region_parent() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(1))],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let (mut module, type_environment) =
        lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let region_id = module.regions[0].id();
    module.regions[0] = HirRegion::lexical(region_id, Some(RegionId(9999)));

    let error = validate_module_for_tests(&module, &string_table, &type_environment)
        .expect_err("validator should reject missing region parents");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("missing parent"));
}

#[test]
fn validator_rejects_out_of_range_return_alias_metadata() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let p = super::symbol("p", &mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![param(
                p.clone(),
                builtin_type_ids::INT,
                false,
                test_location(1),
            )],
            returns: vec![success_return_slot(builtin_type_ids::INT)],
        },
        vec![node(
            NodeKind::Return(vec![reference_expr(
                p,
                builtin_type_ids::INT,
                test_location(2),
                ValueMode::ImmutableReference,
            )]),
            test_location(2),
        )],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let (mut module, type_environment) =
        lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let start_index = module.start_function.0 as usize;
    module.functions[start_index].return_aliases = vec![Some(vec![1])];

    let error = validate_module_for_tests(&module, &string_table, &type_environment)
        .expect_err("validator should reject out-of-range return alias indices");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("out-of-range parameter index"));
}

#[test]
fn validator_rejects_cross_function_cfg_edges() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let helper_name = super::symbol("helper", &mut string_table);

    let helper = function_node(
        helper_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(2))],
        test_location(2),
    );
    let start = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(1))],
        test_location(1),
    );

    let ast = build_ast(vec![helper, start], entry_path);
    let (mut module, type_environment) =
        lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let start_entry = module.functions[module.start_function.0 as usize].entry;
    let helper_entry = module
        .functions
        .iter()
        .find(|function| function.id != module.start_function)
        .map(|function| function.entry)
        .expect("helper function should exist");

    module.blocks[start_entry.0 as usize].terminator = HirTerminator::Jump {
        target: helper_entry,
        args: vec![],
    };

    let error = validate_module_for_tests(&module, &string_table, &type_environment)
        .expect_err("validator should reject cross-function CFG edges");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(
        error.msg.contains("multiple functions") || error.msg.contains("crosses function boundary")
    );
}

#[test]
fn lowering_errors_preserve_string_table_context() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let missing_function = super::symbol("missing_fn", &mut string_table);

    let mut call_location = test_location(2);
    call_location.scope = entry_path.clone();

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::Rvalue(Expression::function_call(
                    missing_function,
                    Vec::new(),
                    Vec::new(),
                    call_location.clone(),
                )),
                call_location.clone(),
            ),
            node(NodeKind::Return(vec![]), test_location(3)),
        ],
        test_location(1),
    );

    let messages = lower_ast(build_ast(vec![start_fn], entry_path), &mut string_table)
        .expect_err("unknown function call should fail HIR lowering");

    let resolved_scope = messages
        .first_error()
        .expect("expected HIR lowering error")
        .primary_location
        .scope
        .to_portable_string(&messages.string_table);
    assert!(
        resolved_scope.ends_with("main.bst"),
        "HIR lowering errors should preserve the source path in the returned StringTable, got '{resolved_scope}'",
    );
}

// ---------------------------------------------------------------------------
// Dynamic trait validation
// ---------------------------------------------------------------------------

#[test]
fn validator_rejects_dynamic_trait_dispatch_with_non_dynamic_receiver() {
    let (mut string_table, mut module, mut type_environment) = minimal_lowered_hir_module();
    let (_dynamic_type_id, trait_id, _evidence_id, requirement_id) =
        register_displayable_evidence_for_type(
            &mut module,
            &mut type_environment,
            &mut string_table,
            builtin_type_ids::INT,
        );
    let entry_block =
        &mut module.blocks[module.functions[module.start_function.0 as usize].entry.0 as usize];
    let region = entry_block.region;
    let receiver_id = HirValueId(9000);
    let statement_id = HirNodeId(9000);
    let location = test_location(40);

    let receiver = HirExpression {
        id: receiver_id,
        kind: HirExpressionKind::Int(1),
        ty: builtin_type_ids::INT,
        value_kind: ValueKind::Const,
        region,
    };
    let statement = HirStatement {
        id: statement_id,
        kind: HirStatementKind::CallDynamicTraitMethod {
            receiver,
            receiver_effect: HirDynamicTraitCallArgumentEffect::SharedBorrow,
            trait_id,
            requirement_id,
            args: Vec::new(),
            result: None,
        },
        location: location.clone(),
    };

    module.side_table.map_statement(&location, &statement);
    module
        .side_table
        .map_value(&location, receiver_id, &location);
    entry_block.statements.push(statement);

    let error = validate_module_for_tests(&module, &string_table, &type_environment)
        .expect_err("validator should reject dynamic dispatch through a non-dynamic receiver");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(
        error.msg.contains("not a dynamic trait type"),
        "expected dynamic receiver type error, got: {}",
        error.msg
    );
}

#[test]
fn validator_rejects_dynamic_trait_construction_with_wrong_evidence_target() {
    let (mut string_table, mut module, mut type_environment) = minimal_lowered_hir_module();
    let (dynamic_type_id, trait_id, evidence_id, _requirement_id) =
        register_displayable_evidence_for_type(
            &mut module,
            &mut type_environment,
            &mut string_table,
            builtin_type_ids::INT,
        );
    let entry_block =
        &mut module.blocks[module.functions[module.start_function.0 as usize].entry.0 as usize];
    let region = entry_block.region;
    let inner_id = HirValueId(9000);
    let outer_id = HirValueId(9001);
    let statement_id = HirNodeId(9000);
    let location = test_location(50);

    let inner_value = HirExpression {
        id: inner_id,
        kind: HirExpressionKind::StringLiteral("wrong target".to_owned()),
        ty: builtin_type_ids::STRING,
        value_kind: ValueKind::Const,
        region,
    };
    let expression = HirExpression {
        id: outer_id,
        kind: HirExpressionKind::ConstructDynamicTraitValue {
            value: Box::new(inner_value),
            trait_id,
            evidence_id,
        },
        ty: dynamic_type_id,
        value_kind: ValueKind::RValue,
        region,
    };
    let statement = HirStatement {
        id: statement_id,
        kind: HirStatementKind::Expr(expression),
        location: location.clone(),
    };

    module.side_table.map_statement(&location, &statement);
    module.side_table.map_value(&location, inner_id, &location);
    module.side_table.map_value(&location, outer_id, &location);
    entry_block.statements.push(statement);

    let error = validate_module_for_tests(&module, &string_table, &type_environment)
        .expect_err("validator should reject evidence that targets a different concrete type");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(
        error.msg.contains("targets type"),
        "expected evidence target type error, got: {}",
        error.msg
    );
}

// ---------------------------------------------------------------------------
// VariantConstruct validation
// ---------------------------------------------------------------------------

#[test]
fn hir_variant_construct_option_invalid_index_rejected() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(1))],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let (mut module, type_environment) =
        lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let entry_block =
        &mut module.blocks[module.functions[module.start_function.0 as usize].entry.0 as usize];
    let region = entry_block.region;

    let mut type_env = type_environment.clone();
    let int_ty = builtin_type_ids::INT;
    let option_ty = type_env.intern_constructed(
        crate::compiler_frontend::datatypes::ids::TypeConstructor::Builtin(
            crate::compiler_frontend::datatypes::ids::BuiltinTypeConstructor::Option,
        ),
        Box::new([int_ty]),
    );

    let expr_id = HirValueId(9000);
    let stmt_id = HirNodeId(9000);
    let location = test_location(10);

    let expression = HirExpression {
        id: expr_id,
        kind: HirExpressionKind::VariantConstruct {
            carrier: HirVariantCarrier::Option,
            variant_index: 99,
            fields: vec![],
        },
        ty: option_ty,
        value_kind: ValueKind::Const,
        region,
    };

    let statement = HirStatement {
        id: stmt_id,
        kind: HirStatementKind::Expr(expression),
        location: location.clone(),
    };

    module.side_table.map_statement(&location, &statement);
    module.side_table.map_value(&location, expr_id, &location);
    entry_block.statements.push(statement);

    let error = validate_module_for_tests(&module, &string_table, &type_env)
        .expect_err("validator should reject out-of-range Option variant index");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(
        error.msg.contains("out of range"),
        "expected 'out of range' in error, got: {}",
        error.msg
    );
}

#[test]
fn hir_variant_construct_result_invalid_index_rejected() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(1))],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let (mut module, type_environment) =
        lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let entry_block =
        &mut module.blocks[module.functions[module.start_function.0 as usize].entry.0 as usize];
    let region = entry_block.region;

    let mut type_env = type_environment.clone();
    let int_ty = builtin_type_ids::INT;
    let result_ty = type_env.intern_constructed(
        crate::compiler_frontend::datatypes::ids::TypeConstructor::Builtin(
            crate::compiler_frontend::datatypes::ids::BuiltinTypeConstructor::FallibleCarrier,
        ),
        Box::new([int_ty, int_ty]),
    );

    let expr_id = HirValueId(9000);
    let stmt_id = HirNodeId(9000);
    let location = test_location(10);

    let expression = HirExpression {
        id: expr_id,
        kind: HirExpressionKind::VariantConstruct {
            carrier: HirVariantCarrier::Fallible,
            variant_index: 99,
            fields: vec![],
        },
        ty: result_ty,
        value_kind: ValueKind::Const,
        region,
    };

    let statement = HirStatement {
        id: stmt_id,
        kind: HirStatementKind::Expr(expression),
        location: location.clone(),
    };

    module.side_table.map_statement(&location, &statement);
    module.side_table.map_value(&location, expr_id, &location);
    entry_block.statements.push(statement);

    let error = validate_module_for_tests(&module, &string_table, &type_env)
        .expect_err("validator should reject out-of-range Result variant index");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(
        error.msg.contains("out of range"),
        "expected 'out of range' in error, got: {}",
        error.msg
    );
}

#[test]
fn hir_variant_construct_choice_wrong_field_name_rejected() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let response_param = super::symbol("response", &mut string_table);
    let ok_name = string_table.intern("Ok");
    let err_name = string_table.intern("Err");
    let wrong_name = string_table.intern("content");

    let choice_variants = vec![
        ChoiceVariant {
            id: ok_name,
            payload: ChoiceVariantPayload::Record {
                fields: vec![Declaration {
                    id: InternedPath::from_single_str("message", &mut string_table),
                    value: no_value_expr(
                        builtin_type_ids::STRING,
                        test_location(2),
                        ValueMode::ImmutableOwned,
                    ),
                }],
            },
            location: test_location(2),
        },
        ChoiceVariant {
            id: err_name,
            payload: ChoiceVariantPayload::Unit,
            location: test_location(2),
        },
    ];

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![param(
                response_param,
                builtin_type_ids::NONE,
                false,
                test_location(2),
            )],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(3))],
        test_location(1),
    );

    let ast = build_ast_with_choices(
        vec![start_fn],
        entry_path,
        vec![HirTestChoiceDefinition {
            nominal_path: InternedPath::from_single_str("Response", &mut string_table),
            variants: choice_variants,
        }],
    );
    let (mut module, type_environment) =
        lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let entry_block =
        &mut module.blocks[module.functions[module.start_function.0 as usize].entry.0 as usize];
    let region = entry_block.region;

    let string_ty = builtin_type_ids::STRING;

    let expr_id = HirValueId(9000);
    let stmt_id = HirNodeId(9000);
    let location = test_location(10);

    let expression = HirExpression {
        id: expr_id,
        kind: HirExpressionKind::VariantConstruct {
            carrier: HirVariantCarrier::Choice {
                choice_id: ChoiceId(0),
            },
            variant_index: 0,
            fields: vec![HirVariantField {
                name: Some(wrong_name),
                value: HirExpression {
                    id: HirValueId(9001),
                    kind: HirExpressionKind::StringLiteral("hello".to_owned()),
                    ty: string_ty,
                    value_kind: ValueKind::Const,
                    region,
                },
            }],
        },
        ty: string_ty,
        value_kind: ValueKind::Const,
        region,
    };

    let statement = HirStatement {
        id: stmt_id,
        kind: HirStatementKind::Expr(expression),
        location: location.clone(),
    };

    module.side_table.map_statement(&location, &statement);
    module.side_table.map_value(&location, expr_id, &location);
    entry_block.statements.push(statement);

    let error = validate_module_for_tests(&module, &string_table, &type_environment)
        .expect_err("validator should reject wrong field name in choice VariantConstruct");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(
        error.msg.contains("field name"),
        "expected 'field name' in error, got: {}",
        error.msg
    );
}

#[test]
fn hir_variant_construct_choice_wrong_field_type_rejected() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let response_param = super::symbol("response", &mut string_table);
    let ok_name = string_table.intern("Ok");
    let err_name = string_table.intern("Err");
    let message_name = string_table.intern("message");

    let choice_variants = vec![
        ChoiceVariant {
            id: ok_name,
            payload: ChoiceVariantPayload::Record {
                fields: vec![Declaration {
                    id: InternedPath::from_single_str("message", &mut string_table),
                    value: no_value_expr(
                        builtin_type_ids::STRING,
                        test_location(2),
                        ValueMode::ImmutableOwned,
                    ),
                }],
            },
            location: test_location(2),
        },
        ChoiceVariant {
            id: err_name,
            payload: ChoiceVariantPayload::Unit,
            location: test_location(2),
        },
    ];

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![param(
                response_param,
                builtin_type_ids::NONE,
                false,
                test_location(2),
            )],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(3))],
        test_location(1),
    );

    let ast = build_ast_with_choices(
        vec![start_fn],
        entry_path,
        vec![HirTestChoiceDefinition {
            nominal_path: InternedPath::from_single_str("Response", &mut string_table),
            variants: choice_variants,
        }],
    );
    let (mut module, type_environment) =
        lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let entry_block =
        &mut module.blocks[module.functions[module.start_function.0 as usize].entry.0 as usize];
    let region = entry_block.region;

    let string_ty = builtin_type_ids::STRING;
    let bool_ty = builtin_type_ids::BOOL;

    let expr_id = HirValueId(9000);
    let stmt_id = HirNodeId(9000);
    let location = test_location(10);

    let expression = HirExpression {
        id: expr_id,
        kind: HirExpressionKind::VariantConstruct {
            carrier: HirVariantCarrier::Choice {
                choice_id: ChoiceId(0),
            },
            variant_index: 0,
            fields: vec![HirVariantField {
                name: Some(message_name),
                value: HirExpression {
                    id: HirValueId(9001),
                    kind: HirExpressionKind::Bool(true),
                    ty: bool_ty,
                    value_kind: ValueKind::Const,
                    region,
                },
            }],
        },
        ty: string_ty,
        value_kind: ValueKind::Const,
        region,
    };

    let statement = HirStatement {
        id: stmt_id,
        kind: HirStatementKind::Expr(expression),
        location: location.clone(),
    };

    module.side_table.map_statement(&location, &statement);
    module.side_table.map_value(&location, expr_id, &location);
    entry_block.statements.push(statement);

    let error = validate_module_for_tests(&module, &string_table, &type_environment)
        .expect_err("validator should reject wrong field type in choice VariantConstruct");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(
        error.msg.contains("field type mismatch"),
        "expected 'field type mismatch' in error, got: {}",
        error.msg
    );
}

#[test]
fn validator_rejects_collection_expression_with_non_collection_type() {
    let (string_table, mut module, type_environment) = minimal_lowered_hir_module();
    let int_type = type_environment.builtins().int;
    inject_collection_expression_statement(&mut module, int_type, test_location(20));

    let error = validate_module_for_tests(&module, &string_table, &type_environment)
        .expect_err("validator should reject Collection expression with non-collection type");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(
        error.msg.contains("not a collection type"),
        "expected 'not a collection type' in error, got: {}",
        error.msg
    );
}

#[test]
fn validator_accepts_collection_expression_with_growable_collection_type() {
    let (string_table, mut module, mut type_environment) = minimal_lowered_hir_module();
    let int_type = type_environment.builtins().int;
    let growable_collection = type_environment.intern_collection(int_type, None);
    inject_collection_expression_statement(&mut module, growable_collection, test_location(20));

    validate_module_for_tests(&module, &string_table, &type_environment)
        .expect("validator should accept Collection expression with growable collection type");
}

#[test]
fn validator_accepts_collection_expression_with_fixed_collection_type() {
    let (string_table, mut module, mut type_environment) = minimal_lowered_hir_module();
    let int_type = type_environment.builtins().int;
    let fixed_collection = type_environment.intern_collection(int_type, Some(64));
    inject_collection_expression_statement(&mut module, fixed_collection, test_location(20));

    validate_module_for_tests(&module, &string_table, &type_environment)
        .expect("validator should accept Collection expression with fixed collection type");
}
