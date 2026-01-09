//! Unit tests for Control Flow Linearizer
//!
//! These tests validate the ControlFlowLinearizer component that converts nested
//! control flow constructs into explicit HIR blocks with terminators.

#[cfg(test)]
mod control_flow_linearizer_unit_tests {
    use crate::compiler::datatypes::{DataType, Ownership};
    use crate::compiler::hir::build_hir::{HirBuilderContext, ScopeType};
    use crate::compiler::hir::control_flow_linearizer::ControlFlowLinearizer;
    use crate::compiler::hir::nodes::{
        HirExpr, HirExprKind, HirKind, HirNode, HirPattern, HirStmt, HirTerminator,
    };
    use crate::compiler::parsers::ast_nodes::AstNode;
    use crate::compiler::parsers::expressions::expression::Expression;
    use crate::compiler::parsers::tokenizer::tokens::TextLocation;
    use crate::compiler::string_interning::{InternedString, StringTable};

    /// Helper to create a test boolean expression
    fn create_bool_expr(value: bool) -> Expression {
        Expression::bool(value, TextLocation::default(), Ownership::default())
    }

    /// Helper to create a test integer expression
    fn create_int_expr(value: i64) -> Expression {
        Expression::int(value, TextLocation::default(), Ownership::default())
    }

    #[test]
    fn test_new_control_flow_linearizer() {
        let linearizer = ControlFlowLinearizer::new();
        // Just verify it creates successfully
        assert!(
            linearizer
                .expr_linearizer()
                .is_compiler_local(&InternedString::from_u32(0))
                == false
        );
    }

    #[test]
    fn test_linearize_simple_if() {
        let mut string_table = StringTable::new();
        let mut ctx = HirBuilderContext::new(&mut string_table);
        let mut linearizer = ControlFlowLinearizer::new();

        // Create entry block
        let _entry_block = ctx.create_block();

        let condition = create_bool_expr(true);
        let then_body: Vec<AstNode> = vec![];

        let result = linearizer.linearize_if_statement(
            &condition,
            &then_body,
            None,
            &TextLocation::default(),
            &mut ctx,
        );

        assert!(result.is_ok());
        let nodes = result.unwrap();

        // Should have at least the If terminator
        assert!(!nodes.is_empty());

        // Last node should be an If terminator
        let last_node = nodes.last().unwrap();
        assert!(matches!(
            last_node.kind,
            HirKind::Terminator(HirTerminator::If { .. })
        ));
    }

    #[test]
    fn test_linearize_if_else() {
        let mut string_table = StringTable::new();
        let mut ctx = HirBuilderContext::new(&mut string_table);
        let mut linearizer = ControlFlowLinearizer::new();

        let _entry_block = ctx.create_block();

        let condition = create_bool_expr(true);
        let then_body: Vec<AstNode> = vec![];
        let else_body: Vec<AstNode> = vec![];

        let result = linearizer.linearize_if_statement(
            &condition,
            &then_body,
            Some(&else_body),
            &TextLocation::default(),
            &mut ctx,
        );

        assert!(result.is_ok());
        let nodes = result.unwrap();

        // Should have the If terminator
        let last_node = nodes.last().unwrap();
        if let HirKind::Terminator(HirTerminator::If { else_block, .. }) = &last_node.kind {
            // Should have an else block
            assert!(else_block.is_some());
        } else {
            panic!("Expected If terminator");
        }
    }

    #[test]
    fn test_linearize_return_empty() {
        let mut string_table = StringTable::new();
        let mut ctx = HirBuilderContext::new(&mut string_table);
        let mut linearizer = ControlFlowLinearizer::new();

        let values: Vec<Expression> = vec![];

        let result = linearizer.linearize_return(&values, &TextLocation::default(), &mut ctx);

        assert!(result.is_ok());
        let nodes = result.unwrap();

        assert_eq!(nodes.len(), 1);
        assert!(matches!(
            nodes[0].kind,
            HirKind::Terminator(HirTerminator::Return(_))
        ));
    }

    #[test]
    fn test_linearize_return_with_value() {
        let mut string_table = StringTable::new();
        let mut ctx = HirBuilderContext::new(&mut string_table);
        let mut linearizer = ControlFlowLinearizer::new();

        let values = vec![create_int_expr(42)];

        let result = linearizer.linearize_return(&values, &TextLocation::default(), &mut ctx);

        assert!(result.is_ok());
        let nodes = result.unwrap();

        // Should have the Return terminator
        let last_node = nodes.last().unwrap();
        if let HirKind::Terminator(HirTerminator::Return(return_values)) = &last_node.kind {
            assert_eq!(return_values.len(), 1);
        } else {
            panic!("Expected Return terminator");
        }
    }

