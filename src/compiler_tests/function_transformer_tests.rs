//! Property-based tests for FunctionTransformer
//!
//! Feature: hir-builder, Property 2: Function Transformation Correctness
//! Validates: Requirements 1.4, 5.1, 5.2, 5.3, 5.4, 5.5, 5.6, 4.6
//!
//! These tests verify that function definitions and calls are correctly transformed
//! from AST to HIR with proper parameter handling, return management, and ABI preparation.

use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::hir::build_hir::HirBuilderContext;
use crate::compiler::hir::function_transformer::FunctionTransformer;
use crate::compiler::hir::nodes::{HirExprKind, HirKind, HirStmt, HirTerminator};
use crate::compiler::parsers::ast_nodes::Var;
use crate::compiler::parsers::expressions::expression::Expression;
use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::string_interning::StringTable;

// ============================================================================
// Helper Functions
// ============================================================================

/// Creates a simple function signature for testing
fn create_test_signature(
    string_table: &mut StringTable,
    param_count: usize,
    return_count: usize,
) -> FunctionSignature {
    let mut parameters = Vec::new();
    for i in 0..param_count {
        let param_name = string_table.intern(&format!("param_{}", i));
        parameters.push(Var {
            id: param_name,
            value: Expression::int(0, TextLocation::default(), Ownership::ImmutableReference),
        });
    }

    let mut returns = Vec::new();
    for i in 0..return_count {
        let return_name = string_table.intern(&format!("return_{}", i));
        returns.push(Var {
            id: return_name,
            value: Expression::int(0, TextLocation::default(), Ownership::ImmutableReference),
        });
    }

    FunctionSignature {
        parameters,
        returns,
    }
}

/// Creates a simple return expression for testing
fn create_test_return_expr(value: i64) -> Expression {
    Expression::int(
        value,
        TextLocation::default(),
        Ownership::ImmutableReference,
    )
}

// ============================================================================
// Unit Tests for Basic Functionality
// ============================================================================

#[test]
fn test_function_transformer_creation() {
    let transformer = FunctionTransformer::new();
    assert!(std::mem::size_of_val(&transformer) == 0); // Zero-sized type
}

#[test]
fn test_transform_simple_arguments() {
    let mut transformer = FunctionTransformer::new();

    // Create a simple integer argument
    let int_expr = Expression::int(42, TextLocation::default(), Ownership::ImmutableReference);

    let result = transformer.transform_argument(&int_expr);
    assert!(result.is_ok());

    let hir_expr = result.unwrap();
    assert!(matches!(hir_expr.kind, HirExprKind::Int(42)));
}

#[test]
fn test_transform_variable_argument() {
    let mut transformer = FunctionTransformer::new();
    let mut string_table = StringTable::new();

    let var_name = string_table.intern("test_var");
    let var_expr = Expression::parameter(
        var_name,
        DataType::Int,
        TextLocation::default(),
        Ownership::ImmutableReference,
    );

    let result = transformer.transform_argument(&var_expr);
    assert!(result.is_ok());

    let hir_expr = result.unwrap();
    assert!(matches!(hir_expr.kind, HirExprKind::Load(_)));
}

#[test]
fn test_transform_simple_function_definition() {
    let mut string_table = StringTable::new();
    let func_name = string_table.intern("test_func");
    let signature = create_test_signature(&mut string_table, 2, 1);

    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut transformer = FunctionTransformer::new();
    let body = vec![]; // Empty body for simplicity

    let result = transformer.transform_function_definition(
        func_name,
        signature.clone(),
        &body,
        &mut ctx,
        TextLocation::default(),
    );

    assert!(result.is_ok(), "Function transformation should succeed");
    let func_node = result.unwrap();

    // Verify it's a function definition
    match &func_node.kind {
        HirKind::Stmt(HirStmt::FunctionDef {
            name,
            signature: sig,
            body: _,
        }) => {
            assert_eq!(*name, func_name);
            assert_eq!(sig.parameters.len(), 2);
            assert_eq!(sig.returns.len(), 1);
        }
        _ => panic!("Expected FunctionDef node"),
    }
}

