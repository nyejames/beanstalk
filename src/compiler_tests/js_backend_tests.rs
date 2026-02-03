//! Unit tests for JavaScript backend
//!
//! These tests verify the core functionality of the JavaScript code generation backend.

use crate::compiler::codegen::js::{JsEmitter, JsLoweringConfig};
use crate::compiler::hir::nodes::{HirBlock, HirModule};
use crate::compiler::string_interning::StringTable;

#[test]
fn test_js_emitter_initialization() {
    // Create a minimal HIR module for testing
    let hir_module = HirModule {
        blocks: vec![HirBlock {
            id: 0,
            params: vec![],
            nodes: vec![],
        }],
        entry_block: 0,
        functions: vec![],
        structs: vec![],
    };

    let string_table = StringTable::new();
    let config = JsLoweringConfig {
        pretty: true,
        emit_locations: false,
    };

    // Test that JsEmitter can be created successfully
    let emitter = JsEmitter::new(&hir_module, &string_table, config);

    // Verify initialization
    assert_eq!(emitter.indent, 0);
    assert_eq!(emitter.temp_counter, 0);
    assert_eq!(emitter.blocks.len(), 1);
    assert!(emitter.blocks.contains_key(&0));
    assert!(emitter.loop_labels.is_empty());
    assert!(emitter.used_names.is_empty());
    assert_eq!(emitter.out, "");
}

#[test]
fn test_js_emitter_block_lookup() {
    // Create a HIR module with multiple blocks
    let hir_module = HirModule {
        blocks: vec![
            HirBlock {
                id: 0,
                params: vec![],
                nodes: vec![],
            },
            HirBlock {
                id: 1,
                params: vec![],
                nodes: vec![],
            },
            HirBlock {
                id: 2,
                params: vec![],
                nodes: vec![],
            },
        ],
        entry_block: 0,
        functions: vec![],
        structs: vec![],
    };

    let string_table = StringTable::new();
    let config = JsLoweringConfig {
        pretty: true,
        emit_locations: false,
    };

    let emitter = JsEmitter::new(&hir_module, &string_table, config);

    // Verify all blocks are in the lookup table
    assert_eq!(emitter.blocks.len(), 3);
    assert!(emitter.blocks.contains_key(&0));
    assert!(emitter.blocks.contains_key(&1));
    assert!(emitter.blocks.contains_key(&2));

    // Verify block lookup returns correct blocks
    assert_eq!(emitter.blocks.get(&0).unwrap().id, 0);
    assert_eq!(emitter.blocks.get(&1).unwrap().id, 1);
    assert_eq!(emitter.blocks.get(&2).unwrap().id, 2);
}

#[test]
fn test_js_emitter_config() {
    let hir_module = HirModule {
        blocks: vec![HirBlock {
            id: 0,
            params: vec![],
            nodes: vec![],
        }],
        entry_block: 0,
        functions: vec![],
        structs: vec![],
    };

    let string_table = StringTable::new();

    // Test with pretty printing enabled
    let config_pretty = JsLoweringConfig {
        pretty: true,
        emit_locations: true,
    };
    let emitter_pretty = JsEmitter::new(&hir_module, &string_table, config_pretty);
    assert!(emitter_pretty.config.pretty);
    assert!(emitter_pretty.config.emit_locations);

    // Test with pretty printing disabled
    let config_compact = JsLoweringConfig {
        pretty: false,
        emit_locations: false,
    };
    let emitter_compact = JsEmitter::new(&hir_module, &string_table, config_compact);
    assert!(!emitter_compact.config.pretty);
    assert!(!emitter_compact.config.emit_locations);
}

#[cfg(test)]
mod js_expr_tests {
    use crate::compiler::codegen::js::{JsExpr, JsIdent, JsStmt};

    #[test]
    fn test_simple_expression() {
        let expr = JsExpr::simple("42".to_string());
        assert!(expr.prelude.is_empty());
        assert_eq!(expr.value, "42");
        assert!(expr.is_pure());
    }

    #[test]
    fn test_expression_with_prelude() {
        let prelude = vec![JsStmt::Let {
            name: JsIdent("_t0".to_string()),
            value: "compute()".to_string(),
        }];
        let expr = JsExpr::with_prelude(prelude, "_t0".to_string());

        assert_eq!(expr.prelude.len(), 1);
        assert_eq!(expr.value, "_t0");
        assert!(!expr.is_pure());
    }

    #[test]
    fn test_combine_simple_expressions() {
        let left = JsExpr::simple("a".to_string());
        let right = JsExpr::simple("b".to_string());

        let combined = left.combine(right, |l, r| format!("{} + {}", l, r));

        assert!(combined.prelude.is_empty());
        assert_eq!(combined.value, "a + b");
        assert!(combined.is_pure());
    }

    #[test]
    fn test_combine_expressions_with_preludes() {
        let left_prelude = vec![JsStmt::Let {
            name: JsIdent("_t0".to_string()),
            value: "foo()".to_string(),
        }];
        let left = JsExpr::with_prelude(left_prelude, "_t0".to_string());

        let right_prelude = vec![JsStmt::Let {
            name: JsIdent("_t1".to_string()),
            value: "bar()".to_string(),
        }];
        let right = JsExpr::with_prelude(right_prelude, "_t1".to_string());

        let combined = left.combine(right, |l, r| format!("{} * {}", l, r));

        // Both preludes should be merged
        assert_eq!(combined.prelude.len(), 2);
        assert_eq!(combined.value, "_t0 * _t1");
        assert!(!combined.is_pure());
    }

    #[test]
    fn test_prepend_prelude() {
        let mut expr = JsExpr::simple("x".to_string());

        let new_statements = vec![JsStmt::Expr("setup()".to_string())];
        expr.prepend_prelude(new_statements);

        assert_eq!(expr.prelude.len(), 1);
        assert_eq!(expr.value, "x");
    }

    #[test]
    fn test_append_prelude() {
        let mut expr = JsExpr::simple("x".to_string());

        let new_statements = vec![JsStmt::Expr("validate()".to_string())];
        expr.append_prelude(new_statements);

        assert_eq!(expr.prelude.len(), 1);
        assert_eq!(expr.value, "x");
    }

    #[test]
    fn test_prepend_and_append_order() {
        let initial_prelude = vec![JsStmt::Expr("middle()".to_string())];
        let mut expr = JsExpr::with_prelude(initial_prelude, "result".to_string());

        expr.prepend_prelude(vec![JsStmt::Expr("first()".to_string())]);
        expr.append_prelude(vec![JsStmt::Expr("last()".to_string())]);

        assert_eq!(expr.prelude.len(), 3);

        // Verify order: first, middle, last
        match &expr.prelude[0] {
            JsStmt::Expr(s) => assert_eq!(s, "first()"),
            _ => panic!("Expected Expr statement"),
        }
        match &expr.prelude[1] {
            JsStmt::Expr(s) => assert_eq!(s, "middle()"),
            _ => panic!("Expected Expr statement"),
        }
        match &expr.prelude[2] {
            JsStmt::Expr(s) => assert_eq!(s, "last()"),
            _ => panic!("Expected Expr statement"),
        }
    }

    #[test]
    fn test_map_value() {
        let expr = JsExpr::simple("x".to_string());
        let wrapped = expr.map_value(|v| format!("({})", v));

        assert_eq!(wrapped.value, "(x)");
        assert!(wrapped.is_pure());
    }

    #[test]
    fn test_map_value_preserves_prelude() {
        let prelude = vec![JsStmt::Let {
            name: JsIdent("_t0".to_string()),
            value: "compute()".to_string(),
        }];
        let expr = JsExpr::with_prelude(prelude, "_t0".to_string());
        let wrapped = expr.map_value(|v| format!("Math.abs({})", v));

        assert_eq!(wrapped.prelude.len(), 1);
        assert_eq!(wrapped.value, "Math.abs(_t0)");
    }

