//! Type-syntax parsing and resolution regression tests.
//!
//! WHAT: validates type annotation parsing and type resolution in composite types.
//! WHY: type syntax is the source of truth for frontend type identity; parser drift here
//!      affects every downstream type check.

use crate::compiler_frontend::ast::TopLevelDeclarationTable;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::Expression;

use crate::compiler_frontend::ast::type_resolution::{
    TypeResolutionContext, resolve_diagnostic_type_to_type_id,
    resolve_diagnostic_type_to_type_id_checked, resolve_diagnostic_type_to_type_id_opt,
    resolve_parsed_type_annotation, resolve_type,
};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DeferredFeatureReason, DiagnosticPayload, GenericApplicationErrorReason,
    InvalidCollectionTypeReason, InvalidGenericInstantiationReason, InvalidTypeAnnotationReason,
    NameNamespace,
};
use crate::compiler_frontend::datatypes::definitions::{StructTypeDefinition, TypeDefinition};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::generic_identity_bridge::GenericBaseType;
use crate::compiler_frontend::datatypes::generic_parameters::{
    GenericParameter, GenericParameterList, TypeParameterId,
};
use crate::compiler_frontend::datatypes::ids::NominalTypeId;
use crate::compiler_frontend::datatypes::parsed::ParsedTypeRef;
use crate::compiler_frontend::datatypes::{DataType, TypeId, builtin_type_ids};
use crate::compiler_frontend::declaration_syntax::type_syntax::{
    TypeAnnotationContext, parse_type_annotation, parse_type_annotation_with_capacity,
};
use crate::compiler_frontend::headers::module_symbols::{
    GenericDeclarationKind, GenericDeclarationMetadata,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::compiler_frontend::value_mode::ValueMode;
use rustc_hash::FxHashMap;
use std::rc::Rc;

fn stream_from_tokens(tokens: Vec<Token>, string_table: &mut StringTable) -> FileTokens {
    FileTokens::new(
        InternedPath::from_single_str("type_syntax_tests", string_table),
        tokens,
    )
}

fn token(kind: TokenKind) -> Token {
    Token::new(kind, SourceLocation::default())
}

fn assert_diagnostic_payload(
    diagnostic: impl std::borrow::Borrow<CompilerDiagnostic>,
    expected: impl FnOnce(&DiagnosticPayload) -> bool,
    expected_description: &str,
) {
    let diagnostic = diagnostic.borrow();

    assert!(
        expected(&diagnostic.payload),
        "expected {expected_description}, got {:?}",
        diagnostic.payload
    );
}

fn single_parameter_metadata(
    parameter_name: crate::compiler_frontend::symbols::string_interning::StringId,
) -> GenericDeclarationMetadata {
    GenericDeclarationMetadata {
        kind: GenericDeclarationKind::Struct,
        parameters: GenericParameterList {
            parameters: vec![GenericParameter {
                id: TypeParameterId(0),
                name: parameter_name,
                location: SourceLocation::default(),
            }],
        },
        declaration_location: SourceLocation::default(),
    }
}

fn resolve_type_annotation_error(
    parsed: ParsedTypeRef,
    string_table: &mut StringTable,
    expected_failure: &str,
) -> CompilerDiagnostic {
    let declaration_table = Rc::new(TopLevelDeclarationTable::new(Vec::new()));
    let mut type_environment = TypeEnvironment::new();
    let mut resolution_context =
        TypeResolutionContext::from_declaration_table(&declaration_table, &mut type_environment);

    resolve_parsed_type_annotation(
        parsed,
        &SourceLocation::default(),
        &mut resolution_context,
        string_table,
    )
    .expect_err(expected_failure)
    .as_ref()
    .to_owned()
}

#[test]
fn declaration_context_allows_inferred_annotations() {
    let mut string_table = StringTable::new();
    let mut stream = stream_from_tokens(
        vec![token(TokenKind::Assign), token(TokenKind::Eof)],
        &mut string_table,
    );

    let parsed = parse_type_annotation(&mut stream, TypeAnnotationContext::DeclarationTarget)
        .expect("declaration type annotation should parse");

    assert_eq!(parsed, ParsedTypeRef::Inferred);
}

#[test]
fn resolved_type_annotation_carries_canonical_type_id() {
    let string_table = StringTable::new();
    let declaration_table = Rc::new(TopLevelDeclarationTable::new(Vec::new()));
    let mut type_environment = TypeEnvironment::new();
    let mut resolution_context =
        TypeResolutionContext::from_declaration_table(&declaration_table, &mut type_environment);

    let location = SourceLocation::default();
    let resolved = resolve_parsed_type_annotation(
        ParsedTypeRef::BuiltinInt {
            location: location.to_owned(),
        },
        &location,
        &mut resolution_context,
        &string_table,
    )
    .expect("builtin annotation resolution should succeed");

    assert_eq!(resolved.diagnostic_type, DataType::Int);
    assert_eq!(resolved.type_id, Some(builtin_type_ids::INT));
}

#[test]
fn resolved_inferred_annotation_has_no_type_id() {
    let string_table = StringTable::new();
    let declaration_table = Rc::new(TopLevelDeclarationTable::new(Vec::new()));
    let mut type_environment = TypeEnvironment::new();
    let mut resolution_context =
        TypeResolutionContext::from_declaration_table(&declaration_table, &mut type_environment);

    let resolved = resolve_parsed_type_annotation(
        ParsedTypeRef::Inferred,
        &SourceLocation::default(),
        &mut resolution_context,
        &string_table,
    )
    .expect("inferred annotation resolution should succeed");

    assert_eq!(resolved.diagnostic_type, DataType::Inferred);
    assert_eq!(resolved.type_id, None);
}

#[test]
fn declaration_context_parses_named_optional_type() {
    let mut string_table = StringTable::new();
    let point = string_table.intern("Point");
    let mut stream = stream_from_tokens(
        vec![
            token(TokenKind::Symbol(point)),
            token(TokenKind::QuestionMark),
            token(TokenKind::Assign),
            token(TokenKind::Eof),
        ],
        &mut string_table,
    );

    let parsed = parse_type_annotation(&mut stream, TypeAnnotationContext::DeclarationTarget)
        .expect("named optional declaration type annotation should parse");

    assert_eq!(
        parsed,
        ParsedTypeRef::Optional {
            inner: Box::new(ParsedTypeRef::Named {
                name: point,
                location: SourceLocation::default()
            }),
            location: SourceLocation::default(),
        }
    );
}

#[test]
fn signature_parameter_rejects_none_type() {
    let mut string_table = StringTable::new();
    let mut stream = stream_from_tokens(
        vec![token(TokenKind::DatatypeNone), token(TokenKind::Eof)],
        &mut string_table,
    );

    let error = parse_type_annotation(&mut stream, TypeAnnotationContext::SignatureParameter)
        .expect_err("none parameter type should fail");

    assert!(matches!(
        error.payload,
        DiagnosticPayload::InvalidTypeAnnotation {
            context: TypeAnnotationContext::SignatureParameter,
            reason: InvalidTypeAnnotationReason::NoneNotAllowed,
        }
    ));
}

#[test]
fn signature_parameter_rejects_reserved_trait_this_type() {
    let mut string_table = StringTable::new();
    let mut stream = stream_from_tokens(
        vec![token(TokenKind::TraitThis), token(TokenKind::Eof)],
        &mut string_table,
    );

    let error = parse_type_annotation(&mut stream, TypeAnnotationContext::SignatureParameter)
        .expect_err("reserved trait keyword type should fail");

    assert!(matches!(
        error.payload,
        DiagnosticPayload::InvalidTypeAnnotation {
            context: TypeAnnotationContext::SignatureParameter,
            reason: InvalidTypeAnnotationReason::ReservedTraitKeyword,
        }
    ));
}

#[test]
fn declaration_target_rejects_type_keyword_inside_type_annotation() {
    let mut string_table = StringTable::new();
    let mut stream = stream_from_tokens(
        vec![token(TokenKind::Type), token(TokenKind::Eof)],
        &mut string_table,
    );

    let error = parse_type_annotation(&mut stream, TypeAnnotationContext::DeclarationTarget)
        .expect_err("type keyword should be reserved");

    assert!(matches!(
        error.payload,
        DiagnosticPayload::InvalidTypeAnnotation {
            context: TypeAnnotationContext::DeclarationTarget,
            reason: InvalidTypeAnnotationReason::ExpectedTypeAnnotation {
                found: TokenKind::Type,
            },
        }
    ));
}

#[test]
fn signature_return_rejects_bare_of_keyword_with_structured_syntax_error() {
    let mut string_table = StringTable::new();
    let mut stream = stream_from_tokens(
        vec![token(TokenKind::Of), token(TokenKind::Eof)],
        &mut string_table,
    );

    let error = parse_type_annotation(&mut stream, TypeAnnotationContext::SignatureReturn)
        .expect_err("of keyword should fail in type position");

    assert!(matches!(
        error.payload,
        DiagnosticPayload::UnexpectedToken {
            found: TokenKind::Of
        }
    ));
}

#[test]
fn parses_generic_type_application() {
    let mut string_table = StringTable::new();
    let box_name = string_table.intern("Box");
    let mut stream = stream_from_tokens(
        vec![
            token(TokenKind::Symbol(box_name)),
            token(TokenKind::Of),
            token(TokenKind::DatatypeString),
            token(TokenKind::Assign),
            token(TokenKind::Eof),
        ],
        &mut string_table,
    );

    let parsed = parse_type_annotation(&mut stream, TypeAnnotationContext::DeclarationTarget)
        .expect("generic type application should parse");

    assert_eq!(
        parsed,
        ParsedTypeRef::Applied {
            base: Box::new(ParsedTypeRef::Named {
                name: box_name,
                location: SourceLocation::default()
            }),
            arguments: vec![ParsedTypeRef::BuiltinString {
                location: SourceLocation::default()
            }],
            location: SourceLocation::default(),
        }
    );
}

#[test]
fn public_option_type_syntax_is_deferred() {
    let mut string_table = StringTable::new();
    let option_name = string_table.intern("Option");
    let mut stream = stream_from_tokens(
        vec![
            token(TokenKind::Symbol(option_name)),
            token(TokenKind::Of),
            token(TokenKind::DatatypeString),
            token(TokenKind::Assign),
            token(TokenKind::Eof),
        ],
        &mut string_table,
    );

    let parsed = parse_type_annotation(&mut stream, TypeAnnotationContext::DeclarationTarget)
        .expect("public option syntax should parse before resolution rejects it");

    let error = resolve_type_annotation_error(
        parsed,
        &mut string_table,
        "public Option of T syntax should be deferred",
    );

    assert_diagnostic_payload(
        error,
        |payload| {
            matches!(
                payload,
                DiagnosticPayload::DeferredFeature {
                    reason: DeferredFeatureReason::PublicOptionTypeSyntax
                }
            )
        },
        "DeferredFeature(PublicOptionTypeSyntax)",
    );
}

#[test]
fn public_result_type_syntax_is_deferred() {
    let mut string_table = StringTable::new();
    let result_name = string_table.intern("Result");
    let mut stream = stream_from_tokens(
        vec![
            token(TokenKind::Symbol(result_name)),
            token(TokenKind::Of),
            token(TokenKind::DatatypeInt),
            token(TokenKind::Comma),
            token(TokenKind::DatatypeString),
            token(TokenKind::Assign),
            token(TokenKind::Eof),
        ],
        &mut string_table,
    );

    let parsed = parse_type_annotation(&mut stream, TypeAnnotationContext::DeclarationTarget)
        .expect("public result syntax should parse before resolution rejects it");

    let error = resolve_type_annotation_error(
        parsed,
        &mut string_table,
        "public Result of T, E syntax should be deferred",
    );

    assert_diagnostic_payload(
        error,
        |payload| {
            matches!(
                payload,
                DiagnosticPayload::DeferredFeature {
                    reason: DeferredFeatureReason::PublicResultTypeSyntax
                }
            )
        },
        "DeferredFeature(PublicResultTypeSyntax)",
    );
}

#[test]
fn parses_collection_of_generic_type_application() {
    let mut string_table = StringTable::new();
    let box_name = string_table.intern("Box");
    let mut stream = stream_from_tokens(
        vec![
            token(TokenKind::OpenCurly),
            token(TokenKind::Symbol(box_name)),
            token(TokenKind::Of),
            token(TokenKind::DatatypeString),
            token(TokenKind::CloseCurly),
            token(TokenKind::Assign),
            token(TokenKind::Eof),
        ],
        &mut string_table,
    );

    let parsed = parse_type_annotation(&mut stream, TypeAnnotationContext::DeclarationTarget)
        .expect("collection element generic type application should parse");

    assert_eq!(
        parsed,
        ParsedTypeRef::Collection {
            element: Box::new(ParsedTypeRef::Applied {
                base: Box::new(ParsedTypeRef::Named {
                    name: box_name,
                    location: SourceLocation::default()
                }),
                arguments: vec![ParsedTypeRef::BuiltinString {
                    location: SourceLocation::default()
                }],
                location: SourceLocation::default(),
            }),
            location: SourceLocation::default(),
        }
    );
}

#[test]
fn rejects_nested_generic_type_application() {
    let mut string_table = StringTable::new();
    let box_name = string_table.intern("Box");
    let map_name = string_table.intern("Map");
    let mut stream = stream_from_tokens(
        vec![
            token(TokenKind::Symbol(box_name)),
            token(TokenKind::Of),
            token(TokenKind::Symbol(map_name)),
            token(TokenKind::Of),
            token(TokenKind::DatatypeString),
            token(TokenKind::Comma),
            token(TokenKind::DatatypeInt),
            token(TokenKind::Assign),
            token(TokenKind::Eof),
        ],
        &mut string_table,
    );

    let error = parse_type_annotation(&mut stream, TypeAnnotationContext::DeclarationTarget)
        .expect_err("nested generic type application should fail");

    assert_diagnostic_payload(
        error,
        |payload| {
            matches!(
                payload,
                DiagnosticPayload::InvalidGenericApplication {
                    reason: GenericApplicationErrorReason::NestedApplication,
                }
            )
        },
        "InvalidGenericApplication(NestedApplication)",
    );
}

#[test]
fn duplicate_optional_marker_is_rejected() {
    let mut string_table = StringTable::new();
    let mut stream = stream_from_tokens(
        vec![
            token(TokenKind::DatatypeString),
            token(TokenKind::QuestionMark),
            token(TokenKind::QuestionMark),
            token(TokenKind::Eof),
        ],
        &mut string_table,
    );

    let error = parse_type_annotation(&mut stream, TypeAnnotationContext::SignatureReturn)
        .expect_err("duplicate optional marker should fail");

    assert_diagnostic_payload(
        error,
        |payload| {
            matches!(
                payload,
                DiagnosticPayload::InvalidTypeAnnotation {
                    reason: InvalidTypeAnnotationReason::DuplicateOptional,
                    ..
                }
            )
        },
        "InvalidTypeAnnotation(DuplicateOptional)",
    );
}

#[test]
fn alias_expanded_nested_optional_type_is_rejected() {
    let mut string_table = StringTable::new();
    let maybe_name = string_table.intern("MaybeString");
    let maybe_path = InternedPath::from_single_str("MaybeString", &mut string_table);

    let unresolved = DataType::Option(Box::new(DataType::NamedType(maybe_name)));
    let declaration_table = Rc::new(TopLevelDeclarationTable::new(Vec::new()));
    let mut type_environment = TypeEnvironment::new();
    let mut visible_type_aliases = FxHashMap::default();
    visible_type_aliases.insert(maybe_name, maybe_path.to_owned());

    let mut resolved_type_aliases = FxHashMap::default();
    resolved_type_aliases.insert(
        maybe_path,
        DataType::Option(Box::new(DataType::StringSlice)),
    );

    let mut resolution_context = TypeResolutionContext {
        declaration_table: &declaration_table,
        visible_declaration_ids: None,
        visible_external_symbols: None,
        visible_source_bindings: None,
        visible_type_aliases: Some(&visible_type_aliases),
        resolved_type_aliases: Some(&resolved_type_aliases),
        generic_declarations_by_path: None,
        generic_parameters: None,
        generic_substitutions: None,
        resolved_struct_fields_by_path: None,
        type_environment: &mut type_environment,
        visible_namespace_records: None,
    };

    let error = resolve_type(
        &unresolved,
        &SourceLocation::default(),
        &mut resolution_context,
        &string_table,
    )
    .expect_err("alias-expanded nested option should fail");

    assert_diagnostic_payload(
        error,
        |payload| {
            matches!(
                payload,
                DiagnosticPayload::InvalidTypeAnnotation {
                    reason: InvalidTypeAnnotationReason::NestedOptional,
                    ..
                }
            )
        },
        "InvalidTypeAnnotation(NestedOptional)",
    );
}

#[test]
fn resolves_named_types_recursively_in_composite_types() {
    let mut string_table = StringTable::new();
    let point_name = string_table.intern("Point");

    let unresolved =
        DataType::collection(DataType::Option(Box::new(DataType::NamedType(point_name))));

    let point_path = InternedPath::from_single_str("Point", &mut string_table);
    let declarations = vec![Declaration {
        id: point_path,
        value: Expression::no_value(
            SourceLocation::default(),
            DataType::Int,
            ValueMode::ImmutableOwned,
        ),
    }];

    let declaration_table = Rc::new(TopLevelDeclarationTable::new(declarations));
    let mut type_environment = TypeEnvironment::new();
    let mut resolution_context =
        TypeResolutionContext::from_declaration_table(&declaration_table, &mut type_environment);

    let location = SourceLocation::default();
    let resolved = resolve_type(
        &unresolved,
        &location,
        &mut resolution_context,
        &string_table,
    )
    .expect("named type resolution should succeed");

    assert_eq!(
        resolved,
        DataType::collection(DataType::Option(Box::new(DataType::Int)))
    );
}

#[test]
fn resolves_generic_instance_base_to_canonical_nominal_path() {
    let mut string_table = StringTable::new();
    let box_name = string_table.intern("Box");
    let t_name = string_table.intern("T");
    let unresolved = DataType::GenericInstance {
        base: GenericBaseType::Named(box_name),
        arguments: vec![DataType::StringSlice],
    };

    let box_path = InternedPath::from_single_str("Box", &mut string_table);
    let declarations = vec![Declaration {
        id: box_path.to_owned(),
        value: Expression::no_value(
            SourceLocation::default(),
            DataType::runtime_struct(box_path.to_owned(), builtin_type_ids::NONE),
            ValueMode::ImmutableOwned,
        ),
    }];
    let mut generic_declarations = FxHashMap::default();
    generic_declarations.insert(box_path.to_owned(), single_parameter_metadata(t_name));

    let declaration_table = Rc::new(TopLevelDeclarationTable::new(declarations));
    let mut type_environment = TypeEnvironment::new();
    let mut resolution_context = TypeResolutionContext {
        declaration_table: &declaration_table,
        visible_declaration_ids: None,
        visible_external_symbols: None,
        visible_source_bindings: None,
        visible_type_aliases: None,
        resolved_type_aliases: None,
        generic_declarations_by_path: Some(&generic_declarations),
        generic_parameters: None,
        generic_substitutions: None,
        resolved_struct_fields_by_path: None,
        type_environment: &mut type_environment,
        visible_namespace_records: None,
    };

    let location = SourceLocation::default();
    let resolved = resolve_type(
        &unresolved,
        &location,
        &mut resolution_context,
        &string_table,
    )
    .expect("generic base should resolve");

    assert_eq!(
        resolved,
        DataType::GenericInstance {
            base: GenericBaseType::ResolvedNominal(box_path),
            arguments: vec![DataType::StringSlice],
        }
    );
}

#[test]
fn generic_instance_resolution_rejects_wrong_arity() {
    let mut string_table = StringTable::new();
    let box_name = string_table.intern("Box");
    let t_name = string_table.intern("T");
    let unresolved = DataType::GenericInstance {
        base: GenericBaseType::Named(box_name),
        arguments: vec![DataType::StringSlice, DataType::Int],
    };

    let box_path = InternedPath::from_single_str("Box", &mut string_table);
    let declarations = vec![Declaration {
        id: box_path.to_owned(),
        value: Expression::no_value(
            SourceLocation::default(),
            DataType::runtime_struct(box_path.to_owned(), builtin_type_ids::NONE),
            ValueMode::ImmutableOwned,
        ),
    }];
    let mut generic_declarations = FxHashMap::default();
    generic_declarations.insert(box_path, single_parameter_metadata(t_name));

    let declaration_table = Rc::new(TopLevelDeclarationTable::new(declarations));
    let mut type_environment = TypeEnvironment::new();
    let mut resolution_context = TypeResolutionContext {
        declaration_table: &declaration_table,
        visible_declaration_ids: None,
        visible_external_symbols: None,
        visible_source_bindings: None,
        visible_type_aliases: None,
        resolved_type_aliases: None,
        generic_declarations_by_path: Some(&generic_declarations),
        generic_parameters: None,
        generic_substitutions: None,
        resolved_struct_fields_by_path: None,
        type_environment: &mut type_environment,
        visible_namespace_records: None,
    };

    let location = SourceLocation::default();
    let error = resolve_type(
        &unresolved,
        &location,
        &mut resolution_context,
        &string_table,
    )
    .expect_err("wrong generic arity should fail");

    assert_diagnostic_payload(
        error,
        |payload| {
            matches!(
                payload,
                DiagnosticPayload::InvalidGenericInstantiation {
                    type_name,
                    reason: InvalidGenericInstantiationReason::WrongArgumentCount {
                        expected: 1,
                        found: 2,
                    },
                } if *type_name == Some(box_name)
            )
        },
        "InvalidGenericInstantiation(WrongArgumentCount)",
    );
}

#[test]
fn bare_generic_type_name_requires_type_arguments() {
    let mut string_table = StringTable::new();
    let box_name = string_table.intern("Box");
    let t_name = string_table.intern("T");
    let unresolved = DataType::NamedType(box_name);

    let box_path = InternedPath::from_single_str("Box", &mut string_table);
    let declarations = vec![Declaration {
        id: box_path.to_owned(),
        value: Expression::no_value(
            SourceLocation::default(),
            DataType::runtime_struct(box_path.to_owned(), builtin_type_ids::NONE),
            ValueMode::ImmutableOwned,
        ),
    }];
    let mut generic_declarations = FxHashMap::default();
    generic_declarations.insert(box_path, single_parameter_metadata(t_name));

    let declaration_table = Rc::new(TopLevelDeclarationTable::new(declarations));
    let mut type_environment = TypeEnvironment::new();
    let mut resolution_context = TypeResolutionContext {
        declaration_table: &declaration_table,
        visible_declaration_ids: None,
        visible_external_symbols: None,
        visible_source_bindings: None,
        visible_type_aliases: None,
        resolved_type_aliases: None,
        generic_declarations_by_path: Some(&generic_declarations),
        generic_parameters: None,
        generic_substitutions: None,
        resolved_struct_fields_by_path: None,
        type_environment: &mut type_environment,
        visible_namespace_records: None,
    };

    let location = SourceLocation::default();
    let error = resolve_type(
        &unresolved,
        &location,
        &mut resolution_context,
        &string_table,
    )
    .expect_err("bare generic type name should fail");

    assert_diagnostic_payload(
        error,
        |payload| {
            matches!(
                payload,
                DiagnosticPayload::InvalidGenericInstantiation {
                    type_name,
                    reason: InvalidGenericInstantiationReason::MissingTypeArguments,
                } if *type_name == Some(box_name)
            )
        },
        "InvalidGenericInstantiation(MissingTypeArguments)",
    );
}

#[test]
fn unknown_named_type_reports_consistent_error() {
    let mut string_table = StringTable::new();
    let missing = string_table.intern("Missing");

    let unresolved = DataType::NamedType(missing);
    let location = SourceLocation::default();
    let declaration_table = Rc::new(TopLevelDeclarationTable::new(vec![]));
    let mut type_environment = TypeEnvironment::new();
    let mut resolution_context =
        TypeResolutionContext::from_declaration_table(&declaration_table, &mut type_environment);

    let error = resolve_type(
        &unresolved,
        &location,
        &mut resolution_context,
        &string_table,
    )
    .expect_err("unknown type should fail");

    assert_diagnostic_payload(
        error,
        |payload| {
            matches!(
                payload,
                DiagnosticPayload::UnknownName {
                    name,
                    namespace: NameNamespace::Type,
                } if *name == missing
            )
        },
        "UnknownName(type)",
    );
}

#[test]
fn returns_data_type_interns_multi_return_tuple_type_id() {
    let mut type_environment = TypeEnvironment::new();

    let returns_type_id = resolve_diagnostic_type_to_type_id(
        &DataType::Returns(vec![DataType::Int, DataType::Bool]),
        &mut type_environment,
    );

    assert_eq!(
        type_environment.tuple_field_ids(returns_type_id),
        Some(
            [
                type_environment.builtins().int,
                type_environment.builtins().bool
            ]
            .as_slice()
        )
    );
}

#[test]
fn optional_generic_instance_conversion_rejects_unresolved_arguments() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let box_path = InternedPath::from_single_str("Box", &mut string_table);
    type_environment.register_nominal_struct(StructTypeDefinition {
        id: NominalTypeId(0),
        path: box_path.to_owned(),
        fields: Box::new([]),
        generic_parameters: None,
        const_record: false,
    });
    let missing_type = string_table.intern("Missing");
    let generic_instance = DataType::GenericInstance {
        base: GenericBaseType::ResolvedNominal(box_path),
        arguments: vec![DataType::StringSlice, DataType::NamedType(missing_type)],
    };

    let converted =
        resolve_diagnostic_type_to_type_id_opt(&generic_instance, &mut type_environment);

    assert_eq!(converted, None);
    let interned_generic_instances = (0..128)
        .filter(|id| {
            matches!(
                type_environment.get(TypeId(*id)),
                Some(TypeDefinition::GenericInstance(_))
            )
        })
        .count();
    assert_eq!(
        interned_generic_instances, 0,
        "unresolved generic arguments must not intern a truncated instance"
    );
}

