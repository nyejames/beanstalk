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
