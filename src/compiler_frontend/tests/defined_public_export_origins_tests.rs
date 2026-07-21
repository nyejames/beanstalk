//! Focused hidden-invariant tests for the directly-defined public export identity component.
//!
//! WHAT: exercises the invariants of [`DefinedPublicExportOrigins`] that integration output
//!      cannot inspect: direct export bindings cover exactly the public declarations authored in
//!      the active module root, category distinctions are exact, receiver methods attach to their
//!      receiver surface rather than free namespace bindings, generic receiver methods attach to
//!      their generic nominal base origin, private declarations and the implicit start function are
//!      excluded, and ordering is deterministic independent of declaration scheduling.
//! WHY: these are construction invariants owned by `compiler_frontend::defined_public_export_origins`,
//!      so they own a focused test beside the module rather than an end-to-end case.

use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::{ReceiverMethodCatalog, ReceiverMethodEntry};
use crate::compiler_frontend::datatypes::ReceiverKey;
use crate::compiler_frontend::defined_public_export_origins::build_defined_public_export_origin_draft;
use crate::compiler_frontend::headers::parse_file_headers::parse_file_headers_tests::parse_single_file_headers_with_table;
use crate::compiler_frontend::headers::parse_file_headers::{BoundModuleHeaders, HeaderKind};
use crate::compiler_frontend::semantic_identity::{
    DefinedPublicExportOrigins, ExportBinding, FunctionOriginKind, ModuleRootRole,
    OriginDeclarationId, OriginTypeCategory, StableModuleOriginIdentity, StablePackageIdentity,
};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;

/// Build the component for one active-root source using a deterministic synthetic module origin.
///
/// The receiver-method catalog defaults to empty, which exercises the free-binding projection
/// and confirms a module with no receiver methods records no receiver surfaces.
fn build_origins(source: &str) -> DefinedPublicExportOrigins {
    build_origins_with_catalog(source, ReceiverMethodCatalog::default())
}

/// Build the component for one active-root source with a caller-supplied receiver-method catalog.
fn build_origins_with_catalog(
    source: &str,
    catalog: ReceiverMethodCatalog,
) -> DefinedPublicExportOrigins {
    build_origins_for_project_with_catalog(source, "test-project", catalog)
}

/// Build the component for one active-root source using a configurable project name so module
/// distinction is testable without a second discovered module.
fn build_origins_for_project_with_catalog(
    source: &str,
    project_name: &str,
    catalog: ReceiverMethodCatalog,
) -> DefinedPublicExportOrigins {
    let (headers, string_table) = parse_single_file_headers_with_table(source);
    let module_origin = StableModuleOriginIdentity::from_portable_path(
        StablePackageIdentity::project_local(project_name),
        String::new(),
        ModuleRootRole::Normal,
    );
    let draft = build_defined_public_export_origin_draft(
        &module_origin,
        &headers.headers,
        &headers.module_symbols,
        &string_table,
    )
    .expect("defined public export origin draft must build for valid headers");
    draft
        .finalize(&catalog, &string_table)
        .expect("receiver surface origins must finalize for a valid catalog")
}

fn binding_for<'a>(
    origins: &'a DefinedPublicExportOrigins,
    public_name: &str,
) -> &'a ExportBinding {
    origins
        .export_bindings()
        .iter()
        .find(|binding| binding.public_name() == public_name)
        .unwrap_or_else(|| panic!("no export binding for public name `{public_name}`"))
}

fn binding_names(origins: &DefinedPublicExportOrigins) -> Vec<&str> {
    origins
        .export_bindings()
        .iter()
        .map(|binding| binding.public_name())
        .collect()
}

/// Find the canonical declaration path of a directly-defined public nominal type by name.
///
/// The receiver-surface projection matches catalog entries by canonical declaration path, so the
/// test catalog must carry the same path the header parser assigned to the type declaration.
fn nominal_type_path(
    headers: &BoundModuleHeaders,
    type_name: &str,
    string_table: &StringTable,
) -> InternedPath {
    for header in &headers.headers {
        if header.kind.is_authored_public_export_declaration()
            && header.tokens.src_path.name_str(string_table) == Some(type_name)
        {
            return header.tokens.src_path.clone();
        }
    }
    panic!("no public nominal type header named `{type_name}` in test source")
}

