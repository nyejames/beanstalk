//! Unit tests for host function registry

#[cfg(test)]
mod host_function_registry_tests {
    use crate::compiler::host_functions::{
        create_builtin_registry, RuntimeBackend,
    };
    use crate::compiler::string_interning::StringTable;
    use crate::compiler::datatypes::DataType;

    #[test]
    fn test_io_function_registered() {
        let mut string_table = StringTable::new();
        let registry = create_builtin_registry(RuntimeBackend::JavaScript, &mut string_table)
            .expect("Failed to create builtin registry");

        let io_name = string_table.intern("io");
        let io_function = registry.get_function(&io_name);

        assert!(io_function.is_some(), "io() function should be registered");
    }

    #[test]
    fn test_io_function_has_correct_module() {
        let mut string_table = StringTable::new();
        let registry = create_builtin_registry(RuntimeBackend::JavaScript, &mut string_table)
            .expect("Failed to create builtin registry");

        let io_name = string_table.intern("io");
        let io_function = registry.get_function(&io_name).unwrap();

        let module_name = string_table.resolve(io_function.module);
        assert_eq!(
            module_name, "beanstalk_io",
            "io() function should use beanstalk_io module"
        );
    }

    #[test]
    fn test_io_function_has_coerce_to_string_parameter() {
        let mut string_table = StringTable::new();
        let registry = create_builtin_registry(RuntimeBackend::JavaScript, &mut string_table)
            .expect("Failed to create builtin registry");

        let io_name = string_table.intern("io");
        let io_function = registry.get_function(&io_name).unwrap();

        assert_eq!(
            io_function.parameters.len(),
            1,
            "io() function should have exactly 1 parameter"
        );

        let param = &io_function.parameters[0];
        assert!(
            matches!(param.data_type, DataType::CoerceToString),
            "io() function parameter should be CoerceToString type"
        );
    }

    #[test]
    fn test_io_function_has_void_return() {
        let mut string_table = StringTable::new();
        let registry = create_builtin_registry(RuntimeBackend::JavaScript, &mut string_table)
            .expect("Failed to create builtin registry");

        let io_name = string_table.intern("io");
        let io_function = registry.get_function(&io_name).unwrap();

        assert!(
            io_function.return_types.is_empty(),
            "io() function should have void return type (no return values)"
        );
    }

    #[test]
    fn test_io_function_has_js_mapping() {
        let mut string_table = StringTable::new();
        let registry = create_builtin_registry(RuntimeBackend::JavaScript, &mut string_table)
            .expect("Failed to create builtin registry");

        let io_name = string_table.intern("io");
        let js_mapping = registry.get_js_mapping(&io_name);

        assert!(
            js_mapping.is_some(),
            "io() function should have JavaScript mapping"
        );
    }

    #[test]
    fn test_io_js_mapping_has_correct_signature() {
        let mut string_table = StringTable::new();
        let registry = create_builtin_registry(RuntimeBackend::JavaScript, &mut string_table)
            .expect("Failed to create builtin registry");

        let io_name = string_table.intern("io");
        let js_mapping = registry.get_js_mapping(&io_name).unwrap();

        assert_eq!(
            js_mapping.module, "beanstalk_io",
            "JS mapping should use beanstalk_io module"
        );
        assert_eq!(js_mapping.name, "io", "JS mapping should be named 'io'");
        assert_eq!(
            js_mapping.parameters.len(),
            2,
            "JS mapping should have 2 parameters (ptr, len)"
        );
        assert!(
            js_mapping.returns.is_empty(),
            "JS mapping should have void return"
        );
    }

    #[test]
    fn test_registry_validation_passes() {
        let mut string_table = StringTable::new();
        let result = create_builtin_registry(RuntimeBackend::JavaScript, &mut string_table);

        assert!(
            result.is_ok(),
            "Registry validation should pass for builtin functions"
        );
    }

    #[test]
    fn test_registry_has_runtime_mapping() {
        let mut string_table = StringTable::new();
        let registry = create_builtin_registry(RuntimeBackend::JavaScript, &mut string_table)
            .expect("Failed to create builtin registry");

        let io_name = string_table.intern("io");
        assert!(
            registry.has_runtime_mapping(&io_name),
            "io() function should have runtime mapping for JavaScript backend"
        );
    }
}
