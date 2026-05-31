use super::{
    BorrowAccessKind, BorrowDiagnosticKind, CompilerDiagnostic, ConfigDiagnosticKind,
    DeferredFeatureDiagnosticKind, DiagnosticBag, DiagnosticCategory, DiagnosticKind,
    DiagnosticLabel, DiagnosticLabelMessage, DiagnosticPayload, DiagnosticPlace,
    DiagnosticSeverity, GenericApplicationErrorReason, ImportClauseKind, ImportDiagnosticKind,
    IncompatibleChoiceComparisonReason, InfrastructureDiagnosticKind,
    InvalidAssignmentTargetReason, InvalidChoiceVariantReason, InvalidCollectionTypeReason,
    InvalidConfigReason, InvalidFunctionSignatureReason, InvalidGenericParameterReason,
    InvalidImportClauseReason, InvalidResultOperandReason, InvalidSignatureMemberReason,
    InvalidTemplateDirectiveReason, InvalidTypeAnnotationReason, NameNamespace,
    NumberLiteralErrorReason, PathKind, RuleDiagnosticKind, SyntaxDiagnosticKind,
    TypeAnnotationContext, TypeDiagnosticKind, TypeMismatchContext, UnsupportedOperatorCategory,
};
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_messages::render::{
    DiagnosticRenderContext, dev_server, terminal, terse,
};
use crate::compiler_frontend::compiler_messages::source_location::{CharPosition, SourceLocation};
use crate::compiler_frontend::datatypes::definitions::StructTypeDefinition;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::{NominalTypeId, builtin_type_ids};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{PathTokenItem, TokenKind};
use std::collections::HashSet;
use std::path::Path;

const DIAGNOSTIC_PAYLOAD_SOURCE: &str = include_str!("../diagnostic_payload/mod.rs");

fn location(path: InternedPath) -> SourceLocation {
    SourceLocation::new(
        path,
        CharPosition {
            line_number: 1,
            char_column: 2,
        },
        CharPosition {
            line_number: 1,
            char_column: 4,
        },
    )
}

fn unknown_name_diagnostic(
    name: StringId,
    namespace: NameNamespace,
    location: SourceLocation,
) -> CompilerDiagnostic {
    CompilerDiagnostic::new(
        DiagnosticKind::Rule(RuleDiagnosticKind::UnknownName),
        location,
        DiagnosticPayload::UnknownName { name, namespace },
    )
}

fn borrow_conflict_diagnostic(
    place: DiagnosticPlace,
    existing_access: BorrowAccessKind,
    requested_access: BorrowAccessKind,
    location: SourceLocation,
) -> CompilerDiagnostic {
    CompilerDiagnostic::new(
        DiagnosticKind::Borrow(BorrowDiagnosticKind::BorrowConflict),
        location,
        DiagnosticPayload::BorrowConflict {
            place,
            existing_access,
            requested_access,
        },
    )
}

#[test]
fn descriptor_codes_are_stable_and_non_empty() {
    let cases = [
        (
            DiagnosticKind::Syntax(SyntaxDiagnosticKind::ExpectedToken),
            "BST-SYNTAX-0001",
            "Expected token",
            DiagnosticSeverity::Error,
        ),
        (
            DiagnosticKind::Syntax(SyntaxDiagnosticKind::UnexpectedToken),
            "BST-SYNTAX-0002",
            "Unexpected token",
            DiagnosticSeverity::Error,
        ),
        (
            DiagnosticKind::Syntax(SyntaxDiagnosticKind::UnexpectedTrailingComma),
            "BST-SYNTAX-0003",
            "Unexpected trailing comma",
            DiagnosticSeverity::Error,
        ),
        (
            DiagnosticKind::Rule(RuleDiagnosticKind::UnknownName),
            "BST-RULE-0001",
            "Unknown name",
            DiagnosticSeverity::Error,
        ),
        (
            DiagnosticKind::Rule(RuleDiagnosticKind::UnusedVariable),
            "BST-RULE-0010",
            "Unused variable",
            DiagnosticSeverity::Warning,
        ),
        (
            DiagnosticKind::Type(TypeDiagnosticKind::TypeMismatch),
            "BST-TYPE-0001",
            "Type mismatch",
            DiagnosticSeverity::Error,
        ),
        (
            DiagnosticKind::Import(ImportDiagnosticKind::MissingImportTarget),
            "BST-IMPORT-0005",
            "Missing import target",
            DiagnosticSeverity::Error,
        ),
        (
            DiagnosticKind::Borrow(BorrowDiagnosticKind::BorrowConflict),
            "BST-BORROW-0001",
            "Borrow conflict",
            DiagnosticSeverity::Error,
        ),
        (
            DiagnosticKind::Config(ConfigDiagnosticKind::InvalidConfig),
            "BST-CONFIG-0001",
            "Invalid config",
            DiagnosticSeverity::Error,
        ),
        (
            DiagnosticKind::Infrastructure(InfrastructureDiagnosticKind::InfrastructureFailure),
            "BST-INFRA-0001",
            "Infrastructure failure",
            DiagnosticSeverity::Error,
        ),
        (
            DiagnosticKind::DeferredFeature(DeferredFeatureDiagnosticKind::DeferredFeature),
            "BST-DEFERRED-0001",
            "Deferred feature",
            DiagnosticSeverity::Error,
        ),
    ];

    for (kind, expected_code, expected_title, expected_severity) in cases {
        let descriptor = kind.descriptor();
        assert_eq!(descriptor.code, expected_code);
        assert_eq!(descriptor.title, expected_title);
        assert_eq!(descriptor.default_severity, expected_severity);
        assert!(!descriptor.title.is_empty());
        assert!(!kind.code().is_empty());
    }
}

