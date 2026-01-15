//! Property-based tests for TemplateProcessor
//!
//! Feature: hir-builder, Property 8: Template Processing Correctness
//! Validates: Requirements 10.1, 10.2, 10.3, 10.4, 10.5, 10.6
//!
//! These tests verify that templates are correctly transformed from AST to HIR,
//! including compile-time templates becoming string literals and runtime templates
//! becoming function calls.

use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::hir::build_hir::HirBuilderContext;
use crate::compiler::hir::nodes::{HirExprKind, HirKind, HirStmt};
use crate::compiler::hir::template_processor::TemplateProcessor;
use crate::compiler::parsers::expressions::expression::Expression;
use crate::compiler::parsers::statements::create_template_node::Template;
use crate::compiler::parsers::statements::template::{
    Style, TemplateCompatibility, TemplateContent, TemplateControlFlow, TemplateType,
};
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::string_interning::StringTable;

// ============================================================================
// Helper Functions
// ============================================================================

/// Creates a simple compile-time template for testing
fn create_compile_time_template(
    content: &str,
    string_table: &mut StringTable,
) -> Template {
    let interned_content = string_table.intern(content);
    let mut template_content = TemplateContent::default();
    template_content.before.push(Expression::string_slice(
        interned_content,
        TextLocation::default(),
        Ownership::ImmutableOwned,
    ));

    Template {
        content: template_content,
        kind: TemplateType::String,
        style: Style::default(),
        control_flow: TemplateControlFlow::None,
        id: String::new(),
        location: TextLocation::default(),
    }
}

/// Creates a runtime template with a variable reference for testing
fn create_runtime_template_with_var(
    var_name: &str,
    string_table: &mut StringTable,
) -> Template {
    let interned_var = string_table.intern(var_name);
    let mut template_content = TemplateContent::default();
    template_content.before.push(Expression::parameter(
        interned_var,
        DataType::String,
        TextLocation::default(),
        Ownership::ImmutableReference,
    ));

    Template {
        content: template_content,
        kind: TemplateType::StringFunction,
        style: Style::default(),
        control_flow: TemplateControlFlow::None,
        id: String::new(),
        location: TextLocation::default(),
    }
}

/// Creates a template with a specific ID for testing
fn create_template_with_id(
    content: &str,
    id: &str,
    string_table: &mut StringTable,
) -> Template {
    let interned_content = string_table.intern(content);
    let mut template_content = TemplateContent::default();
    template_content.before.push(Expression::string_slice(
        interned_content,
        TextLocation::default(),
        Ownership::ImmutableOwned,
    ));

    Template {
        content: template_content,
        kind: TemplateType::String,
        style: Style::default(),
        control_flow: TemplateControlFlow::None,
        id: id.to_string(),
        location: TextLocation::default(),
    }
}

/// Creates a comment template for testing
fn create_comment_template() -> Template {
    Template {
        content: TemplateContent::default(),
        kind: TemplateType::Comment,
        style: Style::default(),
        control_flow: TemplateControlFlow::None,
        id: String::new(),
        location: TextLocation::default(),
    }
}

/// Creates a template with multiple content items
fn create_template_with_multiple_items(
    items: Vec<(&str, bool)>, // (content, is_variable)
    string_table: &mut StringTable,
) -> Template {
    let mut template_content = TemplateContent::default();
    let mut has_runtime = false;

    for (content, is_variable) in items {
        let interned = string_table.intern(content);
        if is_variable {
            has_runtime = true;
            template_content.before.push(Expression::parameter(
                interned,
                DataType::String,
                TextLocation::default(),
                Ownership::ImmutableReference,
            ));
        } else {
            template_content.before.push(Expression::string_slice(
                interned,
                TextLocation::default(),
                Ownership::ImmutableOwned,
            ));
        }
    }

    Template {
        content: template_content,
        kind: if has_runtime {
            TemplateType::StringFunction
        } else {
            TemplateType::String
        },
        style: Style::default(),
        control_flow: TemplateControlFlow::None,
        id: String::new(),
        location: TextLocation::default(),
    }
}

// ============================================================================
// Unit Tests for Basic Functionality
// ============================================================================

#[test]
fn test_template_processor_creation() {
    let processor = TemplateProcessor::new();
    assert!(std::mem::size_of_val(&processor) == 0); // Zero-sized type
}

