//! Type-resolution regression tests.
//!
//! WHAT: validates diagnostic-type syntax conversion into canonical type identity.
//! WHY: these paths sit at the boundary between diagnostic type syntax and canonical
//!      type identity; mistakes here produce misleading errors or silent wrong-types.

use crate::compiler_frontend::ast::TopLevelDeclarationTable;
use crate::compiler_frontend::ast::ast_nodes::{Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::module_ast::scope_context::{ContextKind, ScopeContext};
use crate::compiler_frontend::ast::type_resolution::{
    TypeResolutionContext, resolve_diagnostic_type_to_type_id_checked,
    resolve_diagnostic_type_to_type_id_opt, resolve_parsed_type_annotation,
};
use crate::compiler_frontend::compiler_errors::SourceLocation;
use crate::compiler_frontend::compiler_messages::{
    DiagnosticPayload, InvalidCollectionTypeReason, InvalidTypeAnnotationReason, NameNamespace,
    TypeAnnotationContext,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::builtin_type_ids;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::parsed::{ParsedCollectionCapacity, ParsedTypeRef};
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tests::parse_support::{
    parse_single_file_ast, parse_single_file_ast_diagnostic,
};
use crate::compiler_frontend::tokenizer::tokens::{Token, TokenKind};
use crate::compiler_frontend::value_mode::ValueMode;
use std::rc::Rc;

// ---------------------------------------------------------------
//  Checked / optional diagnostic-type-to-TypeId bridge tests
// ---------------------------------------------------------------
//
// WHAT: prove that unresolved parse placeholders cannot silently
//       fall back to builtin `none` in production paths.
// WHY: the frontend type boundary cleanup removed unchecked
//      `resolve_diagnostic_type_to_type_id` from constructor
//      shells; these tests guard against regression.

#[test]
fn checked_conversion_rejects_inferred_type() {
    let mut type_environment =
        crate::compiler_frontend::datatypes::environment::TypeEnvironment::new();
    let location = SourceLocation::default();

    let error = resolve_diagnostic_type_to_type_id_checked(
        &DataType::Inferred,
        &mut type_environment,
        &location,
    )
    .expect_err("checked conversion should reject Inferred placeholder");

    assert!(
        matches!(
            &error.payload,
            DiagnosticPayload::InvalidTypeAnnotation {
                context: TypeAnnotationContext::DeclarationTarget,
                reason: InvalidTypeAnnotationReason::ExpectedTypeAnnotation {
                    found: TokenKind::Eof
                },
            }
        ),
        "expected InvalidTypeAnnotation for Inferred, got {:?}",
        error.payload
    );
}

#[test]
fn checked_conversion_rejects_unresolved_namespaced_type() {
    let mut string_table = StringTable::new();
    let mut type_environment =
        crate::compiler_frontend::datatypes::environment::TypeEnvironment::new();
    let location = SourceLocation::default();
    let namespace = string_table.intern("missing");
    let name = string_table.intern("Type");

    let error = resolve_diagnostic_type_to_type_id_checked(
        &DataType::NamespacedType { namespace, name },
        &mut type_environment,
        &location,
    )
    .expect_err("checked conversion should reject unresolved namespaced type");

    assert!(
        matches!(
            &error.payload,
            DiagnosticPayload::UnknownName {
                name: n,
                namespace: NameNamespace::Type,
            } if *n == name
        ),
        "expected UnknownName(type) for namespaced type, got {:?}",
        error.payload
    );
}

#[test]
fn optional_conversion_returns_none_for_unresolved_named_type() {
    let mut string_table = StringTable::new();
    let mut type_environment =
        crate::compiler_frontend::datatypes::environment::TypeEnvironment::new();
    let missing = string_table.intern("Missing");

    let result = resolve_diagnostic_type_to_type_id_opt(
        &DataType::NamedType(missing),
        &mut type_environment,
    );

    assert_eq!(
        result, None,
        "optional conversion must return None for unresolved named type"
    );
}

#[test]
fn optional_conversion_returns_none_for_inferred_type() {
    let mut type_environment =
        crate::compiler_frontend::datatypes::environment::TypeEnvironment::new();

    let result = resolve_diagnostic_type_to_type_id_opt(&DataType::Inferred, &mut type_environment);

    assert_eq!(
        result, None,
        "optional conversion must return None for Inferred placeholder"
    );
}

#[test]
fn optional_conversion_returns_some_for_resolved_builtin() {
    let mut type_environment =
        crate::compiler_frontend::datatypes::environment::TypeEnvironment::new();

    let result = resolve_diagnostic_type_to_type_id_opt(&DataType::Int, &mut type_environment);

    assert_eq!(
        result,
        Some(builtin_type_ids::INT),
        "optional conversion must return Some for resolved builtin"
    );
}

// ---------------------------------------------------------------
//  Fixed-collection capacity expression folding tests
// ---------------------------------------------------------------

#[test]
fn literal_capacity_resolves_to_fixed_collection() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let declaration_table = Rc::new(TopLevelDeclarationTable::new(Vec::new()));
    let mut resolution_context =
        TypeResolutionContext::from_declaration_table(&declaration_table, &mut type_environment);

    let location = SourceLocation::default();
    let parsed = ParsedTypeRef::Collection {
        element: Box::new(ParsedTypeRef::BuiltinInt {
            location: location.clone(),
        }),
        location: location.clone(),
        fixed_capacity: Some(ParsedCollectionCapacity {
            tokens: vec![Token::new(TokenKind::IntLiteral(64), location.clone())],
            location: location.clone(),
        }),
    };

    let resolved = resolve_parsed_type_annotation(
        parsed,
        &location,
        &mut resolution_context,
        &mut string_table,
        None,
    )
    .expect("literal capacity should resolve");

    let type_id = resolved.type_id.expect("should have a type id");
    assert_eq!(
        resolution_context
            .type_environment
            .collection_fixed_capacity(type_id),
        Some(64),
        "capacity should be folded to 64"
    );
}

#[test]
fn zero_capacity_rejected() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let declaration_table = Rc::new(TopLevelDeclarationTable::new(Vec::new()));
    let mut resolution_context =
        TypeResolutionContext::from_declaration_table(&declaration_table, &mut type_environment);

    let location = SourceLocation::default();
    let parsed = ParsedTypeRef::Collection {
        element: Box::new(ParsedTypeRef::BuiltinInt {
            location: location.clone(),
        }),
        location: location.clone(),
        fixed_capacity: Some(ParsedCollectionCapacity {
            tokens: vec![Token::new(TokenKind::IntLiteral(0), location.clone())],
            location: location.clone(),
        }),
    };

    let error = resolve_parsed_type_annotation(
        parsed,
        &location,
        &mut resolution_context,
        &mut string_table,
        None,
    )
    .expect_err("zero capacity should be rejected");

    assert!(
        matches!(
            &error.payload,
            DiagnosticPayload::InvalidCollectionType {
                reason: InvalidCollectionTypeReason::ZeroCapacity,
                ..
            }
        ),
        "expected ZeroCapacity error, got {:?}",
        error.payload
    );
}

