//! String-ID remapping tests for declaration shells and type-annotation helpers.
//!
//! WHAT: verifies that `DeclarationSyntax`, `BindingTargetSyntax`, `InitializerReference`,
//!      and `ParsedTypeRef::Collection` can be remapped after a string-table merge.
//! WHY: header parsing produces declaration shells using local string tables; remapping must
//!      preserve all names, type annotations, initializer tokens, and source locations.

use crate::compiler_frontend::compiler_messages::source_location::{CharPosition, SourceLocation};
use crate::compiler_frontend::datatypes::parsed::{ParsedCollectionCapacity, ParsedTypeRef};
use crate::compiler_frontend::declaration_syntax::binding_mode::BindingMode;
use crate::compiler_frontend::declaration_syntax::declaration_shell::{
    BindingTargetSyntax, DeclarationSyntax, InitializerReference,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{Token, TokenKind};

fn make_location(string_table: &mut StringTable) -> SourceLocation {
    let path = InternedPath::from_single_str("test.bst", string_table);
    SourceLocation::new(path, CharPosition::default(), CharPosition::default())
}

fn assert_test_location(location: &SourceLocation, string_table: &StringTable) {
    let scope_components = location
        .scope
        .as_components()
        .iter()
        .map(|id| string_table.resolve(*id))
        .collect::<Vec<_>>();

    assert_eq!(scope_components, vec!["test.bst"]);
}

fn make_symbol_token(name: StringId, string_table: &mut StringTable) -> Token {
    Token::new(TokenKind::Symbol(name), make_location(string_table))
}

#[test]
fn collection_capacity_tokens_remap() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let cap_name = local.intern("capacity");
    let cap_tokens = vec![
        Token::new(TokenKind::Symbol(cap_name), make_location(&mut local)),
        Token::new(TokenKind::Add, make_location(&mut local)),
        Token::new(TokenKind::IntLiteral(16), make_location(&mut local)),
    ];

    let mut parsed = ParsedTypeRef::Collection {
        element: Box::new(ParsedTypeRef::BuiltinInt {
            location: make_location(&mut local),
        }),
        location: make_location(&mut local),
        fixed_capacity: Some(ParsedCollectionCapacity {
            tokens: cap_tokens,
            location: make_location(&mut local),
        }),
    };

    let remap = global.merge_from(&local);
    parsed.remap_string_ids(&remap);

    match parsed {
        ParsedTypeRef::Collection {
            element,
            fixed_capacity,
            ..
        } => {
            assert_eq!(
                *element,
                ParsedTypeRef::BuiltinInt {
                    location: make_location(&mut global),
                }
            );
            let capacity = fixed_capacity.expect("capacity tokens should be present");
            assert_test_location(&capacity.location, &global);
            assert_eq!(capacity.tokens.len(), 3);
            match &capacity.tokens[0].kind {
                TokenKind::Symbol(id) => assert_eq!(global.resolve(*id), "capacity"),
                other => panic!("expected Symbol, got {:?}", other),
            }
            assert_eq!(capacity.tokens[1].kind, TokenKind::Add);
            assert_eq!(capacity.tokens[2].kind, TokenKind::IntLiteral(16));
        }
        other => panic!("expected Collection, got {:?}", other),
    }
}

#[test]
fn initializer_reference_remaps_name_and_location() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let ref_name = local.intern("other_const");
    let member_name = local.intern("content");

    let mut reference = InitializerReference {
        name: ref_name,
        dot_member: Some(member_name),
        location: make_location(&mut local),
        followed_by_call: false,
        followed_by_choice_namespace: false,
    };

    let remap = global.merge_from(&local);
    reference.remap_string_ids(&remap);

    assert_eq!(global.resolve(reference.name), "other_const");
    assert_eq!(
        reference.dot_member.map(|member| global.resolve(member)),
        Some("content")
    );
    assert_test_location(&reference.location, &global);
}

#[test]
fn declaration_syntax_remaps_all_fields() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let type_name = local.intern("String");
    let init_name = local.intern("init_value");
    let ref_name = local.intern("ref_value");

    let mut declaration = DeclarationSyntax {
        binding_mode: BindingMode::MutableRuntime,
        type_annotation: ParsedTypeRef::Named {
            name: type_name,
            location: make_location(&mut local),
        },
        initializer_tokens: vec![make_symbol_token(init_name, &mut local)],
        initializer_references: vec![InitializerReference {
            name: ref_name,
            dot_member: None,
            location: make_location(&mut local),
            followed_by_call: true,
            followed_by_choice_namespace: false,
        }],
        location: make_location(&mut local),
    };

    let remap = global.merge_from(&local);
    declaration.remap_string_ids(&remap);

    match declaration.type_annotation {
        ParsedTypeRef::Named { name, .. } => {
            assert_eq!(global.resolve(name), "String");
        }
        _ => panic!("expected Named type annotation"),
    }

    assert_eq!(declaration.initializer_tokens.len(), 1);
    match &declaration.initializer_tokens[0].kind {
        TokenKind::Symbol(id) => {
            assert_eq!(global.resolve(*id), "init_value");
        }
        _ => panic!("expected Symbol token"),
    }

    assert_eq!(declaration.initializer_references.len(), 1);
    assert_eq!(
        global.resolve(declaration.initializer_references[0].name),
        "ref_value"
    );
    assert_test_location(&declaration.initializer_references[0].location, &global);
    assert_test_location(&declaration.location, &global);
}

#[test]
fn binding_target_syntax_remaps_all_fields() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let name = local.intern("my_var");
    let type_name = local.intern("Bool");

    let mut target = BindingTargetSyntax {
        name,
        binding_mode: BindingMode::ImmutableRuntime,
        type_annotation: ParsedTypeRef::Named {
            name: type_name,
            location: make_location(&mut local),
        },
        location: make_location(&mut local),
    };

    let remap = global.merge_from(&local);
    target.remap_string_ids(&remap);

    assert_eq!(global.resolve(target.name), "my_var");

    match target.type_annotation {
        ParsedTypeRef::Named { name, .. } => {
            assert_eq!(global.resolve(name), "Bool");
        }
        _ => panic!("expected Named type annotation"),
    }
    assert_test_location(&target.location, &global);
}
