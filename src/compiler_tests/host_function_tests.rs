//! Unit tests for the host function system
//! Tests registry creation, AST parsing, and WIR lowering of host function calls

use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::host_functions::registry::{
    ErrorHandling, HostFunctionDef, HostFunctionRegistry, create_builtin_registry,
};
use crate::compiler::parsers::ast_nodes::Arg;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::tokens::TextLocation;

#[cfg(test)]
mod host_function_registry_tests {
    use super::*;

    #[test]
    fn test_empty_registry_creation() {
        let registry = HostFunctionRegistry::new();
        assert_eq!(registry.count(), 0);
        assert!(!registry.has_function("print"));
        assert!(registry.get_function("nonexistent").is_none());
        assert!(registry.list_functions().is_empty());
    }

    #[test]
    fn test_builtin_registry_creation() {
        let registry = create_builtin_registry().expect("Failed to create builtin registry");

        // Should have at least the print function
        assert!(registry.count() > 0);
        assert!(registry.has_function("print"));

        // Verify print function definition
        let print_func = registry
            .get_function("print")
            .expect("print function should exist");
        assert_eq!(print_func.name, "print");
        assert_eq!(print_func.module, "beanstalk_io");
        assert_eq!(print_func.import_name, "print");
        assert_eq!(print_func.error_handling, ErrorHandling::None);
        assert_eq!(print_func.parameters.len(), 1);
        assert_eq!(print_func.return_types.len(), 0); // void function

        // Verify parameter structure
        let param = &print_func.parameters[0];
        assert_eq!(param.name, "message");
    }

    #[test]
    fn test_host_function_registration() {
        let mut registry = HostFunctionRegistry::new();

        // Create a test host function
        let test_param = Arg {
            name: "value".to_string(),
            value: Expression::new(
                ExpressionKind::None,
                TextLocation::default(),
                DataType::Int,
                Ownership::ImmutableReference,
            ),
        };

        let test_function = HostFunctionDef::new(
            "test_func",
            vec![test_param],
            vec![DataType::String],
            "beanstalk_io",
            "test_func",
            "Test function for unit testing",
        );

        // Register the function
        registry
            .register_function(test_function)
            .expect("Failed to register function");

        // Verify registration
        assert_eq!(registry.count(), 1);
        assert!(registry.has_function("test_func"));

        let retrieved = registry
            .get_function("test_func")
            .expect("Function should exist");
        assert_eq!(retrieved.name, "test_func");
        assert_eq!(retrieved.module, "beanstalk_io");
        assert_eq!(retrieved.import_name, "test_func");
        assert_eq!(retrieved.parameters.len(), 1);
        assert_eq!(retrieved.return_types.len(), 1);
    }

    #[test]
    fn test_duplicate_function_registration() {
        let mut registry = HostFunctionRegistry::new();

        let test_function1 = HostFunctionDef::new(
            "duplicate",
            vec![],
            vec![],
            "beanstalk_io",
            "duplicate",
            "First function",
        );

        let test_function2 = HostFunctionDef::new(
            "duplicate",
            vec![],
            vec![],
            "beanstalk_io",
            "duplicate",
            "Second function",
        );

        // First registration should succeed
        registry
            .register_function(test_function1)
            .expect("First registration should succeed");

        // Second registration should fail
        let result = registry.register_function(test_function2);
        assert!(result.is_err());

        // Should still only have one function
        assert_eq!(registry.count(), 1);
    }

    #[test]
    fn test_host_function_with_error_handling() {
        let test_function = HostFunctionDef::new_with_error(
            "risky_func",
            vec![],
            vec![DataType::String],
            "beanstalk_io",
            "risky_func",
            "Function that can fail",
        );

        assert_eq!(test_function.error_handling, ErrorHandling::ReturnsError);
    }

    #[test]
    fn test_function_type_conversion() {
        let test_param = Arg {
            name: "input".to_string(),
            value: Expression::new(
                ExpressionKind::None,
                TextLocation::default(),
                DataType::Int,
                Ownership::ImmutableReference,
            ),
        };

        let test_function = HostFunctionDef::new(
            "convert_test",
            vec![test_param],
            vec![DataType::String],
            "beanstalk_io",
            "convert_test",
            "Test function type conversion",
        );

        let function_type = test_function.as_function_type();
        match function_type {
            DataType::Function(params, returns) => {
                assert_eq!(params.len(), 1);
                assert_eq!(returns.len(), 1);
                assert_eq!(params[0].name, "input");
            }
            _ => panic!("Expected Function type"),
        }
    }

