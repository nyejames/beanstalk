use super::*;

#[test]
fn builder_directive_overrides_builtin_by_name() {
    let builder_specs = vec![StyleDirectiveSpec::new(
        "css",
        TemplateBodyMode::DiscardBalanced,
    )];
    let merged = StyleDirectiveRegistry::merged(&builder_specs);
    let css = merged
        .find("css")
        .expect("css directive should be present after merge");

    assert_eq!(css.body_mode, TemplateBodyMode::DiscardBalanced);
    assert_eq!(css.source, StyleDirectiveSource::Builder);
}

#[test]
fn builder_directive_is_added_when_name_is_new() {
    let builder_specs = vec![StyleDirectiveSpec::new(
        "custom",
        TemplateBodyMode::Balanced,
    )];
    let merged = StyleDirectiveRegistry::merged(&builder_specs);
    let custom = merged
        .find("custom")
        .expect("custom directive should be present after merge");

    assert_eq!(custom.body_mode, TemplateBodyMode::Balanced);
    assert_eq!(custom.source, StyleDirectiveSource::Builder);
}
