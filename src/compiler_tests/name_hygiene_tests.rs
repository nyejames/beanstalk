//! Unit tests for JavaScript name hygiene functionality
///
/// These tests verify that:
/// 1. JavaScript reserved words are properly escaped
/// 2. Temporary variables are generated with collision detection
/// 3. Name collision avoidance works correctly
#[cfg(test)]
mod tests {
    use crate::compiler::codegen::js::{JsEmitter, JsLoweringConfig};
    use crate::compiler::hir::nodes::HirModule;
    use crate::compiler::string_interning::StringTable;

    /// Helper function to create a test emitter
    /// Each test gets its own emitter and string table
    ///
    /// Note: This uses unsafe code to work around Rust's borrow checker for testing.
    /// The string table is leaked and we return both a reference to it and an emitter
    /// that borrows it. This is safe because both live for 'static and tests are isolated.
    fn create_test_setup() -> (JsEmitter<'static>, &'static mut StringTable) {
        let string_table = Box::leak(Box::new(StringTable::new()));
        let hir = Box::leak(Box::new(HirModule {
            blocks: Vec::new(),
            functions: Vec::new(),
            structs: Vec::new(),
            entry_block: 0,
        }));

        let config = JsLoweringConfig {
            pretty: true,
            emit_locations: false,
        };

        // Create the emitter with an immutable borrow
        let emitter = JsEmitter::new(hir, string_table, config);

        // SAFETY: We're creating a second mutable reference to string_table here.
        // This is safe in the test context because:
        // 1. The emitter only reads from string_table (via resolve())
        // 2. Tests use the mutable reference to intern new strings
        // 3. These operations don't conflict
        // 4. Each test gets its own isolated string_table
        let string_table_mut =
            unsafe { &mut *(string_table as *const StringTable as *mut StringTable) };

        (emitter, string_table_mut)
    }

    #[test]
    fn test_js_reserved_word_escaping() {
        let (mut emitter, string_table) = create_test_setup();

        // Test JavaScript keywords
        let keywords = vec![
            "break", "case", "catch", "class", "const", "continue", "function", "if", "return",
            "switch", "var", "while",
        ];

        for keyword in keywords {
            let interned = string_table.intern(keyword);
            let js_ident = emitter.make_js_ident(interned);

            // Reserved words should be prefixed with underscore
            assert_eq!(
                js_ident.0,
                format!("_{}", keyword),
                "Reserved word '{}' should be escaped to '_{}'",
                keyword,
                keyword
            );
        }
    }

    #[test]
    fn test_js_future_reserved_words() {
        let (mut emitter, string_table) = create_test_setup();

        // Test future reserved words
        let future_reserved = vec![
            "enum",
            "implements",
            "interface",
            "let",
            "package",
            "private",
            "protected",
            "public",
            "static",
            "await",
        ];

        for word in future_reserved {
            let interned = string_table.intern(word);
            let js_ident = emitter.make_js_ident(interned);

            // Future reserved words should also be escaped
            assert_eq!(
                js_ident.0,
                format!("_{}", word),
                "Future reserved word '{}' should be escaped to '_{}'",
                word,
                word
            );
        }
    }

    #[test]
    fn test_js_global_names() {
        let (mut emitter, string_table) = create_test_setup();

        // Test common global names that should be avoided
        let globals = vec![
            "undefined",
            "null",
            "true",
            "false",
            "NaN",
            "Infinity",
            "Array",
            "Object",
            "String",
            "Number",
            "Boolean",
            "console",
        ];

        for global in globals {
            let interned = string_table.intern(global);
            let js_ident = emitter.make_js_ident(interned);

            // Global names should be escaped
            assert_eq!(
                js_ident.0,
                format!("_{}", global),
                "Global name '{}' should be escaped to '_{}'",
                global,
                global
            );
        }
    }