    #[test]
    fn test_into_parts() {
        let prelude = vec![
            JsStmt::Let {
                name: JsIdent("_t0".to_string()),
                value: "foo()".to_string(),
            },
            JsStmt::Let {
                name: JsIdent("_t1".to_string()),
                value: "bar()".to_string(),
            },
        ];
        let expr = JsExpr::with_prelude(prelude, "_t0 + _t1".to_string());

        let (statements, value) = expr.into_parts();

        assert_eq!(statements.len(), 2);
        assert_eq!(value, "_t0 + _t1");
    }

    #[test]
    fn test_is_pure() {
        let pure_expr = JsExpr::simple("42".to_string());
        assert!(pure_expr.is_pure());

        let impure_expr = JsExpr::with_prelude(
            vec![JsStmt::Expr("setup()".to_string())],
            "result".to_string(),
        );
        assert!(!impure_expr.is_pure());
    }
}

#[cfg(test)]
mod literal_lowering_tests {
    use crate::compiler::codegen::js::{JsEmitter, JsLoweringConfig};
    use crate::compiler::datatypes::DataType;
    use crate::compiler::hir::nodes::{HirBlock, HirExpr, HirExprKind, HirModule};
    use crate::compiler::interned_path::InternedPath;
    use crate::compiler::parsers::tokenizer::tokens::{CharPosition, TextLocation};
    use crate::compiler::string_interning::StringTable;

    /// Helper to create a test emitter
    fn create_test_emitter() -> (JsEmitter<'static>, Box<HirModule>, Box<StringTable>) {
        let hir_module = Box::new(HirModule {
            blocks: vec![HirBlock {
                id: 0,
                params: vec![],
                nodes: vec![],
            }],
            entry_block: 0,
            functions: vec![],
            structs: vec![],
        });

        let string_table = Box::new(StringTable::new());
        let config = JsLoweringConfig {
            pretty: true,
            emit_locations: false,
        };

        // SAFETY: We're extending the lifetime to 'static by boxing the values
        // and keeping them alive for the duration of the test
        let hir_ref: &'static HirModule = unsafe { &*(hir_module.as_ref() as *const _) };
        let string_ref: &'static StringTable = unsafe { &*(string_table.as_ref() as *const _) };

        let emitter = JsEmitter::new(hir_ref, string_ref, config);
        (emitter, hir_module, string_table)
    }

    /// Helper to create a dummy location for tests
    fn dummy_location() -> TextLocation {
        TextLocation::new(
            InternedPath::new(),
            CharPosition {
                line_number: 1,
                char_column: 1,
            },
            CharPosition {
                line_number: 1,
                char_column: 1,
            },
        )
    }

    #[test]
    fn test_lower_int_literal() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::Int(42),
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "42");
    }

    #[test]
    fn test_lower_negative_int_literal() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::Int(-123),
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "-123");
    }

    #[test]
    fn test_lower_float_literal() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::Float(3.14),
            data_type: DataType::Float,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "3.14");
    }

    #[test]
    fn test_lower_float_nan() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::Float(f64::NAN),
            data_type: DataType::Float,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "NaN");
    }

    #[test]
    fn test_lower_float_infinity() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::Float(f64::INFINITY),
            data_type: DataType::Float,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "Infinity");
    }

    #[test]
    fn test_lower_float_neg_infinity() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::Float(f64::NEG_INFINITY),
            data_type: DataType::Float,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "-Infinity");
    }

    #[test]
    fn test_lower_bool_true() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::Bool(true),
            data_type: DataType::Bool,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "true");
    }

    #[test]
    fn test_lower_bool_false() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::Bool(false),
            data_type: DataType::Bool,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "false");
    }

    #[test]
    fn test_lower_string_literal() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let interned = string_table.intern("hello world");
        let expr = HirExpr {
            kind: HirExprKind::StringLiteral(interned),
            data_type: DataType::String,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "\"hello world\"");
    }

    #[test]
    fn test_lower_string_with_escapes() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let interned = string_table.intern("hello\nworld\t\"quoted\"");
        let expr = HirExpr {
            kind: HirExprKind::StringLiteral(interned),
            data_type: DataType::String,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "\"hello\\nworld\\t\\\"quoted\\\"\"");
    }

    #[test]
    fn test_lower_string_with_backslash() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let interned = string_table.intern("path\\to\\file");
        let expr = HirExpr {
            kind: HirExprKind::StringLiteral(interned),
            data_type: DataType::String,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "\"path\\\\to\\\\file\"");
    }

    #[test]
    fn test_lower_heap_string() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let interned = string_table.intern("heap string");
        let expr = HirExpr {
            kind: HirExprKind::HeapString(interned),
            data_type: DataType::String,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        // HeapString and StringLiteral should produce identical JS in GC semantics
        assert_eq!(result.value, "\"heap string\"");
    }

    #[test]
    fn test_lower_char_simple() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::Char('a'),
            data_type: DataType::Char,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "\"a\"");
    }

    #[test]
    fn test_lower_char_emoji() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::Char('😊'),
            data_type: DataType::Char,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "\"😊\"");
    }

    #[test]
    fn test_lower_char_with_escapes() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        // Test newline
        let expr_newline = HirExpr {
            kind: HirExprKind::Char('\n'),
            data_type: DataType::Char,
            location: dummy_location(),
        };
        let result = emitter.lower_expr(&expr_newline).unwrap();
        assert_eq!(result.value, "\"\\n\"");

        // Test tab
        let expr_tab = HirExpr {
            kind: HirExprKind::Char('\t'),
            data_type: DataType::Char,
            location: dummy_location(),
        };
        let result = emitter.lower_expr(&expr_tab).unwrap();
        assert_eq!(result.value, "\"\\t\"");

        // Test quote
        let expr_quote = HirExpr {
            kind: HirExprKind::Char('"'),
            data_type: DataType::Char,
            location: dummy_location(),
        };
        let result = emitter.lower_expr(&expr_quote).unwrap();
        assert_eq!(result.value, "\"\\\"\"");

        // Test backslash
        let expr_backslash = HirExpr {
            kind: HirExprKind::Char('\\'),
            data_type: DataType::Char,
            location: dummy_location(),
        };
        let result = emitter.lower_expr(&expr_backslash).unwrap();
        assert_eq!(result.value, "\"\\\\\"");
    }
}

#[cfg(test)]
mod binary_and_unary_op_tests {
    use crate::compiler::codegen::js::{JsEmitter, JsLoweringConfig};
    use crate::compiler::datatypes::DataType;
    use crate::compiler::hir::nodes::{BinOp, HirBlock, HirExpr, HirExprKind, HirModule, UnaryOp};
    use crate::compiler::interned_path::InternedPath;
    use crate::compiler::parsers::tokenizer::tokens::{CharPosition, TextLocation};
    use crate::compiler::string_interning::StringTable;

    /// Helper to create a test emitter
    fn create_test_emitter() -> (JsEmitter<'static>, Box<HirModule>, Box<StringTable>) {
        let hir_module = Box::new(HirModule {
            blocks: vec![HirBlock {
                id: 0,
                params: vec![],
                nodes: vec![],
            }],
            entry_block: 0,
            functions: vec![],
            structs: vec![],
        });

        let string_table = Box::new(StringTable::new());
        let config = JsLoweringConfig {
            pretty: true,
            emit_locations: false,
        };

        // SAFETY: We're extending the lifetime to 'static by boxing the values
        // and keeping them alive for the duration of the test
        let hir_ref: &'static HirModule = unsafe { &*(hir_module.as_ref() as *const _) };
        let string_ref: &'static StringTable = unsafe { &*(string_table.as_ref() as *const _) };

        let emitter = JsEmitter::new(hir_ref, string_ref, config);
        (emitter, hir_module, string_table)
    }

    /// Helper to create a dummy location for tests
    fn dummy_location() -> TextLocation {
        TextLocation::new(
            InternedPath::new(),
            CharPosition {
                line_number: 1,
                char_column: 1,
            },
            CharPosition {
                line_number: 1,
                char_column: 1,
            },
        )
    }

    /// Helper to create an integer literal expression
    fn int_expr(value: i64) -> HirExpr {
        HirExpr {
            kind: HirExprKind::Int(value),
            data_type: DataType::Int,
            location: dummy_location(),
        }
    }