/// Build a receiver-method catalog entry for a method on a nominal receiver type.
fn receiver_method_entry(
    method_name: &str,
    receiver: ReceiverKey,
    string_table: &mut StringTable,
) -> (InternedPath, ReceiverMethodEntry) {
    let function_path = InternedPath::from_single_str(method_name, string_table);
    let entry = ReceiverMethodEntry {
        function_path: function_path.clone(),
        receiver,
        source_file: InternedPath::new(),
        receiver_mutable: false,
        signature: FunctionSignature::default(),
    };
    (function_path, entry)
}

/// A public surface exercising every directly-defined public declaration category.
const ALL_CATEGORIES_SOURCE: &str = "\
export:\n\
    render |button Button| -> String:\n\
        return button.label\n\
    ;\n\
    Button = | label String |\n\
    Status :: Ready,\n\
    ;\n\
    Shape as Int\n\
    count #= 1\n\
    DISPLAYABLE must:\n\
        show |This| -> String\n\
    ;\n\
;\n";

#[test]
fn directly_defined_public_exports_get_export_bindings_with_exact_category() {
    let origins = build_origins(ALL_CATEGORIES_SOURCE);

    // Every public declaration category is admitted with its exact origin category.
    assert!(
        matches!(
            binding_for(&origins, "render").origin(),
            OriginDeclarationId::Function(function)
                if matches!(function.kind(), FunctionOriginKind::Free)
        ),
        "a public free function must produce a free-function origin"
    );
    assert!(
        matches!(
            binding_for(&origins, "Button").origin(),
            OriginDeclarationId::Type(type_id)
                if type_id.category() == OriginTypeCategory::Struct
        ),
        "a public struct must produce a struct type origin"
    );
    assert!(
        matches!(
            binding_for(&origins, "Status").origin(),
            OriginDeclarationId::Type(type_id)
                if type_id.category() == OriginTypeCategory::Choice
        ),
        "a public choice must produce a choice type origin"
    );
    assert!(
        matches!(
            binding_for(&origins, "Shape").origin(),
            OriginDeclarationId::Type(type_id)
                if type_id.category() == OriginTypeCategory::TransparentAlias
        ),
        "a public transparent alias must produce a transparent-alias type origin"
    );
    assert!(
        matches!(
            binding_for(&origins, "count").origin(),
            OriginDeclarationId::Constant(_)
        ),
        "a public constant must produce a constant origin"
    );
    assert!(
        matches!(
            binding_for(&origins, "DISPLAYABLE").origin(),
            OriginDeclarationId::Trait(_)
        ),
        "a public trait must produce a trait origin"
    );
}

#[test]
fn receiver_methods_attach_to_receiver_surface_not_free_bindings() {
    // `Counter` is a public struct; `tick` is a private receiver method on `Counter`. Receiver
    // methods cannot be exported directly, so `tick` is never a free namespace binding.
    let source = "\
export:\n\
    Counter = | value Int |\n\
;\n\
tick |this Counter| -> Int:\n\
    return this.value\n\
;\n";

    let (headers, mut string_table) = parse_single_file_headers_with_table(source);
    let counter_path = nominal_type_path(&headers, "Counter", &string_table);
    let (function_path, entry) =
        receiver_method_entry("tick", ReceiverKey::Struct(counter_path), &mut string_table);

    let mut catalog = ReceiverMethodCatalog::default();
    catalog.by_function_path.insert(function_path, entry);

    let origins = build_origins_with_catalog(source, catalog);

    assert!(
        !origins
            .export_bindings()
            .iter()
            .any(|binding| binding.public_name() == "tick"),
        "a receiver method must not become a free namespace export binding"
    );

    let counter_surface = origins
        .receiver_surfaces()
        .iter()
        .find(|surface| surface.receiver().defining_name() == "Counter")
        .expect("a public struct with a receiver method must own a receiver surface");

    assert_eq!(
        counter_surface.receiver().category(),
        OriginTypeCategory::Struct,
        "the receiver surface must carry the receiver's stable type origin"
    );

    let method_names: Vec<&str> = counter_surface
        .methods()
        .iter()
        .map(|method| method.defining_name())
        .collect();
    assert_eq!(
        method_names,
        vec!["tick"],
        "the receiver method must be attached to its receiver surface"
    );

    let tick = &counter_surface.methods()[0];
    assert!(
        matches!(tick.kind(), FunctionOriginKind::Receiver(receiver)
            if receiver.defining_name() == "Counter"
            && receiver.category() == OriginTypeCategory::Struct),
        "the method origin must be built with new_receiver using the stable receiver type origin"
    );
}

