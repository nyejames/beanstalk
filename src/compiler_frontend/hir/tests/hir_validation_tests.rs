#![cfg(test)]

use crate::compiler_frontend::ast::ast::{Ast, ModuleExport};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind, TextLocation, Var};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::{CompilerMessages, ErrorType};
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::hir::hir_builder::{HirBuilder, validate_module_for_tests};
use crate::compiler_frontend::hir::hir_nodes::{
    HirExpression, HirExpressionKind, HirMatchArm, HirPattern, HirPlace, HirTerminator, HirValueId,
    ValueKind,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;

fn test_location(line: i32) -> TextLocation {
    TextLocation::new_just_line(line)
}

fn node(kind: NodeKind, location: TextLocation) -> AstNode {
    AstNode {
        kind,
        location,
        scope: InternedPath::new(),
    }
}

fn var(name: InternedPath, value: Expression) -> Var {
    Var { id: name, value }
}

fn param(name: InternedPath, data_type: DataType, mutable: bool, location: TextLocation) -> Var {
    let ownership = if mutable {
        Ownership::MutableOwned
    } else {
        Ownership::ImmutableOwned
    };

    Var {
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
        entry_path,
        external_exports: Vec::<ModuleExport>::new(),
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
    HirBuilder::new(string_table).build_hir_module(ast)
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
    let entry_block = module.functions[module.start_function.0 as usize].entry;
    module.blocks[entry_block.0 as usize].terminator = HirTerminator::Panic { message: None };

    let error = validate_module_for_tests(&module, &string_table)
        .expect_err("validator should reject placeholder terminator");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("placeholder terminator"));
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