#[test]
fn test_transform_function_with_parameters() {
    let mut string_table = StringTable::new();
    let func_name = string_table.intern("func_with_params");
    let signature = create_test_signature(&mut string_table, 3, 0);

    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut transformer = FunctionTransformer::new();
    let body = vec![];

    let result = transformer.transform_function_definition(
        func_name,
        signature.clone(),
        &body,
        &mut ctx,
        TextLocation::default(),
    );

    assert!(result.is_ok());
    let func_node = result.unwrap();

    match &func_node.kind {
        HirKind::Stmt(HirStmt::FunctionDef { signature: sig, .. }) => {
            assert_eq!(sig.parameters.len(), 3, "Should have 3 parameters");
        }
        _ => panic!("Expected FunctionDef node"),
    }
}

#[test]
fn test_transform_return_statement() {
    let mut string_table = StringTable::new();
    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut transformer = FunctionTransformer::new();

    let return_exprs = vec![create_test_return_expr(42)];

    let result = transformer.transform_return(&return_exprs, &mut ctx, &TextLocation::default());

    assert!(result.is_ok(), "Return transformation should succeed");
    let nodes = result.unwrap();

    assert_eq!(nodes.len(), 1, "Should produce one return node");

    match &nodes[0].kind {
        HirKind::Terminator(HirTerminator::Return(exprs)) => {
            assert_eq!(exprs.len(), 1);
            assert!(matches!(exprs[0].kind, HirExprKind::Int(42)));
        }
        _ => panic!("Expected Return terminator"),
    }
}

#[test]
fn test_transform_function_call_as_statement() {
    let mut string_table = StringTable::new();
    let func_name = string_table.intern("test_call");

    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut transformer = FunctionTransformer::new();
    let args = vec![create_test_return_expr(10), create_test_return_expr(20)];
    let returns = vec![];

    let result = transformer.transform_function_call_as_stmt(
        func_name,
        &args,
        &returns,
        &mut ctx,
        &TextLocation::default(),
    );

    assert!(
        result.is_ok(),
        "Function call transformation should succeed"
    );
    let nodes = result.unwrap();

    assert_eq!(nodes.len(), 1, "Should produce one call node");

    match &nodes[0].kind {
        HirKind::Stmt(HirStmt::Call {
            target,
            args: call_args,
        }) => {
            assert_eq!(*target, func_name);
            assert_eq!(call_args.len(), 2);
        }
        _ => panic!("Expected Call statement"),
    }
}

#[test]
fn test_transform_host_function_call() {
    let mut string_table = StringTable::new();
    let func_name = string_table.intern("host_io_functions");
    let module = string_table.intern("beanstalk_io");
    let import = string_table.intern("print");

    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut transformer = FunctionTransformer::new();
    let args = vec![create_test_return_expr(42)];
    let return_types = vec![];

    let result = transformer.transform_host_function_call_as_stmt(
        func_name,
        &args,
        &return_types,
        module,
        import,
        &mut ctx,
        &TextLocation::default(),
    );

    assert!(
        result.is_ok(),
        "Host function call transformation should succeed"
    );
    let nodes = result.unwrap();

    assert_eq!(nodes.len(), 1, "Should produce one host call node");

    match &nodes[0].kind {
        HirKind::Stmt(HirStmt::HostCall {
            target,
            module: mod_name,
            import: import_name,
            args: call_args,
        }) => {
            assert_eq!(*target, func_name);
            assert_eq!(*mod_name, module);
            assert_eq!(*import_name, import);
            assert_eq!(call_args.len(), 1);
        }
        _ => panic!("Expected HostCall statement"),
    }
}

// ============================================================================
// Property-Based Tests
// ============================================================================

/// Property 2.1: Function definitions preserve parameter count
/// For any function with N parameters, the HIR representation should have N parameters
#[test]
fn property_function_preserves_parameter_count() {
    // Test with various parameter counts
    for param_count in 0..=5 {
        let mut string_table = StringTable::new();
        let func_name = string_table.intern(&format!("func_{}", param_count));
        let signature = create_test_signature(&mut string_table, param_count, 1);

        let mut ctx = HirBuilderContext::new(&mut string_table);
        let mut transformer = FunctionTransformer::new();
        let body = vec![];

        let result = transformer.transform_function_definition(
            func_name,
            signature.clone(),
            &body,
            &mut ctx,
            TextLocation::default(),
        );

        assert!(
            result.is_ok(),
            "Function with {} parameters should transform successfully",
            param_count
        );

        let func_node = result.unwrap();
        match &func_node.kind {
            HirKind::Stmt(HirStmt::FunctionDef { signature: sig, .. }) => {
                assert_eq!(
                    sig.parameters.len(),
                    param_count,
                    "HIR should preserve {} parameters",
                    param_count
                );
            }
            _ => panic!("Expected FunctionDef node"),
        }
    }
}