#[test]
fn every_diagnostic_descriptor_has_a_unique_category_code() {
    let mut codes = HashSet::new();

    for kind in DiagnosticKind::all() {
        let descriptor = kind.descriptor();

        assert!(
            !descriptor.code.is_empty(),
            "{kind:?} must have a stable diagnostic code",
        );
        assert!(
            !descriptor.title.is_empty(),
            "{kind:?} must have a user-facing descriptor title",
        );
        assert!(
            descriptor
                .code
                .starts_with(expected_code_prefix(kind.category())),
            "{kind:?} code '{}' does not match its category {:?}",
            descriptor.code,
            kind.category(),
        );
        assert!(
            codes.insert(descriptor.code),
            "{kind:?} reuses diagnostic code '{}'",
            descriptor.code,
        );
    }
}

#[test]
fn old_error_payload_has_been_removed_from_diagnostic_payloads() {
    assert!(
        !DIAGNOSTIC_PAYLOAD_SOURCE.contains(concat!("Legacy", "Error")),
        "diagnostic payloads should no longer expose the old error variant",
    );
}

fn expected_code_prefix(category: DiagnosticCategory) -> &'static str {
    match category {
        DiagnosticCategory::Syntax => "BST-SYNTAX-",
        DiagnosticCategory::Type => "BST-TYPE-",
        DiagnosticCategory::Rule => "BST-RULE-",
        DiagnosticCategory::Import => "BST-IMPORT-",
        DiagnosticCategory::Borrow => "BST-BORROW-",
        DiagnosticCategory::Config => "BST-CONFIG-",
        DiagnosticCategory::Infrastructure => "BST-INFRA-",
        DiagnosticCategory::DeferredFeature => "BST-DEFERRED-",
    }
}

#[test]
fn category_and_default_severity_derive_from_kind() {
    let syntax = DiagnosticKind::Syntax(SyntaxDiagnosticKind::ExpectedToken);
    let type_mismatch = DiagnosticKind::Type(TypeDiagnosticKind::TypeMismatch);
    let import = DiagnosticKind::Import(ImportDiagnosticKind::MissingImportTarget);

    assert_eq!(syntax.category(), DiagnosticCategory::Syntax);
    assert_eq!(type_mismatch.category(), DiagnosticCategory::Type);
    assert_eq!(import.category(), DiagnosticCategory::Import);
    assert_eq!(syntax.default_severity(), DiagnosticSeverity::Error);
    assert_eq!(type_mismatch.default_severity(), DiagnosticSeverity::Error);
}

#[test]
fn explicit_severity_can_override_descriptor_default() {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("main.bst", &mut string_table);
    let diagnostic = CompilerDiagnostic::with_severity(
        DiagnosticKind::Rule(RuleDiagnosticKind::UnknownName),
        DiagnosticSeverity::Warning,
        location(source_path),
        DiagnosticPayload::UnknownName {
            name: string_table.intern("unused_name"),
            namespace: NameNamespace::Value,
        },
    );

    assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
}

#[test]
fn diagnostic_bag_tracks_errors_warnings_and_order() {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("main.bst", &mut string_table);
    let first = unknown_name_diagnostic(
        string_table.intern("missing"),
        NameNamespace::Value,
        location(source_path.clone()),
    );
    let second = CompilerDiagnostic::with_severity(
        DiagnosticKind::Rule(RuleDiagnosticKind::UnknownName),
        DiagnosticSeverity::Warning,
        location(source_path),
        DiagnosticPayload::UnknownName {
            name: string_table.intern("warning_name"),
            namespace: NameNamespace::Value,
        },
    );

    let mut bag = DiagnosticBag::new();
    bag.push(first.clone());
    bag.push(second.clone());

    assert!(bag.has_errors());
    assert!(bag.has_warnings());
    assert_eq!(bag.errors().count(), 1);
    assert_eq!(bag.warnings().count(), 1);
    assert_eq!(bag.diagnostics(), &[first, second]);
    assert_eq!(bag.into_diagnostics().len(), 2);
}

