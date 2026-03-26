use super::*;
use std::fs;
use std::time::SystemTime;

#[test]
fn to_portable_string_normalizes_windows_separator() {
    let mut string_table = StringTable::new();
    let path = InternedPath::from_components(vec![
        string_table.intern("styles"),
        string_table.intern("docs"),
        string_table.intern("navbar"),
    ]);

    assert_eq!(path.to_portable_string(&string_table), "styles/docs/navbar");
}

#[test]
fn from_path_buf_round_trips_temp_directory_path() {
    let mut string_table = StringTable::new();
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!("beanstalk_interned_path_{unique}"));
    fs::create_dir_all(&temp_dir).expect("should create temp directory");

    let canonical = temp_dir
        .canonicalize()
        .expect("temp directory should canonicalize");
    let path = InternedPath::from_path_buf(&canonical, &mut string_table);
    assert_eq!(path.to_path_buf(&string_table), canonical);

    fs::remove_dir_all(&temp_dir).expect("should remove temp directory");
}

#[test]
fn parent_join_append_and_join_str_preserve_component_order() {
    let mut string_table = StringTable::new();
    let root = InternedPath::from_components(vec![
        string_table.intern("src"),
        string_table.intern("compiler_frontend"),
    ]);
    let suffix = InternedPath::from_components(vec![
        string_table.intern("hir"),
        string_table.intern("hir_builder.rs"),
    ]);

    let joined = root.join(&suffix);
    assert_eq!(
        joined.to_portable_string(&string_table),
        "src/compiler_frontend/hir/hir_builder.rs"
    );

    let parent = joined.parent().expect("joined path should have a parent");
    assert_eq!(
        parent.to_portable_string(&string_table),
        "src/compiler_frontend/hir"
    );

    let appended = root.append(string_table.intern("tests"));
    assert_eq!(
        appended.to_portable_string(&string_table),
        "src/compiler_frontend/tests"
    );

    let joined_str = root.join_str("optimizers", &mut string_table);
    assert_eq!(
        joined_str.to_portable_string(&string_table),
        "src/compiler_frontend/optimizers"
    );
}

#[test]
fn parent_of_single_component_path_is_empty_path() {
    let mut string_table = StringTable::new();
    let path = InternedPath::from_single_str("beanstalk", &mut string_table);

    let parent = path.parent().expect("single-component path should have a root parent");
    assert!(parent.is_empty());
    assert_eq!(parent.to_portable_string(&string_table), "");
}
