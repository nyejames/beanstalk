use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind, Arg};
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::tokens::{TextLocation, VarVisibility};
use crate::compiler::datatypes::{DataType, OwnershipRule};
use crate::compiler::codegen::ir_emitter::CodeGenContext;

#[test]
fn test_simple_function_lowering() {
    // Create a simple function: add_two |x Int| -> Int: return x + 2;
    
    // Create the expression for x + 2 (simplified for this test)
    let x_expression = Expression::int(5, TextLocation::default()); // Just use a constant for now
    
    // Create the return statement
    let return_node = AstNode {
        kind: NodeKind::Return(vec![x_expression]),
        location: TextLocation::default(),
        scope: std::path::PathBuf::new(),
        lifetime: 0,
    };
    
    // Create the function body
    let function_body = vec![return_node];
    
    // Create the function argument
    let arg = Arg {
        name: "x".to_string(),
        value: Expression::int(0, TextLocation::default()), // Placeholder value
    };
    
    // Create the function expression
    let function_expression = Expression::function(
        vec![arg],
        crate::compiler::parsers::build_ast::AstBlock { 
            ast: function_body,
            scope: std::path::PathBuf::new(),
            is_entry_point: false,
        },
        vec![DataType::Int(OwnershipRule::ImmutableOwned)],
        TextLocation::default(),
    );
    
    // Lower the function
    let mut codegen = CodeGenContext::new();
    let result = codegen.lower_function(&function_expression);
    
    assert!(result.is_ok());
    
    let function = result.unwrap();
    println!("Generated function:\n{}", function.to_string());
    
    // Verify the function has the expected structure
    // Check signature parameters and returns
    assert_eq!(function.signature.params.len(), 1);
    assert_eq!(function.signature.returns.len(), 1);
}

#[test]
fn test_function_with_multiple_returns() {
    // Test a function that returns multiple values
    let return_node = AstNode {
        kind: NodeKind::Return(vec![
            Expression::int(42, TextLocation::default()),
            Expression::string("hello".to_string(), TextLocation::default()),
        ]),
        location: TextLocation::default(),
        scope: std::path::PathBuf::new(),
        lifetime: 0,
    };
    
    let function_body = vec![return_node];
    
    let function_expression = Expression::function(
        vec![],
        crate::compiler::parsers::build_ast::AstBlock { 
            ast: function_body,
            scope: std::path::PathBuf::new(),
            is_entry_point: false,
        },
        vec![
            DataType::Int(OwnershipRule::ImmutableOwned),
            DataType::String(OwnershipRule::ImmutableOwned),
        ],
        TextLocation::default(),
    );
    
    let mut codegen = CodeGenContext::new();
    let result = codegen.lower_function(&function_expression);
    
    assert!(result.is_ok());
    
    let function = result.unwrap();
    assert_eq!(function.signature.returns.len(), 2);
}

#[test]
fn test_variable_declaration_lowering() {
    // Test lowering a variable declaration
    let declaration_node = AstNode {
        kind: NodeKind::Declaration(
            "my_var".to_string(),
            Expression::int(123, TextLocation::default()),
            VarVisibility::Public,
        ),
        location: TextLocation::default(),
        scope: std::path::PathBuf::new(),
        lifetime: 0,
    };
    
    let function_body = vec![declaration_node];
    
    let function_expression = Expression::function(
        vec![],
        crate::compiler::parsers::build_ast::AstBlock { 
            ast: function_body,
            scope: std::path::PathBuf::new(),
            is_entry_point: false,
        },
        vec![],
        TextLocation::default(),
    );
    
    let mut codegen = CodeGenContext::new();
    let result = codegen.lower_function(&function_expression);
    
    assert!(result.is_ok());
    
    // Function was successfully generated
    assert!(result.is_ok());
}

#[test]
fn test_if_statement_lowering() {
    // Test lowering an if statement
    let if_node = AstNode {
        kind: NodeKind::If(
            Expression::bool(true, TextLocation::default()),
            crate::compiler::parsers::build_ast::AstBlock { 
                ast: vec![AstNode {
                    kind: NodeKind::Return(vec![Expression::int(1, TextLocation::default())]),
                    location: TextLocation::default(),
                    scope: std::path::PathBuf::new(),
                    lifetime: 0,
                }],
                scope: std::path::PathBuf::new(),
                is_entry_point: false,
            },
        ),
        location: TextLocation::default(),
        scope: std::path::PathBuf::new(),
        lifetime: 0,
    };
    
    let function_body = vec![if_node];
    
    let function_expression = Expression::function(
        vec![],
        crate::compiler::parsers::build_ast::AstBlock { 
            ast: function_body,
            scope: std::path::PathBuf::new(),
            is_entry_point: false,
        },
        vec![],
        TextLocation::default(),
    );
    
    let mut codegen = CodeGenContext::new();
    let result = codegen.lower_function(&function_expression);
    
    assert!(result.is_ok());
    
    // Function was successfully generated
    assert!(result.is_ok());
}

#[test]
fn test_expression_lowering() {
    // Test lowering different types of expressions
    let expressions = vec![
        Expression::int(42, TextLocation::default()),
        Expression::float(3.14, TextLocation::default()),
        Expression::bool(true, TextLocation::default()),
        Expression::string("test".to_string(), TextLocation::default()),
        Expression::none(),
    ];
    
    for expression in expressions {
        let expression_kind = format!("{:?}", expression.kind);
        let return_node = AstNode {
            kind: NodeKind::Return(vec![expression.clone()]),
            location: TextLocation::default(),
            scope: std::path::PathBuf::new(),
            lifetime: 0,
        };
        
        let function_expression = Expression::function(
            vec![],
            crate::compiler::parsers::build_ast::AstBlock { 
                ast: vec![return_node],
                scope: std::path::PathBuf::new(),
                is_entry_point: false,
            },
            vec![],
            TextLocation::default(),
        );
        
        let mut codegen = CodeGenContext::new();
        let result = codegen.lower_function(&function_expression);
        
        assert!(result.is_ok(), "Failed to lower expression: {}", expression_kind);
    }
} 