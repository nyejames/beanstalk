use super::*;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::template::{
    SlotKey, TemplateAtom, TemplateContent, TemplateSegment, TemplateSegmentOrigin,
};
use crate::compiler_frontend::ast::templates::template_head_parser::directive_args::{
    parse_optional_slot_target_argument, parse_required_slot_name_argument,
};
use crate::compiler_frontend::ast::templates::template_render_plan::{
    RenderPiece, RenderTextPiece, TemplateRenderPlan,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext, TopLevelDeclarationTable};
use crate::compiler_frontend::compiler_errors::SourceLocation;
use crate::compiler_frontend::compiler_messages::{DiagnosticPayload, InvalidTemplateSlotReason};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;

// Internal schema helpers are tested here because they drive composition
// correctness. These tests assert structural invariants rather than raw shapes.
use super::schema::collect_slot_schema;
use crate::compiler_frontend::ast::templates::template_slots::{
    SlotResolutionMode, SlotResolutionOutcome,
};
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::value_mode::ValueMode;
use std::rc::Rc;

fn template_tokens_from_source(source: &str, string_table: &mut StringTable) -> FileTokens {
    let scope = InternedPath::from_single_str("main.bst/#const_template0", string_table);
    let style_directives = StyleDirectiveRegistry::built_ins();
    let mut tokens = tokenize(
        source,
        &scope,
        crate::compiler_frontend::tokenizer::tokens::TokenizeMode::Normal,
        &style_directives,
        string_table,
        None,
    )
    .expect("tokenization should succeed");

    tokens.index = tokens
        .tokens
        .iter()
        .position(|token| matches!(token.kind, TokenKind::TemplateHead))
        .expect("expected a template opener");

    tokens
}

fn test_constant_context(scope: InternedPath) -> ScopeContext {
    let cwd = std::env::temp_dir();
    let resolver = ProjectPathResolver::new(
        cwd.clone(),
        cwd,
        &crate::libraries::SourceLibraryRegistry::default(),
    )
    .expect("test path resolver should be valid");
    ScopeContext::new(
        ContextKind::Constant,
        scope.clone(),
        Rc::new(TopLevelDeclarationTable::new(vec![])),
        ExternalPackageRegistry::default(),
        vec![],
    )
    .with_project_path_resolver(Some(resolver))
    .with_source_file_scope(scope)
    .with_path_format_config(PathStringFormatConfig::default())
}

fn template_from_source(source: &str, string_table: &mut StringTable) -> Template {
    let mut tokens = template_tokens_from_source(source, string_table);
    let context = test_constant_context(tokens.src_path.to_owned());
    Template::new(&mut tokens, &context, Vec::new(), string_table).unwrap()
}

fn expect_composed_slot_resolution(outcome: SlotResolutionOutcome) -> TemplateContent {
    match outcome {
        SlotResolutionOutcome::Composed(content) => content,
        SlotResolutionOutcome::Runtime(_) => panic!("expected composed slot resolution"),
    }
}

#[test]
fn test_parse_positional_slot() {
    let mut string_table = StringTable::new();
    let mut tokens = template_tokens_from_source("[$slot(1)]", &mut string_table);

    // Position at directive
    tokens.advance();

    let result = parse_optional_slot_target_argument(&mut tokens);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), SlotKey::Positional(1));
}

#[test]
fn test_parse_positional_slot_zero_errors() {
    let mut string_table = StringTable::new();
    let mut tokens = template_tokens_from_source("[$slot(0)]", &mut string_table);

    tokens.advance();

    let result = parse_optional_slot_target_argument(&mut tokens);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err().payload,
        DiagnosticPayload::UnexpectedToken {
            found: TokenKind::IntLiteral(0),
        }
    ));
}