#[test]
fn checked_type_id_conversion_rejects_unresolved_named_type() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let missing = string_table.intern("Missing");
    let location = SourceLocation::default();

    let error = resolve_diagnostic_type_to_type_id_checked(
        &DataType::NamedType(missing),
        &mut type_environment,
        &location,
    )
    .expect_err("checked conversion should reject unresolved type names");

    assert_diagnostic_payload(
        error,
        |payload| {
            matches!(
                payload,
                DiagnosticPayload::UnknownName {
                    name,
                    namespace: NameNamespace::Type,
                } if *name == missing
            )
        },
        "UnknownName(type)",
    );
}

#[test]
fn parses_collection_with_capacity() {
    let mut string_table = StringTable::new();
    let mut stream = stream_from_tokens(
        vec![
            token(TokenKind::OpenCurly),
            token(TokenKind::DatatypeInt),
            token(TokenKind::IntLiteral(64)),
            token(TokenKind::CloseCurly),
            token(TokenKind::Eof),
        ],
        &mut string_table,
    );

    let parsed =
        parse_type_annotation_with_capacity(&mut stream, TypeAnnotationContext::DeclarationTarget)
            .expect("collection with capacity should parse");

    assert_eq!(
        parsed.parsed_type,
        ParsedTypeRef::Collection {
            element: Box::new(ParsedTypeRef::BuiltinInt {
                location: SourceLocation::default()
            }),
            location: SourceLocation::default(),
        }
    );
    assert!(parsed.collection_capacity.is_some());
    assert_eq!(parsed.collection_capacity.unwrap().value, 64);
}

