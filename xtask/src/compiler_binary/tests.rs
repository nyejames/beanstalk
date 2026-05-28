use super::*;
use std::path::{Path, PathBuf};

#[test]
fn release_compiler_path_without_suffix_uses_unix_name() {
    assert_eq!(release_compiler_path(""), Path::new("target/release/bean"));
}

#[test]
fn release_compiler_path_with_suffix_uses_platform_extension() {
    assert_eq!(
        release_compiler_path(".exe"),
        Path::new("target/release/bean.exe")
    );
}

#[test]
fn compiler_binary_exposes_borrowed_path() {
    let binary = CompilerBinary {
        path: PathBuf::from("target/release/bean.exe"),
    };

    assert_eq!(binary.as_path(), Path::new("target/release/bean.exe"));
}
