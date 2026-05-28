//! Tests for the builder-owned core JS runtime module registry.
//!
//! WHAT: proves that the v1 registry contains exactly `@beanstalk/runtime` and that
//!       its source exports the expected `bstOk` and `bstErr` wrapper functions.

use crate::projects::html_project::external_js::runtime_module_registry::RuntimeModuleRegistry;

#[test]
fn v1_registry_contains_exactly_beanstalk_runtime() {
    let registry = RuntimeModuleRegistry::v1();

    let modules = registry.registered_modules();
    assert_eq!(
        modules.len(),
        1,
        "v1 registry should contain exactly one module"
    );
    assert_eq!(
        modules[0].specifier, "@beanstalk/runtime",
        "v1 registry should contain @beanstalk/runtime"
    );
}

#[test]
fn v1_runtime_module_source_exports_bst_ok_and_bst_err() {
    let registry = RuntimeModuleRegistry::v1();
    let source = registry
        .module_source("@beanstalk/runtime")
        .expect("@beanstalk/runtime should have source");

    assert!(
        source.contains("export function bstOk"),
        "runtime source should export bstOk"
    );
    assert!(
        source.contains("export function bstErr"),
        "runtime source should export bstErr"
    );
}

#[test]
fn v1_runtime_source_produces_success_wrapper() {
    let registry = RuntimeModuleRegistry::v1();
    let source = registry
        .module_source("@beanstalk/runtime")
        .expect("@beanstalk/runtime should have source");

    assert!(
        source.contains("{ ok: true, value: value }"),
        "bstOk should produce {{ ok: true, value: value }} wrapper"
    );
}

#[test]
fn v1_runtime_source_produces_error_wrapper() {
    let registry = RuntimeModuleRegistry::v1();
    let source = registry
        .module_source("@beanstalk/runtime")
        .expect("@beanstalk/runtime should have source");

    assert!(
        source.contains("{ ok: false, error: { code, message } }"),
        "bstErr should produce {{ ok: false, error: {{ code, message }} }} wrapper"
    );
}

#[test]
fn empty_registry_has_no_modules() {
    let registry = RuntimeModuleRegistry::empty();
    assert!(registry.registered_modules().is_empty());
    assert!(!registry.is_registered("@beanstalk/runtime"));
    assert!(registry.module_source("@beanstalk/runtime").is_none());
}

#[test]
fn is_registered_finds_exact_specifier() {
    let registry = RuntimeModuleRegistry::v1();
    assert!(registry.is_registered("@beanstalk/runtime"));
    assert!(!registry.is_registered("@beanstalk/other-runtime"));
    assert!(!registry.is_registered("./helper.js"));
}

#[test]
fn is_exported_name_finds_registered_names() {
    let registry = RuntimeModuleRegistry::v1();
    assert!(registry.is_exported_name("@beanstalk/runtime", "bstOk"));
    assert!(registry.is_exported_name("@beanstalk/runtime", "bstErr"));
}

#[test]
fn is_exported_name_rejects_unknown_names() {
    let registry = RuntimeModuleRegistry::v1();
    assert!(!registry.is_exported_name("@beanstalk/runtime", "nope"));
    assert!(!registry.is_exported_name("@beanstalk/other-runtime", "bstOk"));
}