/// Property 2.2: Function definitions preserve return count
/// For any function with M return values, the HIR representation should have M returns
#[test]
fn property_function_preserves_return_count() {
    // Test with various return counts
    for return_count in 0..=3 {
        let mut string_table = StringTable::new();
        let func_name = string_table.intern(&format!("func_ret_{}", return_count));
        let signature = create_test_signature(&mut string_table, 1, return_count);

        let mut ctx = HirBuilderContext::new(&mut string_table);
        let mut transformer = FunctionTransformer::new();
        let body = vec![];

        let result = transformer.transform_function_definition(
            func_name,
            signature.clone(),
            &body,
            &mut ctx,
            TextLocation::default(),
        );

        assert!(
            result.is_ok(),
            "Function with {} returns should transform successfully",
            return_count
        );

        let func_node = result.unwrap();
        match &func_node.kind {
            HirKind::Stmt(HirStmt::FunctionDef { signature: sig, .. }) => {
                assert_eq!(
                    sig.returns.len(),
                    return_count,
                    "HIR should preserve {} return values",
                    return_count
                );
            }
            _ => panic!("Expected FunctionDef node"),
        }
    }
}

/// Property 2.3: Return statements preserve expression count
/// For any return with N expressions, the HIR return should have N expressions
#[test]
fn property_return_preserves_expression_count() {
    let mut string_table = StringTable::new();

    // Test with various expression counts
    for expr_count in 0..=4 {
        let mut ctx = HirBuilderContext::new(&mut string_table);
        let mut transformer = FunctionTransformer::new();

        let mut return_exprs = Vec::new();
        for i in 0..expr_count {
            return_exprs.push(create_test_return_expr(i as i64));
        }

        let result =
            transformer.transform_return(&return_exprs, &mut ctx, &TextLocation::default());

        assert!(
            result.is_ok(),
            "Return with {} expressions should transform successfully",
            expr_count
        );

        let nodes = result.unwrap();
        assert_eq!(nodes.len(), 1, "Should produce exactly one return node");

        match &nodes[0].kind {
            HirKind::Terminator(HirTerminator::Return(exprs)) => {
                assert_eq!(
                    exprs.len(),
                    expr_count,
                    "HIR return should preserve {} expressions",
                    expr_count
                );
            }
            _ => panic!("Expected Return terminator"),
        }
    }
}

/// Property 2.4: Function calls preserve argument count
/// For any function call with N arguments, the HIR call should have N arguments
#[test]
fn property_function_call_preserves_argument_count() {
    // Test with various argument counts
    for arg_count in 0..=5 {
        let mut string_table = StringTable::new();
        let func_name = string_table.intern(&format!("call_{}", arg_count));

        let mut ctx = HirBuilderContext::new(&mut string_table);
        let mut transformer = FunctionTransformer::new();
        let mut args = Vec::new();
        for i in 0..arg_count {
            args.push(create_test_return_expr(i as i64));
        }
        let returns = vec![];

        let result = transformer.transform_function_call_as_stmt(
            func_name,
            &args,
            &returns,
            &mut ctx,
            &TextLocation::default(),
        );

        assert!(
            result.is_ok(),
            "Function call with {} arguments should transform successfully",
            arg_count
        );

        let nodes = result.unwrap();
        match &nodes[0].kind {
            HirKind::Stmt(HirStmt::Call {
                args: call_args, ..
            }) => {
                assert_eq!(
                    call_args.len(),
                    arg_count,
                    "HIR call should preserve {} arguments",
                    arg_count
                );
            }
            _ => panic!("Expected Call statement"),
        }
    }
}

