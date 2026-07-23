//! Focused hidden-invariant tests for validated generic-template body artefact extraction.
//!
//! WHAT: exercises the total extraction/join owner in
//! [`extract_validated_generic_template_artefacts`]: stable-origin retention, private and
//! non-generic exclusion, and missing/duplicate/mismatch failure. These are side-table facts
//! that integration output cannot inspect.
//! WHY: the store is compiler metadata for the future generated sidecar worklist (R3), not
//! user-visible behaviour. The invariants are owned by
//! `compiler_frontend::validated_generic_template_metadata`.

use crate::compiler_frontend::ast::generic_functions::GenericFunctionTemplate;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::canonical_type_identity::{
    ExportedGenericParameterIdentity, GenericDeclarationOrigin,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::ids::GenericParameterListId;
use crate::compiler_frontend::defined_public_type_surface::PublicGenericParameterSurface;
use crate::compiler_frontend::public_call_summary::PublicCallSummaryState;
use crate::compiler_frontend::public_interface_draft::{
    PublicDeclarationRecord, PublicDeclarationSemantics, PublicFunctionSemantics,
    PublicGenericTemplateDescriptor, PublicInterfaceDraft,
};
use crate::compiler_frontend::semantic_identity::{
    ModuleRootRole, OriginDeclarationId, OriginFunctionId, StableModuleOriginIdentity,
    StablePackageIdentity,
};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation};
use crate::compiler_frontend::validated_generic_template_metadata::extract_validated_generic_template_artefacts;

use rustc_hash::FxHashMap;

fn module_origin() -> StableModuleOriginIdentity {
    StableModuleOriginIdentity::from_portable_path(
        StablePackageIdentity::project_local("test-project"),
        "shapes".to_owned(),
        ModuleRootRole::Normal,
    )
}

fn free_function_origin(name: &str) -> OriginFunctionId {
    OriginFunctionId::new_free(module_origin(), name.to_owned())
}

fn function_record(name: &str, is_generic: bool) -> PublicDeclarationRecord {
    let generic_template = if is_generic {
        Some(PublicGenericTemplateDescriptor {
            generic_parameters: vec![PublicGenericParameterSurface {
                identity: ExportedGenericParameterIdentity::new(
                    GenericDeclarationOrigin::free_function(free_function_origin(name))
                        .expect("free function is a valid generic owner"),
                    0,
                    "T".to_owned(),
                ),
                bounds: vec![],
            }],
        })
    } else {
        None
    };

    PublicDeclarationRecord {
        origin: OriginDeclarationId::Function(free_function_origin(name)),
        semantics: PublicDeclarationSemantics::Function(PublicFunctionSemantics {
            generic_template,
            parameters: vec![],
            returns: vec![],
            error_return: None,
            call_summary: if is_generic {
                PublicCallSummaryState::PendingGenerated
            } else {
                PublicCallSummaryState::PendingLocal
            },
        }),
    }
}

fn empty_draft(records: Vec<PublicDeclarationRecord>) -> PublicInterfaceDraft {
    PublicInterfaceDraft {
        module_origin: module_origin(),
        export_bindings: vec![],
        declarations: records,
        reusable_evidence: vec![],
    }
}

fn template_for(
    name: &str,
    string_table: &mut StringTable,
) -> (InternedPath, GenericFunctionTemplate) {
    let path = InternedPath::from_single_str(name, string_table);
    let template = GenericFunctionTemplate {
        function_path: path.to_owned(),
        source_file: InternedPath::new(),
        generic_parameter_list_id: GenericParameterListId(0),
        signature: FunctionSignature::default(),
        body_tokens: FileTokens::new(path.to_owned(), vec![]),
        declaration_location: SourceLocation::default(),
    };
    (path, template)
}

fn templates_map(
    names: &[&str],
    string_table: &mut StringTable,
) -> FxHashMap<InternedPath, GenericFunctionTemplate> {
    let mut map = FxHashMap::default();
    for name in names {
        let (path, template) = template_for(name, string_table);
        map.insert(path, template);
    }
    map
}

