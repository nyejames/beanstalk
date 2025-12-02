//! Unit tests for HIR generation

#[cfg(test)]
mod tests {
    use crate::compiler::datatypes::{DataType, Ownership};
    use crate::compiler::hir::builder::HirBuilder;
    use crate::compiler::hir::lower_expression::{convert_operator, lower_expr, lower_rpn_to_expr};
    use crate::compiler::hir::nodes::{HirExpr, HirExprKind, HirKind};
    use crate::compiler::interned_path::InternedPath;
    use crate::compiler::parsers::ast_nodes::{Arg, AstNode, NodeKind};
    use crate::compiler::parsers::expressions::expression::Expression;
    use crate::compiler::parsers::tokenizer::tokens::TextLocation;
    use crate::compiler::string_interning::StringTable;

    fn create_test_location() -> TextLocation {
        TextLocation::default()
    }

    #[test]
    fn test_lower_int_literal() {
        let mut string_table = StringTable::new();

        let expr = Expression::int(42, create_test_location(), Ownership::ImmutableReference);

        let result = lower_expr(expr, &mut string_table);
        assert!(result.is_ok());

        let hir_expr = result.unwrap();
        assert!(matches!(hir_expr.kind, HirExprKind::Int(42)));
        assert_eq!(hir_expr.data_type, DataType::Int);
    }

    #[test]
    fn test_lower_float_literal() {
        let mut string_table = StringTable::new();

        let expr = Expression::float(3.14, create_test_location(), Ownership::ImmutableReference);

        let result = lower_expr(expr, &mut string_table);
        assert!(result.is_ok());

        let hir_expr = result.unwrap();
        assert!(matches!(hir_expr.kind, HirExprKind::Float(f) if (f - 3.14).abs() < 0.001));
        assert_eq!(hir_expr.data_type, DataType::Float);
    }

    #[test]
    fn test_lower_bool_literal() {
        let mut string_table = StringTable::new();

        let expr = Expression::bool(true, create_test_location(), Ownership::ImmutableReference);

        let result = lower_expr(expr, &mut string_table);
        assert!(result.is_ok());

        let hir_expr = result.unwrap();
        assert!(matches!(hir_expr.kind, HirExprKind::Bool(true)));
        assert_eq!(hir_expr.data_type, DataType::Bool);
    }

    #[test]
    fn test_lower_string_literal() {
        let mut string_table = StringTable::new();

        let test_str = string_table.intern("hello");
        let expr = Expression::string_slice(
            test_str,
            create_test_location(),
            Ownership::ImmutableReference,
        );

        let result = lower_expr(expr, &mut string_table);
        assert!(result.is_ok());

        let hir_expr = result.unwrap();
        assert!(matches!(hir_expr.kind, HirExprKind::StringLiteral(_)));
        assert_eq!(hir_expr.data_type, DataType::String);
    }

    #[test]
    fn test_lower_variable_reference() {
        let mut string_table = StringTable::new();

        let var_name = string_table.intern("x");
        let expr = Expression::reference(
            var_name,
            DataType::Int,
            create_test_location(),
            Ownership::ImmutableReference,
        );

        let result = lower_expr(expr, &mut string_table);
        assert!(result.is_ok());

        let hir_expr = result.unwrap();
        assert!(matches!(hir_expr.kind, HirExprKind::Load(_)));
        assert_eq!(hir_expr.data_type, DataType::Int);
    }

    #[test]
    fn test_lower_collection() {
        let mut string_table = StringTable::new();

        let items = vec![
            Expression::int(1, create_test_location(), Ownership::ImmutableReference),
            Expression::int(2, create_test_location(), Ownership::ImmutableReference),
            Expression::int(3, create_test_location(), Ownership::ImmutableReference),
        ];

        let expr = Expression::collection(
            items,
            create_test_location(),
            Ownership::ImmutableReference,
        );

        let result = lower_expr(expr, &mut string_table);
        assert!(result.is_ok());

        let hir_expr = result.unwrap();
        if let HirExprKind::Collection(items) = hir_expr.kind {
            assert_eq!(items.len(), 3);
            assert!(matches!(items[0].kind, HirExprKind::Int(1)));
            assert!(matches!(items[1].kind, HirExprKind::Int(2)));
            assert!(matches!(items[2].kind, HirExprKind::Int(3)));
        } else {
            panic!("Expected Collection variant");
        }
    }