#[test]
fn compiler_messages_counts_and_order_come_from_structured_diagnostics() {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("main.bst", &mut string_table);
    let error = unknown_name_diagnostic(
        string_table.intern("missing"),
        NameNamespace::Value,
        location(source_path.clone()),
    );
    let warning = CompilerDiagnostic::with_severity(
        DiagnosticKind::Rule(RuleDiagnosticKind::UnknownName),
        DiagnosticSeverity::Warning,
        location(source_path),
        DiagnosticPayload::UnknownName {
            name: string_table.intern("maybe_unused"),
            namespace: NameNamespace::Value,
        },
    );

    let messages =
        CompilerMessages::from_diagnostics(vec![warning.clone(), error.clone()], string_table);

    assert!(messages.has_errors());
    assert!(messages.has_warnings());
    assert_eq!(messages.error_count(), 1);
    assert_eq!(messages.warning_count(), 1);

    let diagnostics = messages.diagnostics.to_vec();
    assert_eq!(diagnostics, vec![warning, error]);
    assert!(messages.first_infrastructure_error_for_tests().is_none());
}

#[test]
fn compiler_messages_with_warnings_keep_typed_diagnostics_off_error_mirrors() {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("main.bst", &mut string_table);
    let error = unknown_name_diagnostic(
        string_table.intern("missing"),
        NameNamespace::Value,
        location(source_path.clone()),
    );
    let warning = CompilerDiagnostic::with_severity(
        DiagnosticKind::Rule(RuleDiagnosticKind::UnknownName),
        DiagnosticSeverity::Warning,
        location(source_path),
        DiagnosticPayload::UnknownName {
            name: string_table.intern("maybe_unused"),
            namespace: NameNamespace::Value,
        },
    );

    let messages = CompilerMessages::from_diagnostic_with_warnings(
        error.clone(),
        vec![warning.clone()],
        &string_table,
    );

    assert_eq!(messages.error_count(), 1);
    assert_eq!(messages.warning_count(), 1);
    assert_eq!(messages.diagnostics.to_vec(), vec![warning, error]);
    assert!(messages.first_infrastructure_error_for_tests().is_none());
}

#[test]
fn compiler_messages_with_infrastructure_error_preserve_warning_production_order() {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("main.bst", &mut string_table);
    let warning = CompilerDiagnostic::with_severity(
        DiagnosticKind::Rule(RuleDiagnosticKind::UnknownName),
        DiagnosticSeverity::Warning,
        location(source_path),
        DiagnosticPayload::UnknownName {
            name: string_table.intern("maybe_unused"),
            namespace: NameNamespace::Value,
        },
    );
    let error = CompilerError::compiler_error("backend failed after warnings");

    let messages =
        CompilerMessages::from_error_with_warnings(error, vec![warning.clone()], &string_table);

    assert_eq!(messages.error_count(), 1);
    assert_eq!(messages.warning_count(), 1);
    assert_eq!(messages.diagnostics[0], warning);
    assert_eq!(
        messages.diagnostics[1].kind,
        DiagnosticKind::Infrastructure(InfrastructureDiagnosticKind::InfrastructureFailure),
    );
    assert_eq!(messages.diagnostics[1].kind.code(), "BST-INFRA-0001");
}

#[test]
fn compiler_messages_preserve_type_context_ranges_when_prepending_and_appending() {
    let mut string_table = StringTable::new();
    let point_path = InternedPath::from_single_str("Point", &mut string_table);
    let status_path = InternedPath::from_single_str("Status", &mut string_table);

    let mut first_environment = TypeEnvironment::new();
    let (_, point_type) = first_environment.register_nominal_struct(StructTypeDefinition {
        id: NominalTypeId(0),
        path: point_path,
        fields: Box::new([]),
        generic_parameters: None,
        const_record: false,
    });
    let first_error = CompilerDiagnostic::type_mismatch(
        point_type,
        first_environment.builtins().int,
        TypeMismatchContext::Assignment,
        SourceLocation::default(),
    );
    let warning = CompilerDiagnostic::unreachable_match_arm(SourceLocation::default());
    let mut first_messages =
        CompilerMessages::from_diagnostics(vec![first_error], string_table.clone())
            .with_type_context_for_all_diagnostics(first_environment);
    first_messages.prepend_diagnostics_preserving_context(vec![warning]);

    let mut second_environment = TypeEnvironment::new();
    let (_, status_type) = second_environment.register_nominal_struct(StructTypeDefinition {
        id: NominalTypeId(0),
        path: status_path,
        fields: Box::new([]),
        generic_parameters: None,
        const_record: false,
    });
    let second_error = CompilerDiagnostic::type_mismatch(
        status_type,
        second_environment.builtins().string,
        TypeMismatchContext::ReturnValue,
        SourceLocation::default(),
    );
    let second_messages = CompilerMessages::from_diagnostics(vec![second_error], string_table)
        .with_type_context_for_all_diagnostics(second_environment);

    first_messages.append_messages_preserving_context(second_messages);
    let rendered =
        crate::compiler_frontend::compiler_messages::display_messages::format_terse_compiler_messages(
            &first_messages,
        );

    assert_eq!(
        first_messages.render_type_contexts()[0].diagnostic_range,
        1..2
    );
    assert_eq!(
        first_messages.render_type_contexts()[1].diagnostic_range,
        2..3
    );
    assert!(rendered[1].contains("expected Point, found Int"));
    assert!(rendered[2].contains("expected Status, found String"));
}