#[test]
fn test_parse_insert_positional_errors() {
    let mut string_table = StringTable::new();
    let mut tokens = template_tokens_from_source("[$insert(1)]", &mut string_table);

    tokens.advance();

    let result = parse_required_slot_name_argument(&mut tokens);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err().payload,
        DiagnosticPayload::UnexpectedToken {
            found: TokenKind::IntLiteral(1),
        }
    ));
}

#[test]
fn test_positional_composition_basic() {
    let mut string_table = StringTable::new();
    let wrapper = template_from_source("[:[$slot(1)]-[$slot(2)]]", &mut string_table);

    // Manually build fill content for isolation
    let fill_content = TemplateContent {
        atoms: vec![
            TemplateAtom::Content(TemplateSegment::new(
                Expression::template(
                    template_from_source("[:a]", &mut string_table),
                    ValueMode::ImmutableOwned,
                ),
                TemplateSegmentOrigin::Body,
            )),
            TemplateAtom::Content(TemplateSegment::new(
                Expression::template(
                    template_from_source("[:b]", &mut string_table),
                    ValueMode::ImmutableOwned,
                ),
                TemplateSegmentOrigin::Body,
            )),
        ],
    };

    let location = SourceLocation::default();
    let result = resolve_slot_application(
        &wrapper,
        fill_content,
        &location,
        &string_table,
        SlotResolutionMode::AllowRuntimePlans,
    )
    .unwrap();
    let composed = expect_composed_slot_resolution(result);

    // result should contain [a] and [b]
    assert_eq!(composed.atoms.len(), 3); // "[a]", "-", "[b]"
    // The atoms for slots are expanded.
}

#[test]
fn test_positional_composition_with_default_overflow() {
    let mut string_table = StringTable::new();
    let wrapper = template_from_source("[:[$slot(1)]-[$slot]]", &mut string_table);

    let fill_content = TemplateContent {
        atoms: vec![
            TemplateAtom::Content(TemplateSegment::new(
                Expression::template(
                    template_from_source("[:a]", &mut string_table),
                    ValueMode::ImmutableOwned,
                ),
                TemplateSegmentOrigin::Body,
            )),
            TemplateAtom::Content(TemplateSegment::new(
                Expression::template(
                    template_from_source("[:b]", &mut string_table),
                    ValueMode::ImmutableOwned,
                ),
                TemplateSegmentOrigin::Body,
            )),
            TemplateAtom::Content(TemplateSegment::new(
                Expression::template(
                    template_from_source("[:c]", &mut string_table),
                    ValueMode::ImmutableOwned,
                ),
                TemplateSegmentOrigin::Body,
            )),
        ],
    };

    let location = SourceLocation::default();
    let result = resolve_slot_application(
        &wrapper,
        fill_content,
        &location,
        &string_table,
        SlotResolutionMode::AllowRuntimePlans,
    )
    .unwrap();
    let composed = expect_composed_slot_resolution(result);

    // [$slot(1)] should get [a]
    // [$slot] should get [b] and [c] (both are overflow)
    assert_eq!(composed.atoms.len(), 4); // "[a]", "-", "[b]", "[c]"
}

#[test]
fn test_positional_composition_overflow_error() {
    let mut string_table = StringTable::new();
    let wrapper = template_from_source("[:[$slot(1)]]", &mut string_table);

    let fill_content = TemplateContent {
        atoms: vec![
            TemplateAtom::Content(TemplateSegment::new(
                Expression::template(
                    template_from_source("[:a]", &mut string_table),
                    ValueMode::ImmutableOwned,
                ),
                TemplateSegmentOrigin::Body,
            )),
            TemplateAtom::Content(TemplateSegment::new(
                Expression::template(
                    template_from_source("[:b]", &mut string_table),
                    ValueMode::ImmutableOwned,
                ),
                TemplateSegmentOrigin::Body,
            )),
        ],
    };

    let location = SourceLocation::default();
    let result = resolve_slot_application(
        &wrapper,
        fill_content,
        &location,
        &string_table,
        SlotResolutionMode::AllowRuntimePlans,
    );

    assert!(result.is_err());
    let error = result.unwrap_err();
    let super::error::TemplateSlotError::Diagnostic(diagnostic) = error else {
        panic!("expected positional slot overflow diagnostic");
    };
    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidTemplateSlot {
            reason: InvalidTemplateSlotReason::ExtraLooseContentWithoutDefaultSlot,
            ..
        }
    ));
}