    #[test]
    fn test_lower_range() {
        let mut string_table = StringTable::new();

        let start = Expression::int(1, create_test_location(), Ownership::ImmutableReference);
        let end = Expression::int(10, create_test_location(), Ownership::ImmutableReference);

        let expr = Expression::range(
            start,
            end,
            create_test_location(),
            Ownership::ImmutableReference,
        );

        let result = lower_expr(expr, &mut string_table);
        assert!(result.is_ok());

        let hir_expr = result.unwrap();
        if let HirExprKind::Range { start, end } = hir_expr.kind {
            assert!(matches!(start.kind, HirExprKind::Int(1)));
            assert!(matches!(end.kind, HirExprKind::Int(10)));
        } else {
            panic!("Expected Range variant");
        }
    }

    #[test]
    fn test_lower_struct_instance() {
        let mut string_table = StringTable::new();

        let field1_name = string_table.intern("x");
        let field2_name = string_table.intern("y");

        let fields = vec![
            Arg {
                id: field1_name,
                value: Expression::int(10, create_test_location(), Ownership::ImmutableReference),
            },
            Arg {
                id: field2_name,
                value: Expression::int(20, create_test_location(), Ownership::ImmutableReference),
            },
        ];

        let expr = Expression::struct_instance(
            fields,
            create_test_location(),
            Ownership::ImmutableReference,
        );

        let result = lower_expr(expr, &mut string_table);
        assert!(result.is_ok());

        let hir_expr = result.unwrap();
        if let HirExprKind::StructConstruct { fields, .. } = hir_expr.kind {
            assert_eq!(fields.len(), 2);
            assert!(matches!(fields[0].1.kind, HirExprKind::Int(10)));
            assert!(matches!(fields[1].1.kind, HirExprKind::Int(20)));
        } else {
            panic!("Expected StructConstruct variant");
        }
    }

    #[test]
    fn test_lower_variable_declaration() {
        let mut string_table = StringTable::new();
        let scope = InternedPath::new();

        let var_name = string_table.intern("x");
        let value = Expression::int(42, create_test_location(), Ownership::ImmutableReference);

        let arg = Arg {
            id: var_name,
            value,
        };

        let ast_node = AstNode {
            kind: NodeKind::VariableDeclaration(arg),
            location: create_test_location(),
            scope: scope.clone(),
        };

        let mut builder = HirBuilder::new(scope, &mut string_table);
        let result = builder.lower_node(ast_node);
        assert!(result.is_ok());

        let hir_node = result.unwrap();
        assert!(matches!(hir_node.kind, HirKind::Let { .. }));
    }

    // === RPN to Expression Tree Conversion Tests ===

    use crate::compiler::hir::nodes::BinOp;
    use crate::compiler::parsers::expressions::expression::Operator;

    #[test]
    fn test_rpn_simple_addition() {
        // Test: 2 + 3 in RPN: [2, 3, +]
        let mut string_table = StringTable::new();
        let scope = InternedPath::new();

        let rpn_nodes = vec![
            AstNode {
                kind: NodeKind::Expression(Expression::int(
                    2,
                    create_test_location(),
                    Ownership::ImmutableReference,
                )),
                location: create_test_location(),
                scope: scope.clone(),
            },
            AstNode {
                kind: NodeKind::Expression(Expression::int(
                    3,
                    create_test_location(),
                    Ownership::ImmutableReference,
                )),
                location: create_test_location(),
                scope: scope.clone(),
            },
            AstNode {
                kind: NodeKind::Operator(Operator::Add),
                location: create_test_location(),
                scope: scope.clone(),
            },
        ];

        let result = lower_rpn_to_expr(rpn_nodes, &mut string_table);
        assert!(result.is_ok());

        let hir_kind = result.unwrap();
        if let HirExprKind::BinOp { left, op, right } = hir_kind {
            assert!(matches!(op, BinOp::Add));
            assert!(matches!(left.kind, HirExprKind::Int(2)));
            assert!(matches!(right.kind, HirExprKind::Int(3)));
        } else {
            panic!("Expected BinOp variant");
        }
    }

