//! Unit tests for the source file kind registry.

use crate::libraries::source_file_kind_registry::{SourceFileKind, SourceFileKindRegistry};

#[test]
fn empty_registry_has_no_supported_kinds() {
    let registry = SourceFileKindRegistry::new();

    assert!(!registry.is_supported("bd"));
    assert!(!registry.is_supported("bst"));
    assert_eq!(registry.supported_kinds().len(), 0);
}

#[test]
fn registry_default_is_empty() {
    let registry = SourceFileKindRegistry::default();

    assert!(!registry.is_supported("bd"));
    assert_eq!(registry.supported_kinds().len(), 0);
}

#[test]
fn register_beandown_lookup_succeeds() {
    let mut registry = SourceFileKindRegistry::new();
    registry.register("bd", SourceFileKind::Beandown);

    assert!(registry.is_supported("bd"));
    assert_eq!(
        registry.kind_for_extension("bd"),
        Some(SourceFileKind::Beandown)
    );
}

#[test]
fn compiler_owned_beanstalk_is_supported_without_registration() {
    let registry = SourceFileKindRegistry::new();

    assert!(registry.supports_recognized_extension("bst"));
    assert!(!registry.is_supported("bst"));
    assert!(!registry.supports_recognized_extension("bd"));
}

#[test]
fn register_multiple_kinds() {
    let mut registry = SourceFileKindRegistry::new();
    registry.register("bd", SourceFileKind::Beandown);

    let kinds = registry.supported_kinds();
    assert_eq!(kinds.len(), 1);
    assert_eq!(kinds[0].extension, "bd");
    assert_eq!(kinds[0].kind, SourceFileKind::Beandown);
}

#[test]
fn unsupported_extension_returns_none() {
    let registry = SourceFileKindRegistry::new();

    assert_eq!(registry.kind_for_extension("md"), None);
    assert_eq!(registry.kind_for_extension("css"), None);
    assert_eq!(registry.kind_for_extension("json"), None);
}

#[test]
fn registry_is_cloneable() {
    let mut registry = SourceFileKindRegistry::new();
    registry.register("bd", SourceFileKind::Beandown);

    let cloned = registry.clone();
    assert!(cloned.is_supported("bd"));
    assert_eq!(
        cloned.kind_for_extension("bd"),
        Some(SourceFileKind::Beandown)
    );
}
