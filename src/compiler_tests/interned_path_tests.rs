#[cfg(test)]
mod tests {
    use crate::compiler::interned_path::InternedPath;
    use crate::compiler::string_interning::StringTable;
    use std::path::PathBuf;

    #[test]
    fn test_empty_path() {
        let path = InternedPath::new();
        assert!(path.is_empty());
        assert_eq!(path.len(), 0);
        assert_eq!(path.file_name(), None);
    }

    #[test]
    fn test_from_path_buf() {
        let mut string_table = StringTable::new();
        let path_buf = PathBuf::from("src/compiler/ast.rs");
        let interned_path = InternedPath::from_path_buf(&path_buf, &mut string_table);

        assert_eq!(interned_path.len(), 3);
        assert_eq!(
            interned_path.file_name_str(&string_table),
            Some("ast.rs")
        );
    }

    #[test]
    fn test_to_path_buf() {
        let mut string_table = StringTable::new();
        let original_path = PathBuf::from("src/compiler/ast.rs");
        let interned_path = InternedPath::from_path_buf(&original_path, &mut string_table);
        let converted_back = interned_path.to_path_buf(&string_table);

        assert_eq!(original_path, converted_back);
    }

    #[test]
    fn test_push_and_pop() {
        let mut string_table = StringTable::new();
        let mut path = InternedPath::new();

        path.push_str("src", &mut string_table);
        path.push_str("compiler", &mut string_table);
        
        assert_eq!(path.len(), 2);
        
        let popped = path.pop();
        assert!(popped.is_some());
        assert_eq!(path.len(), 1);
        assert_eq!(
            string_table.resolve(popped.unwrap()),
            "compiler"
        );
    }

    #[test]
    fn test_parent() {
        let mut string_table = StringTable::new();
        let path_buf = PathBuf::from("src/compiler/ast.rs");
        let interned_path = InternedPath::from_path_buf(&path_buf, &mut string_table);

        let parent = interned_path.parent().unwrap();
        assert_eq!(parent.len(), 2);
        assert_eq!(
            parent.to_path_buf(&string_table),
            PathBuf::from("src/compiler")
        );

        let grandparent = parent.parent().unwrap();
        assert_eq!(grandparent.len(), 1);
        assert_eq!(
            grandparent.to_path_buf(&string_table),
            PathBuf::from("src")
        );

        let root = grandparent.parent();
        assert!(root.is_some());
        assert!(root.unwrap().is_empty());
    }

    #[test]
    fn test_join() {
        let mut string_table = StringTable::new();
        let base = InternedPath::from_path_buf(&PathBuf::from("src"), &mut string_table);
        let relative = InternedPath::from_path_buf(&PathBuf::from("compiler/ast.rs"), &mut string_table);

        let joined = base.join(&relative);
        assert_eq!(
            joined.to_path_buf(&string_table),
            PathBuf::from("src/compiler/ast.rs")
        );
    }

    #[test]
    fn test_starts_with_and_ends_with() {
        let mut string_table = StringTable::new();
        let full_path = InternedPath::from_path_buf(&PathBuf::from("src/compiler/ast.rs"), &mut string_table);
        let prefix = InternedPath::from_path_buf(&PathBuf::from("src/compiler"), &mut string_table);
        let suffix = InternedPath::from_path_buf(&PathBuf::from("compiler/ast.rs"), &mut string_table);

        assert!(full_path.starts_with(&prefix));
        assert!(full_path.ends_with(&suffix));
        assert!(!prefix.starts_with(&full_path));
        assert!(!suffix.ends_with(&full_path));
    }

    #[test]
    fn test_relative_to() {
        let mut string_table = StringTable::new();
        let full_path = InternedPath::from_path_buf(&PathBuf::from("src/compiler/ast.rs"), &mut string_table);
        let base = InternedPath::from_path_buf(&PathBuf::from("src"), &mut string_table);

        let relative = full_path.relative_to(&base).unwrap();
        assert_eq!(
            relative.to_path_buf(&string_table),
            PathBuf::from("compiler/ast.rs")
        );
    }