    /// Helper to create a float literal expression
    fn float_expr(value: f64) -> HirExpr {
        HirExpr {
            kind: HirExprKind::Float(value),
            data_type: DataType::Float,
            location: dummy_location(),
        }
    }

    /// Helper to create a boolean literal expression
    fn bool_expr(value: bool) -> HirExpr {
        HirExpr {
            kind: HirExprKind::Bool(value),
            data_type: DataType::Bool,
            location: dummy_location(),
        }
    }

    // === Arithmetic Operations ===

    #[test]
    fn test_lower_add_operation() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::BinOp {
                left: Box::new(int_expr(5)),
                op: BinOp::Add,
                right: Box::new(int_expr(3)),
            },
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "(5 + 3)");
    }

    #[test]
    fn test_lower_sub_operation() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::BinOp {
                left: Box::new(int_expr(10)),
                op: BinOp::Sub,
                right: Box::new(int_expr(4)),
            },
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "(10 - 4)");
    }

    #[test]
    fn test_lower_mul_operation() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::BinOp {
                left: Box::new(int_expr(6)),
                op: BinOp::Mul,
                right: Box::new(int_expr(7)),
            },
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "(6 * 7)");
    }

    #[test]
    fn test_lower_div_operation() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::BinOp {
                left: Box::new(int_expr(20)),
                op: BinOp::Div,
                right: Box::new(int_expr(5)),
            },
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "(20 / 5)");
    }

    #[test]
    fn test_lower_mod_operation() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::BinOp {
                left: Box::new(int_expr(17)),
                op: BinOp::Mod,
                right: Box::new(int_expr(5)),
            },
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "(17 % 5)");
    }

    #[test]
    fn test_lower_exponent_operation() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::BinOp {
                left: Box::new(int_expr(2)),
                op: BinOp::Exponent,
                right: Box::new(int_expr(8)),
            },
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "(2 ** 8)");
    }

    #[test]
    fn test_lower_root_operation() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::BinOp {
                left: Box::new(int_expr(3)),
                op: BinOp::Root,
                right: Box::new(int_expr(27)),
            },
            data_type: DataType::Float,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        // Root operation: 3 root 27 = 27^(1/3)
        assert_eq!(result.value, "Math.pow(27, 1 / 3)");
    }

    // === Comparison Operations ===

    #[test]
    fn test_lower_eq_operation() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::BinOp {
                left: Box::new(int_expr(5)),
                op: BinOp::Eq,
                right: Box::new(int_expr(5)),
            },
            data_type: DataType::Bool,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "(5 === 5)");
    }

    #[test]
    fn test_lower_ne_operation() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::BinOp {
                left: Box::new(int_expr(5)),
                op: BinOp::Ne,
                right: Box::new(int_expr(3)),
            },
            data_type: DataType::Bool,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "(5 !== 3)");
    }

    #[test]
    fn test_lower_lt_operation() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::BinOp {
                left: Box::new(int_expr(3)),
                op: BinOp::Lt,
                right: Box::new(int_expr(5)),
            },
            data_type: DataType::Bool,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "(3 < 5)");
    }

    #[test]
    fn test_lower_le_operation() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::BinOp {
                left: Box::new(int_expr(5)),
                op: BinOp::Le,
                right: Box::new(int_expr(5)),
            },
            data_type: DataType::Bool,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "(5 <= 5)");
    }

    #[test]
    fn test_lower_gt_operation() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::BinOp {
                left: Box::new(int_expr(7)),
                op: BinOp::Gt,
                right: Box::new(int_expr(3)),
            },
            data_type: DataType::Bool,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "(7 > 3)");
    }

    #[test]
    fn test_lower_ge_operation() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::BinOp {
                left: Box::new(int_expr(5)),
                op: BinOp::Ge,
                right: Box::new(int_expr(5)),
            },
            data_type: DataType::Bool,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "(5 >= 5)");
    }

    // === Logical Operations ===

    #[test]
    fn test_lower_and_operation() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::BinOp {
                left: Box::new(bool_expr(true)),
                op: BinOp::And,
                right: Box::new(bool_expr(false)),
            },
            data_type: DataType::Bool,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "(true && false)");
    }

    #[test]
    fn test_lower_or_operation() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::BinOp {
                left: Box::new(bool_expr(true)),
                op: BinOp::Or,
                right: Box::new(bool_expr(false)),
            },
            data_type: DataType::Bool,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "(true || false)");
    }

    // === Unary Operations ===

    #[test]
    fn test_lower_neg_operation() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::UnaryOp {
                op: UnaryOp::Neg,
                operand: Box::new(int_expr(42)),
            },
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "(-42)");
    }

    #[test]
    fn test_lower_not_operation() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::UnaryOp {
                op: UnaryOp::Not,
                operand: Box::new(bool_expr(true)),
            },
            data_type: DataType::Bool,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "(!true)");
    }

    // === Nested Operations ===

    #[test]
    fn test_lower_nested_binary_operations() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        // (5 + 3) * 2
        let inner = HirExpr {
            kind: HirExprKind::BinOp {
                left: Box::new(int_expr(5)),
                op: BinOp::Add,
                right: Box::new(int_expr(3)),
            },
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let expr = HirExpr {
            kind: HirExprKind::BinOp {
                left: Box::new(inner),
                op: BinOp::Mul,
                right: Box::new(int_expr(2)),
            },
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "((5 + 3) * 2)");
    }

    #[test]
    fn test_lower_nested_unary_operations() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        // -(-5)
        let inner = HirExpr {
            kind: HirExprKind::UnaryOp {
                op: UnaryOp::Neg,
                operand: Box::new(int_expr(5)),
            },
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let expr = HirExpr {
            kind: HirExprKind::UnaryOp {
                op: UnaryOp::Neg,
                operand: Box::new(inner),
            },
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "(-(-5))");
    }

    #[test]
    fn test_lower_mixed_operations() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        // -(5 + 3)
        let inner = HirExpr {
            kind: HirExprKind::BinOp {
                left: Box::new(int_expr(5)),
                op: BinOp::Add,
                right: Box::new(int_expr(3)),
            },
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let expr = HirExpr {
            kind: HirExprKind::UnaryOp {
                op: UnaryOp::Neg,
                operand: Box::new(inner),
            },
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "(-(5 + 3))");
    }

    #[test]
    fn test_lower_float_operations() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::BinOp {
                left: Box::new(float_expr(3.14)),
                op: BinOp::Mul,
                right: Box::new(float_expr(2.0)),
            },
            data_type: DataType::Float,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "(3.14 * 2)");
    }
}

#[cfg(test)]
mod variable_access_tests {
    use crate::compiler::codegen::js::{JsEmitter, JsLoweringConfig};
    use crate::compiler::datatypes::DataType;
    use crate::compiler::hir::nodes::{HirBlock, HirExpr, HirExprKind, HirModule, HirPlace};
    use crate::compiler::interned_path::InternedPath;
    use crate::compiler::parsers::tokenizer::tokens::{CharPosition, TextLocation};
    use crate::compiler::string_interning::StringTable;

    /// Helper to create a test emitter
    fn create_test_emitter() -> (JsEmitter<'static>, Box<HirModule>, Box<StringTable>) {
        let hir_module = Box::new(HirModule {
            blocks: vec![HirBlock {
                id: 0,
                params: vec![],
                nodes: vec![],
            }],
            entry_block: 0,
            functions: vec![],
            structs: vec![],
        });

        let string_table = Box::new(StringTable::new());
        let config = JsLoweringConfig {
            pretty: true,
            emit_locations: false,
        };

        // SAFETY: We're extending the lifetime to 'static by boxing the values
        // and keeping them alive for the duration of the test
        let hir_ref: &'static HirModule = unsafe { &*(hir_module.as_ref() as *const _) };
        let string_ref: &'static StringTable = unsafe { &*(string_table.as_ref() as *const _) };

        let emitter = JsEmitter::new(hir_ref, string_ref, config);
        (emitter, hir_module, string_table)
    }