    #[test]
    fn test_break_outside_loop_fails() {
        let mut string_table = StringTable::new();
        let mut ctx = HirBuilderContext::new(&mut string_table);
        let mut linearizer = ControlFlowLinearizer::new();

        // No loop scope entered
        let result = linearizer.linearize_break(&TextLocation::default(), &mut ctx);

        assert!(result.is_err());
    }

    #[test]
    fn test_continue_outside_loop_fails() {
        let mut string_table = StringTable::new();
        let mut ctx = HirBuilderContext::new(&mut string_table);
        let mut linearizer = ControlFlowLinearizer::new();

        // No loop scope entered
        let result = linearizer.linearize_continue(&TextLocation::default(), &mut ctx);

        assert!(result.is_err());
    }

    #[test]
    fn test_break_inside_loop() {
        let mut string_table = StringTable::new();
        let mut ctx = HirBuilderContext::new(&mut string_table);
        let mut linearizer = ControlFlowLinearizer::new();

        // Enter a loop scope
        let break_target = ctx.create_block();
        let continue_target = ctx.create_block();
        ctx.enter_scope(ScopeType::Loop {
            break_target,
            continue_target,
        });

        let result = linearizer.linearize_break(&TextLocation::default(), &mut ctx);

        assert!(result.is_ok());
        let node = result.unwrap();

        if let HirKind::Terminator(HirTerminator::Break { target }) = &node.kind {
            assert_eq!(*target, break_target);
        } else {
            panic!("Expected Break terminator");
        }
    }

    #[test]
    fn test_continue_inside_loop() {
        let mut string_table = StringTable::new();
        let mut ctx = HirBuilderContext::new(&mut string_table);
        let mut linearizer = ControlFlowLinearizer::new();

        // Enter a loop scope
        let break_target = ctx.create_block();
        let continue_target = ctx.create_block();
        ctx.enter_scope(ScopeType::Loop {
            break_target,
            continue_target,
        });

        let result = linearizer.linearize_continue(&TextLocation::default(), &mut ctx);

        assert!(result.is_ok());
        let node = result.unwrap();

        if let HirKind::Terminator(HirTerminator::Continue { target }) = &node.kind {
            assert_eq!(*target, continue_target);
        } else {
            panic!("Expected Continue terminator");
        }
    }

    #[test]
    fn test_ensure_block_termination_missing() {
        let mut string_table = StringTable::new();
        let mut ctx = HirBuilderContext::new(&mut string_table);
        let linearizer = ControlFlowLinearizer::new();

        // Create a block with a statement but no terminator
        let block_id = ctx.create_block();

        // Add a non-terminator node
        let stmt_node = HirNode {
            kind: HirKind::Stmt(HirStmt::ExprStmt(HirExpr {
                kind: HirExprKind::Int(42),
                data_type: DataType::Int,
                location: TextLocation::default(),
            })),
            location: TextLocation::default(),
            id: ctx.allocate_node_id(),
        };
        ctx.add_node_to_block(block_id, stmt_node);

        // Block with statement but no terminator should fail
        let result = linearizer.ensure_block_termination(&ctx, block_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_block_has_terminator() {
        let mut string_table = StringTable::new();
        let mut ctx = HirBuilderContext::new(&mut string_table);
        let linearizer = ControlFlowLinearizer::new();

        // Create a block
        let block_id = ctx.create_block();

        // Initially no terminator
        assert!(!linearizer.block_has_terminator(&ctx, block_id));

        // Add a return terminator
        let return_node = HirNode {
            kind: HirKind::Terminator(HirTerminator::Return(vec![])),
            location: TextLocation::default(),
            id: ctx.allocate_node_id(),
        };
        ctx.add_node_to_block(block_id, return_node);

        // Now should have terminator
        assert!(linearizer.block_has_terminator(&ctx, block_id));
    }

    #[test]
    fn test_convert_pattern_int() {
        let mut string_table = StringTable::new();
        let mut ctx = HirBuilderContext::new(&mut string_table);
        let mut linearizer = ControlFlowLinearizer::new();

        let pattern = create_int_expr(42);
        let result = linearizer.convert_pattern(&pattern, &mut ctx);

        assert!(result.is_ok());
        let hir_pattern = result.unwrap();

        if let HirPattern::Literal(expr) = hir_pattern {
            assert!(matches!(expr.kind, HirExprKind::Int(42)));
        } else {
            panic!("Expected Literal pattern");
        }
    }

    #[test]
    fn test_convert_pattern_bool() {
        let mut string_table = StringTable::new();
        let mut ctx = HirBuilderContext::new(&mut string_table);
        let mut linearizer = ControlFlowLinearizer::new();

        let pattern = create_bool_expr(true);
        let result = linearizer.convert_pattern(&pattern, &mut ctx);

        assert!(result.is_ok());
        let hir_pattern = result.unwrap();

        if let HirPattern::Literal(expr) = hir_pattern {
            assert!(matches!(expr.kind, HirExprKind::Bool(true)));
        } else {
            panic!("Expected Literal pattern");
        }
    }
}
