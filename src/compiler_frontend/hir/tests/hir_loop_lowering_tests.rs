//! HIR statement lowering regression tests.
//!
//! WHAT: checks how statement-level AST nodes become HIR blocks, statements, and terminators.
//! WHY: statement lowering owns most CFG construction and benefits from targeted regression coverage.

use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::datatypes::Ownership;
use crate::compiler_frontend::hir::hir_nodes::HirTerminator;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tests::test_support::{function_node, node, test_location};

use crate::compiler_frontend::hir::hir_builder::{build_ast, lower_ast};

#[test]
fn lowers_while_to_header_body_exit_shape() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);

    let while_node = node(
        NodeKind::WhileLoop(
            Expression::bool(false, test_location(2), Ownership::ImmutableOwned),
            vec![node(
                NodeKind::Rvalue(Expression::int(
                    10,
                    test_location(2),
                    Ownership::ImmutableOwned,
                )),
                test_location(2),
            )],
        ),
        test_location(2),
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![while_node],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let module = lower_ast(ast, &mut string_table).expect("HIR lowering should succeed");

    let start = &module.functions[module.start_function.0 as usize];
    let entry_block = &module.blocks[start.entry.0 as usize];

    let header_block = match entry_block.terminator {
        HirTerminator::Jump { target, .. } => target,
        _ => panic!("expected jump to while header"),
    };

    let (body_block, _exit_block) = match module.blocks[header_block.0 as usize].terminator {
        HirTerminator::If {
            then_block,
            else_block,
            ..
        } => (then_block, else_block),
        _ => panic!("expected if in while header"),
    };

    assert!(matches!(
        module.blocks[body_block.0 as usize].terminator,
        HirTerminator::Jump { target, .. } if target == header_block
    ));
}

#[test]
fn break_in_while_targets_loop_exit_block() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);

    let while_node = node(
        NodeKind::WhileLoop(
            Expression::bool(true, test_location(20), Ownership::ImmutableOwned),
            vec![node(NodeKind::Break, test_location(21))],
        ),
        test_location(20),
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![while_node],
        test_location(19),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let module = lower_ast(ast, &mut string_table).expect("HIR lowering should succeed");

    let start = &module.functions[module.start_function.0 as usize];
    let entry_block = &module.blocks[start.entry.0 as usize];
    let header_block = match entry_block.terminator {
        HirTerminator::Jump { target, .. } => target,
        _ => panic!("expected jump to while header"),
    };

    let (body_block, exit_block) = match module.blocks[header_block.0 as usize].terminator {
        HirTerminator::If {
            then_block,
            else_block,
            ..
        } => (then_block, else_block),
        _ => panic!("expected while header conditional terminator"),
    };

    assert!(matches!(
        module.blocks[body_block.0 as usize].terminator,
        HirTerminator::Break { target } if target == exit_block
    ));
}
