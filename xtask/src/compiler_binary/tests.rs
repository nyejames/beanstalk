use super::*;
use std::path::{Path, PathBuf};

// ----------------------------
//  Release compiler path
// ----------------------------

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

// ----------------------------
//  Profiling compiler path
// ----------------------------

#[test]
fn profiling_compiler_path_without_suffix_uses_unix_name() {
    assert_eq!(
        profiling_compiler_path(""),
        Path::new("target/profiling/bean")
    );
}

#[test]
fn profiling_compiler_path_with_suffix_uses_platform_extension() {
    assert_eq!(
        profiling_compiler_path(".exe"),
        Path::new("target/profiling/bean.exe")
    );
}

// ----------------------------
//  CompilerBinary wrapper
// ----------------------------

#[test]
fn compiler_binary_exposes_borrowed_path() {
    let binary = CompilerBinary {
        path: PathBuf::from("target/release/bean.exe"),
        symbol_dirs: Vec::new(),
    };

    assert_eq!(binary.as_path(), Path::new("target/release/bean.exe"));
}

#[test]
fn compiler_binary_clones_release_path() {
    let binary = CompilerBinary {
        path: PathBuf::from("target/release/bean"),
        symbol_dirs: Vec::new(),
    };
    let cloned = binary.clone();

    assert_eq!(binary.as_path(), cloned.as_path());
}

#[test]
fn compiler_binary_clones_profiling_path() {
    let binary = CompilerBinary {
        path: PathBuf::from("target/profiling/bean"),
        symbol_dirs: vec![PathBuf::from("target/profiling")],
    };
    let cloned = binary.clone();

    assert_eq!(binary.as_path(), cloned.as_path());
    assert_eq!(binary.symbol_dirs, cloned.symbol_dirs);
}

#[test]
fn dsym_bundle_path_uses_binary_file_name() {
    assert_eq!(
        dsym_bundle_path(Path::new("target/profiling/bean")),
        Path::new("target/profiling/bean.dSYM")
    );
}
