
#[cfg(test)]
mod tests {
    use crate::compiler::string_interning::{InternedString, StringId, StringTable};

    #[test]
    fn test_basic_interning() {
        let mut table = StringTable::new();

        // Test basic interning
        let id1 = table.intern("hello");
        let id2 = table.intern("world");
        let id3 = table.intern("hello"); // Duplicate

        // Same string should return same ID
        assert_eq!(id1, id3);
        assert_ne!(id1, id2);

        // Test resolution
        assert_eq!(table.resolve(id1), "hello");
        assert_eq!(table.resolve(id2), "world");
        assert_eq!(table.resolve(id3), "hello");
    }

    #[test]
    fn test_string_id_methods() {
        let mut table = StringTable::new();
        let id = table.intern("test_string");

        // Test serialization methods
        let raw_id = id.as_u32();
        let reconstructed = StringId::from_u32(raw_id);
        assert_eq!(id, reconstructed);

        // Test convenience methods
        assert!(id.eq_str(&table, "test_string"));
        assert!(!id.eq_str(&table, "different_string"));
        assert_eq!(id.resolve(&table), "test_string");
    }

    #[test]
    fn test_get_or_intern() {
        let mut table = StringTable::new();

        // Test with owned String
        let owned_string = String::from("owned_test");
        let id1 = table.get_or_intern(owned_string);

        // Test duplicate with owned String
        let duplicate_string = String::from("owned_test");
        let id2 = table.get_or_intern(duplicate_string);

        assert_eq!(id1, id2);
        assert_eq!(table.resolve(id1), "owned_test");
    }

    #[test]
    fn test_memory_efficiency() {
        let mut table = StringTable::new();

        // Intern the same string multiple times
        let test_string = "repeated_identifier";
        let mut ids = Vec::new();

        for _ in 0..100 {
            ids.push(table.intern(test_string));
        }

        // All IDs should be the same
        let first_id = ids[0];
        for id in &ids {
            assert_eq!(*id, first_id);
        }

        // Should only have one unique string
        assert_eq!(table.len(), 1);

        // Check statistics
        let stats = table.stats();
        assert_eq!(stats.unique_strings, 1);
        assert_eq!(stats.total_intern_calls, 100);
        assert_eq!(stats.cache_hits, 99); // First call is not a cache hit
        assert!(stats.cache_hit_rate() >= 99.0); // Use >= instead of > for exact 99.0
    }

    #[test]
    fn test_try_resolve() {
        let mut table = StringTable::new();
        let id = table.intern("valid_string");

        // Valid ID should resolve
        assert_eq!(table.try_resolve(id), Some("valid_string"));

        // Invalid ID should return None
        let invalid_id = StringId::from_u32(999);
        assert_eq!(table.try_resolve(invalid_id), None);
    }

    #[test]
    fn test_get_existing() {
        let mut table = StringTable::new();

        // String not yet interned
        assert_eq!(table.get_existing("not_interned"), None);

        // Intern a string
        let id = table.intern("interned_string");

        // Should now find it
        assert_eq!(table.get_existing("interned_string"), Some(id));
        assert_eq!(table.get_existing("still_not_interned"), None);
    }

    #[test]
    fn test_memory_usage_stats() {
        let mut table = StringTable::new();

        // Start with empty table
        let initial_stats = table.memory_usage();
        assert_eq!(initial_stats.unique_strings, 0);

        // Add some strings
        table.intern("short");
        table.intern("a_much_longer_string_for_testing");
        table.intern("medium_length");

        let final_stats = table.memory_usage();
        assert_eq!(final_stats.unique_strings, 3);
        assert!(final_stats.total_bytes > initial_stats.total_bytes);
        assert!(final_stats.string_content_bytes > 0);
    }

    #[test]
    fn test_empty_and_special_strings() {
        let mut table = StringTable::new();

        // Test empty string
        let empty_id = table.intern("");
        assert_eq!(table.resolve(empty_id), "");

        // Test Unicode string
        let unicode_id = table.intern("Hello, 世界! 🦀");
        assert_eq!(table.resolve(unicode_id), "Hello, 世界! 🦀");

        // Test very long string
        let long_string = "a".repeat(1000);
        let long_id = table.intern(&long_string);
        assert_eq!(table.resolve(long_id), long_string);
    }

    #[test]
    fn test_string_id_display() {
        let id = StringId::from_u32(42);
        let display_str = format!("{}", id);
        assert_eq!(display_str, "StringId(42)");
    }

    #[test]
    #[cfg(debug_assertions)]
    fn test_debug_features() {
        let mut table = StringTable::new();

        // Intern some strings
        let id1 = table.intern("debug_test_1");
        let id2 = table.intern("debug_test_2");
        table.intern("debug_test_1"); // Duplicate for debug info

        // Test debug info
        let debug_info = table.debug_info(id1);
        assert!(debug_info.is_some());
        if let Some(info) = debug_info {
            assert_eq!(info.intern_count, 2); // Original + duplicate
        }

        // Test string dumping
        let dumped = table.dump_strings();
        assert_eq!(dumped.len(), 2);
        assert!(dumped.iter().any(|(_, s)| *s == "debug_test_1"));
        assert!(dumped.iter().any(|(_, s)| *s == "debug_test_2"));

        // Test most frequent strings
        let frequent = table.most_frequent_strings(5);
        assert!(!frequent.is_empty());
        // The first entry should be the most frequent (debug_test_1 with 2 interns)
        assert_eq!(frequent[0].1, "debug_test_1");
        assert_eq!(frequent[0].2, 2);
    }

    #[test]
    fn test_compiler_string_table_integration() {
        use crate::Compiler;
        use crate::compiler::host_functions::registry::HostFunctionRegistry;
        use crate::settings::Config;

        let config = Config::default();
        let host_registry = HostFunctionRegistry::new();
        let mut compiler = Compiler::new(&config, host_registry);

        // Test string interning through compiler
        let id1 = compiler.intern_string("test_identifier");
        let id2 = compiler.intern_string("another_string");
        let id3 = compiler.intern_string("test_identifier"); // Duplicate

        // Same string should return same ID
        assert_eq!(id1, id3);
        assert_ne!(id1, id2);

        // Test string resolution through compiler
        assert_eq!(compiler.resolve_string(id1), "test_identifier");
        assert_eq!(compiler.resolve_string(id2), "another_string");
        assert_eq!(compiler.resolve_string(id3), "test_identifier");

        // Test access to string table
        let string_table = compiler.string_table();
        assert_eq!(string_table.len(), 2); // Two unique strings

        let stats = string_table.stats();
        assert_eq!(stats.unique_strings, 2);
        assert_eq!(stats.total_intern_calls, 3);
        assert_eq!(stats.cache_hits, 1); // One duplicate
    }

    #[test]
    fn test_compiler_string_table_lifetime() {
        use crate::Compiler;
        use crate::compiler::host_functions::registry::HostFunctionRegistry;
        use crate::settings::Config;

        let config = Config::default();
        let host_registry = HostFunctionRegistry::new();
        let mut compiler = Compiler::new(&config, host_registry);

        // Intern strings and store IDs
        let ids: Vec<_> = (0..10)
            .map(|i| compiler.intern_string(&format!("string_{}", i)))
            .collect();

        // All IDs should remain valid throughout compiler lifetime
        for (i, &id) in ids.iter().enumerate() {
            assert_eq!(compiler.resolve_string(id), format!("string_{}", i));
        }

        // String table should persist all strings
        assert_eq!(compiler.string_table().len(), 10);
    }
}
