use super::*;

#[test]
fn builder_cannot_override_frontend_owned_directive_by_name() {
    let builder_specs = vec![StyleDirectiveSpec::handler_no_op(
        "raw",
        TemplateBodyMode::Normal,
    )];

    let error = StyleDirectiveRegistry::merged(&builder_specs)
        .expect_err("overriding a frontend-owned directive should fail");

    assert!(
        error
            .msg
            .contains("cannot override frontend-owned directive"),
        "unexpected error message: {}",
        error.msg
    );
}

#[test]
fn builder_cannot_override_frontend_owned_markdown_directive_by_name() {
    let builder_specs = vec![StyleDirectiveSpec::handler_no_op(
        "markdown",
        TemplateBodyMode::Normal,
    )];

    let error = StyleDirectiveRegistry::merged(&builder_specs)
        .expect_err("overriding a frontend-owned formatter directive should fail");

    assert!(
        error
            .msg
            .contains("cannot override frontend-owned directive"),
        "unexpected error message: {}",
        error.msg
    );
}

#[test]
fn builder_directive_is_added_when_name_is_new() {
    let builder_specs = vec![StyleDirectiveSpec::handler_no_op(
        "custom",
        TemplateBodyMode::Balanced,
    )];
    let merged =
        StyleDirectiveRegistry::merged(&builder_specs).expect("registry merge should succeed");
    let custom = merged
        .find("custom")
        .expect("custom directive should be present after merge");

    assert_eq!(custom.body_mode, TemplateBodyMode::Balanced);
    assert!(matches!(custom.kind, StyleDirectiveKind::Handler(_)));
}

#[test]
fn handler_directive_contract_is_preserved() {
    let builder_specs = vec![StyleDirectiveSpec::handler(
        "brand",
        TemplateBodyMode::Normal,
        TemplateHeadCompatibility::fully_compatible_meaningful(),
        StyleDirectiveHandlerSpec::new(
            Some(StyleDirectiveArgumentType::String),
            StyleDirectiveEffects {
                style_id: Some("brand"),
                ..StyleDirectiveEffects::default()
            },
            None,
        ),
    )];
    let merged =
        StyleDirectiveRegistry::merged(&builder_specs).expect("registry merge should succeed");
    let brand = merged
        .find("brand")
        .expect("brand directive should be present after merge");

    let StyleDirectiveKind::Handler(handler) = &brand.kind else {
        panic!("brand directive should be registered as handler behavior");
    };

    assert_eq!(
        handler.argument_type,
        Some(StyleDirectiveArgumentType::String)
    );
    assert_eq!(handler.effects.style_id, Some("brand"));
}

#[test]
fn frontend_built_ins_have_expected_classification() {
    let built_ins = StyleDirectiveRegistry::built_ins();

    for required in [
        "children", "slot", "insert", "fresh", "doc", "todo", "note", "code", "raw",
    ] {
        let directive = built_ins
            .find(required)
            .unwrap_or_else(|| panic!("missing core built-in directive '{required}'"));
        assert!(matches!(directive.kind, StyleDirectiveKind::Core(_)));
    }

    let markdown = built_ins
        .find("markdown")
        .expect("missing frontend-owned '$markdown' directive");
    assert!(matches!(markdown.kind, StyleDirectiveKind::Handler(_)));

    for non_core in ["css", "html", "escape_html"] {
        assert!(
            built_ins.find(non_core).is_none(),
            "non-core directive '{non_core}' should not be compiler built-in"
        );
    }
}

#[test]
fn handler_no_op_defaults_to_fully_compatible_meaningful_head_item() {
    let no_op = StyleDirectiveSpec::handler_no_op("brand", TemplateBodyMode::Normal);
    assert_eq!(
        no_op.head_compatibility,
        TemplateHeadCompatibility::fully_compatible_meaningful()
    );
}

#[test]
fn frontend_built_in_head_compatibility_profiles_match_contract() {
    let built_ins = StyleDirectiveRegistry::built_ins();

    let slot = built_ins.find("slot").expect("missing '$slot' directive");
    assert_eq!(
        slot.head_compatibility.presence_tags,
        TemplateHeadTag::MEANINGFUL_ITEM | TemplateHeadTag::SLOT_DIRECTIVE
    );
    assert_eq!(
        slot.head_compatibility.required_absent_tags,
        TemplateHeadTag::MEANINGFUL_ITEM
    );
    assert_eq!(
        slot.head_compatibility.blocks_future_tags,
        TemplateHeadTag::MEANINGFUL_ITEM
    );

    let insert = built_ins.find("insert").expect("missing '$insert' directive");
    assert_eq!(
        insert.head_compatibility,
        TemplateHeadCompatibility::blocks_same(TemplateHeadTag::INSERT_DIRECTIVE)
    );

    for comment_directive in ["note", "todo", "doc"] {
        let directive = built_ins
            .find(comment_directive)
            .unwrap_or_else(|| panic!("missing '${comment_directive}' directive"));
        assert_eq!(
            directive.head_compatibility.presence_tags,
            TemplateHeadTag::MEANINGFUL_ITEM | TemplateHeadTag::COMMENT_DIRECTIVE
        );
        assert_eq!(
            directive.head_compatibility.required_absent_tags,
            TemplateHeadTag::MEANINGFUL_ITEM
        );
        assert_eq!(
            directive.head_compatibility.blocks_future_tags,
            TemplateHeadTag::MEANINGFUL_ITEM
        );
    }

    for formatter_directive in ["markdown", "raw", "code"] {
        let directive = built_ins
            .find(formatter_directive)
            .unwrap_or_else(|| panic!("missing '${formatter_directive}' directive"));
        assert!(
            directive
                .head_compatibility
                .presence_tags
                .intersects(TemplateHeadTag::FORMATTER_DIRECTIVE)
        );
        assert_eq!(
            directive.head_compatibility.blocks_future_tags,
            TemplateHeadTag::FORMATTER_DIRECTIVE
        );
    }
}