    fn dummy_location() -> TextLocation {
        TextLocation::new(
            InternedPath::new(),
            CharPosition {
                line_number: 1,
                char_column: 1,
            },
            CharPosition {
                line_number: 1,
                char_column: 1,
            },
        )
    }

    #[test]
    fn test_lower_load_simple_variable() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        // Use the string table to intern the variable name
        let var_name = string_table.intern("my_var");
        let expr = HirExpr {
            kind: HirExprKind::Load(HirPlace::Var(var_name)),
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "my_var");
    }

    #[test]
    fn test_lower_move_simple_variable() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let var_name = string_table.intern("my_var");
        let expr = HirExpr {
            kind: HirExprKind::Move(HirPlace::Var(var_name)),
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "my_var");
    }

    #[test]
    fn test_load_and_move_generate_identical_code() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let var_name = string_table.intern("test_var");

        // Create Load expression
        let load_expr = HirExpr {
            kind: HirExprKind::Load(HirPlace::Var(var_name)),
            data_type: DataType::String,
            location: dummy_location(),
        };

        // Create Move expression
        let move_expr = HirExpr {
            kind: HirExprKind::Move(HirPlace::Var(var_name)),
            data_type: DataType::String,
            location: dummy_location(),
        };

        let load_result = emitter.lower_expr(&load_expr).unwrap();
        let move_result = emitter.lower_expr(&move_expr).unwrap();

        // In GC semantics, Load and Move should generate identical JavaScript
        assert_eq!(load_result.value, move_result.value);
        assert_eq!(load_result.value, "test_var");
    }

    #[test]
    fn test_lower_field_access() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let base_name = string_table.intern("person");
        let field_name = string_table.intern("name");

        let place = HirPlace::Field {
            base: Box::new(HirPlace::Var(base_name)),
            field: field_name,
        };

        let expr = HirExpr {
            kind: HirExprKind::Load(place),
            data_type: DataType::String,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "person.name");
    }

    #[test]
    fn test_lower_nested_field_access() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let obj_name = string_table.intern("obj");
        let field1_name = string_table.intern("inner");
        let field2_name = string_table.intern("value");

        let place = HirPlace::Field {
            base: Box::new(HirPlace::Field {
                base: Box::new(HirPlace::Var(obj_name)),
                field: field1_name,
            }),
            field: field2_name,
        };

        let expr = HirExpr {
            kind: HirExprKind::Load(place),
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "obj.inner.value");
    }

    #[test]
    fn test_lower_index_access() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let array_name = string_table.intern("arr");

        let place = HirPlace::Index {
            base: Box::new(HirPlace::Var(array_name)),
            index: Box::new(HirExpr {
                kind: HirExprKind::Int(0),
                data_type: DataType::Int,
                location: dummy_location(),
            }),
        };

        let expr = HirExpr {
            kind: HirExprKind::Load(place),
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "arr[0]");
    }

    #[test]
    fn test_lower_complex_index_access() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let array_name = string_table.intern("matrix");
        let i_name = string_table.intern("i");

        let place = HirPlace::Index {
            base: Box::new(HirPlace::Var(array_name)),
            index: Box::new(HirExpr {
                kind: HirExprKind::Load(HirPlace::Var(i_name)),
                data_type: DataType::Int,
                location: dummy_location(),
            }),
        };

        let expr = HirExpr {
            kind: HirExprKind::Load(place),
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "matrix[i]");
    }

    #[test]
    fn test_lower_js_reserved_word_variable() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        // Test that JavaScript reserved words are escaped
        let var_name = string_table.intern("function");
        let expr = HirExpr {
            kind: HirExprKind::Load(HirPlace::Var(var_name)),
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        // Reserved words should be prefixed with underscore
        assert_eq!(result.value, "_function");
    }

    #[test]
    fn test_lower_multiple_reserved_words() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let reserved_words = vec![
            "if", "else", "while", "for", "return", "const", "let", "var",
        ];

        for word in reserved_words {
            let var_name = string_table.intern(word);
            let expr = HirExpr {
                kind: HirExprKind::Load(HirPlace::Var(var_name)),
                data_type: DataType::Int,
                location: dummy_location(),
            };

            let result = emitter.lower_expr(&expr).unwrap();
            assert!(result.is_pure());
            assert_eq!(result.value, format!("_{}", word));
        }
    }
}

#[cfg(test)]
mod function_call_tests {
    use crate::compiler::codegen::js::{JsEmitter, JsLoweringConfig};
    use crate::compiler::datatypes::DataType;
    use crate::compiler::hir::nodes::{HirBlock, HirExpr, HirExprKind, HirModule, HirPlace};
    use crate::compiler::interned_path::InternedPath;
    use crate::compiler::parsers::tokenizer::tokens::{CharPosition, TextLocation};
    use crate::compiler::string_interning::StringTable;

    /// Helper to create a test emitter
    fn create_test_emitter() -> (JsEmitter<'static>, Box<HirModule>, Box<StringTable>) {
        let hir_module = Box::new(HirModule {
            blocks: vec![HirBlock {
                id: 0,
                params: vec![],
                nodes: vec![],
            }],
            entry_block: 0,
            functions: vec![],
            structs: vec![],
        });

        let string_table = Box::new(StringTable::new());
        let config = JsLoweringConfig {
            pretty: true,
            emit_locations: false,
        };

        // SAFETY: We're extending the lifetime to 'static by boxing the values
        // and keeping them alive for the duration of the test
        let hir_ref: &'static HirModule = unsafe { &*(hir_module.as_ref() as *const _) };
        let string_ref: &'static StringTable = unsafe { &*(string_table.as_ref() as *const _) };

        let emitter = JsEmitter::new(hir_ref, string_ref, config);
        (emitter, hir_module, string_table)
    }

    fn dummy_location() -> TextLocation {
        TextLocation::new(
            InternedPath::new(),
            CharPosition {
                line_number: 1,
                char_column: 1,
            },
            CharPosition {
                line_number: 1,
                char_column: 1,
            },
        )
    }

    fn int_expr(value: i64) -> HirExpr {
        HirExpr {
            kind: HirExprKind::Int(value),
            data_type: DataType::Int,
            location: dummy_location(),
        }
    }