#[test]
fn remap_string_ids_updates_locations_payloads_labels_and_tokens() {
    let mut local_table = StringTable::new();
    let main_path = InternedPath::from_single_str("main.bst", &mut local_table);
    let import_path = InternedPath::from_single_str("lib.bst", &mut local_table);
    let name = local_table.intern("Button");
    let alias = local_table.intern("AliasButton");
    let label_text = local_table.intern("temporary label");

    let primary_location = location(main_path.clone());
    let first_location = location(import_path.clone());

    let expected_token = CompilerDiagnostic::expected_token(
        TokenKind::Symbol(name),
        Some(TokenKind::Path(vec![PathTokenItem {
            path: import_path.clone(),
            alias: Some(alias),
            path_location: first_location.clone(),
            alias_location: Some(primary_location.clone()),
            from_grouped: true,
        }])),
        primary_location.clone(),
    );
    let duplicate = CompilerDiagnostic::duplicate_declaration(
        name,
        first_location.clone(),
        primary_location.clone(),
    );
    let import = CompilerDiagnostic::import_name_collision(
        alias,
        Some(first_location.clone()),
        primary_location.clone(),
    )
    .with_labels(vec![DiagnosticLabel::secondary(
        first_location,
        Some(DiagnosticLabelMessage::RenderedText(label_text)),
    )]);
    let borrow = borrow_conflict_diagnostic(
        DiagnosticPlace::Local(name),
        BorrowAccessKind::Shared,
        BorrowAccessKind::Mutable,
        primary_location,
    );

    let mut bag = DiagnosticBag::from_diagnostics(vec![expected_token, duplicate, import, borrow]);

    let mut merged_table = StringTable::new();
    let remap = merged_table.merge_from(&local_table);
    bag.remap_string_ids(&remap);

    let diagnostics = bag.diagnostics();
    match &diagnostics[0].payload {
        DiagnosticPayload::ExpectedToken {
            expected,
            found: Some(TokenKind::Path(items)),
        } => {
            assert!(matches!(expected, TokenKind::Symbol(_)));
            let item = items
                .first()
                .expect("path token item should remain present");
            assert_eq!(item.path.to_string(&merged_table), "lib.bst");
            assert_eq!(
                item.alias.map(|id| merged_table.resolve(id)),
                Some("AliasButton")
            );
        }
        payload => panic!("unexpected expected-token payload: {payload:?}"),
    }

    match &diagnostics[1].payload {
        DiagnosticPayload::DuplicateDeclaration {
            name,
            first_location,
        } => {
            assert_eq!(merged_table.resolve(*name), "Button");
            assert_eq!(
                first_location.scope.to_string(&merged_table),
                String::from("lib.bst")
            );
        }
        payload => panic!("unexpected duplicate payload: {payload:?}"),
    }

    match &diagnostics[2].payload {
        DiagnosticPayload::ImportNameCollision {
            name,
            previous_location,
        } => {
            assert_eq!(merged_table.resolve(*name), "AliasButton");
            assert!(previous_location.is_some());
        }
        payload => panic!("unexpected import payload: {payload:?}"),
    }

    match &diagnostics[2].labels[0].message {
        Some(DiagnosticLabelMessage::RenderedText(message)) => {
            assert_eq!(merged_table.resolve(*message), "temporary label");
        }
        message => panic!("unexpected label message: {message:?}"),
    }

    match &diagnostics[3].payload {
        DiagnosticPayload::BorrowConflict {
            place: DiagnosticPlace::Local(name),
            ..
        } => assert_eq!(merged_table.resolve(*name), "Button"),
        payload => panic!("unexpected borrow payload: {payload:?}"),
    }
}

#[test]
fn type_mismatch_constructor_carries_type_ids_without_rendering() {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("main.bst", &mut string_table);

    let diagnostic = CompilerDiagnostic::type_mismatch(
        builtin_type_ids::INT,
        builtin_type_ids::STRING,
        TypeMismatchContext::Declaration,
        location(source_path),
    );

    assert_eq!(
        diagnostic.kind,
        DiagnosticKind::Type(TypeDiagnosticKind::TypeMismatch)
    );

    match diagnostic.payload {
        DiagnosticPayload::TypeMismatch {
            expected, found, ..
        } => {
            assert_eq!(expected, builtin_type_ids::INT);
            assert_eq!(found, builtin_type_ids::STRING);
        }
        payload => panic!("unexpected payload: {payload:?}"),
    }
}