#[test]
fn parses_collection_without_capacity() {
    let mut string_table = StringTable::new();
    let mut stream = stream_from_tokens(
        vec![
            token(TokenKind::OpenCurly),
            token(TokenKind::DatatypeInt),
            token(TokenKind::CloseCurly),
            token(TokenKind::Eof),
        ],
        &mut string_table,
    );

    let parsed =
        parse_type_annotation_with_capacity(&mut stream, TypeAnnotationContext::DeclarationTarget)
            .expect("collection without capacity should parse");

    assert_eq!(
        parsed.parsed_type,
        ParsedTypeRef::Collection {
            element: Box::new(ParsedTypeRef::BuiltinInt {
                location: SourceLocation::default()
            }),
            location: SourceLocation::default(),
        }
    );
    assert!(parsed.collection_capacity.is_none());
}

#[test]
fn parses_collection_with_generic_element_and_capacity() {
    let mut string_table = StringTable::new();
    let box_name = string_table.intern("Box");
    let mut stream = stream_from_tokens(
        vec![
            token(TokenKind::OpenCurly),
            token(TokenKind::Symbol(box_name)),
            token(TokenKind::Of),
            token(TokenKind::DatatypeString),
            token(TokenKind::IntLiteral(16)),
            token(TokenKind::CloseCurly),
            token(TokenKind::Eof),
        ],
        &mut string_table,
    );

    let parsed =
        parse_type_annotation_with_capacity(&mut stream, TypeAnnotationContext::DeclarationTarget)
            .expect("collection with generic element and capacity should parse");

    assert_eq!(
        parsed.parsed_type,
        ParsedTypeRef::Collection {
            element: Box::new(ParsedTypeRef::Applied {
                base: Box::new(ParsedTypeRef::Named {
                    name: box_name,
                    location: SourceLocation::default()
                }),
                arguments: vec![ParsedTypeRef::BuiltinString {
                    location: SourceLocation::default()
                }],
                location: SourceLocation::default(),
            }),
            location: SourceLocation::default(),
        }
    );
    assert_eq!(parsed.collection_capacity.unwrap().value, 16);
}