#[test]
fn negative_capacity_rejected() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let declaration_table = Rc::new(TopLevelDeclarationTable::new(Vec::new()));
    let mut resolution_context =
        TypeResolutionContext::from_declaration_table(&declaration_table, &mut type_environment);

    let location = SourceLocation::default();
    let parsed = ParsedTypeRef::Collection {
        element: Box::new(ParsedTypeRef::BuiltinInt {
            location: location.clone(),
        }),
        location: location.clone(),
        fixed_capacity: Some(ParsedCollectionCapacity {
            tokens: vec![Token::new(TokenKind::IntLiteral(-5), location.clone())],
            location: location.clone(),
        }),
    };

    let error = resolve_parsed_type_annotation(
        parsed,
        &location,
        &mut resolution_context,
        &mut string_table,
        None,
    )
    .expect_err("negative capacity should be rejected");

    assert!(
        matches!(
            &error.payload,
            DiagnosticPayload::InvalidCollectionType {
                reason: InvalidCollectionTypeReason::NegativeCapacity,
                ..
            }
        ),
        "expected NegativeCapacity error for negative value, got {:?}",
        error.payload
    );
}

#[test]
fn float_capacity_rejected() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let declaration_table = Rc::new(TopLevelDeclarationTable::new(Vec::new()));
    let mut resolution_context =
        TypeResolutionContext::from_declaration_table(&declaration_table, &mut type_environment);

    let location = SourceLocation::default();

    // Provide a scope context so expression parsing can run and detect the float type.
    let scope_context = ScopeContext::new(
        ContextKind::Function,
        InternedPath::new(),
        declaration_table.clone(),
        ExternalPackageRegistry::new(),
        vec![],
    );

    let parsed = ParsedTypeRef::Collection {
        element: Box::new(ParsedTypeRef::BuiltinInt {
            location: location.clone(),
        }),
        location: location.clone(),
        fixed_capacity: Some(ParsedCollectionCapacity {
            tokens: vec![Token::new(TokenKind::FloatLiteral(2.5), location.clone())],
            location: location.clone(),
        }),
    };

    let error = resolve_parsed_type_annotation(
        parsed,
        &location,
        &mut resolution_context,
        &mut string_table,
        Some(&scope_context),
    )
    .expect_err("float capacity should be rejected");

    assert!(
        matches!(
            &error.payload,
            DiagnosticPayload::InvalidCollectionType {
                reason: InvalidCollectionTypeReason::CapacityNotInt,
                ..
            }
        ),
        "expected CapacityNotInt error for float capacity, got {:?}",
        error.payload
    );
}