#[test]
fn type_mismatch_terminal_guidance_renders_type_names_with_context() {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("main.bst", &mut string_table);
    let type_environment = TypeEnvironment::new();

    let diagnostic = CompilerDiagnostic::type_mismatch(
        type_environment.builtins().int,
        type_environment.builtins().string,
        TypeMismatchContext::Declaration,
        location(source_path),
    );
    let render_context = DiagnosticRenderContext::new(&string_table)
        .with_optional_type_environment(Some(&type_environment));

    let guidance = terminal::format_payload_guidance(&diagnostic.payload, render_context);

    assert!(guidance.iter().any(|line| line == "Expected: Int"));
    assert!(guidance.iter().any(|line| line == "Found: String"));
    assert!(!guidance.iter().any(|line| line.contains("type id")));
}

#[test]
fn type_mismatch_terse_renderer_renders_type_names_with_context() {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("main.bst", &mut string_table);
    let type_environment = TypeEnvironment::new();

    let diagnostic = CompilerDiagnostic::type_mismatch(
        type_environment.builtins().int,
        type_environment.builtins().string,
        TypeMismatchContext::FunctionArgument,
        location(source_path),
    );
    let render_context = DiagnosticRenderContext::new(&string_table)
        .with_optional_type_environment(Some(&type_environment));

    let line = terse::format_terse_diagnostic_with_context(&diagnostic, render_context);

    assert!(line.contains("expected Int"));
    assert!(line.contains("found String"));
    assert!(!line.contains("Expected type id"));
    assert!(!line.contains("Found type id"));
}

#[test]
fn rule_renderers_use_user_facing_messages_not_reason_debug_names() {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("main.bst", &mut string_table);
    let value_name = string_table.intern("value");

    let diagnostic = CompilerDiagnostic::invalid_assignment_target(
        InvalidAssignmentTargetReason::ImmutableVariable,
        Some(value_name),
        None,
        location(source_path),
    );
    let render_context = DiagnosticRenderContext::new(&string_table);

    let guidance = terminal::format_payload_guidance(&diagnostic.payload, render_context);
    let terse_line = terse::format_terse_diagnostic_with_context(&diagnostic, render_context);

    assert!(guidance.iter().any(|line| line
        == "Cannot mutate immutable variable 'value'. Use '~' to declare a mutable variable."));
    assert!(terse_line.contains("Cannot mutate immutable variable 'value'"));
    assert!(
        !guidance
            .iter()
            .any(|line| line.contains("ImmutableVariable"))
    );
    assert!(!terse_line.contains("ImmutableVariable"));
}

#[test]
fn syntax_and_choice_renderers_use_user_facing_messages_not_reason_debug_names() {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("main.bst", &mut string_table);
    let choice_name = string_table.intern("Status");
    let variant_name = string_table.intern("Ready");

    let diagnostics = [
        (
            CompilerDiagnostic::invalid_path(
                PathKind::WhitespaceMustBeQuoted,
                location(source_path.clone()),
            ),
            "Path components with whitespace must be quoted",
            "WhitespaceMustBeQuoted",
        ),
        (
            CompilerDiagnostic::invalid_import_clause(
                ImportClauseKind::Alias,
                InvalidImportClauseReason::AliasNotValidIdentifier,
                location(source_path.clone()),
            ),
            "Import alias must be a valid local binding name",
            "AliasNotValidIdentifier",
        ),
        (
            CompilerDiagnostic::invalid_choice_variant(
                InvalidChoiceVariantReason::UnitVariantWithParentheses,
                Some(choice_name),
                Some(variant_name),
                Vec::new(),
                location(source_path),
            ),
            "Unit variant 'Status::Ready' cannot be called with empty parentheses",
            "UnitVariantWithParentheses",
        ),
    ];
    let render_context = DiagnosticRenderContext::new(&string_table);

    for (diagnostic, expected_message, debug_name) in diagnostics {
        let guidance = terminal::format_payload_guidance(&diagnostic.payload, render_context);
        let terse_line = terse::format_terse_diagnostic_with_context(&diagnostic, render_context);

        assert!(
            guidance.iter().any(|line| line.contains(expected_message)),
            "Expected guidance to contain '{expected_message}', got {guidance:?}",
        );
        assert!(
            terse_line.contains(expected_message),
            "Expected terse line to contain '{expected_message}', got {terse_line}",
        );
        assert!(
            !guidance.iter().any(|line| line.contains(debug_name)),
            "Guidance should not expose enum variant '{debug_name}': {guidance:?}",
        );
        assert!(
            !terse_line.contains(debug_name),
            "Terse line should not expose enum variant '{debug_name}': {terse_line}",
        );
    }
}