/// Build one template whose donor path is multi-component (`parent::name`) so its defining
/// name differs from its full path. The map key equals the template's own `function_path`.
fn template_under_path(
    parent: &str,
    name: &str,
    string_table: &mut StringTable,
) -> (InternedPath, GenericFunctionTemplate) {
    let path = InternedPath::from_single_str(parent, string_table).join_str(name, string_table);
    let template = GenericFunctionTemplate {
        function_path: path.to_owned(),
        source_file: InternedPath::new(),
        generic_parameter_list_id: GenericParameterListId(0),
        signature: FunctionSignature::default(),
        body_tokens: FileTokens::new(path.to_owned(), vec![]),
        declaration_location: SourceLocation::default(),
    };
    (path, template)
}

/// Build one map entry whose key path differs from the template's own `function_path`.
fn mismatched_template(
    key_name: &str,
    function_path_name: &str,
    string_table: &mut StringTable,
) -> (InternedPath, GenericFunctionTemplate) {
    let key = InternedPath::from_single_str(key_name, string_table);
    let function_path = InternedPath::from_single_str(function_path_name, string_table);
    let template = GenericFunctionTemplate {
        function_path: function_path.to_owned(),
        source_file: InternedPath::new(),
        generic_parameter_list_id: GenericParameterListId(0),
        signature: FunctionSignature::default(),
        body_tokens: FileTokens::new(function_path.to_owned(), vec![]),
        declaration_location: SourceLocation::default(),
    };
    (key, template)
}

#[test]
fn exported_generic_free_function_retains_one_artefact_keyed_by_origin() {
    let mut string_table = StringTable::new();
    let draft = empty_draft(vec![function_record("identity", true)]);
    let templates = templates_map(&["identity"], &mut string_table);

    let store = extract_validated_generic_template_artefacts(&draft, templates, &string_table)
        .expect("one exported generic function with a matching template extracts one artefact");

    assert_eq!(store.len(), 1);
    let artefact = &store.artefacts()[0];
    assert_eq!(
        artefact.origin,
        free_function_origin("identity"),
        "the artefact is keyed by the exact OriginFunctionId"
    );
    assert_eq!(
        artefact.template.function_path,
        InternedPath::from_single_str("identity", &mut StringTable::new()),
        "the artefact retains the one existing template body payload"
    );
}

#[test]
fn non_generic_exported_function_produces_no_artefact() {
    let mut string_table = StringTable::new();
    let draft = empty_draft(vec![function_record("render", false)]);
    let templates = templates_map(&[], &mut string_table);

    let store = extract_validated_generic_template_artefacts(&draft, templates, &string_table)
        .expect("a non-generic exported function with no templates produces an empty store");

    assert!(
        store.is_empty(),
        "a non-generic exported function must produce no artefact"
    );
}

#[test]
fn private_generic_function_is_intentionally_excluded() {
    let mut string_table = StringTable::new();
    // The draft exports no functions. The template map contains one private generic function
    // whose defining name does not match any exported free function.
    let draft = empty_draft(vec![]);
    let templates = templates_map(&["private_helper"], &mut string_table);

    let store = extract_validated_generic_template_artefacts(&draft, templates, &string_table)
        .expect("a private generic template is an intentional exclusion, not an error");

    assert!(
        store.is_empty(),
        "a private generic function must not produce an artefact"
    );
}

#[test]
fn missing_template_for_exported_generic_function_is_compiler_error() {
    let mut string_table = StringTable::new();
    let draft = empty_draft(vec![function_record("identity", true)]);
    let templates = templates_map(&[], &mut string_table);

    let result = extract_validated_generic_template_artefacts(&draft, templates, &string_table);

    assert!(
        matches!(result, Err(CompilerError { .. })),
        "an exported generic function with no matching template must fail as CompilerError"
    );
}

