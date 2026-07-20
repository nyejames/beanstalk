//! Unit tests for the source file kind registry.

use crate::builder_surface::source_file_kind_registry::{SourceFileKind, SourceFileKindRegistry};

#[test]
fn empty_registry_has_no_supported_kinds() {
    let registry = SourceFileKindRegistry::new();

    assert!(!registry.is_supported("bd"));
    assert!(!registry.is_supported("md"));
    assert!(!registry.is_supported("bst"));
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
fn unsupported_extension_returns_none() {
    let registry = SourceFileKindRegistry::new();

    assert_eq!(registry.kind_for_extension("css"), None);
    assert_eq!(registry.kind_for_extension("json"), None);
    assert_eq!(registry.kind_for_extension("html"), None);
}

#[test]
fn empty_registry_recognizes_markdown_but_does_not_support_it() {
    let registry = SourceFileKindRegistry::new();

    assert!(!registry.is_supported("md"));
    assert!(!registry.supports_recognized_extension("md"));
    assert_eq!(registry.kind_for_extension("md"), None);
}

#[test]
fn register_plain_markdown_makes_it_supported() {
    let mut registry = SourceFileKindRegistry::new();
    registry.register("md", SourceFileKind::PlainMarkdown);

    assert!(registry.is_supported("md"));
    assert!(registry.supports_recognized_extension("md"));
    assert_eq!(
        registry.kind_for_extension("md"),
        Some(SourceFileKind::PlainMarkdown)
    );
}

#[test]
fn supported_kinds_sorts_multiple_kinds_deterministically() {
    let mut registry = SourceFileKindRegistry::new();
    registry.register("md", SourceFileKind::PlainMarkdown);
    registry.register("bd", SourceFileKind::Beandown);

    let kinds = registry.supported_kinds();
    assert_eq!(kinds.len(), 2);
    assert_eq!(kinds[0].extension, "bd");
    assert_eq!(kinds[0].kind, SourceFileKind::Beandown);
    assert_eq!(kinds[1].extension, "md");
    assert_eq!(kinds[1].kind, SourceFileKind::PlainMarkdown);
}

#[test]
fn plain_markdown_extension_round_trips() {
    assert_eq!(
        SourceFileKind::from_extension("md"),
        Some(SourceFileKind::PlainMarkdown)
    );
    assert_eq!(SourceFileKind::PlainMarkdown.extension(), "md");
    assert_eq!(SourceFileKind::PlainMarkdown.extension_suffix(), ".md");
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