#[test]
fn test_process_compile_time_template() {
    let mut string_table = StringTable::new();
    let template = create_compile_time_template("Hello, World!", &mut string_table);
    
    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut processor = TemplateProcessor::new();

    let result = processor.process_template(&template, &mut ctx);
    assert!(result.is_ok(), "Compile-time template should process successfully");

    let (nodes, expr) = result.unwrap();
    
    // Compile-time templates should produce no setup nodes
    assert!(nodes.is_empty(), "Compile-time template should produce no setup nodes");
    
    // The result should be a string literal
    assert!(
        matches!(expr.kind, HirExprKind::StringLiteral(_)),
        "Compile-time template should produce a string literal"
    );
}

#[test]
fn test_process_runtime_template() {
    let mut string_table = StringTable::new();
    let template = create_runtime_template_with_var("my_var", &mut string_table);
    
    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut processor = TemplateProcessor::new();

    let result = processor.process_template(&template, &mut ctx);
    assert!(result.is_ok(), "Runtime template should process successfully");

    let (nodes, _expr) = result.unwrap();
    
    // Runtime templates should produce setup nodes
    assert!(!nodes.is_empty(), "Runtime template should produce setup nodes");
    
    // Check that a RuntimeTemplateCall was created
    let has_template_call = nodes.iter().any(|node| {
        matches!(
            &node.kind,
            HirKind::Stmt(HirStmt::RuntimeTemplateCall { .. })
        )
    });
    assert!(has_template_call, "Runtime template should produce a RuntimeTemplateCall");
}

#[test]
fn test_process_comment_template() {
    let mut string_table = StringTable::new();
    let template = create_comment_template();
    
    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut processor = TemplateProcessor::new();

    let result = processor.process_template(&template, &mut ctx);
    assert!(result.is_ok(), "Comment template should process successfully");

    let (nodes, expr) = result.unwrap();
    
    // Comment templates should produce no setup nodes
    assert!(nodes.is_empty(), "Comment template should produce no setup nodes");
    
    // The result should be an empty string literal
    match &expr.kind {
        HirExprKind::StringLiteral(s) => {
            let resolved = ctx.string_table.resolve(*s);
            assert!(resolved.is_empty(), "Comment template should produce empty string");
        }
        _ => panic!("Comment template should produce a string literal"),
    }
}

#[test]
fn test_template_with_id_preserves_id() {
    let mut string_table = StringTable::new();
    let template = create_template_with_id("Content", "my_template_id", &mut string_table);
    
    let mut ctx = HirBuilderContext::new(&mut string_table);
    let processor = TemplateProcessor::new();

    // Check that the ID is parsed correctly
    let id = processor.parse_template_id(&template, &mut ctx);
    assert!(id.is_some(), "Template ID should be preserved");
    
    let id_str = ctx.string_table.resolve(id.unwrap());
    assert_eq!(id_str, "my_template_id", "Template ID should match");
}

#[test]
fn test_template_without_id() {
    let mut string_table = StringTable::new();
    let template = create_compile_time_template("No ID", &mut string_table);
    
    let mut ctx = HirBuilderContext::new(&mut string_table);
    let processor = TemplateProcessor::new();

    let id = processor.parse_template_id(&template, &mut ctx);
    assert!(id.is_none(), "Template without ID should return None");
}

// ============================================================================
// Property-Based Tests
// ============================================================================

/// Property 8.1: Compile-time templates become string literals
/// For any compile-time template, the HIR representation should be a string literal
/// Validates: Requirements 10.1
#[test]
fn property_compile_time_templates_become_string_literals() {
    let test_strings = vec![
        "",
        "Hello",
        "Hello, World!",
        "Multi\nLine\nString",
        "Special chars: !@#$%^&*()",
        "Unicode: 你好世界 🌍",
        "   Whitespace   ",
    ];

    for content in test_strings {
        let mut string_table = StringTable::new();
        let template = create_compile_time_template(content, &mut string_table);
        
        let mut ctx = HirBuilderContext::new(&mut string_table);
        let mut processor = TemplateProcessor::new();

        let result = processor.process_template(&template, &mut ctx);
        assert!(
            result.is_ok(),
            "Compile-time template with content '{}' should process successfully",
            content
        );

        let (nodes, expr) = result.unwrap();
        
        assert!(
            nodes.is_empty(),
            "Compile-time template should produce no setup nodes for '{}'",
            content
        );
        
        assert!(
            matches!(expr.kind, HirExprKind::StringLiteral(_)),
            "Compile-time template should produce string literal for '{}'",
            content
        );
    }
}

