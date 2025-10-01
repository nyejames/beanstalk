//! Tests for error message validation and error handling
//! 
//! This module tests that the compiler generates appropriate error messages
//! with proper error types and helpful context.

use crate::compiler::compiler_errors::{CompileError, ErrorType};
use crate::compiler::parsers::tokens::TextLocation;

#[cfg(test)]
mod error_handling_tests {
    use super::*;

    /// Test error type creation
    #[test]
    fn test_error_types() {
        let location = TextLocation::default();
        
        // Test syntax error
        let syntax_error = CompileError::syntax_error(location.clone(), "Test syntax error");
        assert_eq!(syntax_error.error_type, ErrorType::Syntax, "Should be syntax error");
        assert!(syntax_error.msg.contains("Test syntax error"), "Should contain error message");
        
        // Test rule error
        let rule_error = CompileError::rule_error(location.clone(), "Test rule error");
        assert_eq!(rule_error.error_type, ErrorType::Rule, "Should be rule error");
        assert!(rule_error.msg.contains("Test rule error"), "Should contain error message");
        
        // Test type error
        let type_error = CompileError::type_error(location.clone(), "Test type error");
        assert_eq!(type_error.error_type, ErrorType::Type, "Should be type error");
        assert!(type_error.msg.contains("Test type error"), "Should contain error message");
    }

    /// Test compiler error generation
    #[test]
    fn test_compiler_error_handling() {
        let error = CompileError::compiler_error("Test unimplemented feature");
        
        assert_eq!(error.error_type, ErrorType::Compiler, "Should be compiler error");
        assert!(error.msg.contains("COMPILER BUG"), "Compiler errors should be prefixed");
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
        let error = CompileError::syntax_error(location, "Expected semicolon");
        
        // Error messages should be descriptive
        assert!(!error.msg.is_empty(), "Error message should not be empty");
        assert!(error.msg.len() > 5, "Error message should be descriptive");
        
        // Error messages should not contain internal details
        assert!(!error.msg.contains("panic"), "Error message should not mention panic");
        assert!(!error.msg.contains("unwrap"), "Error message should not mention unwrap");
    }
}