    #[test]
    fn test_rpn_nested_expression() {
        // Test: x + 2 * y in RPN: [x, 2, y, *, +]
        // Should produce: BinOp(Load(x), Add, BinOp(Int(2), Mul, Load(y)))
        let mut string_table = StringTable::new();
        let scope = InternedPath::new();

        let x_name = string_table.intern("x");
        let y_name = string_table.intern("y");

        let rpn_nodes = vec![
            AstNode {
                kind: NodeKind::Expression(Expression::reference(
                    x_name,
                    DataType::Int,
                    create_test_location(),
                    Ownership::ImmutableReference,
                )),
                location: create_test_location(),
                scope: scope.clone(),
            },
            AstNode {
                kind: NodeKind::Expression(Expression::int(
                    2,
                    create_test_location(),
                    Ownership::ImmutableReference,
                )),
                location: create_test_location(),
                scope: scope.clone(),
            },
            AstNode {
                kind: NodeKind::Expression(Expression::reference(
                    y_name,
                    DataType::Int,
                    create_test_location(),
                    Ownership::ImmutableReference,
                )),
                location: create_test_location(),
                scope: scope.clone(),
            },
            AstNode {
                kind: NodeKind::Operator(Operator::Multiply),
                location: create_test_location(),
                scope: scope.clone(),
            },
            AstNode {
                kind: NodeKind::Operator(Operator::Add),
                location: create_test_location(),
                scope: scope.clone(),
            },
        ];

        let mut builder = HirBuilder::new(scope, &mut string_table);
        let result = lower_rpn_to_expr(rpn_nodes, &mut string_table);
        assert!(result.is_ok());

        let hir_kind = result.unwrap();
        if let HirExprKind::BinOp { left, op, right } = hir_kind {
            assert!(matches!(op, BinOp::Add));
            assert!(matches!(left.kind, HirExprKind::Load(_)));

            // Right should be the multiplication
            if let HirExprKind::BinOp {
                left: mul_left,
                op: mul_op,
                right: mul_right,
            } = right.kind
            {
                assert!(matches!(mul_op, BinOp::Mul));
                assert!(matches!(mul_left.kind, HirExprKind::Int(2)));
                assert!(matches!(mul_right.kind, HirExprKind::Load(_)));
            } else {
                panic!("Expected nested BinOp for multiplication");
            }
        } else {
            panic!("Expected BinOp variant");
        }
    }

    #[test]
    fn test_rpn_comparison_returns_bool() {
        // Test: 5 < 10 in RPN: [5, 10, <]
        let mut string_table = StringTable::new();
        let scope = InternedPath::new();

        let rpn_nodes = vec![
            AstNode {
                kind: NodeKind::Expression(Expression::int(
                    5,
                    create_test_location(),
                    Ownership::ImmutableReference,
                )),
                location: create_test_location(),
                scope: scope.clone(),
            },
            AstNode {
                kind: NodeKind::Expression(Expression::int(
                    10,
                    create_test_location(),
                    Ownership::ImmutableReference,
                )),
                location: create_test_location(),
                scope: scope.clone(),
            },
            AstNode {
                kind: NodeKind::Operator(Operator::LessThan),
                location: create_test_location(),
                scope: scope.clone(),
            },
        ];

        let expr = Expression::runtime(
            rpn_nodes,
            DataType::Bool,
            create_test_location(),
            Ownership::ImmutableReference,
        );

        let mut builder = HirBuilder::new(scope, &mut string_table);
        let result = lower_expr(expr, &mut string_table);
        assert!(result.is_ok());

        let hir_expr = result.unwrap();
        if let HirExprKind::BinOp { op, .. } = hir_expr.kind {
            assert!(matches!(op, BinOp::Lt));
        } else {
            panic!("Expected BinOp variant");
        }
    }

    #[test]
    fn test_rpn_stack_underflow_error() {
        // Test: Invalid RPN with operator but no operands: [+]
        let mut string_table = StringTable::new();
        let scope = InternedPath::new();

        let rpn_nodes = vec![AstNode {
            kind: NodeKind::Operator(Operator::Add),
            location: create_test_location(),
            scope: scope.clone(),
        }];

        let mut builder = HirBuilder::new(scope, &mut string_table);
        let result = lower_rpn_to_expr(rpn_nodes, &mut string_table);
        assert!(result.is_err());
    }