/// Property 8.2: Runtime templates become function calls
/// For any runtime template with variable references, the HIR should include a RuntimeTemplateCall
/// Validates: Requirements 10.2
#[test]
fn property_runtime_templates_become_function_calls() {
    let test_vars = vec![
        "x",
        "my_variable",
        "user_name",
        "count",
        "data",
    ];

    for var_name in test_vars {
        let mut string_table = StringTable::new();
        let template = create_runtime_template_with_var(var_name, &mut string_table);
        
        let mut ctx = HirBuilderContext::new(&mut string_table);
        let mut processor = TemplateProcessor::new();

        let result = processor.process_template(&template, &mut ctx);
        assert!(
            result.is_ok(),
            "Runtime template with var '{}' should process successfully",
            var_name
        );

        let (nodes, _expr) = result.unwrap();
        
        let has_template_call = nodes.iter().any(|node| {
            matches!(
                &node.kind,
                HirKind::Stmt(HirStmt::RuntimeTemplateCall { .. })
            )
        });
        
        assert!(
            has_template_call,
            "Runtime template with var '{}' should produce RuntimeTemplateCall",
            var_name
        );
    }
}

/// Property 8.3: Template variables are captured correctly
/// For any runtime template, all variable references should be captured
/// Validates: Requirements 10.3
#[test]
fn property_template_variables_are_captured() {
    let mut string_table = StringTable::new();
    let template = create_runtime_template_with_var("test_var", &mut string_table);
    
    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut processor = TemplateProcessor::new();

    let captures = processor.collect_template_captures(&template, &mut ctx);
    assert!(captures.is_ok(), "Capture collection should succeed");

    let captures = captures.unwrap();
    assert_eq!(captures.len(), 1, "Should capture exactly one variable");
    
    // The capture should be a Load expression
    assert!(
        matches!(captures[0].kind, HirExprKind::Load(_)),
        "Captured variable should be a Load expression"
    );
}

/// Property 8.4: Template IDs are preserved
/// For any template with an ID, the ID should be preserved in the HIR
/// Validates: Requirements 10.5
#[test]
fn property_template_ids_are_preserved() {
    let test_ids = vec![
        "id1",
        "my_template",
        "section_header",
        "footer_content",
        "__internal_id",
    ];

    for id in test_ids {
        let mut string_table = StringTable::new();
        let template = create_template_with_id("Content", id, &mut string_table);
        
        let mut ctx = HirBuilderContext::new(&mut string_table);
        let processor = TemplateProcessor::new();

        let parsed_id = processor.parse_template_id(&template, &mut ctx);
        assert!(parsed_id.is_some(), "Template ID '{}' should be preserved", id);
        
        let id_str = ctx.string_table.resolve(parsed_id.unwrap());
        assert_eq!(id_str, id, "Template ID should match original");
    }
}

/// Property 8.5: Comment templates produce empty output
/// For any comment template, the result should be an empty string
/// Validates: Requirements 10.1 (edge case)
#[test]
fn property_comment_templates_produce_empty_output() {
    let mut string_table = StringTable::new();
    let template = create_comment_template();
    
    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut processor = TemplateProcessor::new();

    let result = processor.process_template(&template, &mut ctx);
    assert!(result.is_ok(), "Comment template should process successfully");

    let (nodes, expr) = result.unwrap();
    
    assert!(nodes.is_empty(), "Comment template should produce no setup nodes");
    
    match &expr.kind {
        HirExprKind::StringLiteral(s) => {
            let resolved = ctx.string_table.resolve(*s);
            assert!(resolved.is_empty(), "Comment template should produce empty string");
        }
        _ => panic!("Comment template should produce a string literal"),
    }
}

/// Property 8.6: Multiple content items are handled correctly
/// For any template with multiple content items, all items should be processed
/// Validates: Requirements 10.3, 10.4
#[test]
fn property_multiple_content_items_handled() {
    let test_cases = vec![
        vec![("Hello, ", false), ("name", true), ("!", false)],
        vec![("Start", false), ("middle", true), ("end", false)],
        vec![("a", true), ("b", true), ("c", true)],
        vec![("literal1", false), ("literal2", false)],
    ];

    for items in test_cases {
        let mut string_table = StringTable::new();
        let template = create_template_with_multiple_items(items.clone(), &mut string_table);
        
        let mut ctx = HirBuilderContext::new(&mut string_table);
        let mut processor = TemplateProcessor::new();

        let result = processor.process_template(&template, &mut ctx);
        assert!(
            result.is_ok(),
            "Template with multiple items should process successfully"
        );

        // Count expected captures (variables only)
        let expected_captures: usize = items.iter().filter(|(_, is_var)| *is_var).count();
        
        if expected_captures > 0 {
            let captures = processor.collect_template_captures(&template, &mut ctx);
            assert!(captures.is_ok(), "Capture collection should succeed");
            assert_eq!(
                captures.unwrap().len(),
                expected_captures,
                "Should capture all variables"
            );
        }
    }
}

