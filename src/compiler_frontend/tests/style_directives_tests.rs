use super::*;

#[test]
fn builder_directive_overrides_builtin_by_name() {
    let builder_specs = vec![StyleDirectiveSpec::explicit_noop(
        "css",
        TemplateBodyMode::DiscardBalanced,
    )];
    let merged = StyleDirectiveRegistry::merged(&builder_specs);
    let css = merged
        .find("css")
        .expect("css directive should be present after merge");

    assert_eq!(css.body_mode, TemplateBodyMode::DiscardBalanced);
    assert_eq!(css.source, StyleDirectiveSource::Builder);
    assert_eq!(
        css.behavior,
        StyleDirectiveBehavior::ExplicitNoOp {
            argument_policy: NoOpDirectiveArgumentPolicy::Reject
        }
    );
}

#[test]
fn builder_directive_is_added_when_name_is_new() {
    let builder_specs = vec![StyleDirectiveSpec::explicit_noop(
        "custom",
        TemplateBodyMode::Balanced,
    )];
    let merged = StyleDirectiveRegistry::merged(&builder_specs);
    let custom = merged
        .find("custom")
        .expect("custom directive should be present after merge");

    assert_eq!(custom.body_mode, TemplateBodyMode::Balanced);
    assert_eq!(custom.source, StyleDirectiveSource::Builder);
    assert_eq!(
        custom.behavior,
        StyleDirectiveBehavior::ExplicitNoOp {
            argument_policy: NoOpDirectiveArgumentPolicy::Reject
        }
    );
}

#[test]
fn explicit_noop_policy_can_allow_optional_parenthesized_arguments() {
    let builder_specs = vec![StyleDirectiveSpec::explicit_noop_with_argument_policy(
        "brand",
        TemplateBodyMode::Normal,
        NoOpDirectiveArgumentPolicy::ConsumeOptionalParenthesized,
    )];
    let merged = StyleDirectiveRegistry::merged(&builder_specs);
    let brand = merged
        .find("brand")
        .expect("brand directive should be present after merge");

    assert_eq!(
        brand.behavior,
        StyleDirectiveBehavior::ExplicitNoOp {
            argument_policy: NoOpDirectiveArgumentPolicy::ConsumeOptionalParenthesized,
        }
    );
}

#[test]
fn built_ins_include_new_html_raw_and_escape_html_directives() {
    let built_ins = StyleDirectiveRegistry::built_ins();

    let html = built_ins
        .find("html")
        .expect("html directive should be registered as a built-in");
    assert_eq!(html.body_mode, TemplateBodyMode::Normal);
    assert_eq!(html.source, StyleDirectiveSource::BuiltIn);
    assert_eq!(
        html.behavior,
        StyleDirectiveBehavior::BuiltIn(BuiltInStyleDirectiveKind::Html)
    );

    let raw = built_ins
        .find("raw")
        .expect("raw directive should be registered as a built-in");
    assert_eq!(raw.body_mode, TemplateBodyMode::Normal);
    assert_eq!(raw.source, StyleDirectiveSource::BuiltIn);
    assert_eq!(
        raw.behavior,
        StyleDirectiveBehavior::BuiltIn(BuiltInStyleDirectiveKind::Raw)
    );

    let escape_html = built_ins
        .find("escape_html")
        .expect("escape_html directive should be registered as a built-in");
    assert_eq!(escape_html.body_mode, TemplateBodyMode::Normal);
    assert_eq!(escape_html.source, StyleDirectiveSource::BuiltIn);
    assert_eq!(
        escape_html.behavior,
        StyleDirectiveBehavior::BuiltIn(BuiltInStyleDirectiveKind::EscapeHtml)
    );
}