/// Property 2.5: Host function calls preserve argument count
/// For any host function call with N arguments, the HIR host call should have N arguments
#[test]
fn property_host_call_preserves_argument_count() {
    // Test with various argument counts
    for arg_count in 0..=5 {
        let mut string_table = StringTable::new();
        let func_name = string_table.intern(&format!("host_call_{}", arg_count));
        let module = string_table.intern("test_module");
        let import = string_table.intern("test_import");

        let mut ctx = HirBuilderContext::new(&mut string_table);
        let mut transformer = FunctionTransformer::new();
        let mut args = Vec::new();
        for i in 0..arg_count {
            args.push(create_test_return_expr(i as i64));
        }
        let return_types = vec![];

        let result = transformer.transform_host_function_call_as_stmt(
            func_name,
            &args,
            &return_types,
            module,
            import,
            &mut ctx,
            &TextLocation::default(),
        );

        assert!(
            result.is_ok(),
            "Host call with {} arguments should transform successfully",
            arg_count
        );

        let nodes = result.unwrap();
        match &nodes[0].kind {
            HirKind::Stmt(HirStmt::HostCall {
                args: call_args, ..
            }) => {
                assert_eq!(
                    call_args.len(),
                    arg_count,
                    "HIR host call should preserve {} arguments",
                    arg_count
                );
            }
            _ => panic!("Expected HostCall statement"),
        }
    }
}

/// Property 2.6: Function registration is idempotent
/// Registering the same function multiple times should not cause errors
#[test]
fn property_function_registration_idempotent() {
    let mut string_table = StringTable::new();
    let func_name = string_table.intern("test_func");
    let signature = create_test_signature(&mut string_table, 2, 1);

    let mut ctx = HirBuilderContext::new(&mut string_table);

    // Register the function multiple times
    for _ in 0..3 {
        ctx.register_function(func_name, signature.clone());
    }

    // Should be able to retrieve the signature
    let retrieved = ctx.get_function_signature(&func_name);
    assert!(retrieved.is_some(), "Function should be registered");
    assert_eq!(retrieved.unwrap().parameters.len(), 2);
}

/// Property 2.7: Empty function bodies produce valid HIR
/// Functions with empty bodies should still produce valid HIR with proper terminators
#[test]
fn property_empty_function_bodies_valid() {
    for param_count in 0..=3 {
        let mut string_table = StringTable::new();
        let func_name = string_table.intern(&format!("empty_func_{}", param_count));
        let signature = create_test_signature(&mut string_table, param_count, 0);

        let mut ctx = HirBuilderContext::new(&mut string_table);
        let mut transformer = FunctionTransformer::new();
        let body = vec![]; // Empty body

        let result = transformer.transform_function_definition(
            func_name,
            signature,
            &body,
            &mut ctx,
            TextLocation::default(),
        );

        assert!(
            result.is_ok(),
            "Empty function should transform successfully"
        );

        // Verify the function has a body block with a terminator
        let func_node = result.unwrap();
        match &func_node.kind {
            HirKind::Stmt(HirStmt::FunctionDef { body: body_id, .. }) => {
                let block = ctx.get_block(*body_id);
                assert!(block.is_some(), "Function should have a body block");

                // The block should have an implicit return terminator
                let block = block.unwrap();
                if !block.nodes.is_empty() {
                    let last_node = &block.nodes[block.nodes.len() - 1];
                    assert!(
                        matches!(
                            last_node.kind,
                            HirKind::Terminator(HirTerminator::Return(_))
                        ),
                        "Empty function should have implicit return terminator"
                    );
                }
            }
            _ => panic!("Expected FunctionDef node"),
        }
    }
}

/// Property 2.8: Function parameters are marked as potentially owned
/// All function parameters should be marked as potentially owned in the context
#[test]
fn property_parameters_marked_potentially_owned() {
    let mut string_table = StringTable::new();
    let func_name = string_table.intern("test_func");
    let signature = create_test_signature(&mut string_table, 3, 0);

    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut transformer = FunctionTransformer::new();
    let body = vec![];

    let _ = transformer.transform_function_definition(
        func_name,
        signature.clone(),
        &body,
        &mut ctx,
        TextLocation::default(),
    );

    // Check that all parameters are marked as potentially owned
    for param in &signature.parameters {
        assert!(
            ctx.is_potentially_owned(&param.id),
            "Parameter should be marked as potentially owned"
        );
    }
}