    #[test]
    fn test_display() {
        let mut string_table = StringTable::new();
        let path = InternedPath::from_path_buf(&PathBuf::from("src/compiler/ast.rs"), &mut string_table);
        
        let display_str = format!("{}", path.to_string(&string_table));
        assert_eq!(display_str, "src/compiler/ast.rs");
    }

    #[test]
    fn test_eq_path_buf() {
        let mut string_table = StringTable::new();
        let path_buf = PathBuf::from("src/compiler/ast.rs");
        let interned_path = InternedPath::from_path_buf(&path_buf, &mut string_table);

        assert!(interned_path.eq_path_buf(&path_buf, &string_table));
        assert!(!interned_path.eq_path_buf(&PathBuf::from("different/path.rs"), &string_table));
    }

    #[test]
    fn test_join_str() {
        let mut string_table = StringTable::new();
        let base = InternedPath::from_path_buf(&PathBuf::from("src"), &mut string_table);
        
        let joined = base.join_str("compiler", &mut string_table);
        assert_eq!(
            joined.to_path_buf(&string_table),
            PathBuf::from("src/compiler")
        );
    }

    #[test]
    fn test_from_components() {
        let mut string_table = StringTable::new();
        let src_id = string_table.intern("src");
        let compiler_id = string_table.intern("compiler");
        let ast_id = string_table.intern("ast.rs");

        let path = InternedPath::from_components(vec![src_id, compiler_id, ast_id]);
        assert_eq!(path.len(), 3);
        assert_eq!(
            path.to_path_buf(&string_table),
            PathBuf::from("src/compiler/ast.rs")
        );
    }

    #[test]
    fn test_as_components() {
        let mut string_table = StringTable::new();
        let path_buf = PathBuf::from("src/compiler/ast.rs");
        let interned_path = InternedPath::from_path_buf(&path_buf, &mut string_table);

        let components = interned_path.as_components();
        assert_eq!(components.len(), 3);
        assert_eq!(string_table.resolve(components[0]), "src");
        assert_eq!(string_table.resolve(components[1]), "compiler");
        assert_eq!(string_table.resolve(components[2]), "ast.rs");
    }

    #[test]
    fn test_components_iterator() {
        let mut string_table = StringTable::new();
        let path_buf = PathBuf::from("src/compiler");
        let interned_path = InternedPath::from_path_buf(&path_buf, &mut string_table);

        let component_strings: Vec<&str> = interned_path
            .components()
            .map(|id| string_table.resolve(id))
            .collect();

        assert_eq!(component_strings, vec!["src", "compiler"]);
    }

    #[test]
    fn test_extract_header_name_with_header_suffix() {
        let mut string_table = StringTable::new();
        let path_buf = PathBuf::from("tests/cases/success/basic_function.bst/simple_function.header");
        let interned_path = InternedPath::from_path_buf(&path_buf, &mut string_table);

        let simple_name = interned_path.extract_header_name(&mut string_table);
        assert!(simple_name.is_some());
        assert_eq!(
            string_table.resolve(simple_name.unwrap()),
            "simple_function"
        );
    }

    #[test]
    fn test_extract_header_name_without_header_suffix() {
        let mut string_table = StringTable::new();
        let path_buf = PathBuf::from("file.bst/no_suffix");
        let interned_path = InternedPath::from_path_buf(&path_buf, &mut string_table);

        let simple_name = interned_path.extract_header_name(&mut string_table);
        assert!(simple_name.is_some());
        assert_eq!(
            string_table.resolve(simple_name.unwrap()),
            "no_suffix"
        );
    }

    #[test]
    fn test_extract_header_name_empty_path() {
        let mut string_table = StringTable::new();
        let interned_path = InternedPath::new();

        let simple_name = interned_path.extract_header_name(&mut string_table);
        assert!(simple_name.is_none());
    }

    #[test]
    fn test_extract_header_name_multi_component_path() {
        let mut string_table = StringTable::new();
        let path_buf = PathBuf::from("src/utils/math.bst/add.header");
        let interned_path = InternedPath::from_path_buf(&path_buf, &mut string_table);

        let simple_name = interned_path.extract_header_name(&mut string_table);
        assert!(simple_name.is_some());
        assert_eq!(
            string_table.resolve(simple_name.unwrap()),
            "add"
        );
    }
}