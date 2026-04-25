//! Receiver method catalog and dispatch regression tests.
//!
//! WHAT: validates how receiver methods are indexed, resolved, and dispatched across struct
//!       and scalar types.
//! WHY: receiver methods bridge user-defined behavior to builtin types; catalog drift breaks
//!      both call resolution and backend lowering.

use super::scope_context::{ContextKind, ScopeContext, TopLevelDeclarationIndex};
use super::*;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::type_resolution::validate_no_recursive_runtime_structs;
use crate::compiler_frontend::datatypes::{BuiltinScalarReceiver, DataType, ReceiverKey};
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;
use rustc_hash::FxHashMap;
use std::rc::Rc;

fn interned_path(parts: &[&str], string_table: &mut StringTable) -> InternedPath {
    InternedPath::from_components(parts.iter().map(|part| string_table.intern(part)).collect())
}

fn empty_receiver_entry(
    function_path: InternedPath,
    source_file: InternedPath,
    receiver: ReceiverKey,
    exported: bool,
) -> ReceiverMethodEntry {
    ReceiverMethodEntry {
        function_path,
        receiver,
        source_file,
        exported,
        receiver_mutable: false,
        signature: FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
    }
}

fn context_for_source_file(
    source_file: InternedPath,
    receiver_methods: ReceiverMethodCatalog,
) -> ScopeContext {
    ScopeContext::new(
        ContextKind::Function,
        InternedPath::new(),
        Rc::new(TopLevelDeclarationIndex::new(vec![])),
        HostRegistry::new(),
        vec![],
    )
    .with_source_file_scope(source_file)
    .with_receiver_methods(Rc::new(receiver_methods))
}

#[test]
fn lookup_receiver_method_requires_exact_source_file_match_when_not_exported() {
    let mut string_table = StringTable::new();
    let method_name = string_table.intern("reset");
    let receiver = ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::Int);
    let key = (receiver.to_owned(), method_name);

    let mut catalog = ReceiverMethodCatalog::default();
    catalog.by_receiver_and_name.insert(
        key,
        empty_receiver_entry(
            interned_path(&["module", "reset"], &mut string_table),
            interned_path(&["src", "#page.bst"], &mut string_table),
            receiver.to_owned(),
            false,
        ),
    );

    let different_shape_context = context_for_source_file(
        interned_path(&["project", "src", "#page.bst"], &mut string_table),
        catalog.to_owned(),
    );
    assert!(
        different_shape_context
            .lookup_receiver_method(&receiver, method_name)
            .is_none(),
        "non-exported methods must not match by suffix-shaped source-file paths"
    );

    let exact_context = context_for_source_file(
        interned_path(&["src", "#page.bst"], &mut string_table),
        catalog,
    );
    assert!(
        exact_context
            .lookup_receiver_method(&receiver, method_name)
            .is_some(),
        "non-exported methods should remain visible inside their exact source file"
    );
}

#[test]
fn visible_method_lookup_prefers_same_file_before_exported_fallback() {
    let mut string_table = StringTable::new();
    let method_name = string_table.intern("render");

    let local_source = interned_path(&["src", "#page.bst"], &mut string_table);
    let exported_source = interned_path(&["lib", "shared.bst"], &mut string_table);

    let local_entry = empty_receiver_entry(
        interned_path(&["src", "render_local"], &mut string_table),
        local_source.to_owned(),
        ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::String),
        false,
    );
    let exported_entry = empty_receiver_entry(
        interned_path(&["lib", "render_exported"], &mut string_table),
        exported_source,
        ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::String),
        true,
    );

    let mut catalog = ReceiverMethodCatalog::default();
    catalog
        .by_method_name
        .insert(method_name, vec![exported_entry, local_entry.to_owned()]);

    let context = context_for_source_file(local_source, catalog);
    let resolved = context
        .lookup_visible_receiver_method_by_name(method_name)
        .expect("same-file receiver method should be visible");
    assert_eq!(
        resolved.function_path, local_entry.function_path,
        "same-file methods must be preferred over exported fallback entries"
    );
}

#[test]
fn visible_method_lookup_uses_stable_exported_fallback_order() {
    let mut string_table = StringTable::new();
    let method_name = string_table.intern("render");
    let context_source = interned_path(&["src", "#page.bst"], &mut string_table);

    let first_exported = empty_receiver_entry(
        interned_path(&["lib", "a_render"], &mut string_table),
        interned_path(&["lib", "a.bst"], &mut string_table),
        ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::String),
        true,
    );
    let second_exported = empty_receiver_entry(
        interned_path(&["lib", "z_render"], &mut string_table),
        interned_path(&["lib", "z.bst"], &mut string_table),
        ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::String),
        true,
    );

    let mut catalog = ReceiverMethodCatalog::default();
    catalog.by_method_name.insert(
        method_name,
        vec![first_exported.to_owned(), second_exported],
    );

    let context = context_for_source_file(context_source, catalog);
    let resolved = context
        .lookup_visible_receiver_method_by_name(method_name)
        .expect("exported receiver method should be visible");
    assert_eq!(
        resolved.function_path, first_exported.function_path,
        "exported fallback lookup should resolve using stable catalog order"
    );
}

#[test]
fn recursive_runtime_struct_cycles_are_rejected() {
    let mut string_table = StringTable::new();
    let struct_a = interned_path(&["A"], &mut string_table);
    let struct_b = interned_path(&["B"], &mut string_table);
    let field_ab = interned_path(&["A", "b"], &mut string_table);
    let field_ba = interned_path(&["B", "a"], &mut string_table);

    let mut struct_fields = FxHashMap::default();
    struct_fields.insert(
        struct_a.to_owned(),
        vec![Declaration {
            id: field_ab,
            value: Expression::new(
                ExpressionKind::NoValue,
                SourceLocation::default(),
                DataType::runtime_struct(struct_b.to_owned(), vec![]),
                ValueMode::ImmutableOwned,
            ),
        }],
    );
    struct_fields.insert(
        struct_b.to_owned(),
        vec![Declaration {
            id: field_ba,
            value: Expression::new(
                ExpressionKind::NoValue,
                SourceLocation::default(),
                DataType::runtime_struct(struct_a, vec![]),
                ValueMode::ImmutableOwned,
            ),
        }],
    );

    let error = validate_no_recursive_runtime_structs(&struct_fields, &string_table)
        .expect_err("recursive runtime struct cycle should be rejected");
    assert!(error.msg.contains("Recursive runtime struct definitions"));
}

#[test]
fn non_recursive_runtime_structs_are_allowed() {
    let mut string_table = StringTable::new();
    let struct_a = interned_path(&["A"], &mut string_table);
    let field_ax = interned_path(&["A", "x"], &mut string_table);

    let mut struct_fields = FxHashMap::default();
    struct_fields.insert(
        struct_a,
        vec![Declaration {
            id: field_ax,
            value: Expression::new(
                ExpressionKind::NoValue,
                SourceLocation::default(),
                DataType::Int,
                ValueMode::ImmutableOwned,
            ),
        }],
    );

    validate_no_recursive_runtime_structs(&struct_fields, &string_table)
        .expect("non-recursive runtime structs should pass validation");
}