    #[test]
    fn test_lower_function_call_no_args() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let func_name = string_table.intern("my_function");
        let expr = HirExpr {
            kind: HirExprKind::Call {
                target: func_name,
                args: vec![],
            },
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "my_function()");
    }

    #[test]
    fn test_lower_function_call_single_arg() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let func_name = string_table.intern("square");
        let expr = HirExpr {
            kind: HirExprKind::Call {
                target: func_name,
                args: vec![int_expr(5)],
            },
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "square(5)");
    }

    #[test]
    fn test_lower_function_call_multiple_args() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let func_name = string_table.intern("add");
        let expr = HirExpr {
            kind: HirExprKind::Call {
                target: func_name,
                args: vec![int_expr(3), int_expr(7)],
            },
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "add(3, 7)");
    }

    #[test]
    fn test_lower_function_call_with_variable_args() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let func_name = string_table.intern("process");
        let var_name = string_table.intern("data");

        let expr = HirExpr {
            kind: HirExprKind::Call {
                target: func_name,
                args: vec![HirExpr {
                    kind: HirExprKind::Load(HirPlace::Var(var_name)),
                    data_type: DataType::String,
                    location: dummy_location(),
                }],
            },
            data_type: DataType::String,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "process(data)");
    }

    #[test]
    fn test_lower_function_call_with_expression_args() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let func_name = string_table.intern("calculate");

        // Call calculate(5 + 3, 10 - 2)
        let arg1 = HirExpr {
            kind: HirExprKind::BinOp {
                left: Box::new(int_expr(5)),
                op: crate::compiler::hir::nodes::BinOp::Add,
                right: Box::new(int_expr(3)),
            },
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let arg2 = HirExpr {
            kind: HirExprKind::BinOp {
                left: Box::new(int_expr(10)),
                op: crate::compiler::hir::nodes::BinOp::Sub,
                right: Box::new(int_expr(2)),
            },
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let expr = HirExpr {
            kind: HirExprKind::Call {
                target: func_name,
                args: vec![arg1, arg2],
            },
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "calculate((5 + 3), (10 - 2))");
    }

    #[test]
    fn test_lower_nested_function_calls() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let outer_func = string_table.intern("outer");
        let inner_func = string_table.intern("inner");

        // Call outer(inner(5))
        let inner_call = HirExpr {
            kind: HirExprKind::Call {
                target: inner_func,
                args: vec![int_expr(5)],
            },
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let expr = HirExpr {
            kind: HirExprKind::Call {
                target: outer_func,
                args: vec![inner_call],
            },
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "outer(inner(5))");
    }

    #[test]
    fn test_lower_function_call_with_reserved_word_name() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        // Test that function names that are reserved words are escaped
        let func_name = string_table.intern("eval");
        let expr = HirExpr {
            kind: HirExprKind::Call {
                target: func_name,
                args: vec![],
            },
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "_eval()");
    }

    #[test]
    fn test_lower_method_call_no_args() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let var_name = string_table.intern("obj");
        let method_name = string_table.intern("toString");

        let receiver = HirExpr {
            kind: HirExprKind::Load(HirPlace::Var(var_name)),
            data_type: DataType::String,
            location: dummy_location(),
        };

        let expr = HirExpr {
            kind: HirExprKind::MethodCall {
                receiver: Box::new(receiver),
                method: method_name,
                args: vec![],
            },
            data_type: DataType::String,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "obj.toString()");
    }

    #[test]
    fn test_lower_method_call_with_args() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let var_name = string_table.intern("arr");
        let method_name = string_table.intern("push");

        let receiver = HirExpr {
            kind: HirExprKind::Load(HirPlace::Var(var_name)),
            data_type: DataType::String,
            location: dummy_location(),
        };

        let expr = HirExpr {
            kind: HirExprKind::MethodCall {
                receiver: Box::new(receiver),
                method: method_name,
                args: vec![int_expr(42)],
            },
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "arr.push(42)");
    }

    #[test]
    fn test_lower_method_call_multiple_args() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let var_name = string_table.intern("str");
        let method_name = string_table.intern("substring");

        let receiver = HirExpr {
            kind: HirExprKind::Load(HirPlace::Var(var_name)),
            data_type: DataType::String,
            location: dummy_location(),
        };

        let expr = HirExpr {
            kind: HirExprKind::MethodCall {
                receiver: Box::new(receiver),
                method: method_name,
                args: vec![int_expr(0), int_expr(5)],
            },
            data_type: DataType::String,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "str.substring(0, 5)");
    }

    #[test]
    fn test_lower_chained_method_calls() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let var_name = string_table.intern("str");
        let method1_name = string_table.intern("trim");
        let method2_name = string_table.intern("toLowerCase");

        // str.trim()
        let receiver = HirExpr {
            kind: HirExprKind::Load(HirPlace::Var(var_name)),
            data_type: DataType::String,
            location: dummy_location(),
        };

        let first_call = HirExpr {
            kind: HirExprKind::MethodCall {
                receiver: Box::new(receiver),
                method: method1_name,
                args: vec![],
            },
            data_type: DataType::String,
            location: dummy_location(),
        };

        // str.trim().toLowerCase()
        let expr = HirExpr {
            kind: HirExprKind::MethodCall {
                receiver: Box::new(first_call),
                method: method2_name,
                args: vec![],
            },
            data_type: DataType::String,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "str.trim().toLowerCase()");
    }

    #[test]
    fn test_lower_method_call_on_field_access() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let obj_name = string_table.intern("person");
        let field_name = string_table.intern("name");
        let method_name = string_table.intern("toUpperCase");

        // person.name
        let receiver = HirExpr {
            kind: HirExprKind::Load(HirPlace::Field {
                base: Box::new(HirPlace::Var(obj_name)),
                field: field_name,
            }),
            data_type: DataType::String,
            location: dummy_location(),
        };

        // person.name.toUpperCase()
        let expr = HirExpr {
            kind: HirExprKind::MethodCall {
                receiver: Box::new(receiver),
                method: method_name,
                args: vec![],
            },
            data_type: DataType::String,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "person.name.toUpperCase()");
    }

    #[test]
    fn test_lower_method_call_with_expression_receiver() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let func_name = string_table.intern("getString");
        let method_name = string_table.intern("length");

        // getString()
        let receiver = HirExpr {
            kind: HirExprKind::Call {
                target: func_name,
                args: vec![],
            },
            data_type: DataType::String,
            location: dummy_location(),
        };

        // getString().length()
        let expr = HirExpr {
            kind: HirExprKind::MethodCall {
                receiver: Box::new(receiver),
                method: method_name,
                args: vec![],
            },
            data_type: DataType::Int,
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "getString().length()");
    }
}

#[cfg(test)]
mod constructor_lowering_tests {
    use crate::compiler::codegen::js::{JsEmitter, JsLoweringConfig};
    use crate::compiler::datatypes::DataType;
    use crate::compiler::hir::nodes::{HirBlock, HirExpr, HirExprKind, HirModule};
    use crate::compiler::interned_path::InternedPath;
    use crate::compiler::parsers::tokenizer::tokens::{CharPosition, TextLocation};
    use crate::compiler::string_interning::StringTable;

    /// Helper to create a test emitter
    fn create_test_emitter() -> (JsEmitter<'static>, Box<HirModule>, Box<StringTable>) {
        let hir_module = Box::new(HirModule {
            blocks: vec![HirBlock {
                id: 0,
                params: vec![],
                nodes: vec![],
            }],
            entry_block: 0,
            functions: vec![],
            structs: vec![],
        });

        let string_table = Box::new(StringTable::new());
        let config = JsLoweringConfig {
            pretty: true,
            emit_locations: false,
        };

        // SAFETY: We're extending the lifetime to 'static by boxing the values
        // and keeping them alive for the duration of the test
        let hir_ref: &'static HirModule = unsafe { &*(hir_module.as_ref() as *const _) };
        let string_ref: &'static StringTable = unsafe { &*(string_table.as_ref() as *const _) };

        let emitter = JsEmitter::new(hir_ref, string_ref, config);
        (emitter, hir_module, string_table)
    }

    /// Helper to create a dummy location for tests
    fn dummy_location() -> TextLocation {
        TextLocation::new(
            InternedPath::new(),
            CharPosition {
                line_number: 1,
                char_column: 1,
            },
            CharPosition {
                line_number: 1,
                char_column: 1,
            },
        )
    }

    /// Helper to create an integer literal expression
    fn int_expr(value: i64) -> HirExpr {
        HirExpr {
            kind: HirExprKind::Int(value),
            data_type: DataType::Int,
            location: dummy_location(),
        }
    }

    // === Struct Construction Tests ===

