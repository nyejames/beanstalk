//! Registry-authority tests for compile-time field inlining.
//!
//! WHAT: exercises receiver-authored and resolved-default field values whose templates belong to
//!       a foreign registry store while their compatibility content deliberately says runtime.
//! WHY: field access must classify the effective TIR view instead of rebuilding stale content in
//!      whichever store happens to be active at the access site.

use std::cell::RefCell;
use std::rc::Rc;

use rustc_hash::FxHashMap;

use super::{const_inline_field_value, const_inline_field_value_from_receiver};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::template::{SlotKey, Style, TemplateType};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::tir::{
    TemplateIrBuilder, TemplateIrRegistry, TemplateIrStore, TemplateIrSummary, TemplateOverlaySet,
    TemplateOverlaySetId, TemplateRef, TemplateTirPhase, TemplateTirReference,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::definitions::{FieldDefinition, StructTypeDefinition};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::NominalTypeId;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;

fn stale_content_slot_template(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
) -> Template {
    let location = SourceLocation::default();
    let mut builder = TemplateIrBuilder::new(store);
    let slot = builder.push_slot_node(SlotKey::Default, location.clone());
    let template_id = builder.finish_template(
        slot,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        location.clone(),
    );

    let mut template = Template::empty();
    template.content.add(Expression::reference(
        InternedPath::from_single_str("runtime_value", string_table),
        DataType::StringSlice,
        location.clone(),
        ValueMode::ImmutableReference,
    ));
    template.kind = TemplateType::String;
    template.location = location;
    template.tir_reference = Some(TemplateTirReference {
        root: TemplateRef::new(store.store_id(), template_id),
        store_owner: store.owner(),
        is_composed: true,
        phase: TemplateTirPhase::Composed,
        overlay_set_id: TemplateOverlaySetId::empty_for_test(),
    });
    template
}

fn registry_with_foreign_template(
    string_table: &mut StringTable,
) -> (Rc<RefCell<TemplateIrRegistry>>, Template) {
    let primary_store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let foreign_store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut registry = TemplateIrRegistry::new();
    registry.allocate_overlay_set(TemplateOverlaySet::empty());
    registry.adopt_store(primary_store);
    registry.adopt_store(Rc::clone(&foreign_store));

    let template = stale_content_slot_template(&mut foreign_store.borrow_mut(), string_table);
    (Rc::new(RefCell::new(registry)), template)
}

#[test]
fn receiver_authored_field_uses_foreign_effective_tir() {
    let mut string_table = StringTable::new();
    let (registry, template) = registry_with_foreign_template(&mut string_table);
    let field_name = string_table.intern("content");
    let field_path = InternedPath::from_components(vec![field_name]);
    let receiver_value = Expression::struct_instance(
        InternedPath::from_single_str("Card", &mut string_table),
        vec![Declaration {
            id: field_path,
            value: Expression::template(template, ValueMode::ImmutableOwned),
        }],
        SourceLocation::default(),
        ValueMode::ImmutableOwned,
        true,
        None,
        TypeEnvironment::new().builtins().none,
    );
    let receiver = AstNode {
        kind: NodeKind::ExpressionStatement(receiver_value),
        location: SourceLocation::default(),
        scope: InternedPath::from_single_str("scope", &mut string_table),
    };

    let inlined =
        const_inline_field_value_from_receiver(&receiver, field_name, &registry, &string_table)
            .expect("effective TIR classification should succeed")
            .expect("receiver-authored const field should inline");

    assert!(matches!(inlined.kind, ExpressionKind::Template(_)));
    assert_eq!(inlined.value_mode, ValueMode::ImmutableOwned);
}

#[test]
fn resolved_default_field_uses_foreign_effective_tir() {
    let mut string_table = StringTable::new();
    let (registry, template) = registry_with_foreign_template(&mut string_table);
    let mut type_environment = TypeEnvironment::new();
    let field_name = string_table.intern("content");
    let struct_path = InternedPath::from_single_str("Card", &mut string_table);
    let field_path = struct_path.clone().append(field_name);
    let (_, struct_type_id) = type_environment.register_nominal_struct(StructTypeDefinition {
        id: NominalTypeId(0),
        path: struct_path.clone(),
        fields: vec![FieldDefinition {
            name: field_path.clone(),
            type_id: type_environment.builtins().string,
            location: SourceLocation::default(),
        }]
        .into_boxed_slice(),
        generic_parameters: None,
        const_record: true,
    });
    let resolved_fields = FxHashMap::from_iter([(
        struct_path.clone(),
        vec![Declaration {
            id: field_path,
            value: Expression::template(template, ValueMode::ImmutableOwned),
        }],
    )]);
    let receiver = AstNode {
        kind: NodeKind::ExpressionStatement(Expression::reference(
            InternedPath::from_single_str("card", &mut string_table),
            DataType::const_struct_record(struct_path, struct_type_id),
            SourceLocation::default(),
            ValueMode::ImmutableReference,
        )),
        location: SourceLocation::default(),
        scope: InternedPath::from_single_str("scope", &mut string_table),
    };

    let inlined = const_inline_field_value(
        &receiver,
        struct_type_id,
        field_name,
        &type_environment,
        Some(&resolved_fields),
        &registry,
        &string_table,
    )
    .expect("effective TIR classification should succeed")
    .expect("resolved const default should inline");

    assert!(matches!(inlined.kind, ExpressionKind::Template(_)));
    assert_eq!(inlined.value_mode, ValueMode::ImmutableOwned);
}