#[test]
fn generic_non_generic_mismatch_is_compiler_error() {
    let mut string_table = StringTable::new();
    // The draft marks "identity" as non-generic, but a generic template exists for it.
    let draft = empty_draft(vec![function_record("identity", false)]);
    let templates = templates_map(&["identity"], &mut string_table);

    let result = extract_validated_generic_template_artefacts(&draft, templates, &string_table);

    assert!(
        matches!(result, Err(CompilerError { .. })),
        "a template for an exported non-generic function is a mismatch CompilerError"
    );
}

#[test]
fn multiple_exported_generic_functions_retain_one_artefact_each_in_deterministic_order() {
    let mut string_table = StringTable::new();
    let draft = empty_draft(vec![
        function_record("zebra", true),
        function_record("alpha", true),
        function_record("mid", true),
    ]);
    // Insert in non-sorted order to prove the store sorts by defining name.
    let templates = templates_map(&["mid", "zebra", "alpha"], &mut string_table);

    let store = extract_validated_generic_template_artefacts(&draft, templates, &string_table)
        .expect("three exported generic functions with matching templates extract three artefacts");

    assert_eq!(store.len(), 3);
    let names: Vec<&str> = store
        .artefacts()
        .iter()
        .map(|artefact| artefact.origin.defining_name())
        .collect();
    assert_eq!(
        names,
        vec!["alpha", "mid", "zebra"],
        "artefacts are sorted by defining name for deterministic iteration"
    );
}

#[test]
fn mixed_exported_and_private_generic_functions_retain_only_exported() {
    let mut string_table = StringTable::new();
    let draft = empty_draft(vec![function_record("public_generic", true)]);
    let templates = templates_map(&["public_generic", "private_generic"], &mut string_table);

    let store = extract_validated_generic_template_artefacts(&draft, templates, &string_table)
        .expect("a private generic template alongside an exported one is an intentional exclusion");

    assert_eq!(store.len(), 1);
    assert_eq!(
        store.artefacts()[0].origin,
        free_function_origin("public_generic"),
        "only the exported generic function retains an artefact"
    );
}

#[test]
fn duplicate_draft_origin_is_compiler_error() {
    let mut string_table = StringTable::new();
    // Two draft records carry the same exported generic free-function origin.
    let draft = empty_draft(vec![
        function_record("identity", true),
        function_record("identity", true),
    ]);
    let templates = templates_map(&["identity"], &mut string_table);

    let result = extract_validated_generic_template_artefacts(&draft, templates, &string_table);

    assert!(
        matches!(result, Err(CompilerError { .. })),
        "a duplicate generic free-function origin in the draft must fail as CompilerError"
    );
}

#[test]
fn duplicate_template_defining_name_with_distinct_donor_paths_is_compiler_error() {
    let mut string_table = StringTable::new();
    let draft = empty_draft(vec![function_record("identity", true)]);
    // Two templates share the defining name "identity" but live under distinct donor paths.
    let mut templates = FxHashMap::default();
    let (first_path, first_template) = template_under_path("shapes", "identity", &mut string_table);
    let (second_path, second_template) =
        template_under_path("other", "identity", &mut string_table);
    templates.insert(first_path, first_template);
    templates.insert(second_path, second_template);

    let result = extract_validated_generic_template_artefacts(&draft, templates, &string_table);

    assert!(
        matches!(result, Err(CompilerError { .. })),
        "two templates sharing a defining name via distinct donor paths must fail as CompilerError"
    );
}

#[test]
fn map_key_template_function_path_mismatch_is_compiler_error() {
    let mut string_table = StringTable::new();
    let draft = empty_draft(vec![function_record("identity", true)]);
    // The map key is "identity" but the template's own function_path is "mismatch".
    let mut templates = FxHashMap::default();
    let (key, template) = mismatched_template("identity", "mismatch", &mut string_table);
    templates.insert(key, template);

    let result = extract_validated_generic_template_artefacts(&draft, templates, &string_table);

    assert!(
        matches!(result, Err(CompilerError { .. })),
        "a map key that does not equal the template's own function_path must fail as CompilerError"
    );
}