    #[test]
    fn test_lower_empty_struct_construct() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let type_name = string_table.intern("EmptyStruct");
        let expr = HirExpr {
            kind: HirExprKind::StructConstruct {
                type_name,
                fields: vec![],
            },
            data_type: DataType::Struct(vec![], crate::compiler::datatypes::Ownership::default()),
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "{}");
    }

    #[test]
    fn test_lower_struct_construct_single_field() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let type_name = string_table.intern("Person");
        let field_name = string_table.intern("age");

        let expr = HirExpr {
            kind: HirExprKind::StructConstruct {
                type_name,
                fields: vec![(field_name, int_expr(30))],
            },
            data_type: DataType::Struct(vec![], crate::compiler::datatypes::Ownership::default()),
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "{ age: 30 }");
    }

    #[test]
    fn test_lower_struct_construct_multiple_fields() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let type_name = string_table.intern("Person");
        let name_field = string_table.intern("name");
        let age_field = string_table.intern("age");

        let name_str = string_table.intern("Alice");
        let name_expr = HirExpr {
            kind: HirExprKind::StringLiteral(name_str),
            data_type: DataType::String,
            location: dummy_location(),
        };

        let expr = HirExpr {
            kind: HirExprKind::StructConstruct {
                type_name,
                fields: vec![(name_field, name_expr), (age_field, int_expr(30))],
            },
            data_type: DataType::Struct(vec![], crate::compiler::datatypes::Ownership::default()),
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "{ name: \"Alice\", age: 30 }");
    }

    #[test]
    fn test_lower_nested_struct_construct() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let address_type = string_table.intern("Address");
        let person_type = string_table.intern("Person");
        let street_field = string_table.intern("street");
        let address_field = string_table.intern("address");

        let street_str = string_table.intern("Main St");
        let street_expr = HirExpr {
            kind: HirExprKind::StringLiteral(street_str),
            data_type: DataType::String,
            location: dummy_location(),
        };

        // Create inner struct (Address)
        let address_expr = HirExpr {
            kind: HirExprKind::StructConstruct {
                type_name: address_type,
                fields: vec![(street_field, street_expr)],
            },
            data_type: DataType::Struct(vec![], crate::compiler::datatypes::Ownership::default()),
            location: dummy_location(),
        };

        // Create outer struct (Person with Address)
        let expr = HirExpr {
            kind: HirExprKind::StructConstruct {
                type_name: person_type,
                fields: vec![(address_field, address_expr)],
            },
            data_type: DataType::Struct(vec![], crate::compiler::datatypes::Ownership::default()),
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "{ address: { street: \"Main St\" } }");
    }

    // === Collection Construction Tests ===

    #[test]
    fn test_lower_empty_collection() {
        let (mut emitter, _hir, _string_table) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::Collection(vec![]),
            data_type: DataType::Collection(
                Box::new(DataType::Int),
                crate::compiler::datatypes::Ownership::default(),
            ),
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "[]");
    }

    #[test]
    fn test_lower_collection_single_element() {
        let (mut emitter, _hir, _string_table) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::Collection(vec![int_expr(42)]),
            data_type: DataType::Collection(
                Box::new(DataType::Int),
                crate::compiler::datatypes::Ownership::default(),
            ),
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "[42]");
    }

    #[test]
    fn test_lower_collection_multiple_elements() {
        let (mut emitter, _hir, _string_table) = create_test_emitter();

        let expr = HirExpr {
            kind: HirExprKind::Collection(vec![
                int_expr(1),
                int_expr(2),
                int_expr(3),
                int_expr(4),
                int_expr(5),
            ]),
            data_type: DataType::Collection(
                Box::new(DataType::Int),
                crate::compiler::datatypes::Ownership::default(),
            ),
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "[1, 2, 3, 4, 5]");
    }

    #[test]
    fn test_lower_collection_with_string_literals() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let hello = string_table.intern("hello");
        let world = string_table.intern("world");

        let hello_expr = HirExpr {
            kind: HirExprKind::StringLiteral(hello),
            data_type: DataType::String,
            location: dummy_location(),
        };

        let world_expr = HirExpr {
            kind: HirExprKind::StringLiteral(world),
            data_type: DataType::String,
            location: dummy_location(),
        };

        let expr = HirExpr {
            kind: HirExprKind::Collection(vec![hello_expr, world_expr]),
            data_type: DataType::Collection(
                Box::new(DataType::String),
                crate::compiler::datatypes::Ownership::default(),
            ),
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "[\"hello\", \"world\"]");
    }

    #[test]
    fn test_lower_nested_collection() {
        let (mut emitter, _hir, _string_table) = create_test_emitter();

        // Create inner collections
        let inner1 = HirExpr {
            kind: HirExprKind::Collection(vec![int_expr(1), int_expr(2)]),
            data_type: DataType::Collection(
                Box::new(DataType::Int),
                crate::compiler::datatypes::Ownership::default(),
            ),
            location: dummy_location(),
        };

        let inner2 = HirExpr {
            kind: HirExprKind::Collection(vec![int_expr(3), int_expr(4)]),
            data_type: DataType::Collection(
                Box::new(DataType::Int),
                crate::compiler::datatypes::Ownership::default(),
            ),
            location: dummy_location(),
        };

        // Create outer collection
        let expr = HirExpr {
            kind: HirExprKind::Collection(vec![inner1, inner2]),
            data_type: DataType::Collection(
                Box::new(DataType::Collection(
                    Box::new(DataType::Int),
                    crate::compiler::datatypes::Ownership::default(),
                )),
                crate::compiler::datatypes::Ownership::default(),
            ),
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "[[1, 2], [3, 4]]");
    }

    #[test]
    fn test_lower_collection_of_structs() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let point_type = string_table.intern("Point");
        let x_field = string_table.intern("x");
        let y_field = string_table.intern("y");

        // Create first point
        let point1 = HirExpr {
            kind: HirExprKind::StructConstruct {
                type_name: point_type,
                fields: vec![(x_field, int_expr(0)), (y_field, int_expr(0))],
            },
            data_type: DataType::Struct(vec![], crate::compiler::datatypes::Ownership::default()),
            location: dummy_location(),
        };

        // Create second point
        let point2 = HirExpr {
            kind: HirExprKind::StructConstruct {
                type_name: point_type,
                fields: vec![(x_field, int_expr(10)), (y_field, int_expr(20))],
            },
            data_type: DataType::Struct(vec![], crate::compiler::datatypes::Ownership::default()),
            location: dummy_location(),
        };

        // Create collection of points
        let expr = HirExpr {
            kind: HirExprKind::Collection(vec![point1, point2]),
            data_type: DataType::Collection(
                Box::new(DataType::Struct(
                    vec![],
                    crate::compiler::datatypes::Ownership::default(),
                )),
                crate::compiler::datatypes::Ownership::default(),
            ),
            location: dummy_location(),
        };

        let result = emitter.lower_expr(&expr).unwrap();
        assert!(result.is_pure());
        assert_eq!(result.value, "[{ x: 0, y: 0 }, { x: 10, y: 20 }]");
    }
}

#[cfg(test)]
mod statement_emission_tests {
    use crate::compiler::codegen::js::{JsEmitter, JsIdent, JsLoweringConfig, JsStmt};
    use crate::compiler::hir::nodes::{HirBlock, HirModule};
    use crate::compiler::string_interning::StringTable;

    /// Helper to create a test emitter
    fn create_test_emitter() -> (JsEmitter<'static>, Box<HirModule>, Box<StringTable>) {
        let hir_module = Box::new(HirModule {
            blocks: vec![HirBlock {
                id: 0,
                params: vec![],
                nodes: vec![],
            }],
            entry_block: 0,
            functions: vec![],
            structs: vec![],
        });

        let string_table = Box::new(StringTable::new());
        let config = JsLoweringConfig {
            pretty: true,
            emit_locations: false,
        };

        // SAFETY: We're extending the lifetime to 'static by boxing the values
        // and keeping them alive for the duration of the test
        let hir_ref: &'static HirModule = unsafe { &*(hir_module.as_ref() as *const _) };
        let string_ref: &'static StringTable = unsafe { &*(string_table.as_ref() as *const _) };

        let emitter = JsEmitter::new(hir_ref, string_ref, config);
        (emitter, hir_module, string_table)
    }

    #[test]
    fn test_emit_let_statement() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let stmt = JsStmt::Let {
            name: JsIdent("x".to_string()),
            value: "42".to_string(),
        };

        emitter.emit_stmt(&stmt);