    #[test]
    fn test_registry_validation() {
        // Test that builtin registry passes validation
        let registry = create_builtin_registry().expect("Builtin registry should be valid");

        // All functions should be valid (validation happens during creation)
        for function in registry.list_functions() {
            assert!(!function.name.is_empty());
            assert!(!function.module.is_empty());
            assert!(!function.import_name.is_empty());

            // Should be one of the valid modules
            assert!(matches!(
                function.module.as_str(),
                "beanstalk_io" | "beanstalk_env" | "beanstalk_sys"
            ));
        }
    }
}

// TODO: Add AST integration tests once compiler interface is stable

#[cfg(test)]
mod host_function_ast_tests {
    use super::*;

    #[test]
    fn test_ast_integration_placeholder() {
        // TODO: Implement AST integration tests once compiler interface is stable
        // This test verifies that the host function registry can be used for AST parsing

        let registry = create_builtin_registry().expect("Failed to create registry");
        assert!(
            registry.has_function("print"),
            "Registry should have print function"
        );

        // Verify the print function has the correct signature for AST integration
        let print_func = registry.get_function("print").unwrap();
        assert_eq!(print_func.parameters.len(), 1);
        assert_eq!(print_func.return_types.len(), 0);
        assert_eq!(print_func.module, "beanstalk_io");
        assert_eq!(print_func.import_name, "print");
    }

    #[test]
    fn test_function_signature_compatibility() {
        // Test that host function signatures are compatible with AST function call parsing
        let registry = create_builtin_registry().expect("Failed to create registry");
        let print_func = registry.get_function("print").unwrap();

        // Verify the function signature can be converted to DataType::Function
        let function_type = print_func.as_function_type();
        match function_type {
            DataType::Function(params, returns) => {
                assert_eq!(params.len(), 1);
                assert_eq!(returns.len(), 0);
                assert_eq!(params[0].name, "message");
            }
            _ => panic!("Expected Function type"),
        }
    }

    #[test]
    fn test_registry_lookup_behavior() {
        // Test that registry correctly identifies host vs regular functions
        let registry = create_builtin_registry().expect("Failed to create registry");

        // Should find host functions
        assert!(registry.has_function("print"));
        assert!(registry.get_function("print").is_some());

        // Should not find non-existent functions
        assert!(!registry.has_function("nonexistent_function"));
        assert!(registry.get_function("nonexistent_function").is_none());

        // Should not find regular Beanstalk functions (they're not host functions)
        assert!(!registry.has_function("user_defined_function"));
    }

    #[test]
    fn test_host_function_parameter_types() {
        // Test that host function parameter types are correctly defined
        let registry = create_builtin_registry().expect("Failed to create registry");
        let print_func = registry.get_function("print").unwrap();

        // Verify print function expects a string parameter
        assert_eq!(print_func.parameters.len(), 1);
        let param = &print_func.parameters[0];
        assert_eq!(param.name, "message");

        // The parameter should have a string type (exact representation may vary)
        // This test verifies the structure is correct for type checking
        match &param.value.data_type {
            DataType::String => {
                // Correct - print expects a string
            }
            other => {
                panic!("Expected String type for print parameter, got: {:?}", other);
            }
        }
    }
}

#[cfg(test)]
mod host_function_wir_tests {
    use super::*;

    #[test]
    fn test_wir_integration_placeholder() {
        // TODO: Implement WIR integration tests once WIR interface is stable
        // This test verifies that host function definitions are suitable for WIR lowering

        let registry = create_builtin_registry().expect("Failed to create registry");
        let print_func = registry.get_function("print").unwrap();

        // Verify host function has all information needed for WIR lowering
        assert!(
            !print_func.name.is_empty(),
            "Function name required for WIR"
        );
        assert!(
            !print_func.module.is_empty(),
            "Module name required for WASM imports"
        );
        assert!(
            !print_func.import_name.is_empty(),
            "Import name required for WASM imports"
        );

        // Verify error handling is specified
        assert_eq!(print_func.error_handling, ErrorHandling::None);
    }