#[test]
fn generic_struct_receiver_method_attaches_to_generic_base_struct_origin() {
    // `Box` is a public generic struct; `get` is a receiver method whose resolved receiver key is
    // the generic nominal base `Box`, not the instantiated `Box of A`. The resolved catalog
    // carries the base ReceiverKey, so the method attaches to the `Box` stable struct origin.
    let source = "\
export:\n\
    Box type A = |\n\
        value A,\n\
    |\n\
;\n\
get type A |this Box of A| -> A:\n\
    return this.value\n\
;\n";

    let (headers, mut string_table) = parse_single_file_headers_with_table(source);
    let box_path = nominal_type_path(&headers, "Box", &string_table);
    let (function_path, entry) =
        receiver_method_entry("get", ReceiverKey::Struct(box_path), &mut string_table);

    let mut catalog = ReceiverMethodCatalog::default();
    catalog.by_function_path.insert(function_path, entry);

    let origins = build_origins_with_catalog(source, catalog);

    let box_surface = origins
        .receiver_surfaces()
        .iter()
        .find(|surface| surface.receiver().defining_name() == "Box")
        .expect("a public generic struct with a receiver method must own a receiver surface");

    assert_eq!(
        box_surface.receiver().category(),
        OriginTypeCategory::Struct,
        "the generic receiver method must attach to the generic base struct origin"
    );

    let method_names: Vec<&str> = box_surface
        .methods()
        .iter()
        .map(|method| method.defining_name())
        .collect();
    assert_eq!(
        method_names,
        vec!["get"],
        "the generic receiver method must be attached to its receiver surface"
    );
}

#[test]
fn generic_choice_receiver_method_attaches_to_generic_base_choice_origin() {
    // `Maybe` is a public generic choice; `label` is a receiver method whose resolved receiver key
    // is the generic nominal base `Maybe`, not the instantiated `Maybe of A`. The resolved catalog
    // carries the base ReceiverKey, so the method attaches to the `Maybe` stable choice origin.
    let source = "\
export:\n\
    Maybe type A ::\n\
        Some | value A |,\n\
        Missing,\n\
    ;\n\
;\n\
label type A |this Maybe of A| -> String:\n\
    return \"maybe\"\n\
;\n";

    let (headers, mut string_table) = parse_single_file_headers_with_table(source);
    let maybe_path = nominal_type_path(&headers, "Maybe", &string_table);
    let (function_path, entry) =
        receiver_method_entry("label", ReceiverKey::Choice(maybe_path), &mut string_table);

    let mut catalog = ReceiverMethodCatalog::default();
    catalog.by_function_path.insert(function_path, entry);

    let origins = build_origins_with_catalog(source, catalog);

    let maybe_surface = origins
        .receiver_surfaces()
        .iter()
        .find(|surface| surface.receiver().defining_name() == "Maybe")
        .expect("a public generic choice with a receiver method must own a receiver surface");

    assert_eq!(
        maybe_surface.receiver().category(),
        OriginTypeCategory::Choice,
        "the generic receiver method must attach to the generic base choice origin"
    );

    let method_names: Vec<&str> = maybe_surface
        .methods()
        .iter()
        .map(|method| method.defining_name())
        .collect();
    assert_eq!(
        method_names,
        vec!["label"],
        "the generic choice receiver method must be attached to its receiver surface"
    );
}

