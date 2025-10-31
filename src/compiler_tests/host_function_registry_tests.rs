//! Unit tests for host function registry functionality

use crate::compiler::host_functions::registry::{
    HostFunctionRegistry, HostFunctionDef, WasixFunctionDef, JsFunctionDef, 
    RuntimeBackend, RuntimeFunctionMapping, BasicParameter, ErrorHandling,
    create_builtin_registry, create_builtin_registry_with_backend
};
use crate::compiler::datatypes::{DataType, Ownership};
use wasm_encoder::ValType;

#[cfg(test)]
mod tests {
    use super::*;

    /// Test basic registry creation and function registration
    #[test]
    fn test_registry_creation_and_basic_registration() {
        let mut registry = HostFunctionRegistry::new();
        
        // Verify empty registry
        assert_eq!(registry.count(), 0);
        assert_eq!(registry.wasix_mapping_count(), 0);
        assert_eq!(registry.js_mapping_count(), 0);
        assert_eq!(registry.get_current_backend(), &RuntimeBackend::Wasix);
        
        // Register a basic function
        let test_function = HostFunctionDef::new(
            "test_func",
            vec![BasicParameter {
                name: "param1".to_string(),
                data_type: DataType::String,
                ownership: Ownership::default(),
            }],
            vec![DataType::Int],
            "beanstalk_io",
            "test_import",
            "Test function for registry testing",
        );
        
        registry.register_function(test_function).expect("Function registration should succeed");
        
        // Verify registration
        assert_eq!(registry.count(), 1);
        assert!(registry.has_function("test_func"));
        assert!(!registry.has_function("nonexistent"));
        
        let retrieved_func = registry.get_function("test_func").expect("Function should exist");
        assert_eq!(retrieved_func.name, "test_func");
        assert_eq!(retrieved_func.module, "beanstalk_io");
        assert_eq!(retrieved_func.import_name, "test_import");
    }

    /// Test runtime backend switching
    #[test]
    fn test_runtime_backend_switching() {
        let mut registry = HostFunctionRegistry::new_with_backend(RuntimeBackend::JavaScript);
        
        assert_eq!(registry.get_current_backend(), &RuntimeBackend::JavaScript);
        
        registry.set_current_backend(RuntimeBackend::Native);
        assert_eq!(registry.get_current_backend(), &RuntimeBackend::Native);
        
        registry.set_current_backend(RuntimeBackend::Wasix);
        assert_eq!(registry.get_current_backend(), &RuntimeBackend::Wasix);
    }

    /// Test WASIX mapping registration and retrieval
    #[test]
    fn test_wasix_mapping_registration() {
        let mut registry = HostFunctionRegistry::new();
        
        // Register a WASIX mapping
        let wasix_mapping = WasixFunctionDef::new(
            "wasix_32v1",
            "fd_write",
            vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32],
            vec![ValType::I32],
            "WASIX fd_write function for print implementation",
        );
        
        registry.register_wasix_mapping("print", wasix_mapping).expect("WASIX mapping registration should succeed");
        
        // Verify registration
        assert_eq!(registry.wasix_mapping_count(), 1);
        assert!(registry.get_wasix_mapping("print").is_some());
        assert!(registry.get_wasix_mapping("nonexistent").is_none());
        