    #[test]
    fn test_non_reserved_words() {
        let (mut emitter, string_table) = create_test_setup();

        // Test normal identifiers that should not be escaped
        let normal_names = vec![
            "myVariable",
            "count",
            "user_name",
            "calculate_sum",
            "data",
            "result",
            "value",
            "index",
        ];

        for name in normal_names {
            let interned = string_table.intern(name);
            let js_ident = emitter.make_js_ident(interned);

            // Normal names should not be modified
            assert_eq!(
                js_ident.0, name,
                "Normal identifier '{}' should not be escaped",
                name
            );
        }
    }

    #[test]
    fn test_temporary_variable_generation() {
        let (mut emitter, _string_table) = create_test_setup();

        // Generate multiple temporary variables
        let temp1 = emitter.gen_temp();
        let temp2 = emitter.gen_temp();
        let temp3 = emitter.gen_temp();

        // Temporaries should be unique
        assert_eq!(temp1.0, "_t0");
        assert_eq!(temp2.0, "_t1");
        assert_eq!(temp3.0, "_t2");

        // All temporaries should be in used_names
        assert!(emitter.used_names.contains("_t0"));
        assert!(emitter.used_names.contains("_t1"));
        assert!(emitter.used_names.contains("_t2"));
    }

    #[test]
    fn test_temporary_collision_avoidance() {
        let (mut emitter, string_table) = create_test_setup();

        // Pre-register a name that would collide with a temporary
        let collision_name = string_table.intern("_t0");
        let _ = emitter.make_js_ident(collision_name);

        // Now generate a temporary - it should skip _t0
        let temp = emitter.gen_temp();

        // Should generate _t1 instead of _t0 since _t0 is already used
        assert_eq!(
            temp.0, "_t1",
            "Temporary generation should skip already-used names"
        );
    }

    #[test]
    fn test_name_tracking() {
        let (mut emitter, string_table) = create_test_setup();

        // Create several identifiers
        let name1 = string_table.intern("myVar");
        let name2 = string_table.intern("count");
        let name3 = string_table.intern("result");

        let _ = emitter.make_js_ident(name1);
        let _ = emitter.make_js_ident(name2);
        let _ = emitter.make_js_ident(name3);

        // All names should be tracked
        assert!(emitter.used_names.contains("myVar"));
        assert!(emitter.used_names.contains("count"));
        assert!(emitter.used_names.contains("result"));
    }

    #[test]
    fn test_reserved_word_tracking() {
        let (mut emitter, string_table) = create_test_setup();

        // Create identifiers from reserved words
        let reserved1 = string_table.intern("function");
        let reserved2 = string_table.intern("class");

        let ident1 = emitter.make_js_ident(reserved1);
        let ident2 = emitter.make_js_ident(reserved2);

        // Escaped names should be tracked
        assert!(emitter.used_names.contains(&ident1.0));
        assert!(emitter.used_names.contains(&ident2.0));
        assert_eq!(ident1.0, "_function");
        assert_eq!(ident2.0, "_class");
    }

    #[test]
    fn test_multiple_temporary_generation() {
        let (mut emitter, _string_table) = create_test_setup();

        // Generate many temporaries to test counter increment
        let mut temps = Vec::new();
        for i in 0..10 {
            let temp = emitter.gen_temp();
            assert_eq!(temp.0, format!("_t{}", i));
            temps.push(temp);
        }

        // Verify all are unique
        let unique_count = temps
            .iter()
            .map(|t| &t.0)
            .collect::<std::collections::HashSet<_>>()
            .len();
        assert_eq!(
            unique_count, 10,
            "All generated temporaries should be unique"
        );
    }

    #[test]
    fn test_loop_label_generation() {
        let (mut emitter, _string_table) = create_test_setup();

        // Generate loop labels for different block IDs
        let label1 = emitter.gen_loop_label(0);
        let label2 = emitter.gen_loop_label(1);
        let label3 = emitter.gen_loop_label(42);

        // Labels should be based on block ID
        assert_eq!(label1.0, "loop_0");
        assert_eq!(label2.0, "loop_1");
        assert_eq!(label3.0, "loop_42");
    }
}