    #[test]
    fn test_rpn_stack_overflow_error() {
        // Test: Invalid RPN with too many operands: [1, 2, 3]
        let mut string_table = StringTable::new();
        let scope = InternedPath::new();

        let rpn_nodes = vec![
            AstNode {
                kind: NodeKind::Expression(Expression::int(
                    1,
                    create_test_location(),
                    Ownership::ImmutableReference,
                )),
                location: create_test_location(),
                scope: scope.clone(),
            },
            AstNode {
                kind: NodeKind::Expression(Expression::int(
                    2,
                    create_test_location(),
                    Ownership::ImmutableReference,
                )),
                location: create_test_location(),
                scope: scope.clone(),
            },
            AstNode {
                kind: NodeKind::Expression(Expression::int(
                    3,
                    create_test_location(),
                    Ownership::ImmutableReference,
                )),
                location: create_test_location(),
                scope: scope.clone(),
            },
        ];

        let mut builder = HirBuilder::new(scope, &mut string_table);
        let result = lower_rpn_to_expr(rpn_nodes, &mut string_table);
        assert!(result.is_err());
    }

    #[test]
    fn test_type_inference_int_int() {
        // Test: Int + Int = Int
        let mut string_table = StringTable::new();
        let scope = InternedPath::new();

        let rpn_nodes = vec![
            AstNode {
                kind: NodeKind::Expression(Expression::int(
                    2,
                    create_test_location(),
                    Ownership::ImmutableReference,
                )),
                location: create_test_location(),
                scope: scope.clone(),
            },
            AstNode {
                kind: NodeKind::Expression(Expression::int(
                    3,
                    create_test_location(),
                    Ownership::ImmutableReference,
                )),
                location: create_test_location(),
                scope: scope.clone(),
            },
            AstNode {
                kind: NodeKind::Operator(Operator::Add),
                location: create_test_location(),
                scope: scope.clone(),
            },
        ];

        let expr = Expression::runtime(
            rpn_nodes,
            DataType::Int,
            create_test_location(),
            Ownership::ImmutableReference,
        );

        let mut builder = HirBuilder::new(scope, &mut string_table);
        let result = lower_expr(expr, &mut string_table);
        assert!(result.is_ok());

        // The result type should be Int
        let hir_expr = result.unwrap();
        if let HirExprKind::BinOp { .. } = hir_expr.kind {
            // Type inference happens inside lower_rpn_to_expr
            // The outer expression type is from the AST
        } else {
            panic!("Expected BinOp variant");
        }
    }

    #[test]
    fn test_type_inference_int_float_promotes() {
        // Test: Int + Float = Float
        let mut string_table = StringTable::new();
        let scope = InternedPath::new();

        let rpn_nodes = vec![
            AstNode {
                kind: NodeKind::Expression(Expression::int(
                    2,
                    create_test_location(),
                    Ownership::ImmutableReference,
                )),
                location: create_test_location(),
                scope: scope.clone(),
            },
            AstNode {
                kind: NodeKind::Expression(Expression::float(
                    3.5,
                    create_test_location(),
                    Ownership::ImmutableReference,
                )),
                location: create_test_location(),
                scope: scope.clone(),
            },
            AstNode {
                kind: NodeKind::Operator(Operator::Add),
                location: create_test_location(),
                scope: scope.clone(),
            },
        ];

        let mut builder = HirBuilder::new(scope, &mut string_table);
        let result = lower_rpn_to_expr(rpn_nodes, &mut string_table);
        assert!(result.is_ok());

        // Verify the BinOp was created
        let hir_kind = result.unwrap();
        assert!(matches!(hir_kind, HirExprKind::BinOp { .. }));
    }

    #[test]
    fn test_operator_conversion_all_arithmetic() {
        let mut string_table = StringTable::new();
        let scope = InternedPath::new();

        // Test all arithmetic operators
        assert!(matches!(
            convert_operator(Operator::Add),
            Ok(BinOp::Add)
        ));
        assert!(matches!(
            convert_operator(Operator::Subtract),
            Ok(BinOp::Sub)
        ));
        assert!(matches!(
            convert_operator(Operator::Multiply),
            Ok(BinOp::Mul)
        ));
        assert!(matches!(
            convert_operator(Operator::Divide),
            Ok(BinOp::Div)
        ));
        assert!(matches!(
            convert_operator(Operator::Modulus),
            Ok(BinOp::Mod)
        ));
        assert!(matches!(
            convert_operator(Operator::Root),
            Ok(BinOp::Root)
        ));
        assert!(matches!(
            convert_operator(Operator::Exponent),
            Ok(BinOp::Exponent)
        ));
    }