        // With pretty printing enabled, should have newline and indentation
        assert_eq!(emitter.out, "\nlet x = 42;");
    }

    #[test]
    fn test_emit_assign_statement() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let stmt = JsStmt::Assign {
            name: JsIdent("x".to_string()),
            value: "100".to_string(),
        };

        emitter.emit_stmt(&stmt);

        assert_eq!(emitter.out, "\nx = 100;");
    }

    #[test]
    fn test_emit_expr_statement() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let stmt = JsStmt::Expr("console.log(\"hello\")".to_string());

        emitter.emit_stmt(&stmt);

        assert_eq!(emitter.out, "\nconsole.log(\"hello\");");
    }

    #[test]
    fn test_emit_multiple_statements() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let stmts = vec![
            JsStmt::Let {
                name: JsIdent("x".to_string()),
                value: "10".to_string(),
            },
            JsStmt::Let {
                name: JsIdent("y".to_string()),
                value: "20".to_string(),
            },
            JsStmt::Assign {
                name: JsIdent("x".to_string()),
                value: "x + y".to_string(),
            },
        ];

        emitter.emit_stmts(&stmts);

        let expected = "\nlet x = 10;\nlet y = 20;\nx = x + y;";
        assert_eq!(emitter.out, expected);
    }

    #[test]
    fn test_emit_with_indentation() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        // Increase indentation level
        emitter.indent();

        let stmt = JsStmt::Let {
            name: JsIdent("x".to_string()),
            value: "42".to_string(),
        };

        emitter.emit_stmt(&stmt);

        // Should have 4 spaces of indentation (1 level * 4 spaces)
        assert_eq!(emitter.out, "\n    let x = 42;");
    }

    #[test]
    fn test_emit_with_multiple_indentation_levels() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        // Test at indentation level 0
        let stmt1 = JsStmt::Let {
            name: JsIdent("a".to_string()),
            value: "1".to_string(),
        };
        emitter.emit_stmt(&stmt1);

        // Increase to level 1
        emitter.indent();
        let stmt2 = JsStmt::Let {
            name: JsIdent("b".to_string()),
            value: "2".to_string(),
        };
        emitter.emit_stmt(&stmt2);

        // Increase to level 2
        emitter.indent();
        let stmt3 = JsStmt::Let {
            name: JsIdent("c".to_string()),
            value: "3".to_string(),
        };
        emitter.emit_stmt(&stmt3);

        // Decrease to level 1
        emitter.dedent();
        let stmt4 = JsStmt::Let {
            name: JsIdent("d".to_string()),
            value: "4".to_string(),
        };
        emitter.emit_stmt(&stmt4);

        let expected = "\nlet a = 1;\n    let b = 2;\n        let c = 3;\n    let d = 4;";
        assert_eq!(emitter.out, expected);
    }

    #[test]
    fn test_emit_without_pretty_printing() {
        let hir_module = Box::new(HirModule {
            blocks: vec![HirBlock {
                id: 0,
                params: vec![],
                nodes: vec![],
            }],
            entry_block: 0,
            functions: vec![],
            structs: vec![],
        });

        let string_table = Box::new(StringTable::new());
        let config = JsLoweringConfig {
            pretty: false, // Disable pretty printing
            emit_locations: false,
        };

        let hir_ref: &'static HirModule = unsafe { &*(hir_module.as_ref() as *const _) };
        let string_ref: &'static StringTable = unsafe { &*(string_table.as_ref() as *const _) };

        let mut emitter = JsEmitter::new(hir_ref, string_ref, config);

        let stmt = JsStmt::Let {
            name: JsIdent("x".to_string()),
            value: "42".to_string(),
        };

        emitter.emit_stmt(&stmt);

        // Without pretty printing, should have no newline or indentation
        assert_eq!(emitter.out, "let x = 42;");
    }

    #[test]
    fn test_emit_complex_values() {
        let (mut emitter, _hir, _st) = create_test_emitter();

        let stmts = vec![
            JsStmt::Let {
                name: JsIdent("obj".to_string()),
                value: "{ x: 10, y: 20 }".to_string(),
            },
            JsStmt::Let {
                name: JsIdent("arr".to_string()),
                value: "[1, 2, 3]".to_string(),
            },
            JsStmt::Expr("console.log(obj.x + arr[0])".to_string()),
        ];

        emitter.emit_stmts(&stmts);

        let expected =
            "\nlet obj = { x: 10, y: 20 };\nlet arr = [1, 2, 3];\nconsole.log(obj.x + arr[0]);";
        assert_eq!(emitter.out, expected);
    }
}

#[cfg(test)]
mod assignment_statement_tests {
    use crate::compiler::codegen::js::{JsEmitter, JsLoweringConfig};
    use crate::compiler::datatypes::DataType;
    use crate::compiler::hir::nodes::{
        HirBlock, HirExpr, HirExprKind, HirModule, HirPlace, HirStmt,
    };
    use crate::compiler::interned_path::InternedPath;
    use crate::compiler::parsers::tokenizer::tokens::{CharPosition, TextLocation};
    use crate::compiler::string_interning::StringTable;

    /// Helper to create a test emitter
    fn create_test_emitter() -> (JsEmitter<'static>, Box<HirModule>, Box<StringTable>) {
        let hir_module = Box::new(HirModule {
            blocks: vec![HirBlock {
                id: 0,
                params: vec![],
                nodes: vec![],
            }],
            entry_block: 0,
            functions: vec![],
            structs: vec![],
        });

        let string_table = Box::new(StringTable::new());
        let config = JsLoweringConfig {
            pretty: true,
            emit_locations: false,
        };

        // SAFETY: We're extending the lifetime to 'static by boxing the values
        // and keeping them alive for the duration of the test
        let hir_ref: &'static HirModule = unsafe { &*(hir_module.as_ref() as *const _) };
        let string_ref: &'static StringTable = unsafe { &*(string_table.as_ref() as *const _) };

        let emitter = JsEmitter::new(hir_ref, string_ref, config);
        (emitter, hir_module, string_table)
    }

    /// Helper to create a dummy location for tests
    fn dummy_location() -> TextLocation {
        TextLocation::new(
            InternedPath::new(),
            CharPosition {
                line_number: 1,
                char_column: 1,
            },
            CharPosition {
                line_number: 1,
                char_column: 1,
            },
        )
    }

    #[test]
    fn test_first_assignment_emits_let() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let var_name = string_table.intern("x");
        let stmt = HirStmt::Assign {
            target: HirPlace::Var(var_name),
            value: HirExpr {
                kind: HirExprKind::Int(42),
                data_type: DataType::Int,
                location: dummy_location(),
            },
            is_mutable: false,
        };

        emitter.lower_stmt(&stmt);