#[test]
fn rejects_negative_collection_capacity() {
    let mut string_table = StringTable::new();
    let mut stream = stream_from_tokens(
        vec![
            token(TokenKind::OpenCurly),
            token(TokenKind::DatatypeInt),
            token(TokenKind::IntLiteral(-1)),
            token(TokenKind::CloseCurly),
            token(TokenKind::Eof),
        ],
        &mut string_table,
    );

    let error =
        parse_type_annotation_with_capacity(&mut stream, TypeAnnotationContext::DeclarationTarget)
            .expect_err("negative capacity should fail");

    assert_diagnostic_payload(
        error,
        |payload| {
            matches!(
                payload,
                DiagnosticPayload::InvalidCollectionType {
                    reason: InvalidCollectionTypeReason::NegativeCapacity,
                }
            )
        },
        "InvalidCollectionType(NegativeCapacity)",
    );
}

#[test]
fn rejects_collection_type_missing_close_curly_with_expected_token() {
    let mut string_table = StringTable::new();
    let mut stream = stream_from_tokens(
        vec![
            token(TokenKind::OpenCurly),
            token(TokenKind::DatatypeInt),
            token(TokenKind::Eof),
        ],
        &mut string_table,
    );

    let error =
        parse_type_annotation_with_capacity(&mut stream, TypeAnnotationContext::DeclarationTarget)
            .expect_err("missing collection close delimiter should fail");

    assert_diagnostic_payload(
        error,
        |payload| {
            matches!(
                payload,
                DiagnosticPayload::ExpectedToken {
                    expected: TokenKind::CloseCurly,
                    found: Some(TokenKind::Eof),
                }
            )
        },
        "ExpectedToken(CloseCurly)",
    );
}

#[test]
fn collection_type_identity_ignores_capacity() {
    let with_capacity = DataType::collection(DataType::Int);
    let without_capacity = DataType::collection(DataType::Int);
    assert_eq!(with_capacity, without_capacity);
}