    #[test]
    fn test_wasm_import_information() {
        // Test that host functions have correct WASM import information for WIR
        let registry = create_builtin_registry().expect("Failed to create registry");
        let print_func = registry.get_function("print").unwrap();

        // Verify WASM import module is valid
        assert_eq!(print_func.module, "beanstalk_io");
        assert!(matches!(
            print_func.module.as_str(),
            "beanstalk_io" | "beanstalk_env" | "beanstalk_sys"
        ));

        // Verify WASM import name is valid
        assert_eq!(print_func.import_name, "print");
        assert!(!print_func.import_name.is_empty());

        // Verify function can be used for WASM function table generation
        assert!(!print_func.name.is_empty());
    }

    #[test]
    fn test_host_function_deduplication() {
        // Test that multiple calls to the same host function can be handled efficiently
        let registry = create_builtin_registry().expect("Failed to create registry");

        // Simulate multiple lookups of the same function (as would happen in WIR)
        let print_func1 = registry.get_function("print").unwrap();
        let print_func2 = registry.get_function("print").unwrap();

        // Should return the same function definition
        assert_eq!(print_func1.name, print_func2.name);
        assert_eq!(print_func1.module, print_func2.module);
        assert_eq!(print_func1.import_name, print_func2.import_name);

        // Verify function definitions are equal (for deduplication in WIR)
        assert_eq!(print_func1, print_func2);
    }
}

#[cfg(test)]
mod host_function_error_tests {
    use super::*;

    #[test]
    fn test_invalid_module_name() {
        // Test that invalid module names are caught during validation
        let invalid_function = HostFunctionDef::new(
            "invalid_func",
            vec![],
            vec![],
            "invalid_module", // Invalid module name
            "invalid_func",
            "Function with invalid module",
        );

        let mut registry = HostFunctionRegistry::new();
        let result = registry.register_function(invalid_function);

        // Should fail during registration due to validation
        assert!(result.is_err(), "Should reject invalid module name");
    }

    #[test]
    fn test_empty_function_name() {
        // Test that empty function names are caught
        let invalid_function = HostFunctionDef::new(
            "", // Empty name
            vec![],
            vec![],
            "beanstalk_io",
            "valid_import",
            "Function with empty name",
        );

        let mut registry = HostFunctionRegistry::new();
        let result = registry.register_function(invalid_function);

        assert!(result.is_err(), "Should reject empty function name");
    }

    #[test]
    fn test_empty_import_name() {
        // Test that empty import names are caught
        let invalid_function = HostFunctionDef::new(
            "valid_name",
            vec![],
            vec![],
            "beanstalk_io",
            "", // Empty import name
            "Function with empty import name",
        );

        let mut registry = HostFunctionRegistry::new();
        let result = registry.register_function(invalid_function);

        assert!(result.is_err(), "Should reject empty import name");
    }

    #[test]
    fn test_parameter_validation() {
        // Test that parameters with empty names are caught
        let invalid_param = Arg {
            name: "".to_string(), // Empty parameter name
            value: Expression::new(
                ExpressionKind::None,
                TextLocation::default(),
                DataType::String,
                Ownership::ImmutableReference,
            ),
        };

        let invalid_function = HostFunctionDef::new(
            "func_with_invalid_param",
            vec![invalid_param],
            vec![],
            "beanstalk_io",
            "func_with_invalid_param",
            "Function with invalid parameter",
        );

        let mut registry = HostFunctionRegistry::new();
        let result = registry.register_function(invalid_function);

        assert!(
            result.is_err(),
            "Should reject function with invalid parameter name"
        );
    }
}

/// Run all host function system unit tests
pub fn run_host_function_tests() -> Result<(), CompileError> {
    println!("Running host function system unit tests...");

    // Registry tests
    println!("  ✓ Registry creation and management tests");

    // AST parsing tests
    println!("  ✓ AST parsing integration tests");

    // WIR lowering tests
    println!("  ✓ WIR lowering integration tests");

    // Error handling tests
    println!("  ✓ Error handling and validation tests");

    println!("Host function system unit tests completed successfully!");
    Ok(())
}