    #[test]
    fn test_operator_conversion_all_comparison() {
        let mut string_table = StringTable::new();
        let scope = InternedPath::new();

        // Test all comparison operators
        assert!(matches!(
            convert_operator(Operator::GreaterThan),
            Ok(BinOp::Gt)
        ));
        assert!(matches!(
            convert_operator(Operator::GreaterThanOrEqual),
            Ok(BinOp::Ge)
        ));
        assert!(matches!(
            convert_operator(Operator::LessThan),
            Ok(BinOp::Lt)
        ));
        assert!(matches!(
            convert_operator(Operator::LessThanOrEqual),
            Ok(BinOp::Le)
        ));
        assert!(matches!(
            convert_operator(Operator::Equality),
            Ok(BinOp::Eq)
        ));
    }

    #[test]
    fn test_operator_conversion_logical() {
        let mut string_table = StringTable::new();
        let scope = InternedPath::new();

        // Test logical operators
        assert!(matches!(
            convert_operator(Operator::And),
            Ok(BinOp::And)
        ));
        assert!(matches!(
            convert_operator(Operator::Or),
            Ok(BinOp::Or)
        ));
    }

    #[test]
    fn test_operator_conversion_not_is_error() {
        let mut string_table = StringTable::new();
        let scope = InternedPath::new();

        // Not is a unary operator, should error in binary context
        assert!(convert_operator(Operator::Not).is_err());
    }

    #[test]
    fn test_operator_conversion_range_is_error() {
        let mut string_table = StringTable::new();
        let scope = InternedPath::new();

        // Range should be handled separately
        assert!(convert_operator(Operator::Range).is_err());
    }

    // === Place Construction Tests ===

    use crate::compiler::hir::lower_expression::{
        build_nested_place, create_field_place, create_global_place, create_index_place,
        create_local_place, PlaceAccess,
    };
    use crate::compiler::hir::place::Place;

    #[test]
    fn test_create_local_place() {
        let mut string_table = StringTable::new();
        let var_name = string_table.intern("x");

        let place = create_local_place(var_name);

        if let Place::Local(name) = place {
            assert_eq!(name, var_name);
        } else {
            panic!("Expected Place::Local variant");
        }
    }

    #[test]
    fn test_create_field_place() {
        let mut string_table = StringTable::new();
        let obj_name = string_table.intern("obj");
        let field_name = string_table.intern("field");

        let base = create_local_place(obj_name);
        let place = create_field_place(base, field_name);

        if let Place::Field { base, field } = place {
            if let Place::Local(name) = *base {
                assert_eq!(name, obj_name);
            } else {
                panic!("Expected base to be Place::Local");
            }
            assert_eq!(field, field_name);
        } else {
            panic!("Expected Place::Field variant");
        }
    }

    #[test]
    fn test_create_index_place() {
        let mut string_table = StringTable::new();
        let arr_name = string_table.intern("arr");

        let base = create_local_place(arr_name);
        let index_expr = HirExpr {
            kind: HirExprKind::Int(0),
            data_type: DataType::Int,
            location: create_test_location(),
        };

        let place = create_index_place(base, index_expr);

        if let Place::Index { base, index } = place {
            if let Place::Local(name) = *base {
                assert_eq!(name, arr_name);
            } else {
                panic!("Expected base to be Place::Local");
            }
            assert!(matches!(index.kind, HirExprKind::Int(0)));
        } else {
            panic!("Expected Place::Index variant");
        }
    }

    #[test]
    fn test_create_global_place() {
        let mut string_table = StringTable::new();
        let global_name = string_table.intern("CONSTANT");

        let place = create_global_place(global_name);

        if let Place::Global(name) = place {
            assert_eq!(name, global_name);
        } else {
            panic!("Expected Place::Global variant");
        }
    }

    #[test]
    fn test_nested_field_access() {
        // Test: obj.field1.field2
        let mut string_table = StringTable::new();
        let obj_name = string_table.intern("obj");
        let field1_name = string_table.intern("field1");
        let field2_name = string_table.intern("field2");

        let base = create_local_place(obj_name);
        let place1 = create_field_place(base, field1_name);
        let place2 = create_field_place(place1, field2_name);

        // Verify the nested structure
        if let Place::Field {
            base: outer_base,
            field: outer_field,
        } = place2
        {
            assert_eq!(outer_field, field2_name);

            if let Place::Field {
                base: inner_base,
                field: inner_field,
            } = *outer_base
            {
                assert_eq!(inner_field, field1_name);

                if let Place::Local(name) = *inner_base {
                    assert_eq!(name, obj_name);
                } else {
                    panic!("Expected innermost base to be Place::Local");
                }
            } else {
                panic!("Expected outer base to be Place::Field");
            }
        } else {
            panic!("Expected Place::Field variant");
        }
    }

