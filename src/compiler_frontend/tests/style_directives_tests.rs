use super::*;

#[test]
fn builder_cannot_override_core_directive_by_name() {
    let builder_specs = vec![StyleDirectiveSpec::provided_no_op(
        "raw",
        TemplateBodyMode::Normal,
    )];

    let error = StyleDirectiveRegistry::merged(&builder_specs)
        .expect_err("overriding a core directive should fail");

    assert!(
        error
            .msg
            .contains("cannot override compiler core directive"),
        "unexpected error message: {}",
        error.msg
    );
}

#[test]
fn builder_directive_is_added_when_name_is_new() {
    let builder_specs = vec![StyleDirectiveSpec::provided_no_op(
        "custom",
        TemplateBodyMode::Balanced,
    )];
    let merged =
        StyleDirectiveRegistry::merged(&builder_specs).expect("registry merge should succeed");
    let custom = merged
        .find("custom")
        .expect("custom directive should be present after merge");

    assert_eq!(custom.body_mode, TemplateBodyMode::Balanced);
    assert!(matches!(custom.kind, StyleDirectiveKind::Provided(_)));
}

#[test]
fn provided_directive_contract_is_preserved() {
    let builder_specs = vec![StyleDirectiveSpec::provided(
        "brand",
        TemplateBodyMode::Normal,
        ProvidedStyleDirectiveSpec::new(
            Some(StyleDirectiveArgumentType::String),
            ProvidedStyleEffects {
                style_id: Some("brand"),
                ..ProvidedStyleEffects::default()
            },
            None,
        ),
    )];
    let merged =
        StyleDirectiveRegistry::merged(&builder_specs).expect("registry merge should succeed");
    let brand = merged
        .find("brand")
        .expect("brand directive should be present after merge");

    let StyleDirectiveKind::Provided(provided) = &brand.kind else {
        panic!("brand directive should be registered as provided behavior");
    };

    assert_eq!(
        provided.argument_type,
        Some(StyleDirectiveArgumentType::String)
    );
    assert_eq!(provided.style_effects.style_id, Some("brand"));
}

#[test]
fn core_built_ins_are_core_only() {
    let built_ins = StyleDirectiveRegistry::built_ins();

    for required in [
        "children", "slot", "insert", "fresh", "doc", "todo", "note", "code", "raw",
    ] {
        let directive = built_ins
            .find(required)
            .unwrap_or_else(|| panic!("missing core built-in directive '{required}'"));
        assert!(matches!(directive.kind, StyleDirectiveKind::Core(_)));
    }

    for non_core in ["markdown", "css", "html", "escape_html"] {
        assert!(
            built_ins.find(non_core).is_none(),
            "non-core directive '{non_core}' should not be compiler built-in"
        );
    }
}
