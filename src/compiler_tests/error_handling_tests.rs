//! Tests for error message validation and error handling
//! 
//! This module tests that the compiler generates appropriate error messages
//! with proper error types and helpful context.

use crate::compiler::compiler_errors::{CompileError, ErrorType};
use crate::compiler::parsers::tokens::TextLocation;
use crate::compiler::mir::build_mir::ast_to_mir;
use crate::compiler::parsers::build_ast::AstBlock;
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind};
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::tokens::VarVisibility;

#[cfg(test)]
mod error_handling_tests {
    use super::*;

    /// Test error type creation
    #[test]
    fn test_error_types() {
        let location = TextLocation::default();
        
        // Test syntax error
        let syntax_error = CompileError::new_syntax_error("Test syntax error".to_string(), location.clone());
        assert_eq!(syntax_error.error_type, ErrorType::Syntax, "Should be syntax error");
        assert!(syntax_error.msg.contains("Test syntax error"), "Should contain error message");
        
        // Test rule error
        let rule_error = CompileError::new_rule_error("Test rule error".to_string(), location.clone());
        assert_eq!(rule_error.error_type, ErrorType::Rule, "Should be rule error");
        assert!(rule_error.msg.contains("Test rule error"), "Should contain error message");
        
        // Test type error
        let type_error = CompileError::new_type_error("Test type error".to_string(), location.clone());
        assert_eq!(type_error.error_type, ErrorType::Type, "Should be type error");
        assert!(type_error.msg.contains("Test type error"), "Should contain error message");
    }

    /// Test compiler error generation
    #[test]
    fn test_compiler_error_handling() {
        let error = CompileError::compiler_error("Test unimplemented feature");
        
        assert_eq!(error.error_type, ErrorType::Compiler, "Should be compiler error");
        assert!(error.msg.contains("Test unimplemented feature"), "Should contain original message");
    }

    /// Test file error generation
    #[test]
    fn test_file_error_handling() {
        let test_path = std::path::Path::new("test.bst");
        let error = CompileError::file_error(test_path, "File not found");
        
        assert_eq!(error.error_type, ErrorType::File, "Should be file error");
        assert!(error.msg.contains("File not found"), "Should contain error message");
        assert_eq!(error.file_path, test_path, "Should contain file path");
    }

    /// Test error message quality
    #[test]
    fn test_error_message_quality() {
        let location = TextLocation::default();
        let error = CompileError::new_syntax_error("Expected semicolon".to_string(), location);
        
        // Error messages should be descriptive
        assert!(!error.msg.is_empty(), "Error message should not be empty");
        assert!(error.msg.len() > 5, "Error message should be descriptive");
        
        // Error messages should not contain internal details
        assert!(!error.msg.contains("panic"), "Error message should not mention panic");
        assert!(!error.msg.contains("unwrap"), "Error message should not mention unwrap");
    }

    /// Test error macro usage patterns
    #[test]
    fn test_error_macro_patterns() {
        // Test that we can create different error types using the macros
        // This tests the error handling infrastructure without depending on MIR
        
        let location = TextLocation::default();
        
        // Test rule error creation
        let rule_error = CompileError::new_rule_error(
            "Test rule violation with context".to_string(), 
            location.clone()
        );
        assert_eq!(rule_error.error_type, ErrorType::Rule);
        assert!(rule_error.msg.contains("rule violation"));
        
        // Test type error creation
        let type_error = CompileError::new_type_error(
            "Test type mismatch with details".to_string(), 
            location.clone()
        );
        assert_eq!(type_error.error_type, ErrorType::Type);
        assert!(type_error.msg.contains("type mismatch"));
        
        // Test syntax error creation
        let syntax_error = CompileError::new_syntax_error(
            "Test syntax error with location".to_string(), 
            location
        );
        assert_eq!(syntax_error.error_type, ErrorType::Syntax);
        assert!(syntax_error.msg.contains("syntax error"));
    }

    /// Test error message quality and descriptiveness
    #[test]
    fn test_descriptive_error_messages() {
        let location = TextLocation::default();
        
        // Test that error messages are descriptive and helpful
        let descriptive_error = CompileError::new_rule_error(
            "Variable 'test_var' is already declared in this scope. Shadowing is not supported in Beanstalk - each variable name can only be used once per scope. Try using a different name like 'test_var_2' or 'test_var_new'.".to_string(),
            location.clone()
        );
        
        // Check that the error message is descriptive
        assert!(descriptive_error.msg.len() > 50, "Error message should be descriptive");
        assert!(descriptive_error.msg.contains("test_var"), "Should mention specific variable");
        assert!(descriptive_error.msg.contains("Shadowing is not supported"), "Should explain the rule");
        assert!(descriptive_error.msg.contains("Try using"), "Should provide suggestions");
        
        // Test compiler error with helpful context
        let compiler_error = CompileError::compiler_error(
            "Variable references in expressions not yet implemented for variable 'my_var' at line 42, column 15. This feature is coming soon - for now, try using the variable directly in assignments."
        );
        
        assert_eq!(compiler_error.error_type, ErrorType::Compiler);
        assert!(compiler_error.msg.contains("my_var"), "Should mention specific variable");
        assert!(compiler_error.msg.contains("line 42"), "Should include location");
        assert!(compiler_error.msg.contains("coming soon"), "Should provide context");
        assert!(compiler_error.msg.contains("try using"), "Should provide workaround");
    }

    /// Test error message formatting and consistency
    #[test]
    fn test_error_message_formatting() {
        let location = TextLocation::default();
        
        // Test that error messages follow consistent patterns
        let rule_error = CompileError::new_rule_error("Test rule violation".to_string(), location.clone());
        let type_error = CompileError::new_type_error("Test type mismatch".to_string(), location.clone());
        let syntax_error = CompileError::new_syntax_error("Test syntax issue".to_string(), location);
        
        // All error messages should be properly formatted
        assert!(!rule_error.msg.is_empty(), "Rule error message should not be empty");
        assert!(!type_error.msg.is_empty(), "Type error message should not be empty");
        assert!(!syntax_error.msg.is_empty(), "Syntax error message should not be empty");
        
        // Error messages should be helpful
        assert!(rule_error.msg.len() > 10, "Rule error should be descriptive");
        assert!(type_error.msg.len() > 10, "Type error should be descriptive");
        assert!(syntax_error.msg.len() > 10, "Syntax error should be descriptive");
    }

    /// Test source location preservation in errors
    #[test]
    fn test_source_location_preservation() {
        let mut location = TextLocation::default();
        location.start_pos.line_number = 42;
        location.start_pos.char_column = 15;
        
        let error = CompileError::new_rule_error("Test error with location".to_string(), location.clone());
        
        // Location should be preserved
        assert_eq!(error.location.start_pos.line_number, 42, "Line number should be preserved");
        assert_eq!(error.location.start_pos.char_column, 15, "Column number should be preserved");
    }

    /// Test that error handling doesn't panic
    #[test]
    fn test_error_handling_robustness() {
        let location = TextLocation::default();
        
        // Test with empty strings
        let empty_error = CompileError::new_rule_error("".to_string(), location.clone());
        assert_eq!(empty_error.error_type, ErrorType::Rule, "Should handle empty messages");
        
        // Test with very long strings
        let long_message = "a".repeat(1000);
        let long_error = CompileError::new_syntax_error(long_message.clone(), location);
        assert_eq!(long_error.error_type, ErrorType::Syntax, "Should handle long messages");
        assert!(long_error.msg.contains(&long_message), "Should preserve long messages");
    }
}