#[test]
fn test_positional_composition_repeated_slots() {
    let mut string_table = StringTable::new();
    let wrapper = template_from_source("[:[$slot(1)]and[$slot(1)]]", &mut string_table);

    let fill_content = TemplateContent {
        atoms: vec![TemplateAtom::Content(TemplateSegment::new(
            Expression::template(
                template_from_source("[:a]", &mut string_table),
                ValueMode::ImmutableOwned,
            ),
            TemplateSegmentOrigin::Body,
        ))],
    };

    let location = SourceLocation::default();
    let result = resolve_slot_application(
        &wrapper,
        fill_content,
        &location,
        &string_table,
        SlotResolutionMode::AllowRuntimePlans,
    )
    .unwrap();
    let composed = expect_composed_slot_resolution(result);

    // Both should get [a]
    assert_eq!(composed.atoms.len(), 3); // "[a]", "and", "[a]"
}

#[test]
fn test_positional_composition_mixed_content() {
    let mut string_table = StringTable::new();
    let wrapper = template_from_source("[:[$slot(1)]:[$slot]]", &mut string_table);

    // Mixed text and templates
    let fill_content = TemplateContent {
        atoms: vec![
            TemplateAtom::Content(TemplateSegment::new(
                Expression::template(
                    template_from_source("[:a]", &mut string_table),
                    ValueMode::ImmutableOwned,
                ),
                TemplateSegmentOrigin::Body,
            )),
            TemplateAtom::Content(TemplateSegment::new(
                Expression::string_slice(
                    string_table.intern(" text "),
                    SourceLocation::default(),
                    ValueMode::ImmutableOwned,
                ),
                TemplateSegmentOrigin::Body,
            )),
            TemplateAtom::Content(TemplateSegment::new(
                Expression::template(
                    template_from_source("[:b]", &mut string_table),
                    ValueMode::ImmutableOwned,
                ),
                TemplateSegmentOrigin::Body,
            )),
        ],
    };

    let location = SourceLocation::default();
    let result = resolve_slot_application(
        &wrapper,
        fill_content,
        &location,
        &string_table,
        SlotResolutionMode::AllowRuntimePlans,
    )
    .unwrap();
    let composed = expect_composed_slot_resolution(result);

    // [a] -> [$slot(1)]
    // " text " and [b] -> [$slot]
    assert_eq!(composed.atoms.len(), 4); // "[a]", ":", " text ", "[b]"
}

// ------------------------------------------------------------------------
// Slot schema tests
// ------------------------------------------------------------------------

#[test]
fn schema_collects_default_named_and_positional_slots() {
    let mut string_table = StringTable::new();
    let wrapper = template_from_source("[:[$slot]-[$slot(\"a\")]-[$slot(1)]]", &mut string_table);

    let schema = collect_slot_schema(&wrapper, &SourceLocation::default()).unwrap();

    assert!(schema.has_default_slot);
    assert_eq!(schema.named_slots.len(), 1);
    assert_eq!(schema.positional_slots.len(), 1);
    assert!(schema.accepts_target(&SlotKey::Default));
    assert!(schema.has_any_slots());
}

