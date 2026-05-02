//! Type-syntax parsing and resolution regression tests.
//!
//! WHAT: validates type annotation parsing and type resolution in composite types.
//! WHY: type syntax is the source of truth for frontend type identity; parser drift here
//!      affects every downstream type check.

use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::generics::{
    GenericBaseType, GenericParameter, GenericParameterList, TypeParameterId,
};
use crate::compiler_frontend::declaration_syntax::type_syntax::{
    TypeAnnotationContext, TypeResolutionContext, parse_type_annotation,
    parse_type_annotation_with_capacity, resolve_type,
};
use crate::compiler_frontend::headers::module_symbols::{
    GenericDeclarationKind, GenericDeclarationMetadata,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::compiler_frontend::value_mode::ValueMode;
use rustc_hash::FxHashMap;

fn stream_from_tokens(tokens: Vec<Token>, string_table: &mut StringTable) -> FileTokens {
    FileTokens::new(
        InternedPath::from_single_str("type_syntax_tests", string_table),
        tokens,
    )
}

fn token(kind: TokenKind) -> Token {
    Token::new(kind, SourceLocation::default())
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

#[test]
fn declaration_context_allows_inferred_annotations() {
    let mut string_table = StringTable::new();
    let mut stream = stream_from_tokens(
        vec![token(TokenKind::Assign), token(TokenKind::Eof)],
        &mut string_table,
    );

    let parsed = parse_type_annotation(&mut stream, TypeAnnotationContext::DeclarationTarget)
        .expect("declaration type annotation should parse");

    assert_eq!(parsed, DataType::Inferred);
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
        DataType::Option(Box::new(DataType::NamedType(point)))
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

    assert!(error.msg.contains("None is not a valid parameter type"));
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

    assert!(error.msg.contains("Keyword 'This' is reserved for traits"));
    assert!(error.msg.contains("deferred for Alpha"));
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

    assert!(
        error
            .msg
            .contains("`type` starts a generic declaration and is not valid")
    );
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

    assert!(error.msg.contains("Unexpected `of`"));
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
        DataType::GenericInstance {
            base: GenericBaseType::Named(box_name),
            arguments: vec![DataType::StringSlice],
        }
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
        DataType::collection(DataType::GenericInstance {
            base: GenericBaseType::Named(box_name),
            arguments: vec![DataType::StringSlice],
        })
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

    assert!(
        error
            .msg
            .contains("Nested generic type applications are not supported")
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

    assert!(error.msg.contains("Duplicate optional marker '?'"));
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

    let resolution_context = TypeResolutionContext::from_declarations(&declarations);

    let location = SourceLocation::default();
    let resolved = resolve_type(&unresolved, &location, &resolution_context, &string_table)
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
            DataType::runtime_struct(box_path.to_owned(), vec![]),
            ValueMode::ImmutableOwned,
        ),
    }];
    let mut generic_declarations = FxHashMap::default();
    generic_declarations.insert(box_path.to_owned(), single_parameter_metadata(t_name));

    let resolution_context = TypeResolutionContext {
        declarations: &declarations,
        visible_declaration_ids: None,
        visible_external_symbols: None,
        visible_source_bindings: None,
        visible_type_aliases: None,
        resolved_type_aliases: None,
        generic_declarations_by_path: Some(&generic_declarations),
        generic_parameters: None,
    };

    let location = SourceLocation::default();
    let resolved = resolve_type(&unresolved, &location, &resolution_context, &string_table)
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
            DataType::runtime_struct(box_path.to_owned(), vec![]),
            ValueMode::ImmutableOwned,
        ),
    }];
    let mut generic_declarations = FxHashMap::default();
    generic_declarations.insert(box_path, single_parameter_metadata(t_name));

    let resolution_context = TypeResolutionContext {
        declarations: &declarations,
        visible_declaration_ids: None,
        visible_external_symbols: None,
        visible_source_bindings: None,
        visible_type_aliases: None,
        resolved_type_aliases: None,
        generic_declarations_by_path: Some(&generic_declarations),
        generic_parameters: None,
    };

    let location = SourceLocation::default();
    let error = resolve_type(&unresolved, &location, &resolution_context, &string_table)
        .expect_err("wrong generic arity should fail");

    assert!(error.msg.contains("expects 1 type argument"));
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
            DataType::runtime_struct(box_path.to_owned(), vec![]),
            ValueMode::ImmutableOwned,
        ),
    }];
    let mut generic_declarations = FxHashMap::default();
    generic_declarations.insert(box_path, single_parameter_metadata(t_name));

    let resolution_context = TypeResolutionContext {
        declarations: &declarations,
        visible_declaration_ids: None,
        visible_external_symbols: None,
        visible_source_bindings: None,
        visible_type_aliases: None,
        resolved_type_aliases: None,
        generic_declarations_by_path: Some(&generic_declarations),
        generic_parameters: None,
    };

    let location = SourceLocation::default();
    let error = resolve_type(&unresolved, &location, &resolution_context, &string_table)
        .expect_err("bare generic type name should fail");

    assert!(error.msg.contains("requires type arguments"));
}

#[test]
fn unknown_named_type_reports_consistent_error() {
    let mut string_table = StringTable::new();
    let missing = string_table.intern("Missing");

    let unresolved = DataType::NamedType(missing);
    let location = SourceLocation::default();
    let resolution_context = TypeResolutionContext::from_declarations(&[]);

    let error = resolve_type(&unresolved, &location, &resolution_context, &string_table)
        .expect_err("unknown type should fail");

    assert!(
        error
            .msg
            .contains("Unknown type 'Missing'. Type names must be declared before use."),
        "{}",
        error.msg
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

    assert_eq!(parsed.data_type, DataType::collection(DataType::Int));
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

    assert_eq!(parsed.data_type, DataType::collection(DataType::Int));
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
        parsed.data_type,
        DataType::collection(DataType::GenericInstance {
            base: GenericBaseType::Named(box_name),
            arguments: vec![DataType::StringSlice],
        })
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

    assert!(error.msg.contains("non-negative integer"));
}

#[test]
fn collection_type_identity_ignores_capacity() {
    let with_capacity = DataType::collection(DataType::Int);
    let without_capacity = DataType::collection(DataType::Int);
    assert_eq!(with_capacity, without_capacity);
}