#[test]
fn constant_capacity_resolves_to_fixed_collection() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let declaration_table = Rc::new(TopLevelDeclarationTable::new(Vec::new()));
    let mut resolution_context =
        TypeResolutionContext::from_declaration_table(&declaration_table, &mut type_environment);

    let location = SourceLocation::default();
    let capacity_name = string_table.intern("capacity");

    // Build a scope context with a local constant declaration.
    let mut scope_context = ScopeContext::new(
        ContextKind::Function,
        InternedPath::new(),
        declaration_table.clone(),
        ExternalPackageRegistry::new(),
        vec![],
    );
    let constant_declaration = Declaration {
        id: InternedPath::from_components(vec![capacity_name]),
        value: Expression::new(
            ExpressionKind::Int(42),
            location.clone(),
            builtin_type_ids::INT,
            DataType::Int,
            ValueMode::ImmutableOwned,
        ),
    };
    scope_context.add_var(constant_declaration);

    let parsed = ParsedTypeRef::Collection {
        element: Box::new(ParsedTypeRef::BuiltinInt {
            location: location.clone(),
        }),
        location: location.clone(),
        fixed_capacity: Some(ParsedCollectionCapacity {
            tokens: vec![Token::new(
                TokenKind::Symbol(capacity_name),
                location.clone(),
            )],
            location: location.clone(),
        }),
    };

    let resolved = resolve_parsed_type_annotation(
        parsed,
        &location,
        &mut resolution_context,
        &mut string_table,
        Some(&scope_context),
    )
    .expect("constant capacity should resolve");

    let type_id = resolved.type_id.expect("should have a type id");
    assert_eq!(
        resolution_context
            .type_environment
            .collection_fixed_capacity(type_id),
        Some(42),
        "capacity should be folded from constant to 42"
    );
}

#[test]
fn capacity_expression_folds_correctly() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let declaration_table = Rc::new(TopLevelDeclarationTable::new(Vec::new()));
    let mut resolution_context =
        TypeResolutionContext::from_declaration_table(&declaration_table, &mut type_environment);

    let location = SourceLocation::default();
    let capacity_name = string_table.intern("capacity");

    // Build a scope context with a local constant declaration.
    let mut scope_context = ScopeContext::new(
        ContextKind::Function,
        InternedPath::new(),
        declaration_table.clone(),
        ExternalPackageRegistry::new(),
        vec![],
    );
    let constant_declaration = Declaration {
        id: InternedPath::from_components(vec![capacity_name]),
        value: Expression::new(
            ExpressionKind::Int(32),
            location.clone(),
            builtin_type_ids::INT,
            DataType::Int,
            ValueMode::ImmutableOwned,
        ),
    };
    scope_context.add_var(constant_declaration);

    let parsed = ParsedTypeRef::Collection {
        element: Box::new(ParsedTypeRef::BuiltinInt {
            location: location.clone(),
        }),
        location: location.clone(),
        fixed_capacity: Some(ParsedCollectionCapacity {
            tokens: vec![
                Token::new(TokenKind::Symbol(capacity_name), location.clone()),
                Token::new(TokenKind::Add, location.clone()),
                Token::new(TokenKind::IntLiteral(16), location.clone()),
            ],
            location: location.clone(),
        }),
    };

    let resolved = resolve_parsed_type_annotation(
        parsed,
        &location,
        &mut resolution_context,
        &mut string_table,
        Some(&scope_context),
    )
    .expect("capacity expression should fold");

    let type_id = resolved.type_id.expect("should have a type id");
    assert_eq!(
        resolution_context
            .type_environment
            .collection_fixed_capacity(type_id),
        Some(48),
        "capacity expression 32 + 16 should fold to 48"
    );
}