#[test]
fn syntax_renderers_keep_typed_prose_without_error_conversion() {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("main.bst", &mut string_table);
    let literal = string_table.intern("1.");
    let style_directive = string_table.intern("unknown");
    let supported_directives = string_table.intern("'$html', '$css'");
    let declaration_name = string_table.intern("Card");
    let template_directive = string_table.intern("insert");
    let namespace_name = string_table.intern("card");

    let diagnostics = [
        (
            CompilerDiagnostic::duplicate_declaration(
                declaration_name,
                location(source_path.clone()),
                location(source_path.clone()),
            ),
            "There is already a top-level declaration using the name 'Card'",
            "StringId",
        ),
        (
            CompilerDiagnostic::invalid_number_literal(
                literal,
                NumberLiteralErrorReason::MultipleDecimalPoints,
                location(source_path.clone()),
            ),
            "Can't have more than one decimal point in numeric literal '1.'",
            "MultipleDecimalPoints",
        ),
        (
            CompilerDiagnostic::invalid_style_directive(
                style_directive,
                supported_directives,
                location(source_path.clone()),
            ),
            "Style directive '$unknown' is unsupported here",
            "InvalidStyleDirective",
        ),
        (
            CompilerDiagnostic::invalid_type_annotation(
                TypeAnnotationContext::DeclarationTarget,
                InvalidTypeAnnotationReason::UnexpectedColon,
                location(source_path.clone()),
            ),
            "Unexpected ':' after declaration name",
            "UnexpectedColon",
        ),
        (
            CompilerDiagnostic::invalid_signature_member(
                InvalidSignatureMemberReason::ChoicePayloadDefaultValue,
                location(source_path.clone()),
            ),
            "Choice payload fields cannot have default values.",
            "ChoicePayloadDefaultValue",
        ),
        (
            CompilerDiagnostic::invalid_function_signature(
                InvalidFunctionSignatureReason::MissingColonAfterReturns,
                location(source_path.clone()),
            ),
            "Function return declarations must end with ':'",
            "MissingColonAfterReturns",
        ),
        (
            CompilerDiagnostic::invalid_generic_application(
                GenericApplicationErrorReason::NestedApplication,
                location(source_path.clone()),
            ),
            "Nested generic type applications are not supported",
            "NestedApplication",
        ),
        (
            CompilerDiagnostic::invalid_collection_type(
                InvalidCollectionTypeReason::NegativeCapacity,
                location(source_path.clone()),
            ),
            "Collection capacity must be a non-negative integer.",
            "NegativeCapacity",
        ),
        (
            CompilerDiagnostic::invalid_generic_parameter(
                InvalidGenericParameterReason::BoundsNotSupported,
                location(source_path.clone()),
            ),
            "Generic parameter bounds are not supported yet.",
            "BoundsNotSupported",
        ),
        (
            CompilerDiagnostic::invalid_template_directive(
                Some(template_directive),
                InvalidTemplateDirectiveReason::MissingArgument,
                location(source_path.clone()),
            ),
            "Template directive 'insert' is missing a required argument.",
            "MissingArgument",
        ),
        (
            CompilerDiagnostic::namespace_misuse(
                namespace_name,
                NameNamespace::Type,
                NameNamespace::Value,
                location(source_path.clone()),
            ),
            "'card' is a value and cannot be used as a type.",
            "NamespaceMisuse",
        ),
        (
            CompilerDiagnostic::unsupported_operator_types(
                UnsupportedOperatorCategory::Arithmetic,
                builtin_type_ids::STRING,
                Some(builtin_type_ids::INT),
                location(source_path.clone()),
            ),
            "Unsupported operand types for arithmetic operator",
            "Arithmetic",
        ),
        (
            CompilerDiagnostic::invalid_result_operand(
                InvalidResultOperandReason::ResultNotUnwrapped,
                UnsupportedOperatorCategory::Arithmetic,
                builtin_type_ids::STRING,
                location(source_path),
            ),
            "arithmetic operator does not implicitly unwrap Result values",
            "ResultNotUnwrapped",
        ),
    ];
    let render_context = DiagnosticRenderContext::new(&string_table);

    for (diagnostic, expected_message, debug_name) in diagnostics {
        let guidance = terminal::format_payload_guidance(&diagnostic.payload, render_context);
        let terse_line = terse::format_terse_diagnostic_with_context(&diagnostic, render_context);

        assert!(
            guidance.iter().any(|line| line.contains(expected_message)),
            "Expected guidance to contain '{expected_message}', got {guidance:?}",
        );
        assert!(
            terse_line.contains(expected_message),
            "Expected terse line to contain '{expected_message}', got {terse_line}",
        );
        assert!(
            !guidance.iter().any(|line| line.contains(debug_name)),
            "Guidance should not expose '{debug_name}': {guidance:?}",
        );
        assert!(
            !terse_line.contains(debug_name),
            "Terse output should not expose '{debug_name}': {terse_line}",
        );
    }
}