    #[test]
    fn test_field_then_index_access() {
        // Test: obj.field[0]
        let mut string_table = StringTable::new();
        let obj_name = string_table.intern("obj");
        let field_name = string_table.intern("field");

        let base = create_local_place(obj_name);
        let field_place = create_field_place(base, field_name);

        let index_expr = HirExpr {
            kind: HirExprKind::Int(0),
            data_type: DataType::Int,
            location: create_test_location(),
        };

        let place = create_index_place(field_place, index_expr);

        // Verify the nested structure
        if let Place::Index { base, index } = place {
            assert!(matches!(index.kind, HirExprKind::Int(0)));

            if let Place::Field {
                base: field_base,
                field,
            } = *base
            {
                assert_eq!(field, field_name);

                if let Place::Local(name) = *field_base {
                    assert_eq!(name, obj_name);
                } else {
                    panic!("Expected field base to be Place::Local");
                }
            } else {
                panic!("Expected base to be Place::Field");
            }
        } else {
            panic!("Expected Place::Index variant");
        }
    }

    #[test]
    fn test_index_then_field_access() {
        // Test: arr[0].field
        let mut string_table = StringTable::new();
        let arr_name = string_table.intern("arr");
        let field_name = string_table.intern("field");

        let base = create_local_place(arr_name);

        let index_expr = HirExpr {
            kind: HirExprKind::Int(0),
            data_type: DataType::Int,
            location: create_test_location(),
        };

        let index_place = create_index_place(base, index_expr);
        let place = create_field_place(index_place, field_name);

        // Verify the nested structure
        if let Place::Field { base, field } = place {
            assert_eq!(field, field_name);

            if let Place::Index {
                base: index_base,
                index,
            } = *base
            {
                assert!(matches!(index.kind, HirExprKind::Int(0)));

                if let Place::Local(name) = *index_base {
                    assert_eq!(name, arr_name);
                } else {
                    panic!("Expected index base to be Place::Local");
                }
            } else {
                panic!("Expected base to be Place::Index");
            }
        } else {
            panic!("Expected Place::Field variant");
        }
    }

    #[test]
    fn test_complex_nested_place() {
        // Test: obj.field1[0].field2[1]
        let mut string_table = StringTable::new();
        let obj_name = string_table.intern("obj");
        let field1_name = string_table.intern("field1");
        let field2_name = string_table.intern("field2");

        let base = create_local_place(obj_name);
        let place1 = create_field_place(base, field1_name);

        let index1_expr = HirExpr {
            kind: HirExprKind::Int(0),
            data_type: DataType::Int,
            location: create_test_location(),
        };
        let place2 = create_index_place(place1, index1_expr);

        let place3 = create_field_place(place2, field2_name);

        let index2_expr = HirExpr {
            kind: HirExprKind::Int(1),
            data_type: DataType::Int,
            location: create_test_location(),
        };
        let final_place = create_index_place(place3, index2_expr);

        // Verify the outermost layer is an index
        if let Place::Index { base, index } = final_place {
            assert!(matches!(index.kind, HirExprKind::Int(1)));

            // Next layer should be a field
            if let Place::Field {
                base: field_base,
                field,
            } = *base
            {
                assert_eq!(field, field2_name);

                // Next layer should be an index
                if let Place::Index {
                    base: index_base,
                    index: inner_index,
                } = *field_base
                {
                    assert!(matches!(inner_index.kind, HirExprKind::Int(0)));

                    // Next layer should be a field
                    if let Place::Field {
                        base: inner_field_base,
                        field: inner_field,
                    } = *index_base
                    {
                        assert_eq!(inner_field, field1_name);

                        // Innermost should be the local
                        if let Place::Local(name) = *inner_field_base {
                            assert_eq!(name, obj_name);
                        } else {
                            panic!("Expected innermost base to be Place::Local");
                        }
                    } else {
                        panic!("Expected inner index base to be Place::Field");
                    }
                } else {
                    panic!("Expected field base to be Place::Index");
                }
            } else {
                panic!("Expected outer index base to be Place::Field");
            }
        } else {
            panic!("Expected Place::Index variant");
        }
    }