#[test]
fn nested_fixed_collections_fold_both_capacities() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let declaration_table = Rc::new(TopLevelDeclarationTable::new(Vec::new()));
    let mut resolution_context =
        TypeResolutionContext::from_declaration_table(&declaration_table, &mut type_environment);

    let location = SourceLocation::default();
    let inner = ParsedTypeRef::Collection {
        element: Box::new(ParsedTypeRef::BuiltinInt {
            location: location.clone(),
        }),
        location: location.clone(),
        fixed_capacity: Some(ParsedCollectionCapacity {
            tokens: vec![Token::new(TokenKind::IntLiteral(4), location.clone())],
            location: location.clone(),
        }),
    };
    let outer = ParsedTypeRef::Collection {
        element: Box::new(inner),
        location: location.clone(),
        fixed_capacity: Some(ParsedCollectionCapacity {
            tokens: vec![Token::new(TokenKind::IntLiteral(8), location.clone())],
            location: location.clone(),
        }),
    };

    let resolved = resolve_parsed_type_annotation(
        outer,
        &location,
        &mut resolution_context,
        &mut string_table,
        None,
    )
    .expect("nested fixed collections should resolve");

    let outer_type_id = resolved.type_id.expect("should have a type id");
    let outer_shape = resolution_context
        .type_environment
        .collection_shape(outer_type_id)
        .expect("outer should be a collection");
    assert_eq!(
        outer_shape.fixed_capacity,
        Some(8),
        "outer capacity should be 8"
    );

    let inner_type_id = outer_shape.element_type;
    let inner_shape = resolution_context
        .type_environment
        .collection_shape(inner_type_id)
        .expect("inner should be a collection");
    assert_eq!(
        inner_shape.fixed_capacity,
        Some(4),
        "inner capacity should be 4"
    );
}

#[test]
fn struct_field_alias_capacity_expression_folds_after_constants() {
    let source = r#"
capacity #Int = 4
Names as {capacity String}

Buffer = |
    items Names,
|
"#;
    let (ast, mut string_table) = parse_single_file_ast(source);

    let buffer_name = string_table.intern("Buffer");
    let buffer_path =
        InternedPath::from_single_str("#page.bst", &mut string_table).append(buffer_name);
    let buffer_type_id = ast
        .type_environment
        .nominal_id_for_path(&buffer_path)
        .and_then(|nominal_id| ast.type_environment.type_id_for_nominal_id(nominal_id))
        .expect("Buffer should have a nominal TypeId");
    let items_name = string_table.intern("items");
    let items_field = ast
        .type_environment
        .field_for(buffer_type_id, items_name)
        .expect("Buffer.items should be registered");

    assert_eq!(
        ast.type_environment
            .collection_fixed_capacity(items_field.type_id),
        Some(4),
        "type alias capacity expression should fold before final struct field TypeId registration"
    );
}

#[test]
fn invalid_function_signature_capacity_is_not_erased_to_growable() {
    let diagnostic = parse_single_file_ast_diagnostic(
        r#"
take |items {0 Int}|:
;
"#,
    );

    assert!(
        matches!(
            diagnostic.payload,
            DiagnosticPayload::InvalidCollectionType {
                reason: InvalidCollectionTypeReason::ZeroCapacity,
                ..
            }
        ),
        "expected invalid fixed-capacity diagnostic, got {:?}",
        diagnostic.payload
    );
}

#[test]
fn function_return_capacity_expression_folds_to_type_id() {
    let source = r#"
capacity #Int = 3

make || -> {capacity Int}:
    assert(false, "signature only")
;
"#;
    let (ast, mut string_table) = parse_single_file_ast(source);

    let make_name = string_table.intern("make");
    let make_path = InternedPath::from_single_str("#page.bst", &mut string_table).append(make_name);
    let signature = ast
        .nodes
        .iter()
        .find_map(|node| match &node.kind {
            NodeKind::Function(path, signature, _) if path == &make_path => Some(signature),
            _ => None,
        })
        .expect("make signature should be registered");
    let return_type_id = signature.returns[0]
        .type_id
        .expect("return slot should have a TypeId");

    assert_eq!(
        ast.type_environment
            .collection_fixed_capacity(return_type_id),
        Some(3),
        "fixed capacity in return annotation should fold into the canonical return TypeId"
    );
}
