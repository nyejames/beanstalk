use crate::compiler_frontend::ast::ast::{Ast, AstDocFragment, AstDocFragmentKind, ModuleExport};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind, TextLocation};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::functions::{FunctionReturn, FunctionSignature};
use crate::compiler_frontend::compiler_errors::{CompilerMessages, ErrorType};
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::hir::hir_builder::{HirBuilder, validate_module_for_tests};
use crate::compiler_frontend::hir::hir_nodes::{
    HirExpression, HirExpressionKind, HirMatchArm, HirPattern, HirPlace, HirRegion, HirTerminator,
    HirValueId, RegionId, ValueKind,
};
use crate::compiler_frontend::hir::tests::hir_expression_lowering_tests::location;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::string_interning::StringTable;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;

fn test_location(line: i32) -> TextLocation {
    location(line)
}

fn node(kind: NodeKind, location: TextLocation) -> AstNode {
    AstNode {
        kind,
        location,
        scope: InternedPath::new(),
    }
}

fn var(name: InternedPath, value: Expression) -> Declaration {
    Declaration { id: name, value }
}

fn param(
    name: InternedPath,
    data_type: DataType,
    mutable: bool,
    location: TextLocation,
) -> Declaration {
    let ownership = if mutable {
        Ownership::MutableOwned
    } else {
        Ownership::ImmutableOwned
    };

    Declaration {
        id: name,
        value: Expression::new(ExpressionKind::None, location, data_type, ownership),
    }
}

fn function_node(
    name: InternedPath,
    signature: FunctionSignature,
    body: Vec<AstNode>,
    location: TextLocation,
) -> AstNode {
    node(NodeKind::Function(name, signature, body), location)
}

fn symbol(name: &str, string_table: &mut StringTable) -> InternedPath {
    InternedPath::from_single_str(name, string_table)
}

fn build_ast(nodes: Vec<AstNode>, entry_path: InternedPath) -> Ast {
    Ast {
        nodes,
        module_constants: vec![],
        doc_fragments: vec![],
        entry_path,
        external_exports: Vec::<ModuleExport>::new(),
        start_template_items: vec![],
        warnings: vec![],
    }
}

fn entry_path_and_start_name(string_table: &mut StringTable) -> (InternedPath, InternedPath) {
    let entry_path = InternedPath::from_single_str("main.bst", string_table);
    let start_name = entry_path.join_str(IMPLICIT_START_FUNC_NAME, string_table);

    (entry_path, start_name)
}

fn lower_ast(
    ast: Ast,
    string_table: &mut StringTable,
) -> Result<crate::compiler_frontend::hir::hir_nodes::HirModule, CompilerMessages> {
    HirBuilder::new(string_table, PathStringFormatConfig::default()).build_hir_module(ast)
}

#[test]
fn valid_module_passes_explicit_validation() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);

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
    let module = lower_ast(ast, &mut string_table).expect("lowering should succeed");
    validate_module_for_tests(&module, &string_table)
        .expect("validator should accept a valid lowered module");
}

#[test]
fn validator_rejects_invalid_jump_target() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);

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
    let mut module = lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let entry_block = module.functions[module.start_function.0 as usize].entry;
    module.blocks[entry_block.0 as usize].terminator = HirTerminator::Jump {
        target: crate::compiler_frontend::hir::hir_nodes::BlockId(999),
        args: vec![],
    };

    let error = validate_module_for_tests(&module, &string_table)
        .expect_err("validator should reject invalid jump target");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("Unknown HIR block id"));
}

#[test]
fn validator_rejects_non_literal_match_pattern() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let x = symbol("x", &mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![param(x.clone(), DataType::Int, false, test_location(2))],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(3))],
        test_location(2),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let mut module = lower_ast(ast, &mut string_table).expect("lowering should succeed");
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

    let error = validate_module_for_tests(&module, &string_table)
        .expect_err("validator should reject non-literal match pattern");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("Match literal pattern"));
}

#[test]
fn validator_rejects_missing_side_table_mappings() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let x = symbol("x", &mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(var(
                    x,
                    Expression::int(1, test_location(4), Ownership::ImmutableOwned),
                )),
                test_location(4),
            ),
            node(NodeKind::Return(vec![]), test_location(5)),
        ],
        test_location(3),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let mut module = lower_ast(ast, &mut string_table).expect("lowering should succeed");
    module.side_table.clear();

    let error = validate_module_for_tests(&module, &string_table)
        .expect_err("validator should reject missing side-table mappings");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("side-table mapping"));
}

#[test]
fn validator_rejects_invalid_doc_fragment_location() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);

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
    assert!(
        error
            .errors
            .iter()
            .any(|diagnostic| diagnostic.msg.contains("Doc fragment"))
    );
}

#[test]
fn validator_rejects_placeholder_terminator() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);

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
    let mut module = lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let entry = module.functions[module.start_function.0 as usize].entry;
    module.blocks[entry.0 as usize].terminator = HirTerminator::Panic { message: None };

    let error = validate_module_for_tests(&module, &string_table)
        .expect_err("validator should reject placeholder terminators");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("placeholder terminator"));
}

#[test]
fn validator_rejects_region_cycle() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);

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
    let mut module = lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let region_id = module.regions[0].id();
    module.regions[0] = HirRegion::lexical(region_id, Some(region_id));

    let error = validate_module_for_tests(&module, &string_table)
        .expect_err("validator should reject cyclic region parents");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("cycle"));
}

#[test]
fn validator_rejects_missing_region_parent() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);

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
    let mut module = lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let region_id = module.regions[0].id();
    module.regions[0] = HirRegion::lexical(region_id, Some(RegionId(9999)));

    let error = validate_module_for_tests(&module, &string_table)
        .expect_err("validator should reject missing region parents");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("missing parent"));
}

#[test]
fn validator_rejects_out_of_range_return_alias_metadata() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let p = symbol("p", &mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![param(p.clone(), DataType::Int, false, test_location(1))],
            returns: vec![FunctionReturn::Value(DataType::Int)],
        },
        vec![node(
            NodeKind::Return(vec![Expression::reference(
                p,
                DataType::Int,
                test_location(2),
                Ownership::ImmutableReference,
            )]),
            test_location(2),
        )],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let mut module = lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let start_index = module.start_function.0 as usize;
    module.functions[start_index].return_aliases = vec![Some(vec![1])];

    let error = validate_module_for_tests(&module, &string_table)
        .expect_err("validator should reject out-of-range return alias indices");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("out-of-range parameter index"));
}

#[test]
fn validator_rejects_cross_function_cfg_edges() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let helper_name = symbol("helper", &mut string_table);

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
    let mut module = lower_ast(ast, &mut string_table).expect("lowering should succeed");
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

    let error = validate_module_for_tests(&module, &string_table)
        .expect_err("validator should reject cross-function CFG edges");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(
        error.msg.contains("multiple functions") || error.msg.contains("crosses function boundary")
    );
}