#[test]
fn schema_duplicate_default_slot_errors() {
    let mut string_table = StringTable::new();
    let wrapper = template_from_source("[:[$slot]-[$slot]]", &mut string_table);

    let result = collect_slot_schema(&wrapper, &SourceLocation::default());
    assert!(result.is_err());
    let error = result.unwrap_err();
    let super::error::TemplateSlotError::Diagnostic(diagnostic) = error else {
        panic!("expected duplicate default slot diagnostic");
    };
    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidTemplateSlot {
            reason: InvalidTemplateSlotReason::MultipleDefaultSlots,
            ..
        }
    ));
}

#[test]
fn schema_accepts_correct_targets_and_rejects_unknown() {
    let mut string_table = StringTable::new();
    let wrapper = template_from_source("[:[$slot(\"style\")]-[$slot(2)]]", &mut string_table);

    let schema = collect_slot_schema(&wrapper, &SourceLocation::default()).unwrap();

    let style_name = string_table.intern("style");
    assert!(schema.accepts_target(&SlotKey::Named(style_name)));
    assert!(schema.accepts_target(&SlotKey::Positional(2)));
    assert!(!schema.accepts_target(&SlotKey::Default));
    assert!(!schema.accepts_target(&SlotKey::Positional(1)));
}

#[test]
fn schema_collects_nested_template_slots() {
    // Build a wrapper whose content contains a regular template expression
    // (not a slot definition itself) that itself contains a slot atom.
    // This tests the recursive walk in collect_slot_schema_atoms.
    let mut string_table = StringTable::new();
    let mut wrapper = Template::empty();

    let inner_template = template_from_source("[:[$slot(\"deep\")]]", &mut string_table);
    let inner_expr = Expression::template(inner_template, ValueMode::ImmutableOwned);

    wrapper.content.add(inner_expr);

    let schema = collect_slot_schema(&wrapper, &SourceLocation::default()).unwrap();

    let deep_name = string_table.intern("deep");
    assert!(schema.accepts_target(&SlotKey::Named(deep_name)));
}

#[test]
fn schema_ordered_positional_slots_is_sorted() {
    let mut string_table = StringTable::new();
    let wrapper = template_from_source("[:[$slot(3)]-[$slot(1)]-[$slot(2)]]", &mut string_table);

    let schema = collect_slot_schema(&wrapper, &SourceLocation::default()).unwrap();
    let ordered: Vec<usize> = schema.ordered_positional_slots().cloned().collect();

    assert_eq!(ordered, vec![1, 2, 3]);
}

// ------------------------------------------------------------------------
// Routing model tests
// ------------------------------------------------------------------------

#[test]
fn route_slot_contributions_partitions_explicit_inserts_and_loose_atoms() {
    let mut string_table = StringTable::new();
    let wrapper = template_from_source("[:[$slot(1)]-[$slot]]", &mut string_table);

    let a_template = template_from_source("[:a]", &mut string_table);
    let b_template = template_from_source("[:b]", &mut string_table);

    let fill_content = TemplateContent {
        atoms: vec![
            TemplateAtom::Content(TemplateSegment::new(
                Expression::template(a_template, ValueMode::ImmutableOwned),
                TemplateSegmentOrigin::Body,
            )),
            TemplateAtom::Content(TemplateSegment::new(
                Expression::template(b_template, ValueMode::ImmutableOwned),
                TemplateSegmentOrigin::Body,
            )),
        ],
    };

    let routed = super::composition::route_slot_contributions(
        &wrapper,
        fill_content,
        &SourceLocation::default(),
        &string_table,
    )
    .unwrap();

    assert!(routed.schema.has_default_slot);
    assert!(routed.schema.accepts_target(&SlotKey::Positional(1)));

    let default_atoms = routed.contributions.atoms_for_slot(&SlotKey::Default);
    let pos1_atoms = routed.contributions.atoms_for_slot(&SlotKey::Positional(1));

    assert_eq!(pos1_atoms.len(), 1);
    assert_eq!(default_atoms.len(), 1);
}

