//! Focused unit tests for the transient AST-owned resolved public trait-root producer.
//!
//! WHAT: validates that `build_resolved_public_trait_roots` admits only directly-defined
//! active-root public source traits in deterministic sorted-header order, retains their
//! owning `this_type` and ordered requirement facts, and excludes private traits,
//! non-active-root/imported traits and compiler-owned core traits.
//! WHY: the trait-root vector is a hidden side-table consumed immediately before HIR lowering
//! by the public-interface draft projection; integration output cannot expose its contents
//! or ordering.

use super::environment::resolved_public_trait_roots::build_resolved_public_trait_roots;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::headers::parse_file_headers::{
    FileRole, Header, HeaderExportMode, HeaderKind,
};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation};
use crate::compiler_frontend::traits::definitions::{ResolvedTraitDefinition, TraitVisibility};
use crate::compiler_frontend::traits::environment::TraitEnvironment;
use crate::compiler_frontend::traits::syntax::TraitDeclarationSyntax;

fn trait_header(
    name: &str,
    file_role: FileRole,
    export_mode: HeaderExportMode,
    string_table: &mut StringTable,
) -> Header {
    Header {
        kind: HeaderKind::Trait {
            declaration: TraitDeclarationSyntax {
                name: string_table.intern(name),
                name_location: SourceLocation::default(),
                requirements: Vec::new(),
                location: SourceLocation::default(),
            },
        },
        file_role,
        export_mode,
        local_ordering_hints: std::collections::HashSet::new(),
        name_location: SourceLocation::default(),
        tokens: FileTokens::new(
            InternedPath::from_single_str(name, string_table),
            Vec::new(),
        ),
        source_file: InternedPath::from_single_str("root.bst", string_table),
        capacity_references: Vec::new(),
    }
}

fn function_header(
    name: &str,
    file_role: FileRole,
    export_mode: HeaderExportMode,
    string_table: &mut StringTable,
) -> Header {
    Header {
        kind: HeaderKind::Function {
            generic_parameters: Default::default(),
            signature: Default::default(),
        },
        file_role,
        export_mode,
        local_ordering_hints: std::collections::HashSet::new(),
        name_location: SourceLocation::default(),
        tokens: FileTokens::new(
            InternedPath::from_single_str(name, string_table),
            Vec::new(),
        ),
        source_file: InternedPath::from_single_str("root.bst", string_table),
        capacity_references: Vec::new(),
    }
}

fn this_type(env: &mut TypeEnvironment, string_table: &mut StringTable) -> TypeId {
    env.register_synthetic_generic_parameter(string_table.intern("This"))
}

fn register_source_trait(
    trait_environment: &mut TraitEnvironment,
    string_table: &mut StringTable,
    trait_name: &str,
    this_type: TypeId,
) -> crate::compiler_frontend::traits::ids::TraitId {
    let trait_id = trait_environment.next_trait_id();
    let definition = ResolvedTraitDefinition {
        id: trait_id,
        name: string_table.intern(trait_name),
        canonical_path: InternedPath::from_single_str(trait_name, string_table),
        source_file: InternedPath::new(),
        this_type,
        requirements: Vec::new(),
        declaration_location: SourceLocation::default(),
        visibility: TraitVisibility::Source { exported: true },
    };
    trait_environment.insert(definition);
    trait_id
}

#[test]
fn retains_directly_authored_active_root_public_source_traits_in_order() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let mut trait_environment = TraitEnvironment::new();

    let this_id = this_type(&mut type_environment, &mut string_table);
    register_source_trait(&mut trait_environment, &mut string_table, "Alpha", this_id);
    register_source_trait(&mut trait_environment, &mut string_table, "Beta", this_id);

    let headers = vec![
        trait_header(
            "Alpha",
            FileRole::ActiveModuleRoot,
            HeaderExportMode::Public,
            &mut string_table,
        ),
        trait_header(
            "Beta",
            FileRole::ActiveModuleRoot,
            HeaderExportMode::Public,
            &mut string_table,
        ),
    ];

    let trait_roots =
        build_resolved_public_trait_roots(&headers, &trait_environment, &string_table)
            .expect("two public active-root traits should produce two roots");

    assert_eq!(trait_roots.len(), 2);
    assert_eq!(
        trait_roots[0].canonical_path.to_string(&string_table),
        "Alpha"
    );
    assert_eq!(
        trait_roots[1].canonical_path.to_string(&string_table),
        "Beta"
    );
    assert_eq!(trait_roots[0].this_type, this_id);
    assert_eq!(trait_roots[1].this_type, this_id);
}