        // Should emit: let x = 42;
        assert!(emitter.out.contains("let x = 42;"));
        assert!(emitter.declared_vars.contains("x"));
    }

    #[test]
    fn test_second_assignment_emits_reassignment() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let var_name = string_table.intern("x");

        // First assignment
        let stmt1 = HirStmt::Assign {
            target: HirPlace::Var(var_name),
            value: HirExpr {
                kind: HirExprKind::Int(42),
                data_type: DataType::Int,
                location: dummy_location(),
            },
            is_mutable: false,
        };
        emitter.lower_stmt(&stmt1);

        // Second assignment to same variable
        let stmt2 = HirStmt::Assign {
            target: HirPlace::Var(var_name),
            value: HirExpr {
                kind: HirExprKind::Int(100),
                data_type: DataType::Int,
                location: dummy_location(),
            },
            is_mutable: false,
        };
        emitter.lower_stmt(&stmt2);

        // Should emit: let x = 42; followed by x = 100;
        assert!(emitter.out.contains("let x = 42;"));
        assert!(emitter.out.contains("x = 100;"));
        // Should NOT emit "let x = 100;"
        assert!(!emitter.out.contains("let x = 100;"));
    }

    #[test]
    fn test_mutable_assignment_ignored_in_gc_semantics() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let var_name = string_table.intern("y");

        // Assignment with is_mutable = true
        let stmt = HirStmt::Assign {
            target: HirPlace::Var(var_name),
            value: HirExpr {
                kind: HirExprKind::Int(10),
                data_type: DataType::Int,
                location: dummy_location(),
            },
            is_mutable: true,
        };
        emitter.lower_stmt(&stmt);

        // Should still emit: let y = 10; (is_mutable flag is ignored)
        assert!(emitter.out.contains("let y = 10;"));
    }

    #[test]
    fn test_assignment_with_expression() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let var_name = string_table.intern("result");

        // Assignment with a binary operation: result = 5 + 3
        let stmt = HirStmt::Assign {
            target: HirPlace::Var(var_name),
            value: HirExpr {
                kind: HirExprKind::BinOp {
                    left: Box::new(HirExpr {
                        kind: HirExprKind::Int(5),
                        data_type: DataType::Int,
                        location: dummy_location(),
                    }),
                    op: crate::compiler::hir::nodes::BinOp::Add,
                    right: Box::new(HirExpr {
                        kind: HirExprKind::Int(3),
                        data_type: DataType::Int,
                        location: dummy_location(),
                    }),
                },
                data_type: DataType::Int,
                location: dummy_location(),
            },
            is_mutable: false,
        };
        emitter.lower_stmt(&stmt);

        // Should emit: let result = (5 + 3);
        assert!(emitter.out.contains("let result = (5 + 3);"));
    }

    #[test]
    fn test_multiple_different_variables() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let var_x = string_table.intern("x");
        let var_y = string_table.intern("y");
        let var_z = string_table.intern("z");

        // Assign to x
        let stmt1 = HirStmt::Assign {
            target: HirPlace::Var(var_x),
            value: HirExpr {
                kind: HirExprKind::Int(1),
                data_type: DataType::Int,
                location: dummy_location(),
            },
            is_mutable: false,
        };
        emitter.lower_stmt(&stmt1);

        // Assign to y
        let stmt2 = HirStmt::Assign {
            target: HirPlace::Var(var_y),
            value: HirExpr {
                kind: HirExprKind::Int(2),
                data_type: DataType::Int,
                location: dummy_location(),
            },
            is_mutable: false,
        };
        emitter.lower_stmt(&stmt2);

        // Assign to z
        let stmt3 = HirStmt::Assign {
            target: HirPlace::Var(var_z),
            value: HirExpr {
                kind: HirExprKind::Int(3),
                data_type: DataType::Int,
                location: dummy_location(),
            },
            is_mutable: false,
        };
        emitter.lower_stmt(&stmt3);

        // All should be declarations
        assert!(emitter.out.contains("let x = 1;"));
        assert!(emitter.out.contains("let y = 2;"));
        assert!(emitter.out.contains("let z = 3;"));
        assert_eq!(emitter.declared_vars.len(), 3);
    }

    #[test]
    fn test_field_assignment() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let obj_name = string_table.intern("obj");
        let field_name = string_table.intern("value");

        // First, declare the object
        let stmt1 = HirStmt::Assign {
            target: HirPlace::Var(obj_name),
            value: HirExpr {
                kind: HirExprKind::StructConstruct {
                    type_name: string_table.intern("MyStruct"),
                    fields: vec![],
                },
                data_type: DataType::Inferred,
                location: dummy_location(),
            },
            is_mutable: false,
        };
        emitter.lower_stmt(&stmt1);

        // Then assign to a field: obj.value = 42
        let stmt2 = HirStmt::Assign {
            target: HirPlace::Field {
                base: Box::new(HirPlace::Var(obj_name)),
                field: field_name,
            },
            value: HirExpr {
                kind: HirExprKind::Int(42),
                data_type: DataType::Int,
                location: dummy_location(),
            },
            is_mutable: false,
        };
        emitter.lower_stmt(&stmt2);

        // Should emit: let obj = {}; followed by obj.value = 42;
        assert!(emitter.out.contains("let obj = {};"));
        assert!(emitter.out.contains("obj.value = 42;"));
        // Field assignment should NOT use let
        assert!(!emitter.out.contains("let obj.value"));
    }

    #[test]
    fn test_index_assignment() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let arr_name = string_table.intern("arr");

        // First, declare the array
        let stmt1 = HirStmt::Assign {
            target: HirPlace::Var(arr_name),
            value: HirExpr {
                kind: HirExprKind::Collection(vec![]),
                data_type: DataType::Inferred,
                location: dummy_location(),
            },
            is_mutable: false,
        };
        emitter.lower_stmt(&stmt1);

        // Then assign to an index: arr[0] = 42
        let stmt2 = HirStmt::Assign {
            target: HirPlace::Index {
                base: Box::new(HirPlace::Var(arr_name)),
                index: Box::new(HirExpr {
                    kind: HirExprKind::Int(0),
                    data_type: DataType::Int,
                    location: dummy_location(),
                }),
            },
            value: HirExpr {
                kind: HirExprKind::Int(42),
                data_type: DataType::Int,
                location: dummy_location(),
            },
            is_mutable: false,
        };
        emitter.lower_stmt(&stmt2);

        // Should emit: let arr = []; followed by arr[0] = 42;
        assert!(emitter.out.contains("let arr = [];"));
        assert!(emitter.out.contains("arr[0] = 42;"));
        // Index assignment should NOT use let
        assert!(!emitter.out.contains("let arr[0]"));
    }

    #[test]
    fn test_possible_drop_is_noop() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let var_name = string_table.intern("x");

        // PossibleDrop statement
        let stmt = HirStmt::PossibleDrop(HirPlace::Var(var_name));

        let output_before = emitter.out.clone();
        emitter.lower_stmt(&stmt);
        let output_after = emitter.out.clone();

        // Output should be unchanged (no-op)
        assert_eq!(output_before, output_after);
    }

    #[test]
    fn test_expression_statement() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let func_name = string_table.intern("foo");

        // Expression statement: foo()
        let stmt = HirStmt::ExprStmt(HirExpr {
            kind: HirExprKind::Call {
                target: func_name,
                args: vec![],
            },
            data_type: DataType::Inferred,
            location: dummy_location(),
        });
        emitter.lower_stmt(&stmt);

        // Should emit: foo();
        assert!(emitter.out.contains("foo();"));
    }

    #[test]
    fn test_function_call_statement_no_args() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let func_name = string_table.intern("myFunction");

        // HirStmt::Call with no arguments
        let stmt = HirStmt::Call {
            target: func_name,
            args: vec![],
        };
        emitter.lower_stmt(&stmt);

        // Should emit: myFunction();
        assert!(emitter.out.contains("myFunction();"));
    }

    #[test]
    fn test_function_call_statement_with_args() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let func_name = string_table.intern("calculate");

        // HirStmt::Call with arguments: calculate(5, 10)
        let stmt = HirStmt::Call {
            target: func_name,
            args: vec![
                HirExpr {
                    kind: HirExprKind::Int(5),
                    data_type: DataType::Int,
                    location: dummy_location(),
                },
                HirExpr {
                    kind: HirExprKind::Int(10),
                    data_type: DataType::Int,
                    location: dummy_location(),
                },
            ],
        };
        emitter.lower_stmt(&stmt);

        // Should emit: calculate(5, 10);
        assert!(emitter.out.contains("calculate(5, 10);"));
    }

    #[test]
    fn test_host_call_statement() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let func_name = string_table.intern("io");
        let module_name = string_table.intern("host");
        let import_name = string_table.intern("print");

        // HirStmt::HostCall: io("Hello, World!")
        let stmt = HirStmt::HostCall {
            target: func_name,
            module: module_name,
            import: import_name,
            args: vec![HirExpr {
                kind: HirExprKind::StringLiteral(string_table.intern("Hello, World!")),
                data_type: DataType::String,
                location: dummy_location(),
            }],
        };
        emitter.lower_stmt(&stmt);

        // Should emit: io("Hello, World!");
        // Note: Host function mapping (io -> console.log) will be implemented in a later task
        assert!(emitter.out.contains("io(\"Hello, World!\");"));
    }

    #[test]
    fn test_host_call_statement_multiple_args() {
        let (mut emitter, _hir, mut string_table) = create_test_emitter();

        let func_name = string_table.intern("io");
        let module_name = string_table.intern("host");
        let import_name = string_table.intern("print");

        // HirStmt::HostCall with multiple arguments
        let stmt = HirStmt::HostCall {
            target: func_name,
            module: module_name,
            import: import_name,
            args: vec![
                HirExpr {
                    kind: HirExprKind::StringLiteral(string_table.intern("Value: ")),
                    data_type: DataType::String,
                    location: dummy_location(),
                },
                HirExpr {
                    kind: HirExprKind::Int(42),
                    data_type: DataType::Int,
                    location: dummy_location(),
                },
            ],
        };
        emitter.lower_stmt(&stmt);

        // Should emit: io("Value: ", 42);
        assert!(emitter.out.contains("io(\"Value: \", 42);"));
    }
}