        let retrieved_mapping = registry.get_wasix_mapping("print").expect("WASIX mapping should exist");
        assert_eq!(retrieved_mapping.module, "wasix_32v1");
        assert_eq!(retrieved_mapping.name, "fd_write");
        assert_eq!(retrieved_mapping.parameters.len(), 4);
        assert_eq!(retrieved_mapping.returns.len(), 1);
    }

    /// Test JavaScript mapping registration and retrieval
    #[test]
    fn test_js_mapping_registration() {
        let mut registry = HostFunctionRegistry::new();
        
        // Register a JavaScript mapping
        let js_mapping = JsFunctionDef::new(
            "beanstalk_io",
            "print",
            vec![ValType::I32, ValType::I32],
            vec![],
            "JavaScript console.log function for print implementation",
        );
        
        registry.register_js_mapping("print", js_mapping).expect("JavaScript mapping registration should succeed");
        
        // Verify registration
        assert_eq!(registry.js_mapping_count(), 1);
        assert!(registry.get_js_mapping("print").is_some());
        assert!(registry.get_js_mapping("nonexistent").is_none());
        
        let retrieved_mapping = registry.get_js_mapping("print").expect("JavaScript mapping should exist");
        assert_eq!(retrieved_mapping.module, "beanstalk_io");
        assert_eq!(retrieved_mapping.name, "print");
        assert_eq!(retrieved_mapping.parameters.len(), 2);
        assert_eq!(retrieved_mapping.returns.len(), 0);
    }

    /// Test function registration with multiple runtime mappings
    #[test]
    fn test_function_registration_with_mappings() {
        let mut registry = HostFunctionRegistry::new();
        
        let host_function = HostFunctionDef::new(
            "print",
            vec![BasicParameter {
                name: "message".to_string(),
                data_type: DataType::String,
                ownership: Ownership::default(),
            }],
            vec![],
            "beanstalk_io",
            "print",
            "Print function with multiple runtime mappings",
        );
        
        let wasix_mapping = WasixFunctionDef::new(
            "wasix_32v1",
            "fd_write",
            vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32],
            vec![ValType::I32],
            "WASIX fd_write for print",
        );
        
        let js_mapping = JsFunctionDef::new(
            "beanstalk_io",
            "print",
            vec![ValType::I32, ValType::I32],
            vec![],
            "JavaScript console.log for print",
        );
        
        registry.register_function_with_mappings(
            host_function,
            Some(wasix_mapping),
            Some(js_mapping),
        ).expect("Function registration with mappings should succeed");
        
        // Verify all registrations
        assert_eq!(registry.count(), 1);
        assert_eq!(registry.wasix_mapping_count(), 1);
        assert_eq!(registry.js_mapping_count(), 1);
        
        assert!(registry.has_function("print"));
        assert!(registry.get_wasix_mapping("print").is_some());
        assert!(registry.get_js_mapping("print").is_some());
    }

    /// Test runtime-specific function mapping lookup
    #[test]
    fn test_runtime_mapping_lookup() {
        let mut registry = HostFunctionRegistry::new();
        
        // Register function with all mappings
        let host_function = HostFunctionDef::new(
            "print",
            vec![BasicParameter {
                name: "message".to_string(),
                data_type: DataType::String,
                ownership: Ownership::default(),
            }],
            vec![],
            "beanstalk_io",
            "print",
            "Print function",
        );
        
        let wasix_mapping = WasixFunctionDef::new(
            "wasix_32v1",
            "fd_write",
            vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32],
            vec![ValType::I32],
            "WASIX fd_write",
        );
        
        let js_mapping = JsFunctionDef::new(
            "beanstalk_io",
            "print",
            vec![ValType::I32, ValType::I32],
            vec![],
            "JavaScript console.log",
        );
        
        registry.register_function_with_mappings(
            host_function,
            Some(wasix_mapping),
            Some(js_mapping),
        ).expect("Registration should succeed");
        
        // Test WASIX backend
        registry.set_current_backend(RuntimeBackend::Wasix);
        assert!(registry.has_runtime_mapping("print"));
        let wasix_runtime_mapping = registry.get_runtime_mapping("print").expect("WASIX mapping should exist");
        assert!(matches!(wasix_runtime_mapping, RuntimeFunctionMapping::Wasix(_)));
        
        // Test JavaScript backend
        registry.set_current_backend(RuntimeBackend::JavaScript);
        assert!(registry.has_runtime_mapping("print"));
        let js_runtime_mapping = registry.get_runtime_mapping("print").expect("JavaScript mapping should exist");
        assert!(matches!(js_runtime_mapping, RuntimeFunctionMapping::JavaScript(_)));
        
        // Test Native backend
        registry.set_current_backend(RuntimeBackend::Native);
        assert!(registry.has_runtime_mapping("print"));
        let native_runtime_mapping = registry.get_runtime_mapping("print").expect("Native mapping should exist");
        assert!(matches!(native_runtime_mapping, RuntimeFunctionMapping::Native(_)));
    }

    /// Test builtin registry creation
    #[test]
    fn test_builtin_registry_creation() {
        let registry = create_builtin_registry().expect("Builtin registry creation should succeed");
        
        // Should have print function registered
        assert!(registry.has_function("print"));
        assert!(registry.get_wasix_mapping("print").is_some());
        assert!(registry.get_js_mapping("print").is_some());
        
        // Verify print function details
        let print_func = registry.get_function("print").expect("Print function should exist");
        assert_eq!(print_func.name, "print");
        assert_eq!(print_func.parameters.len(), 1);
        assert_eq!(print_func.return_types.len(), 0);
        
        // Verify WASIX mapping
        let wasix_mapping = registry.get_wasix_mapping("print").expect("WASIX mapping should exist");
        assert_eq!(wasix_mapping.module, "wasix_32v1");
        assert_eq!(wasix_mapping.name, "fd_write");
        
        // Verify JavaScript mapping
        let js_mapping = registry.get_js_mapping("print").expect("JavaScript mapping should exist");
        assert_eq!(js_mapping.module, "beanstalk_io");
        assert_eq!(js_mapping.name, "print");
    }

    /// Test builtin registry creation with specific backend
    #[test]
    fn test_builtin_registry_creation_with_backend() {
        let registry = create_builtin_registry_with_backend(RuntimeBackend::JavaScript)
            .expect("Builtin registry creation with backend should succeed");
        
        assert_eq!(registry.get_current_backend(), &RuntimeBackend::JavaScript);
        assert!(registry.has_function("print"));
        assert!(registry.has_runtime_mapping("print"));
        
        // Should be able to get JavaScript runtime mapping
        let runtime_mapping = registry.get_runtime_mapping("print").expect("Runtime mapping should exist");
        assert!(matches!(runtime_mapping, RuntimeFunctionMapping::JavaScript(_)));
    }

    /// Test error cases for duplicate registrations
    #[test]
    fn test_duplicate_registration_errors() {
        let mut registry = HostFunctionRegistry::new();
        
        let function1 = HostFunctionDef::new(
            "test_func",
            vec![],
            vec![],
            "beanstalk_io",
            "func1",
            "First function",
        );
        
        let function2 = HostFunctionDef::new(
            "test_func", // Same name
            vec![],
            vec![],
            "beanstalk_env",
            "func2",
            "Second function",
        );
        
        // First registration should succeed
        registry.register_function(function1).expect("First registration should succeed");
        
        // Second registration should fail
        let result = registry.register_function(function2);
        assert!(result.is_err());
        
        // Test duplicate WASIX mapping
        let wasix_mapping1 = WasixFunctionDef::new(
            "wasix_32v1",
            "fd_write",
            vec![ValType::I32],
            vec![ValType::I32],
            "First WASIX mapping",
        );
        
        let wasix_mapping2 = WasixFunctionDef::new(
            "wasix_32v1",
            "fd_read",
            vec![ValType::I32],
            vec![ValType::I32],
            "Second WASIX mapping",
        );
        
        registry.register_wasix_mapping("test_func", wasix_mapping1).expect("First WASIX mapping should succeed");
        let result = registry.register_wasix_mapping("test_func", wasix_mapping2);
        assert!(result.is_err());
        
        // Test duplicate JavaScript mapping
        let js_mapping1 = JsFunctionDef::new(
            "beanstalk_io",
            "func1",
            vec![ValType::I32],
            vec![],
            "First JS mapping",
        );
        
        let js_mapping2 = JsFunctionDef::new(
            "beanstalk_io",
            "func2",
            vec![ValType::I32],
            vec![],
            "Second JS mapping",
        );
        
        registry.register_js_mapping("test_func", js_mapping1).expect("First JS mapping should succeed");
        let result = registry.register_js_mapping("test_func", js_mapping2);
        assert!(result.is_err());
    }

    /// Test error cases for missing mappings
    #[test]
    fn test_missing_mapping_cases() {
        let registry = HostFunctionRegistry::new();
        
        // Test missing function
        assert!(registry.get_function("nonexistent").is_none());
        assert!(registry.get_wasix_mapping("nonexistent").is_none());
        assert!(registry.get_js_mapping("nonexistent").is_none());
        assert!(registry.get_runtime_mapping("nonexistent").is_none());
        
        // Test missing runtime mapping for different backends
        let mut registry_with_function = HostFunctionRegistry::new();
        let function = HostFunctionDef::new(
            "test_func",
            vec![],
            vec![],
            "beanstalk_io",
            "test_import",
            "Test function without runtime mappings",
        );
        
        registry_with_function.register_function(function).expect("Function registration should succeed");
        
        // Should have native mapping but not WASIX or JavaScript
        registry_with_function.set_current_backend(RuntimeBackend::Native);
        assert!(registry_with_function.has_runtime_mapping("test_func"));
        
        registry_with_function.set_current_backend(RuntimeBackend::Wasix);
        assert!(!registry_with_function.has_runtime_mapping("test_func"));
        
        registry_with_function.set_current_backend(RuntimeBackend::JavaScript);
        assert!(!registry_with_function.has_runtime_mapping("test_func"));
    }

    /// Test validation of invalid function definitions
    #[test]
    fn test_invalid_function_definition_validation() {
        let mut registry = HostFunctionRegistry::new();
        
        // Test invalid WASIX module
        let invalid_wasix = WasixFunctionDef::new(
            "invalid_module",
            "test_func",
            vec![ValType::I32],
            vec![ValType::I32],
            "Invalid WASIX module",
        );
        
        let result = registry.register_wasix_mapping("test", invalid_wasix);
        assert!(result.is_err());
        
        // Test invalid JavaScript module
        let invalid_js = JsFunctionDef::new(
            "invalid_js_module",
            "test_func",
            vec![ValType::I32],
            vec![],
            "Invalid JavaScript module",
        );
        
        let result = registry.register_js_mapping("test", invalid_js);
        assert!(result.is_err());
        
        // Test WASIX function with too many parameters
        let too_many_params_wasix = WasixFunctionDef::new(
            "wasix_32v1",
            "test_func",
            vec![ValType::I32; 15], // 15 parameters, exceeds limit of 10
            vec![ValType::I32],
            "WASIX function with too many parameters",
        );
        
        let result = registry.register_wasix_mapping("test", too_many_params_wasix);
        assert!(result.is_err());
        
        // Test JavaScript function with too many return values
        let too_many_returns_js = JsFunctionDef::new(
            "beanstalk_io",
            "test_func",
            vec![ValType::I32],
            vec![ValType::I32, ValType::I32], // 2 return values, exceeds limit of 1
            "JavaScript function with too many return values",
        );
        
        let result = registry.register_js_mapping("test", too_many_returns_js);
        assert!(result.is_err());
    }
}