#[test]
fn route_slot_contributions_routes_loose_to_positional_then_default() {
    let mut string_table = StringTable::new();
    let wrapper = template_from_source("[:[$slot(1)]-[$slot(2)]-[$slot]]", &mut string_table);

    let a_template = template_from_source("[:a]", &mut string_table);
    let b_template = template_from_source("[:b]", &mut string_table);
    let c_template = template_from_source("[:c]", &mut string_table);

    let fill_content = TemplateContent {
        atoms: vec![
            TemplateAtom::Content(TemplateSegment::new(
                Expression::template(a_template, ValueMode::ImmutableOwned),
                TemplateSegmentOrigin::Body,
            )),
            TemplateAtom::Content(TemplateSegment::new(
                Expression::template(b_template, ValueMode::ImmutableOwned),
                TemplateSegmentOrigin::Body,
            )),
            TemplateAtom::Content(TemplateSegment::new(
                Expression::template(c_template, ValueMode::ImmutableOwned),
                TemplateSegmentOrigin::Body,
            )),
        ],
    };

    let routed = super::composition::route_slot_contributions(
        &wrapper,
        fill_content,
        &SourceLocation::default(),
        &string_table,
    )
    .unwrap();

    assert_eq!(
        routed
            .contributions
            .atoms_for_slot(&SlotKey::Positional(1))
            .len(),
        1
    );
    assert_eq!(
        routed
            .contributions
            .atoms_for_slot(&SlotKey::Positional(2))
            .len(),
        1
    );
    assert_eq!(
        routed.contributions.atoms_for_slot(&SlotKey::Default).len(),
        1
    );
}

#[test]
fn route_slot_contributions_detects_runtime_contribution_content() {
    let mut string_table = StringTable::new();
    let wrapper = template_from_source("[:[$slot]]", &mut string_table);
    assert!(wrapper.has_unresolved_slots());

    let runtime_expression = Expression::runtime_with_type_id(
        Vec::new(),
        DataType::Int,
        builtin_type_ids::INT,
        SourceLocation::default(),
        ValueMode::ImmutableOwned,
    );
    let fill_content = TemplateContent {
        atoms: vec![TemplateAtom::Content(TemplateSegment::new(
            runtime_expression,
            TemplateSegmentOrigin::Body,
        ))],
    };

    let routed = super::composition::route_slot_contributions(
        &wrapper,
        fill_content,
        &SourceLocation::default(),
        &string_table,
    )
    .unwrap();

    assert!(
        super::runtime_plan::routed_slot_contributions_contain_runtime_content(&routed),
        "runtime-producing slot content should be visible to the Phase 3 planning model"
    );
}

#[test]
fn runtime_slot_application_plan_model_holds_wrapper_plan_and_contributions() {
    let mut string_table = StringTable::new();
    let wrapper = template_from_source("[:[$slot]]", &mut string_table);
    assert!(wrapper.has_unresolved_slots());
    let site_id = super::runtime_plan::RuntimeSlotSiteId(0);
    let source_id = super::runtime_plan::RuntimeSlotContributionSourceId(0);
    let wrapper_plan = TemplateRenderPlan {
        pieces: vec![RenderPiece::RuntimeSlotSite(site_id)],
    };

    let contrib_template = template_from_source("[:hello]", &mut string_table);
    let contrib_content = TemplateContent {
        atoms: vec![TemplateAtom::Content(TemplateSegment::new(
            Expression::template(contrib_template, ValueMode::ImmutableOwned),
            TemplateSegmentOrigin::Body,
        ))],
    };

    let contribution = super::runtime_plan::RuntimeSlotContributionSource {
        id: source_id,
        target: SlotKey::Default,
        render_plan: TemplateRenderPlan::from_content(&contrib_content),
        renders_wrapper_unconditionally: true,
        location: SourceLocation::default(),
    };

    let plan = super::runtime_plan::RuntimeSlotApplicationPlan {
        wrapper_plan,
        contribution_sources: vec![contribution],
        slot_sites: vec![super::runtime_plan::RuntimeSlotSitePlan {
            id: site_id,
            key: SlotKey::Default,
            render_plan: super::runtime_plan::RuntimeSlotSiteRenderPlan {
                pieces: vec![
                    super::runtime_plan::RuntimeSlotSitePiece::ContributionSource(source_id),
                ],
            },
            location: SourceLocation::default(),
        }],
        location: SourceLocation::default(),
    };

    assert!(!plan.wrapper_plan.pieces.is_empty());
    assert_eq!(plan.contribution_sources.len(), 1);
    assert_eq!(plan.slot_sites.len(), 1);
    assert!(!plan.contribution_sources[0].render_plan.pieces.is_empty());
}