#[test]
fn render_boundary_smoke_coverage_hides_internal_debug_names_by_family() {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("main.bst", &mut string_table);
    let import_path = InternedPath::from_single_str("missing.bst", &mut string_table);
    let value_name = string_table.intern("value");
    let config_key = string_table.intern("homepage");
    let feature_name = string_table.intern("traits");
    let type_environment = TypeEnvironment::new();

    let diagnostics = vec![
        CompilerDiagnostic::invalid_path(
            PathKind::WhitespaceMustBeQuoted,
            location(source_path.clone()),
        ),
        CompilerDiagnostic::type_mismatch(
            type_environment.builtins().int,
            type_environment.builtins().string,
            TypeMismatchContext::Declaration,
            location(source_path.clone()),
        ),
        CompilerDiagnostic::invalid_assignment_target(
            InvalidAssignmentTargetReason::ImmutableVariable,
            Some(value_name),
            None,
            location(source_path.clone()),
        ),
        CompilerDiagnostic::missing_import_target(import_path, location(source_path.clone())),
        borrow_conflict_diagnostic(
            DiagnosticPlace::Local(value_name),
            BorrowAccessKind::Shared,
            BorrowAccessKind::Mutable,
            location(source_path.clone()),
        ),
        CompilerDiagnostic::invalid_config_reason(
            Some(config_key),
            InvalidConfigReason::UnsupportedScalarValue,
            location(source_path.clone()),
        ),
        CompilerDiagnostic::deferred_feature(feature_name, location(source_path)),
    ];
    let render_context = DiagnosticRenderContext::new(&string_table)
        .with_optional_type_environment(Some(&type_environment));

    for diagnostic in &diagnostics {
        let terminal_guidance =
            terminal::format_payload_guidance(&diagnostic.payload, render_context).join("\n");
        let terse_line = terse::format_terse_diagnostic_with_context(diagnostic, render_context);
        let dev_server_html = dev_server::render_diagnostics_html_with_context(
            std::slice::from_ref(diagnostic),
            Path::new("/tmp"),
            render_context,
        );

        assert_rendered_diagnostic_hides_internal_names(&terminal_guidance);
        assert_rendered_diagnostic_hides_internal_names(&terse_line);
        assert_rendered_diagnostic_hides_internal_names(&dev_server_html);
        assert!(terse_line.contains(diagnostic.kind.code()));
        assert!(dev_server_html.contains(diagnostic.kind.code()));
    }
}

fn assert_rendered_diagnostic_hides_internal_names(rendered: &str) {
    for internal_name in [
        "StringId(",
        "TypeId(",
        "DiagnosticPlace",
        "BorrowAccessKind",
        "InvalidConfigReason",
        "DeferredFeatureReason",
        "WhitespaceMustBeQuoted",
        "ImmutableVariable",
    ] {
        assert!(
            !rendered.contains(internal_name),
            "renderer leaked internal name '{internal_name}' in: {rendered}",
        );
    }
}

#[test]
fn incompatible_choice_comparison_renderer_hides_reason_debug_names() {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("main.bst", &mut string_table);
    let diagnostic = CompilerDiagnostic::incompatible_choice_comparison(
        IncompatibleChoiceComparisonReason::ChoiceWithNonChoice,
        builtin_type_ids::BOOL,
        builtin_type_ids::INT,
        location(source_path),
    );
    let render_context = DiagnosticRenderContext::new(&string_table);

    let guidance = terminal::format_payload_guidance(&diagnostic.payload, render_context);
    let terse_line = terse::format_terse_diagnostic_with_context(&diagnostic, render_context);

    assert!(
        guidance
            .iter()
            .any(|line| line.contains("Cannot compare choice")),
        "{guidance:?}"
    );
    assert!(terse_line.contains("Cannot compare choice"), "{terse_line}");
    assert!(
        !guidance
            .iter()
            .any(|line| line.contains("ChoiceWithNonChoice"))
    );
    assert!(!terse_line.contains("ChoiceWithNonChoice"));
}

#[test]
fn type_mismatch_renderer_fallback_uses_stable_type_id_text() {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("main.bst", &mut string_table);

    let diagnostic = CompilerDiagnostic::type_mismatch(
        builtin_type_ids::INT,
        builtin_type_ids::STRING,
        TypeMismatchContext::Declaration,
        location(source_path),
    );
    let render_context = DiagnosticRenderContext::new(&string_table);

    let guidance = terminal::format_payload_guidance(&diagnostic.payload, render_context);
    let terse_line = terse::format_terse_diagnostic_with_context(&diagnostic, render_context);

    assert!(guidance.iter().any(|line| line == "Expected: TypeId(1)"));
    assert!(guidance.iter().any(|line| line == "Found: TypeId(4)"));
    assert!(terse_line.contains("expected TypeId(1)"));
    assert!(terse_line.contains("found TypeId(4)"));
    assert!(
        !guidance
            .iter()
            .any(|line| line.contains("Expected type id"))
    );
    assert!(!guidance.iter().any(|line| line.contains("Found type id")));
}

#[test]
fn generic_instantiation_rendering_resolves_type_name() {
    use crate::compiler_frontend::compiler_messages::{
        CompilerDiagnostic, InvalidGenericInstantiationReason,
    };
    use crate::compiler_frontend::symbols::string_interning::StringTable;
    use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

    let mut string_table = StringTable::new();
    let box_name = string_table.intern("Box");
    let location = SourceLocation::default();

    let diagnostic = CompilerDiagnostic::invalid_generic_instantiation(
        Some(box_name),
        InvalidGenericInstantiationReason::WrongArgumentCount {
            expected: 1,
            found: 2,
        },
        location,
    );

    let render_context = DiagnosticRenderContext::new(&string_table);
    let terse_line = terse::format_terse_diagnostic_with_context(&diagnostic, render_context);

    assert!(
        terse_line.contains("'Box'"),
        "Expected 'Box' in message, got: {terse_line}"
    );
}