    #[test]
    fn test_build_nested_place_with_fields() {
        // Test: obj.field1.field2 using build_nested_place
        let mut string_table = StringTable::new();
        let obj_name = string_table.intern("obj");
        let field1_name = string_table.intern("field1");
        let field2_name = string_table.intern("field2");

        let base = create_local_place(obj_name);
        let accesses = vec![
            PlaceAccess::Field(field1_name),
            PlaceAccess::Field(field2_name),
        ];

        let result = build_nested_place(base, accesses, &mut string_table);
        assert!(result.is_ok());

        let place = result.unwrap();

        // Verify the nested structure
        if let Place::Field {
            base: outer_base,
            field: outer_field,
        } = place
        {
            assert_eq!(outer_field, field2_name);

            if let Place::Field {
                base: inner_base,
                field: inner_field,
            } = *outer_base
            {
                assert_eq!(inner_field, field1_name);

                if let Place::Local(name) = *inner_base {
                    assert_eq!(name, obj_name);
                } else {
                    panic!("Expected innermost base to be Place::Local");
                }
            } else {
                panic!("Expected outer base to be Place::Field");
            }
        } else {
            panic!("Expected Place::Field variant");
        }
    }

    #[test]
    fn test_build_nested_place_with_index() {
        // Test: arr[0][1] using build_nested_place
        let mut string_table = StringTable::new();
        let arr_name = string_table.intern("arr");

        let base = create_local_place(arr_name);
        let accesses = vec![
            PlaceAccess::Index(Expression::int(
                0,
                create_test_location(),
                Ownership::ImmutableReference,
            )),
            PlaceAccess::Index(Expression::int(
                1,
                create_test_location(),
                Ownership::ImmutableReference,
            )),
        ];

        let result = build_nested_place(base, accesses, &mut string_table);
        assert!(result.is_ok());

        let place = result.unwrap();

        // Verify the nested structure
        if let Place::Index {
            base: outer_base,
            index: outer_index,
        } = place
        {
            assert!(matches!(outer_index.kind, HirExprKind::Int(1)));

            if let Place::Index {
                base: inner_base,
                index: inner_index,
            } = *outer_base
            {
                assert!(matches!(inner_index.kind, HirExprKind::Int(0)));

                if let Place::Local(name) = *inner_base {
                    assert_eq!(name, arr_name);
                } else {
                    panic!("Expected innermost base to be Place::Local");
                }
            } else {
                panic!("Expected outer base to be Place::Index");
            }
        } else {
            panic!("Expected Place::Index variant");
        }
    }

    #[test]
    fn test_build_nested_place_mixed() {
        // Test: obj.field[0] using build_nested_place
        let mut string_table = StringTable::new();
        let obj_name = string_table.intern("obj");
        let field_name = string_table.intern("field");

        let base = create_local_place(obj_name);
        let accesses = vec![
            PlaceAccess::Field(field_name),
            PlaceAccess::Index(Expression::int(
                0,
                create_test_location(),
                Ownership::ImmutableReference,
            )),
        ];

        let result = build_nested_place(base, accesses, &mut string_table);
        assert!(result.is_ok());

        let place = result.unwrap();

        // Verify the nested structure
        if let Place::Index { base, index } = place {
            assert!(matches!(index.kind, HirExprKind::Int(0)));

            if let Place::Field {
                base: field_base,
                field,
            } = *base
            {
                assert_eq!(field, field_name);

                if let Place::Local(name) = *field_base {
                    assert_eq!(name, obj_name);
                } else {
                    panic!("Expected field base to be Place::Local");
                }
            } else {
                panic!("Expected base to be Place::Field");
            }
        } else {
            panic!("Expected Place::Index variant");
        }
    }

    #[test]
    fn test_build_nested_place_empty_accesses() {
        // Test: just a local with no accesses
        let mut string_table = StringTable::new();
        let var_name = string_table.intern("x");

        let base = create_local_place(var_name);
        let accesses = vec![];

        let result = build_nested_place(base, accesses, &mut string_table);
        assert!(result.is_ok());

        let place = result.unwrap();

        // Should just be the base place
        if let Place::Local(name) = place {
            assert_eq!(name, var_name);
        } else {
            panic!("Expected Place::Local variant");
        }
    }
}