#[test]
fn runtime_slot_application_plan_builds_render_plan_for_runtime_contribution() {
    let mut string_table = StringTable::new();
    let wrapper = template_from_source("[:[$slot]]", &mut string_table);
    let runtime_expression = Expression::runtime_with_type_id(
        Vec::new(),
        DataType::Int,
        builtin_type_ids::INT,
        SourceLocation::default(),
        ValueMode::ImmutableOwned,
    );

    let fill_content = TemplateContent {
        atoms: vec![TemplateAtom::Content(TemplateSegment::new(
            runtime_expression,
            TemplateSegmentOrigin::Body,
        ))],
    };

    let outcome = super::runtime_plan::resolve_slot_application(
        &wrapper,
        fill_content,
        &SourceLocation::default(),
        &string_table,
        SlotResolutionMode::AllowRuntimePlans,
    )
    .unwrap();

    let SlotResolutionOutcome::Runtime(plan) = outcome else {
        panic!("runtime contribution should force runtime slot planning");
    };

    let contribution = plan
        .contribution_sources
        .iter()
        .find(|source| source.target == SlotKey::Default)
        .expect("runtime default contribution should be planned");

    assert!(!contribution.render_plan.pieces.is_empty());
    assert!(!contribution.renders_wrapper_unconditionally);
    assert_eq!(plan.slot_sites.len(), 1);
}

#[test]
fn runtime_slot_application_plan_builds_one_site_per_repeated_placeholder() {
    let mut string_table = StringTable::new();
    let wrapper = template_from_source("[:[$slot(1)] and [$slot(1)]]", &mut string_table);
    let runtime_expression = Expression::runtime_with_type_id(
        Vec::new(),
        DataType::Int,
        builtin_type_ids::INT,
        SourceLocation::default(),
        ValueMode::ImmutableOwned,
    );

    let fill_content = TemplateContent {
        atoms: vec![TemplateAtom::Content(TemplateSegment::new(
            runtime_expression,
            TemplateSegmentOrigin::Body,
        ))],
    };

    let outcome = super::runtime_plan::resolve_slot_application(
        &wrapper,
        fill_content,
        &SourceLocation::default(),
        &string_table,
        SlotResolutionMode::AllowRuntimePlans,
    )
    .unwrap();

    let SlotResolutionOutcome::Runtime(plan) = outcome else {
        panic!("runtime contribution should force runtime slot planning");
    };

    assert_eq!(plan.contribution_sources.len(), 1);
    assert_eq!(plan.slot_sites.len(), 2);
    assert!(matches!(
        plan.wrapper_plan.pieces[0],
        RenderPiece::RuntimeSlotSite(super::runtime_plan::RuntimeSlotSiteId(0))
    ));
    assert!(matches!(
        plan.wrapper_plan.pieces[2],
        RenderPiece::RuntimeSlotSite(super::runtime_plan::RuntimeSlotSiteId(1))
    ));
}