#[test]
fn receiver_methods_on_private_types_are_excluded_from_the_public_surface() {
    // `Hidden` is private, so its receiver method is not part of any public surface.
    let source = "\
Hidden = | x Int |\n\
poke |this Hidden| -> Int:\n\
    return this.x\n\
;\n";

    let (headers, mut string_table) = parse_single_file_headers_with_table(source);
    // `Hidden` is not public, so nominal_type_path would not find it. Build the receiver path
    // from the private header directly.
    let hidden_path = headers
        .headers
        .iter()
        .find(|header| {
            matches!(header.kind, HeaderKind::Struct { .. })
                && header.tokens.src_path.name_str(&string_table) == Some("Hidden")
        })
        .map(|header| header.tokens.src_path.clone())
        .expect("private struct Hidden must be in the parsed headers");

    let (function_path, entry) =
        receiver_method_entry("poke", ReceiverKey::Struct(hidden_path), &mut string_table);

    let mut catalog = ReceiverMethodCatalog::default();
    catalog.by_function_path.insert(function_path, entry);

    let origins = build_origins_with_catalog(source, catalog);

    assert!(
        origins.export_bindings().is_empty(),
        "a module with no public exports must record no free bindings"
    );
    assert!(
        origins.receiver_surfaces().is_empty(),
        "a receiver method on a private type must not attach to a public receiver surface"
    );
}

#[test]
fn private_declarations_and_implicit_start_are_excluded() {
    // `helper` and `Inner` are private; the implicit start function is always present for an
    // active module root. Only the public `public_fn` must be recorded.
    let source = "\
helper |value Int| -> Int:\n\
    return value\n\
;\n\
Inner = | x Int |\n\
export:\n\
    public_fn |x Int| -> Int:\n\
        return x\n\
    ;\n\
;\n";

    let origins = build_origins(source);

    assert_eq!(
        binding_names(&origins),
        vec!["public_fn"],
        "private declarations and the implicit start function must be excluded"
    );
    assert!(
        origins.receiver_surfaces().is_empty(),
        "a module with no public receiver types must record no receiver surfaces"
    );
}

#[test]
fn ordering_is_deterministic_independent_of_declaration_scheduling() {
    let order_one = "\
export:\n\
    zebra #= 1\n\
    alpha #= 2\n\
;\n";
    let order_two = "\
export:\n\
    alpha #= 2\n\
    zebra #= 1\n\
;\n";

    let first = build_origins(order_one);
    let second = build_origins(order_two);

    assert_eq!(
        binding_names(&first),
        vec!["alpha", "zebra"],
        "export bindings must be sorted by public name"
    );
    assert_eq!(
        first, second,
        "the component must be identical regardless of declaration scheduling"
    );
}

#[test]
fn receiver_surfaces_are_ordered_by_receiver_name_with_methods_ordered_by_name() {
    // Two public structs each with two receiver methods. Surfaces sort by receiver name; methods
    // sort by method name, independent of source order.
    let source = "\
export:\n\
    Alpha = | v Int |\n\
    Beta = | v Int |\n\
;\n\
zap |this Beta| -> Int:\n\
    return this.v\n\
;\n\
beta_first |this Beta| -> Int:\n\
    return this.v\n\
;\n\
zoom |this Alpha| -> Int:\n\
    return this.v\n\
;\n\
alpha_first |this Alpha| -> Int:\n\
    return this.v\n\
;\n";

    let (headers, mut string_table) = parse_single_file_headers_with_table(source);
    let alpha_path = nominal_type_path(&headers, "Alpha", &string_table);
    let beta_path = nominal_type_path(&headers, "Beta", &string_table);

    let mut catalog = ReceiverMethodCatalog::default();
    for (method_name, receiver) in [
        ("zap", ReceiverKey::Struct(beta_path.clone())),
        ("beta_first", ReceiverKey::Struct(beta_path.clone())),
        ("zoom", ReceiverKey::Struct(alpha_path.clone())),
        ("alpha_first", ReceiverKey::Struct(alpha_path.clone())),
    ] {
        let (function_path, entry) =
            receiver_method_entry(method_name, receiver, &mut string_table);
        catalog.by_function_path.insert(function_path, entry);
    }

    let origins = build_origins_with_catalog(source, catalog);

    let receiver_names: Vec<&str> = origins
        .receiver_surfaces()
        .iter()
        .map(|surface| surface.receiver().defining_name())
        .collect();
    assert_eq!(
        receiver_names,
        vec!["Alpha", "Beta"],
        "receiver surfaces must be ordered by receiver defining name"
    );

    let alpha = &origins.receiver_surfaces()[0];
    let method_names: Vec<&str> = alpha
        .methods()
        .iter()
        .map(|method| method.defining_name())
        .collect();
    assert_eq!(
        method_names,
        vec!["alpha_first", "zoom"],
        "methods within a receiver surface must be ordered by method defining name"
    );
}