#[test]
fn excludes_private_traits() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let mut trait_environment = TraitEnvironment::new();

    let this_id = this_type(&mut type_environment, &mut string_table);
    register_source_trait(&mut trait_environment, &mut string_table, "Public", this_id);
    register_source_trait(
        &mut trait_environment,
        &mut string_table,
        "Private",
        this_id,
    );

    let headers = vec![
        trait_header(
            "Public",
            FileRole::ActiveModuleRoot,
            HeaderExportMode::Public,
            &mut string_table,
        ),
        trait_header(
            "Private",
            FileRole::ActiveModuleRoot,
            HeaderExportMode::Private,
            &mut string_table,
        ),
    ];

    let trait_roots =
        build_resolved_public_trait_roots(&headers, &trait_environment, &string_table)
            .expect("private traits are excluded, not errors");

    assert_eq!(trait_roots.len(), 1);
    assert_eq!(
        trait_roots[0].canonical_path.to_string(&string_table),
        "Public"
    );
}

#[test]
fn excludes_imported_and_non_active_root_traits() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let mut trait_environment = TraitEnvironment::new();

    let this_id = this_type(&mut type_environment, &mut string_table);
    register_source_trait(&mut trait_environment, &mut string_table, "Local", this_id);
    register_source_trait(
        &mut trait_environment,
        &mut string_table,
        "Imported",
        this_id,
    );

    let headers = vec![
        trait_header(
            "Local",
            FileRole::ActiveModuleRoot,
            HeaderExportMode::Public,
            &mut string_table,
        ),
        trait_header(
            "Imported",
            FileRole::ImportedModuleRoot,
            HeaderExportMode::Public,
            &mut string_table,
        ),
    ];

    let trait_roots =
        build_resolved_public_trait_roots(&headers, &trait_environment, &string_table)
            .expect("imported traits are excluded, not errors");

    assert_eq!(trait_roots.len(), 1);
    assert_eq!(
        trait_roots[0].canonical_path.to_string(&string_table),
        "Local"
    );
}

#[test]
fn rejects_source_trait_header_resolving_to_compiler_owned_core_trait() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let mut trait_environment = TraitEnvironment::new();

    // Register a core trait (Displayable) so it exists in the environment.
    let _displayable_id =
        trait_environment.register_core_displayable(&mut type_environment, &mut string_table);

    // Core traits cannot be authored as source declarations. This synthetic source header
    // deliberately resolves to the registered core definition to exercise that invariant.
    let headers = vec![trait_header(
        "DISPLAYABLE",
        FileRole::ActiveModuleRoot,
        HeaderExportMode::Public,
        &mut string_table,
    )];

    let result = build_resolved_public_trait_roots(&headers, &trait_environment, &string_table);

    let error = result.expect_err("a source header must not project a compiler-owned core trait");
    assert!(
        error
            .msg
            .contains("resolved to a compiler-owned core trait")
    );
}

#[test]
fn excludes_non_trait_declarations() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let mut trait_environment = TraitEnvironment::new();

    let this_id = this_type(&mut type_environment, &mut string_table);
    register_source_trait(&mut trait_environment, &mut string_table, "Shape", this_id);

    let headers = vec![
        function_header(
            "render",
            FileRole::ActiveModuleRoot,
            HeaderExportMode::Public,
            &mut string_table,
        ),
        trait_header(
            "Shape",
            FileRole::ActiveModuleRoot,
            HeaderExportMode::Public,
            &mut string_table,
        ),
    ];

    let trait_roots =
        build_resolved_public_trait_roots(&headers, &trait_environment, &string_table)
            .expect("non-trait headers are skipped");

    assert_eq!(trait_roots.len(), 1);
    assert_eq!(
        trait_roots[0].canonical_path.to_string(&string_table),
        "Shape"
    );
}

#[test]
fn missing_trait_definition_is_compiler_error() {
    let mut string_table = StringTable::new();
    let trait_environment = TraitEnvironment::new();

    let headers = vec![trait_header(
        "Orphan",
        FileRole::ActiveModuleRoot,
        HeaderExportMode::Public,
        &mut string_table,
    )];

    let result = build_resolved_public_trait_roots(&headers, &trait_environment, &string_table);

    assert!(
        matches!(result, Err(CompilerError { .. })),
        "a public trait header with no registered definition must be a CompilerError"
    );
}