#[test]
fn runtime_slot_application_plan_builds_one_source_per_contribution_atom() {
    let mut string_table = StringTable::new();
    let wrapper = template_from_source("[:[$slot]]", &mut string_table);
    let first_expression = Expression::runtime_with_type_id(
        Vec::new(),
        DataType::Int,
        builtin_type_ids::INT,
        SourceLocation::default(),
        ValueMode::ImmutableOwned,
    );
    let second_expression = Expression::runtime_with_type_id(
        Vec::new(),
        DataType::Int,
        builtin_type_ids::INT,
        SourceLocation::default(),
        ValueMode::ImmutableOwned,
    );

    let fill_content = TemplateContent {
        atoms: vec![
            TemplateAtom::Content(TemplateSegment::new(
                first_expression,
                TemplateSegmentOrigin::Body,
            )),
            TemplateAtom::Content(TemplateSegment::new(
                second_expression,
                TemplateSegmentOrigin::Body,
            )),
        ],
    };

    let outcome = super::runtime_plan::resolve_slot_application(
        &wrapper,
        fill_content,
        &SourceLocation::default(),
        &string_table,
        SlotResolutionMode::AllowRuntimePlans,
    )
    .unwrap();

    let SlotResolutionOutcome::Runtime(plan) = outcome else {
        panic!("runtime contributions should force runtime slot planning");
    };

    assert_eq!(plan.contribution_sources.len(), 2);
    assert_eq!(
        plan.contribution_sources[0].id,
        super::runtime_plan::RuntimeSlotContributionSourceId(0)
    );
    assert_eq!(
        plan.contribution_sources[1].id,
        super::runtime_plan::RuntimeSlotContributionSourceId(1)
    );
    assert_eq!(plan.slot_sites.len(), 1);
}

#[test]
fn runtime_slot_contribution_render_plan_remaps_string_ids() {
    let mut local_table = StringTable::new();
    let contribution_text = local_table.intern("contribution");
    let target_name = local_table.intern("title");
    let source_id = super::runtime_plan::RuntimeSlotContributionSourceId(0);

    let mut plan = super::runtime_plan::RuntimeSlotApplicationPlan {
        wrapper_plan: TemplateRenderPlan::from_content(&TemplateContent::default()),
        contribution_sources: vec![super::runtime_plan::RuntimeSlotContributionSource {
            id: source_id,
            target: SlotKey::Named(target_name),
            render_plan: TemplateRenderPlan {
                pieces: vec![RenderPiece::Text(RenderTextPiece {
                    text: contribution_text,
                    location: SourceLocation::default(),
                })],
            },
            renders_wrapper_unconditionally: true,
            location: SourceLocation::default(),
        }],
        slot_sites: vec![super::runtime_plan::RuntimeSlotSitePlan {
            id: super::runtime_plan::RuntimeSlotSiteId(0),
            key: SlotKey::Named(target_name),
            render_plan: super::runtime_plan::RuntimeSlotSiteRenderPlan {
                pieces: vec![super::runtime_plan::RuntimeSlotSitePiece::Render(Box::new(
                    RenderPiece::Text(RenderTextPiece {
                        text: contribution_text,
                        location: SourceLocation::default(),
                    }),
                ))],
            },
            location: SourceLocation::default(),
        }],
        location: SourceLocation::default(),
    };

    let mut global_table = StringTable::new();
    global_table.intern("preexisting");
    let remap = global_table.merge_from(&local_table);

    plan.remap_string_ids(&remap);

    let RenderPiece::Text(text) = &plan.contribution_sources[0].render_plan.pieces[0] else {
        panic!("expected contribution render plan to keep its text piece");
    };

    assert_eq!(global_table.resolve(text.text), "contribution");

    let super::runtime_plan::RuntimeSlotSitePiece::Render(site_piece) =
        &plan.slot_sites[0].render_plan.pieces[0]
    else {
        panic!("expected site render plan to keep render piece");
    };
    let RenderPiece::Text(text) = site_piece.as_ref() else {
        panic!("expected site render plan to remap render text");
    };
    assert_eq!(global_table.resolve(text.text), "contribution");
}