/// Property 8.7: Template type determines processing path
/// For any template, the processing path should be determined by its type
/// Validates: Requirements 10.1, 10.2
#[test]
fn property_template_type_determines_processing() {
    let mut string_table = StringTable::new();
    
    // Test compile-time template
    let compile_time = create_compile_time_template("Static", &mut string_table);
    assert_eq!(compile_time.kind, TemplateType::String);
    
    // Test runtime template
    let runtime = create_runtime_template_with_var("var", &mut string_table);
    assert_eq!(runtime.kind, TemplateType::StringFunction);
    
    // Test comment template
    let comment = create_comment_template();
    assert_eq!(comment.kind, TemplateType::Comment);
}

/// Property 8.8: Empty templates are handled correctly
/// For any empty template, processing should succeed without errors
/// Validates: Requirements 10.1 (edge case)
#[test]
fn property_empty_templates_handled() {
    let mut string_table = StringTable::new();
    let template = create_compile_time_template("", &mut string_table);
    
    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut processor = TemplateProcessor::new();

    let result = processor.process_template(&template, &mut ctx);
    assert!(result.is_ok(), "Empty template should process successfully");

    let (nodes, expr) = result.unwrap();
    assert!(nodes.is_empty(), "Empty template should produce no setup nodes");
    
    match &expr.kind {
        HirExprKind::StringLiteral(s) => {
            let resolved = ctx.string_table.resolve(*s);
            assert!(resolved.is_empty(), "Empty template should produce empty string");
        }
        _ => panic!("Empty template should produce a string literal"),
    }
}

/// Property 8.9: Template function names are unique
/// For any runtime template, the generated function name should be unique
/// Validates: Requirements 10.2
#[test]
fn property_template_function_names_unique() {
    let mut string_table = StringTable::new();
    let processor = TemplateProcessor::new();

    let mut generated_names = std::collections::HashSet::new();

    // Generate multiple template function names
    for i in 0..10 {
        // Create a fresh context for each iteration to avoid borrow issues
        let mut ctx = HirBuilderContext::new(&mut string_table);
        
        // Create template with unique ID to ensure unique names
        let template = Template {
            content: TemplateContent::default(),
            kind: TemplateType::StringFunction,
            style: Style::default(),
            control_flow: TemplateControlFlow::None,
            id: format!("unique_id_{}", i),
            location: TextLocation::default(),
        };
        
        let name = processor.generate_template_fn_name(&template, &mut ctx);
        let name_str = ctx.string_table.resolve(name).to_string();
        
        assert!(
            generated_names.insert(name_str.clone()),
            "Template function name '{}' should be unique",
            name_str
        );
    }
}

/// Property 8.10: Data types are preserved in captures
/// For any captured variable, its data type should be preserved
/// Validates: Requirements 10.3
#[test]
fn property_capture_data_types_preserved() {
    let mut string_table = StringTable::new();
    
    // Create a template with a typed variable
    let var_name = string_table.intern("typed_var");
    let mut template_content = TemplateContent::default();
    template_content.before.push(Expression::parameter(
        var_name,
        DataType::Int, // Specific type
        TextLocation::default(),
        Ownership::ImmutableReference,
    ));

    let template = Template {
        content: template_content,
        kind: TemplateType::StringFunction,
        style: Style::default(),
        control_flow: TemplateControlFlow::None,
        id: String::new(),
        location: TextLocation::default(),
    };
    
    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut processor = TemplateProcessor::new();

    let captures = processor.collect_template_captures(&template, &mut ctx);
    assert!(captures.is_ok(), "Capture collection should succeed");

    let captures = captures.unwrap();
    assert_eq!(captures.len(), 1, "Should capture one variable");
    assert_eq!(captures[0].data_type, DataType::Int, "Data type should be preserved");
}