#[test]
fn generic_conflict_rendering_resolves_concrete_type_names() {
    use crate::compiler_frontend::compiler_messages::{
        CompilerDiagnostic, InvalidGenericInstantiationReason,
    };
    use crate::compiler_frontend::datatypes::ids::GenericParameterId;
    use crate::compiler_frontend::symbols::string_interning::StringTable;
    use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

    let mut string_table = StringTable::new();
    let function_name = string_table.intern("first");
    let parameter_name = string_table.intern("T");
    let location = SourceLocation::default();
    let type_environment = TypeEnvironment::new();

    let diagnostic = CompilerDiagnostic::invalid_generic_instantiation(
        Some(function_name),
        InvalidGenericInstantiationReason::ConflictingFunctionArgument {
            parameter_id: GenericParameterId(0),
            parameter_name,
            existing_type_id: type_environment.builtins().int,
            replacement_type_id: type_environment.builtins().string,
            current_evidence_location: location.clone(),
            previous_evidence_location: None,
        },
        location,
    );

    let render_context = DiagnosticRenderContext::new(&string_table)
        .with_optional_type_environment(Some(&type_environment));
    let guidance = terminal::format_payload_guidance(&diagnostic.payload, render_context);

    assert!(
        guidance
            .iter()
            .any(|line| line.contains("Generic parameter 'T'")
                && line.contains("Int")
                && line.contains("String")),
        "expected rendered type names in guidance, got: {guidance:?}"
    );
}

#[test]
fn builtin_cast_shape_diagnostic_preserves_stable_code_and_rendering() {
    use crate::compiler_frontend::compiler_messages::InvalidBuiltinCallReason;

    let mut string_table = StringTable::new();
    let int_name = string_table.intern("Int");

    let diagnostic = CompilerDiagnostic::invalid_builtin_call(
        InvalidBuiltinCallReason::CastMissingArgument,
        Some(int_name),
        SourceLocation::default(),
    );

    let render_context = DiagnosticRenderContext::new(&string_table);
    let terse_line = terse::format_terse_diagnostic_with_context(&diagnostic, render_context);

    assert_eq!(diagnostic.kind.descriptor().code, "BST-RULE-0046");
    assert!(
        terse_line.contains("'Int' cast requires exactly one argument"),
        "{terse_line}"
    );
}

#[test]
fn token_diagnostics_render_source_spelling_not_token_debug_names() {
    let mut string_table = StringTable::new();
    let name = string_table.intern("item");
    let location = SourceLocation::default();

    let expected = CompilerDiagnostic::expected_token(
        TokenKind::OpenParenthesis,
        Some(TokenKind::Symbol(name)),
        location.clone(),
    );
    let unexpected = CompilerDiagnostic::unexpected_token(TokenKind::OpenCurly, location);

    let render_context = DiagnosticRenderContext::new(&string_table);
    let expected_terse = terse::format_terse_diagnostic_with_context(&expected, render_context);
    let unexpected_terminal =
        terminal::format_payload_guidance(&unexpected.payload, render_context).join("\n");

    assert!(expected_terse.contains("Expected `(`"), "{expected_terse}");
    assert!(expected_terse.contains("name `item`"), "{expected_terse}");
    assert!(
        unexpected_terminal.contains("Unexpected token `{`"),
        "{unexpected_terminal}"
    );
    assert!(
        !expected_terse.contains("OpenParenthesis") && !expected_terse.contains("Symbol"),
        "{expected_terse}"
    );
}

#[test]
fn borrow_conflict_rendering_hides_payload_debug_names() {
    let mut string_table = StringTable::new();
    let value_name = string_table.intern("value");
    let location = SourceLocation::default();

    let diagnostic = borrow_conflict_diagnostic(
        DiagnosticPlace::Local(value_name),
        BorrowAccessKind::Shared,
        BorrowAccessKind::Mutable,
        location,
    );

    let render_context = DiagnosticRenderContext::new(&string_table);
    let terminal_guidance =
        terminal::format_payload_guidance(&diagnostic.payload, render_context).join("\n");
    let terse_line = terse::format_terse_diagnostic_with_context(&diagnostic, render_context);

    assert!(
        terminal_guidance
            .contains("existing shared access conflicts with requested mutable access"),
        "{terminal_guidance}"
    );
    assert!(
        !terminal_guidance.contains("DiagnosticPlace") && !terminal_guidance.contains("Shared"),
        "{terminal_guidance}"
    );
    assert!(!terse_line.contains("BorrowConflict"), "{terse_line}");
}
